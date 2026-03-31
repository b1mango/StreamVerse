const RESERVED_FILENAME_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const MAX_FILENAME_CHARS: usize = 80;
const URL_MARKERS: [&str; 13] = [
    "https://",
    "http://",
    "www.douyin.com/",
    "v.douyin.com/",
    "www.iesdouyin.com/",
    "www.bilibili.com/",
    "bilibili.com/",
    "space.bilibili.com/",
    "b23.tv/",
    "www.youtube.com/",
    "youtube.com/",
    "m.youtube.com/",
    "youtu.be/",
];

pub fn extract_first_url(raw: &str) -> Option<String> {
    raw.split_whitespace().find_map(find_url_in_token)
}

pub fn sanitize_filename(input: &str) -> String {
    let cleaned = input
        .chars()
        .map(|ch| {
            if ch.is_control() || RESERVED_FILENAME_CHARS.contains(&ch) {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();

    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(['.', ' ']);
    let truncated = trimmed.chars().take(MAX_FILENAME_CHARS).collect::<String>();
    let final_name = truncated.trim_matches(['.', ' ']);

    if final_name.is_empty() {
        "untitled".to_string()
    } else {
        final_name.to_string()
    }
}

fn is_url(input: &str) -> bool {
    input.starts_with("http://") || input.starts_with("https://")
}

fn find_url_in_token(token: &str) -> Option<String> {
    for marker in URL_MARKERS {
        let Some(start) = token.find(marker) else {
            continue;
        };
        let candidate = strip_wrapping_punctuation(&token[start..])?;
        let normalized = normalize_url(candidate)?;
        if is_url(&normalized) {
            return Some(normalized);
        }
    }

    None
}

fn normalize_url(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }

    if trimmed.starts_with("//") {
        return Some(format!("https:{trimmed}"));
    }

    Some(format!("https://{trimmed}"))
}

fn strip_wrapping_punctuation(input: &str) -> Option<&str> {
    let trimmed = input.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\''
                | '“'
                | '”'
                | '‘'
                | '’'
                | '('
                | ')'
                | '（'
                | '）'
                | '['
                | ']'
                | '{'
                | '}'
                | '，'
                | ','
                | '。'
                | '！'
                | '!'
                | '；'
                | ';'
        )
    });

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_first_url, sanitize_filename};

    #[test]
    fn extracts_first_url_from_share_text() {
        let raw = "8.88 复制打开抖音，看看 https://v.douyin.com/AbCdEf/ 这是补充文案";
        let url = extract_first_url(raw);
        assert_eq!(url.as_deref(), Some("https://v.douyin.com/AbCdEf/"));
    }

    #[test]
    fn strips_wrapping_punctuation_around_url() {
        let raw = "链接在这里（https://www.douyin.com/video/1234567890）";
        let url = extract_first_url(raw);
        assert_eq!(
            url.as_deref(),
            Some("https://www.douyin.com/video/1234567890")
        );
    }

    #[test]
    fn sanitizes_invalid_filename_characters() {
        let filename = sanitize_filename("春夜/街景:*? 高码率版");
        assert_eq!(filename, "春夜 街景 高码率版");
    }

    #[test]
    fn falls_back_for_empty_filename() {
        let filename = sanitize_filename("////");
        assert_eq!(filename, "untitled");
    }

    #[test]
    fn truncates_overlong_filenames() {
        let filename = sanitize_filename(&"春".repeat(120));
        assert_eq!(filename.chars().count(), 80);
    }

    #[test]
    fn normalizes_scheme_less_bilibili_url() {
        let raw = "bilibili.com/video/BV1VPQSBsEdR/?spm_id_from=333.1387.homepage.video_card.click";
        let url = extract_first_url(raw);
        assert_eq!(
            url.as_deref(),
            Some("https://bilibili.com/video/BV1VPQSBsEdR/?spm_id_from=333.1387.homepage.video_card.click")
        );
    }

    #[test]
    fn normalizes_scheme_less_youtube_url() {
        let raw = "youtu.be/u1JB_opf2u8?t=5";
        let url = extract_first_url(raw);
        assert_eq!(url.as_deref(), Some("https://youtu.be/u1JB_opf2u8?t=5"));
    }
}
