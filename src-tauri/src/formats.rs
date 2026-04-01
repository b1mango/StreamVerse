use crate::VideoFormat;

pub fn dedupe_formats(formats: Vec<VideoFormat>) -> Vec<VideoFormat> {
    let mut deduped = Vec::<VideoFormat>::new();

    for candidate in formats {
        if let Some(existing) = deduped
            .iter_mut()
            .find(|existing| same_profile(existing, &candidate))
        {
            *existing = merge_formats(existing.clone(), candidate);
        } else {
            deduped.push(candidate);
        }
    }

    if !deduped.iter().any(|format| format.recommended) {
        if let Some(first) = deduped.first_mut() {
            first.recommended = true;
        }
    }

    deduped
}

pub fn pick_preferred_format(
    formats: &[VideoFormat],
    quality_preference: &str,
    include_login_formats: bool,
) -> Option<VideoFormat> {
    let deduped = dedupe_formats(formats.to_vec());
    let visible = if include_login_formats {
        deduped
    } else {
        deduped
            .into_iter()
            .filter(|format| !format.requires_login)
            .collect()
    };
    let candidates = if visible.is_empty() {
        dedupe_formats(formats.to_vec())
    } else {
        visible
    };

    let mut ranked = candidates.clone();
    ranked.sort_by(|left, right| {
        profile_height(right)
            .cmp(&profile_height(left))
            .then_with(|| right.bitrate_kbps.cmp(&left.bitrate_kbps))
    });

    match quality_preference {
        "highest" => ranked.first().cloned(),
        "smallest" => ranked.last().cloned().or_else(|| ranked.first().cloned()),
        "no_watermark" => ranked
            .iter()
            .find(|format| format.no_watermark)
            .cloned()
            .or_else(|| ranked.iter().find(|format| format.recommended).cloned())
            .or_else(|| ranked.first().cloned()),
        _ => candidates
            .iter()
            .find(|format| format.recommended)
            .cloned()
            .or_else(|| ranked.first().cloned()),
    }
}

fn same_profile(left: &VideoFormat, right: &VideoFormat) -> bool {
    quality_key(left) == quality_key(right) && left.requires_login == right.requires_login
}

fn normalized(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

fn profile_height(format: &VideoFormat) -> u32 {
    format
        .resolution
        .split('x')
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_default()
}

fn merge_formats(existing: VideoFormat, candidate: VideoFormat) -> VideoFormat {
    let prefer_candidate = candidate.recommended && !existing.recommended
        || candidate.no_watermark && !existing.no_watermark
        || candidate.direct_url.is_some() && existing.direct_url.is_none()
        || candidate.audio_direct_url.is_some() && existing.audio_direct_url.is_none()
        || candidate.bitrate_kbps > existing.bitrate_kbps
        || (candidate.bitrate_kbps == existing.bitrate_kbps
            && codec_priority(&candidate.codec) < codec_priority(&existing.codec))
        || (existing.id == "best" && candidate.id != "best");

    let (mut primary, secondary) = if prefer_candidate {
        (candidate, existing)
    } else {
        (existing, candidate)
    };

    if !primary.recommended && secondary.recommended {
        primary.recommended = true;
    }
    if primary.direct_url.is_none() {
        primary.direct_url = secondary.direct_url;
    }
    if primary.referer.is_none() {
        primary.referer = secondary.referer;
    }
    if primary.user_agent.is_none() {
        primary.user_agent = secondary.user_agent;
    }
    if primary.audio_direct_url.is_none() {
        primary.audio_direct_url = secondary.audio_direct_url;
    }
    if primary.audio_referer.is_none() {
        primary.audio_referer = secondary.audio_referer;
    }
    if primary.audio_user_agent.is_none() {
        primary.audio_user_agent = secondary.audio_user_agent;
    }
    if primary.bitrate_kbps == 0 {
        primary.bitrate_kbps = secondary.bitrate_kbps;
    }

    primary
}

fn quality_key(format: &VideoFormat) -> String {
    let height = profile_height(format);
    if height > 0 {
        return format!("H{height}");
    }

    format!(
        "{}|{}",
        normalized(&format.label),
        normalized(&format.resolution)
    )
}

fn codec_priority(value: &str) -> u8 {
    let normalized = normalized(value);
    if normalized.starts_with("H264") || normalized.starts_with("AVC") {
        return 0;
    }
    if normalized.starts_with("H265") || normalized.starts_with("HEVC") {
        return 1;
    }
    if normalized.starts_with("AV1") {
        return 2;
    }
    if normalized.starts_with("VP9") {
        return 3;
    }
    4
}

#[cfg(test)]
mod tests {
    use super::{dedupe_formats, pick_preferred_format};
    use crate::VideoFormat;

    #[test]
    fn collapses_duplicate_visible_formats() {
        let formats = vec![
            sample_format("fmt-1", false, false, 4000, false),
            sample_format("fmt-2", false, false, 4200, true),
            sample_format("fmt-3", true, false, 3000, false),
        ];

        let deduped = dedupe_formats(formats);
        assert_eq!(deduped.len(), 1);
        assert!(deduped.iter().any(|format| format.recommended));
        assert!(deduped[0].no_watermark);
        assert_eq!(deduped[0].bitrate_kbps, 3000);
        assert_eq!(deduped[0].id, "fmt-3");
    }

    #[test]
    fn treats_spacing_and_case_as_same_visible_format() {
        let mut first = sample_format("fmt-1", false, false, 4000, false);
        first.label = "1080p ".to_string();
        first.codec = " h.265".to_string();

        let mut second = sample_format("fmt-2", false, false, 4200, true);
        second.label = " 1080P".to_string();
        second.codec = "H.265 ".to_string();

        let deduped = dedupe_formats(vec![first, second]);
        assert_eq!(deduped.len(), 1);
        assert!(deduped[0].recommended);
        assert_eq!(deduped[0].id, "fmt-2");
    }

    #[test]
    fn prefers_no_watermark_when_requested() {
        let mut standard = sample_format("fmt-1", false, false, 5200, false);
        standard.label = "1080P".to_string();

        let mut no_watermark = sample_format("fmt-2", true, false, 4800, false);
        no_watermark.label = "1080P 高码率".to_string();

        let selected =
            pick_preferred_format(&[standard, no_watermark.clone()], "no_watermark", false)
                .unwrap();

        assert_eq!(selected.id, no_watermark.id);
    }

    fn sample_format(
        id: &str,
        no_watermark: bool,
        requires_login: bool,
        bitrate_kbps: u32,
        recommended: bool,
    ) -> VideoFormat {
        VideoFormat {
            id: id.to_string(),
            label: "1080P".to_string(),
            resolution: "1920x1080".to_string(),
            bitrate_kbps,
            codec: "H.265".to_string(),
            container: "MP4".to_string(),
            no_watermark,
            requires_login,
            requires_processing: false,
            recommended,
            direct_url: Some(format!("https://example.com/{id}.mp4")),
            referer: None,
            user_agent: None,
            audio_direct_url: None,
            audio_referer: None,
            audio_user_agent: None,
            file_size_bytes: None,
        }
    }
}
