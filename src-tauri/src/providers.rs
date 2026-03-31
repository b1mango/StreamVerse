use crate::{pack_host, parser, platforms, BrowserLaunchResult, ProfileBatch, VideoAsset};
use std::path::Path;

pub fn analyze_input(
    raw_input: &str,
    cookie_browser: Option<&str>,
    progress_file: Option<&Path>,
) -> Result<VideoAsset, String> {
    let source_url = extract_source_url(
        raw_input,
        "未在输入内容里找到可用链接，请粘贴完整分享文案或作品链接。",
    )?;
    let platform = platforms::detect_platform(&source_url);

    if platform == "youtube" {
        return pack_host::analyze_single(&source_url, None, None, progress_file);
    }

    if platform == "douyin" {
        let selected_browser = cookie_browser
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "抖音单视频解析需要登录态。请先在设置中选择一个已登录抖音的浏览器 Cookie 后再试。"
                    .to_string()
            })?;

        return pack_host::analyze_single(&source_url, Some(selected_browser), None, progress_file)
            .map_err(normalize_douyin_error);
    }

    match pack_host::analyze_single(&source_url, None, None, progress_file) {
        Ok(asset) => Ok(asset),
        Err(error) if cookie_browser.is_some() => {
            pack_host::analyze_single(&source_url, cookie_browser, None, progress_file)
                .or(Err(error))
        }
        Err(error) => Err(error),
    }
}

fn normalize_douyin_error(error: String) -> String {
    if error.contains("获取数据失败")
        || error.contains("Fresh cookies")
        || error.contains("Failed to download web detail JSON")
    {
        return "抖音登录态已失效，或当前浏览器 Cookie 不可用。请重新登录抖音后，在设置里重新选择浏览器 Cookie 再试。".to_string();
    }

    error
}

pub fn analyze_profile_input(
    raw_input: &str,
    cookie_browser: Option<&str>,
    progress_file: Option<&Path>,
) -> Result<ProfileBatch, String> {
    let source_url = extract_source_url(
        raw_input,
        "未在输入内容里找到可用主页链接，请粘贴完整主页分享文案或主页链接。",
    )?;

    match platforms::detect_platform(&source_url) {
        "douyin" => Err("抖音主页批量下载请使用“打开浏览器”后再点“读取完整列表”。".to_string()),
        _ => pack_host::analyze_profile(&source_url, cookie_browser, None, progress_file),
    }
}

pub fn open_profile_browser(
    raw_input: &str,
    cookie_browser: Option<&str>,
) -> Result<BrowserLaunchResult, String> {
    let source_url = extract_source_url(
        raw_input,
        "未在输入内容里找到可用主页链接，请粘贴完整主页分享文案或主页链接。",
    )?;
    pack_host::open_profile_browser(&source_url, cookie_browser)
}

pub fn collect_profile_browser(
    raw_input: &str,
    port: u16,
    cookie_browser: Option<&str>,
    progress_file: Option<&Path>,
) -> Result<ProfileBatch, String> {
    let source_url = extract_source_url(
        raw_input,
        "未在输入内容里找到可用主页链接，请粘贴完整主页分享文案或主页链接。",
    )?;
    pack_host::collect_profile_browser(&source_url, port, cookie_browser, progress_file)
}

fn extract_source_url(raw_input: &str, message: &str) -> Result<String, String> {
    parser::extract_first_url(raw_input.trim()).ok_or_else(|| message.to_string())
}

#[cfg(test)]
mod tests {
    use super::{analyze_profile_input, extract_source_url, normalize_douyin_error};

    #[test]
    fn rejects_missing_url_for_provider_entry() {
        let error = extract_source_url("没有链接", "missing").unwrap_err();
        assert_eq!(error, "missing");
    }

    #[test]
    fn douyin_profile_requires_manual_browser_flow() {
        let error =
            analyze_profile_input("https://www.douyin.com/user/test", None, None).unwrap_err();
        assert!(error.contains("打开浏览器"));
    }

    #[test]
    fn accepts_scheme_less_bilibili_video_url() {
        let url = extract_source_url("bilibili.com/video/BV1VPQSBsEdR", "missing").unwrap();
        assert_eq!(url, "https://bilibili.com/video/BV1VPQSBsEdR");
    }

    #[test]
    fn normalizes_generic_douyin_fetch_errors() {
        let message = normalize_douyin_error("获取数据失败".to_string());
        assert!(message.contains("浏览器 Cookie"));
    }
}
