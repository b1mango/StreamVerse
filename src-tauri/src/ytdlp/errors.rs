pub(super) fn normalize_download_failure(platform: &str, reason: String) -> String {
    if reason.contains("Unable to connect to proxy") || reason.contains("Tunnel connection failed") {
        return "代理隧道连接失败（代理返回 503）。这通常不是视频本身的问题——请在代理软件中切换到可用节点后重试；若持续失败，请检查代理软件的节点连通性。"
            .to_string();
    }
    if platform == "youtube"
        && (reason.contains("HTTP Error 403") || reason.contains("403: Forbidden"))
    {
        return "YouTube 拒绝了当前播放地址（HTTP 403）。请重新解析后重试；如果仍失败，请在设置中切换代理节点，或清空代理后再试。"
            .to_string();
    }
    reason
}

pub(super) fn readable_error(stderr: &[u8], fallback: &str) -> String {
    let message = String::from_utf8_lossy(stderr);
    let trimmed = message.trim();

    if trimmed.contains("Fresh cookies") {
        return "当前抖音链接需要新鲜浏览器 Cookie。请在应用顶部选择 Chrome、Edge、Firefox 等浏览器来源后重新解析。".to_string();
    }

    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}
