const RESERVED_FILENAME_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const MAX_FILENAME_CHARS: usize = 80;

pub fn extract_first_url(raw: &str) -> Option<String> {
    raw.split_whitespace()
        .find_map(find_url_in_token)
        .map(ToOwned::to_owned)
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

fn find_url_in_token(token: &str) -> Option<&str> {
    let start = token.find("https://").or_else(|| token.find("http://"))?;

    strip_wrapping_punctuation(&token[start..]).filter(|item| is_url(item))
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
}
