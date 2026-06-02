use crate::{ProfileBatch, VideoAsset};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProfileItemStatus {
    Idle,
    Pending,
    Loading,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileItemPatch {
    pub(crate) asset_id: String,
    pub(crate) thumbnail_status: ProfileItemStatus,
    pub(crate) format_status: ProfileItemStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub(crate) enum ProfileSessionEvent {
    Started {
        session_id: String,
        profile_title: String,
        source_url: String,
        total_available: u32,
    },
    ItemsAppended {
        session_id: String,
        items: Vec<VideoAsset>,
    },
    ItemPatched {
        session_id: String,
        patch: ProfileItemPatch,
    },
    Completed {
        session_id: String,
        fetched_count: u32,
        skipped_count: u32,
        session_cookie_file: Option<String>,
    },
    Failed {
        session_id: String,
        message: String,
    },
}

pub(crate) fn started_event(session_id: &str, source_url: &str) -> ProfileSessionEvent {
    ProfileSessionEvent::Started {
        session_id: session_id.to_string(),
        profile_title: "读取中".to_string(),
        source_url: source_url.to_string(),
        total_available: 0,
    }
}

pub(crate) fn batch_to_events(session_id: &str, batch: &ProfileBatch) -> Vec<ProfileSessionEvent> {
    let mut events = Vec::with_capacity(batch.items.len() + 3);
    let session_id = session_id.to_string();

    events.push(ProfileSessionEvent::Started {
        session_id: session_id.clone(),
        profile_title: batch.profile_title.clone(),
        source_url: batch.source_url.clone(),
        total_available: batch.total_available,
    });

    events.push(ProfileSessionEvent::ItemsAppended {
        session_id: session_id.clone(),
        items: batch.items.clone(),
    });

    events.extend(batch.items.iter().map(|item| {
        ProfileSessionEvent::ItemPatched {
            session_id: session_id.clone(),
            patch: ProfileItemPatch {
                asset_id: item.asset_id.clone(),
                thumbnail_status: if item.cover_url.as_deref().is_some_and(|url| !url.is_empty()) {
                    ProfileItemStatus::Ready
                } else {
                    ProfileItemStatus::Pending
                },
                format_status: if item.formats.is_empty() {
                    ProfileItemStatus::Pending
                } else {
                    ProfileItemStatus::Ready
                },
            },
        }
    }));

    events.push(ProfileSessionEvent::Completed {
        session_id,
        fetched_count: batch.fetched_count,
        skipped_count: batch.skipped_count,
        session_cookie_file: batch.session_cookie_file.clone(),
    });

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProfileBatch, VideoAsset, VideoFormat, DEFAULT_GRADIENT};

    fn sample_format(id: &str, label: &str) -> VideoFormat {
        VideoFormat {
            id: id.to_string(),
            label: label.to_string(),
            resolution: "1920x1080".to_string(),
            bitrate_kbps: 4200,
            codec: "H.264".to_string(),
            container: "MP4".to_string(),
            no_watermark: true,
            requires_login: false,
            requires_processing: false,
            recommended: true,
            direct_url: Some("https://example.com/video.mp4".to_string()),
            referer: None,
            user_agent: None,
            audio_direct_url: None,
            audio_referer: None,
            audio_user_agent: None,
            file_size_bytes: Some(1024),
        }
    }

    fn sample_asset(asset_id: &str, title: &str, with_format: bool) -> VideoAsset {
        VideoAsset {
            asset_id: asset_id.to_string(),
            platform: "bilibili".to_string(),
            source_url: format!("https://www.bilibili.com/video/{asset_id}"),
            title: title.to_string(),
            author: "UP".to_string(),
            duration_seconds: 120,
            publish_date: "2026-06-02".to_string(),
            caption: String::new(),
            category_label: Some("普通视频".to_string()),
            group_title: None,
            cover_url: Some(format!("https://example.com/{asset_id}.jpg")),
            cover_gradient: DEFAULT_GRADIENT.to_string(),
            formats: if with_format {
                vec![sample_format("1080", "1080P")]
            } else {
                vec![]
            },
        }
    }

    fn sample_batch() -> ProfileBatch {
        ProfileBatch {
            profile_title: "UP 主".to_string(),
            source_url: "https://space.bilibili.com/1".to_string(),
            total_available: 2,
            fetched_count: 2,
            skipped_count: 0,
            session_cookie_file: None,
            items: vec![
                sample_asset("BV1", "第一条", true),
                sample_asset("BV2", "第二条", false),
            ],
        }
    }

    #[test]
    fn streams_batch_in_order() {
        let events = batch_to_events("session-1", &sample_batch());

        assert!(matches!(events.first(), Some(ProfileSessionEvent::Started { .. })));
        assert!(matches!(
            events.get(1),
            Some(ProfileSessionEvent::ItemsAppended { items, .. }) if items.iter().map(|item| item.asset_id.as_str()).collect::<Vec<_>>() == vec!["BV1", "BV2"]
        ));
        assert!(matches!(events.last(), Some(ProfileSessionEvent::Completed { .. })));
    }

    #[test]
    fn patches_include_thumbnail_and_format_status() {
        let events = batch_to_events("session-1", &sample_batch());

        let patches = events
            .iter()
            .filter_map(|event| match event {
                ProfileSessionEvent::ItemPatched { patch, .. } => Some(patch),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0].asset_id, "BV1");
        assert_eq!(patches[0].thumbnail_status, ProfileItemStatus::Ready);
        assert_eq!(patches[0].format_status, ProfileItemStatus::Ready);
        assert_eq!(patches[1].asset_id, "BV2");
        assert_eq!(patches[1].thumbnail_status, ProfileItemStatus::Ready);
        assert_eq!(patches[1].format_status, ProfileItemStatus::Pending);
    }

    #[test]
    fn started_event_uses_requested_url_before_metadata() {
        let event = started_event("session-1", "https://space.bilibili.com/1");

        assert!(matches!(
            event,
            ProfileSessionEvent::Started {
                ref session_id,
                ref source_url,
                total_available,
                ..
            } if session_id == "session-1"
                && source_url == "https://space.bilibili.com/1"
                && total_available == 0
        ));
    }
}
