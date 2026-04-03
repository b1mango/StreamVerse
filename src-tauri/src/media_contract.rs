use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_GRADIENT: &str =
    "linear-gradient(135deg, rgba(13, 190, 165, 0.95), rgba(97, 87, 255, 0.8))";

fn default_gradient() -> String {
    DEFAULT_GRADIENT.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoFormat {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) resolution: String,
    pub(crate) bitrate_kbps: u32,
    pub(crate) codec: String,
    pub(crate) container: String,
    pub(crate) no_watermark: bool,
    pub(crate) requires_login: bool,
    #[serde(default)]
    pub(crate) requires_processing: bool,
    pub(crate) recommended: bool,
    pub(crate) direct_url: Option<String>,
    pub(crate) referer: Option<String>,
    pub(crate) user_agent: Option<String>,
    #[serde(default)]
    pub(crate) audio_direct_url: Option<String>,
    #[serde(default)]
    pub(crate) audio_referer: Option<String>,
    #[serde(default)]
    pub(crate) audio_user_agent: Option<String>,
    #[serde(default)]
    pub(crate) file_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoAsset {
    #[serde(alias = "awemeId")]
    pub(crate) asset_id: String,
    #[serde(default)]
    pub(crate) platform: String,
    pub(crate) source_url: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) author: String,
    #[serde(default)]
    pub(crate) duration_seconds: u32,
    #[serde(default)]
    pub(crate) publish_date: String,
    #[serde(default)]
    pub(crate) caption: String,
    pub(crate) category_label: Option<String>,
    pub(crate) group_title: Option<String>,
    pub(crate) cover_url: Option<String>,
    #[serde(default = "default_gradient")]
    pub(crate) cover_gradient: String,
    #[serde(default)]
    pub(crate) formats: Vec<VideoFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileBatch {
    pub(crate) profile_title: String,
    pub(crate) source_url: String,
    pub(crate) total_available: u32,
    pub(crate) fetched_count: u32,
    pub(crate) skipped_count: u32,
    pub(crate) session_cookie_file: Option<String>,
    pub(crate) items: Vec<VideoAsset>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchItemSelection {
    pub(crate) asset: VideoAsset,
    pub(crate) selected_format_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadContentSelection {
    pub(crate) download_video: bool,
    #[serde(default)]
    pub(crate) download_audio: bool,
    pub(crate) download_cover: bool,
    pub(crate) download_caption: bool,
    pub(crate) download_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserLaunchResult {
    pub(crate) port: u16,
    pub(crate) browser: String,
}

#[cfg(test)]
mod tests {
    use super::ProfileBatch;

    #[test]
    fn deserializes_profile_batch_without_optional_format_fields() {
        let payload = r#"{
          "profileTitle":"测试主页",
          "sourceUrl":"https://www.douyin.com/user/test",
          "totalAvailable":1,
          "fetchedCount":1,
          "skippedCount":0,
          "items":[
            {
              "awemeId":"123",
              "platform":"douyin",
              "sourceUrl":"https://www.douyin.com/video/123",
              "title":"测试视频",
              "author":"测试作者",
              "durationSeconds":10,
              "publishDate":"2026-03-31",
              "caption":"caption",
              "coverUrl":null,
              "coverGradient":"linear-gradient(135deg,#000,#111)",
              "formats":[
                {
                  "id":"fmt-1",
                  "label":"1080P",
                  "resolution":"1920x1080",
                  "bitrateKbps":4200,
                  "codec":"H.264",
                  "container":"MP4",
                  "noWatermark":true,
                  "requiresLogin":false,
                  "recommended":true,
                  "directUrl":"https://example.com/video.mp4",
                  "referer":"https://www.douyin.com/",
                  "userAgent":"Mozilla/5.0"
                }
              ]
            }
          ]
        }"#;

        let batch: ProfileBatch =
            serde_json::from_str(payload).expect("profile batch should deserialize");
        assert_eq!(batch.items.len(), 1);
        assert_eq!(batch.items[0].asset_id, "123");
        assert!(!batch.items[0].formats[0].requires_processing);
        assert!(batch.items[0].formats[0].audio_direct_url.is_none());
    }
}
