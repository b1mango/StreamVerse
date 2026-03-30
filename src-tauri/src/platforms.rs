pub fn detect_platform(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();

    if lower.contains("douyin.com") || lower.contains("iesdouyin.com") {
        return "douyin";
    }

    if lower.contains("bilibili.com") || lower.contains("b23.tv") {
        return "bilibili";
    }

    if lower.contains("youtube.com") || lower.contains("youtu.be") {
        return "youtube";
    }

    "unknown"
}

pub fn human_platform_name(platform: &str) -> &'static str {
    match platform {
        "douyin" => "抖音",
        "bilibili" => "Bilibili",
        "youtube" => "YouTube",
        _ => "视频",
    }
}
