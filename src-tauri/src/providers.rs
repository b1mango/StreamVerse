use crate::{
    pack_host, parser, platforms, settings, BrowserLaunchResult, ProfileBatch, VideoAsset,
};
use std::collections::BTreeMap;
use std::path::Path;

pub fn analyze_input(
    raw_input: &str,
    platform_auth: &BTreeMap<String, settings::PlatformAuthSettings>,
    progress_file: Option<&Path>,
) -> Result<VideoAsset, String> {
    let source_url = extract_source_url(
        raw_input,
        "未在输入内容里找到可用链接，请粘贴完整分享文案或作品链接。",
    )?;
    let platform = platforms::detect_platform(&source_url);
    let auth = settings::platform_auth_for(platform_auth, platform);
    let selected_browser = auth
        .cookie_browser
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let selected_cookie_file = auth
        .cookie_file
        .as_deref()
        .filter(|value| !value.trim().is_empty());

    preflight_auth(platform, selected_browser, selected_cookie_file)?;

    if platform == "youtube" {
        return pack_host::analyze_single(
            &source_url,
            selected_browser,
            selected_cookie_file,
            progress_file,
        );
    }

    if platform == "douyin" {
        if selected_browser.is_none() && selected_cookie_file.is_none() {
            return Err(
                "抖音单视频解析需要登录态。请先在设置中选择一个已登录抖音的浏览器，或导入一个有效的 cookies.txt 文件后再试。"
                    .to_string(),
            );
        }

        return pack_host::analyze_single(
            &source_url,
            selected_browser,
            selected_cookie_file,
            progress_file,
        )
        .map_err(normalize_douyin_error);
    }

    match pack_host::analyze_single(&source_url, None, None, progress_file) {
        Ok(asset) => Ok(asset),
        Err(error) if selected_browser.is_some() || selected_cookie_file.is_some() => {
            pack_host::analyze_single(
                &source_url,
                selected_browser,
                selected_cookie_file,
                progress_file,
            )
            .or(Err(error))
        }
        Err(error) => Err(error),
    }
}

fn normalize_douyin_error(error: String) -> String {
    if error.contains("Failed to decrypt with DPAPI") {
        return "Chrome 在 Windows 上启用了新的 Cookie 加密，当前无法直接从浏览器解密。请先用浏览器扩展导出 Netscape 格式的 cookies.txt，再到设置里选择该文件后重试。"
            .to_string();
    }

    if error.contains("获取数据失败")
        || error.contains("Fresh cookies")
        || error.contains("Failed to download web detail JSON")
    {
        return "抖音登录态已失效，或当前 Cookie 来源不可用。请重新登录抖音后再试；如果你使用的是 Chrome，建议改为导入 cookies.txt 文件。".to_string();
    }

    if error.contains("timed out")
        || error.contains("Timeout")
        || error.contains("ConnectTimeout")
        || error.contains("ReadTimeout")
    {
        return "网络连接超时，请检查网络连接后重试。".to_string();
    }

    if error.contains("getaddrinfo failed")
        || error.contains("Name or service not known")
        || error.contains("nodename nor servname")
    {
        return "DNS 解析失败，请检查网络连接或尝试更换 DNS 服务器。".to_string();
    }

    if error.contains("SSL")
        || error.contains("CERTIFICATE_VERIFY_FAILED")
        || error.contains("certificate verify failed")
    {
        return "SSL 证书验证失败，请检查系统时间是否正确以及网络环境是否安全。".to_string();
    }

    if error.contains("HTTP Error 429")
        || error.contains("Too Many Requests")
        || error.contains("rate limit")
    {
        return "请求过于频繁被限流，请稍等几分钟后重试。".to_string();
    }

    if error.contains("HTTP Error 403") || error.contains("Forbidden") {
        return "访问被拒绝（403），可能需要重新登录或更换 Cookie。".to_string();
    }

    error
}

pub fn analyze_profile_input(
    raw_input: &str,
    platform_auth: &BTreeMap<String, settings::PlatformAuthSettings>,
    progress_file: Option<&Path>,
) -> Result<ProfileBatch, String> {
    let source_url = extract_source_url(
        raw_input,
        "未在输入内容里找到可用主页链接，请粘贴完整主页分享文案或主页链接。",
    )?;

    let platform = platforms::detect_platform(&source_url);
    let auth = settings::platform_auth_for(platform_auth, platform);
    let cookie_browser = auth
        .cookie_browser
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let cookie_file = auth
        .cookie_file
        .as_deref()
        .filter(|value| !value.trim().is_empty());

    preflight_auth(platform, cookie_browser, cookie_file)?;

    match platform {
        "douyin" => {
            if cookie_file
                .filter(|value| !value.trim().is_empty())
                .is_some()
                || cookie_browser
                    .filter(|value| !value.trim().is_empty())
                    .is_some()
            {
                pack_host::analyze_profile(&source_url, cookie_browser, cookie_file, progress_file)
                    .map_err(normalize_douyin_error)
            } else {
                Err(
                    "抖音主页批量下载需要登录态。请先在设置中选择浏览器或导入 Cookie 后再试。"
                        .to_string(),
                )
            }
        }
        _ => pack_host::analyze_profile(&source_url, cookie_browser, cookie_file, progress_file),
    }
}

pub fn open_profile_browser(
    raw_input: &str,
    platform_auth: &BTreeMap<String, settings::PlatformAuthSettings>,
) -> Result<BrowserLaunchResult, String> {
    let source_url = extract_source_url(
        raw_input,
        "未在输入内容里找到可用主页链接，请粘贴完整主页分享文案或主页链接。",
    )?;
    let platform = platforms::detect_platform(&source_url);
    let auth = settings::platform_auth_for(platform_auth, platform);
    pack_host::open_profile_browser(&source_url, auth.cookie_browser.as_deref())
}

pub fn collect_profile_browser(
    raw_input: &str,
    port: u16,
    platform_auth: &BTreeMap<String, settings::PlatformAuthSettings>,
    progress_file: Option<&Path>,
) -> Result<ProfileBatch, String> {
    let source_url = extract_source_url(
        raw_input,
        "未在输入内容里找到可用主页链接，请粘贴完整主页分享文案或主页链接。",
    )?;
    let platform = platforms::detect_platform(&source_url);
    let auth = settings::platform_auth_for(platform_auth, platform);
    pack_host::collect_profile_browser(
        &source_url,
        port,
        auth.cookie_browser.as_deref(),
        auth.cookie_file.as_deref(),
        progress_file,
    )
}

fn preflight_auth(
    platform: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if matches!(platform, "douyin" | "bilibili")
            && cookie_browser == Some("chrome")
            && cookie_file.is_none()
        {
            return Err(format!(
                "Chrome 在 Windows 上启用了新的 Cookie 加密，当前无法直接从浏览器读取 {} 登录态。请先导出 Netscape 格式的 cookies.txt，再到设置里为 {} 选择该文件后重试。",
                settings_platform_human_name(platform),
                settings_platform_human_name(platform)
            ));
        }
    }

    if let Some(cookie_file) = cookie_file {
        settings::validate_cookie_file_for_platform(cookie_file, platform)?;
    }

    Ok(())
}

fn settings_platform_human_name(platform: &str) -> &'static str {
    match platform {
        "douyin" => "抖音",
        "bilibili" => "Bilibili",
        "youtube" => "YouTube",
        _ => "当前平台",
    }
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
        use std::collections::BTreeMap;
        let empty_auth = BTreeMap::new();
        let error = analyze_profile_input("https://www.douyin.com/user/test", &empty_auth, None)
            .unwrap_err();
        assert!(error.contains("打开浏览器") || error.contains("Cookie") || error.contains("登录"));
    }

    #[test]
    fn accepts_scheme_less_bilibili_video_url() {
        let url = extract_source_url("bilibili.com/video/BV1VPQSBsEdR", "missing").unwrap();
        assert_eq!(url, "https://bilibili.com/video/BV1VPQSBsEdR");
    }

    #[test]
    fn normalizes_generic_douyin_fetch_errors() {
        let message = normalize_douyin_error("获取数据失败".to_string());
        assert!(message.contains("Cookie") || message.contains("登录"));
    }
}
