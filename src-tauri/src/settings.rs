use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const AUTH_PLATFORM_IDS: [&str; 3] = ["douyin", "bilibili", "youtube"];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlatformAuthSettings {
    pub mode: String,
    pub browser_id: Option<String>,
    pub profile_id: Option<String>,
    pub consented_at: Option<u64>,
    pub status: String,
    #[serde(skip)]
    pub cookie_browser: Option<String>,
    #[serde(skip)]
    pub cookie_file: Option<String>,
}

impl Default for PlatformAuthSettings {
    fn default() -> Self {
        Self {
            mode: "none".to_string(),
            browser_id: None,
            profile_id: None,
            consented_at: None,
            status: "guest".to_string(),
            cookie_browser: None,
            cookie_file: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub platform_auth: BTreeMap<String, PlatformAuthSettings>,
    pub save_directory: String,
    pub download_mode: String,
    pub quality_preference: String,
    pub auto_reveal_in_finder: bool,
    pub max_concurrent_downloads: u32,
    pub proxy_url: Option<String>,
    pub speed_limit: Option<String>,
    pub auto_update: bool,
    pub theme: String,
    pub notify_on_complete: bool,
    pub language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            platform_auth: default_platform_auth(),
            save_directory: default_save_directory(),
            download_mode: "manual".to_string(),
            quality_preference: "recommended".to_string(),
            auto_reveal_in_finder: false,
            max_concurrent_downloads: 3,
            proxy_url: None,
            speed_limit: None,
            auto_update: true,
            theme: "dark".to_string(),
            notify_on_complete: true,
            language: "zh-CN".to_string(),
        }
    }
}

pub fn load_settings() -> AppSettings {
    let mut settings = fs::read_to_string(settings_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<AppSettings>(&raw).ok())
        .unwrap_or_default();
    settings
        .platform_auth
        .retain(|platform, _| AUTH_PLATFORM_IDS.contains(&platform.as_str()));
    for platform in AUTH_PLATFORM_IDS {
        let entry = settings
            .platform_auth
            .entry(platform.to_string())
            .or_default();
        hydrate_auth(platform, entry);
    }
    settings.max_concurrent_downloads = normalize_max_concurrent(settings.max_concurrent_downloads);
    settings
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建设置目录失败：{error}"))?;
    }
    let content =
        serde_json::to_vec_pretty(settings).map_err(|error| format!("序列化设置失败：{error}"))?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, content).map_err(|error| format!("写入设置失败：{error}"))?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| format!("替换设置失败：{error}"))?;
    }
    fs::rename(temporary, path).map_err(|error| format!("保存设置失败：{error}"))
}

pub fn platform_auth_for(
    platform_auth: &BTreeMap<String, PlatformAuthSettings>,
    platform: &str,
) -> PlatformAuthSettings {
    let mut auth = platform_auth.get(platform).cloned().unwrap_or_default();
    hydrate_auth(platform, &mut auth);
    auth
}

fn hydrate_auth(platform: &str, entry: &mut PlatformAuthSettings) {
    entry.cookie_browser = None;
    entry.cookie_file = crate::auth::cookie_file_for(platform);
    if entry.status == "active" && entry.cookie_file.is_none() {
        entry.status = "expired".to_string();
    }
}

pub fn normalize_cookie_file(input: Option<String>) -> Result<Option<String>, String> {
    let Some(input) = input else {
        return Ok(None);
    };
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(expand_home(trimmed));
    if !path.is_file() {
        return Err("Cookie 文件不存在，请重新选择有效的 cookies.txt。".to_string());
    }
    Ok(Some(path.to_string_lossy().to_string()))
}

pub fn validate_cookie_file_for_platform(path: &str, platform: &str) -> Result<(), String> {
    let spec = match platform {
        "douyin" => (
            &["douyin.com", "iesdouyin.com"][..],
            &["sessionid", "sessionid_ss"][..],
        ),
        "bilibili" => (&["bilibili.com", "b23.tv"][..], &["SESSDATA"][..]),
        "youtube" => (&["youtube.com", "google.com"][..], &[][..]),
        _ => return Err("不支持的平台。".to_string()),
    };
    let names = collect_cookie_names(Path::new(path), spec.0)?;
    // YouTube 的登录态可以由 LOGIN_INFO 或 SAPISID 家族（SAPISIDHASH 鉴权）单独成立，
    // 浏览器导出/部分解密缺失其中一族属常见情况，不应误判为登录态失效
    let valid = if platform == "youtube" {
        ["LOGIN_INFO", "SAPISID", "__Secure-1PAPISID", "__Secure-3PAPISID"]
            .iter()
            .any(|name| names.contains(*name))
    } else {
        spec.1.iter().any(|name| names.contains(*name))
    };
    if valid {
        Ok(())
    } else {
        Err("当前登录态缺少平台关键 Cookie，请重新导入。".to_string())
    }
}

fn collect_cookie_names(path: &Path, domains: &[&str]) -> Result<BTreeSet<String>, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("读取 Cookie 文件失败：{error}"))?;
    let mut names = BTreeSet::new();
    for raw in content.lines() {
        let line = raw.strip_prefix("#HttpOnly_").unwrap_or(raw);
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 7 {
            continue;
        }
        let domain = columns[0].trim_start_matches('.');
        if domains
            .iter()
            .any(|allowed| domain == *allowed || domain.ends_with(&format!(".{allowed}")))
            && !columns[6..].join("\t").trim().is_empty()
        {
            names.insert(columns[5].to_string());
        }
    }
    Ok(names)
}

pub fn normalize_save_directory(input: String) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("保存目录不能为空。".to_string());
    }
    Ok(expand_home(trimmed))
}

pub fn normalize_download_mode(input: String) -> Result<String, String> {
    if input.trim() == "manual" {
        Ok("manual".to_string())
    } else {
        Err("不支持的下载模式。".to_string())
    }
}

pub fn normalize_quality_preference(input: String) -> Result<String, String> {
    let value = input.trim().to_ascii_lowercase();
    if matches!(
        value.as_str(),
        "recommended" | "highest" | "smallest" | "no_watermark"
    ) {
        Ok(value)
    } else {
        Err("不支持的清晰度偏好。".to_string())
    }
}

pub fn normalize_max_concurrent(input: u32) -> u32 {
    input.clamp(1, 8)
}

pub fn normalize_proxy_url(input: Option<String>) -> Result<Option<String>, String> {
    let Some(input) = input else {
        return Ok(None);
    };
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    normalize_proxy_address(trimmed).map(Some)
}

/// 容忍常见笔误（`http:/127.0.0.1:1082` 少一个斜杠、省略 scheme 的 `127.0.0.1:1082`），
/// 并按 scheme/host 校验，避免把无法解析的地址透传给 yt-dlp / reqwest
fn normalize_proxy_address(value: &str) -> Result<String, String> {
    let candidate = if value.contains("://") {
        value.to_string()
    } else if value.contains(":/") {
        value.replacen(":/", "://", 1)
    } else {
        format!("http://{value}")
    };
    let parsed = reqwest::Url::parse(&candidate).map_err(|_| {
        "代理地址格式不正确，示例：http://127.0.0.1:7890 或 socks5://127.0.0.1:1080".to_string()
    })?;
    if !matches!(
        parsed.scheme(),
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
    ) {
        return Err("代理协议仅支持 http / https / socks5。".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("代理地址缺少主机名。".to_string());
    }
    Ok(candidate)
}

pub fn effective_proxy_url(configured: Option<&str>) -> Option<String> {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| normalize_proxy_address(value).ok())
        .or_else(system_proxy_url)
}

fn system_proxy_url() -> Option<String> {
    #[cfg(target_os = "windows")]
    if let Some(proxy) = windows_system_proxy_url() {
        return Some(proxy);
    }
    #[cfg(target_os = "macos")]
    if let Some(proxy) = macos_system_proxy_url() {
        return Some(proxy);
    }

    env::var("HTTPS_PROXY")
        .ok()
        .or_else(|| env::var("HTTP_PROXY").ok())
        .and_then(|value| normalize_proxy_server(&value))
}

/// macOS 系统代理（Shadowrocket/Clash 等「设置为系统代理」后生效），通过 scutil 读取
#[cfg(target_os = "macos")]
fn macos_system_proxy_url() -> Option<String> {
    let output = std::process::Command::new("scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let entry = |key: &str| {
        text.lines()
            .filter_map(|line| line.trim().split_once(" : "))
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.trim().to_string())
    };
    for (enabled, host_key, port_key) in [
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        ("HTTPEnable", "HTTPProxy", "HTTPPort"),
        ("SOCKSEnable", "SOCKSProxy", "SOCKSPort"),
    ] {
        if entry(enabled).as_deref() != Some("1") {
            continue;
        }
        let host = entry(host_key)?;
        let scheme = if enabled == "SOCKSEnable" { "socks5" } else { "http" };
        let port = entry(port_key).and_then(|value| value.parse::<u16>().ok());
        return Some(match port {
            Some(port) => format!("{scheme}://{host}:{port}"),
            None => format!("{scheme}://{host}"),
        });
    }
    None
}

fn normalize_proxy_server(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let selected = if trimmed.contains(';') {
        trimmed
            .split(';')
            .find_map(|entry| entry.trim().strip_prefix("https="))
            .or_else(|| {
                trimmed
                    .split(';')
                    .find_map(|entry| entry.trim().strip_prefix("http="))
            })?
    } else {
        trimmed.split_once('=').map_or(trimmed, |(_, proxy)| proxy)
    }
    .trim();
    if selected.is_empty() {
        None
    } else if selected.contains("://") {
        Some(selected.to_string())
    } else {
        Some(format!("http://{selected}"))
    }
}

#[cfg(target_os = "windows")]
fn windows_system_proxy_url() -> Option<String> {
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };

    let key: Vec<u16> =
        OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
            .encode_wide()
            .chain(Some(0))
            .collect();
    let enabled_name: Vec<u16> = OsStr::new("ProxyEnable")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut enabled = 0u32;
    let mut enabled_size = std::mem::size_of::<u32>() as u32;
    let enabled_result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            enabled_name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut enabled as *mut u32).cast::<c_void>(),
            &mut enabled_size,
        )
    };
    if enabled_result != 0 || enabled == 0 {
        return None;
    }

    let server_name: Vec<u16> = OsStr::new("ProxyServer")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let server_result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            server_name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut size,
        )
    };
    if server_result != 0 || size < 2 {
        return None;
    }
    let value = String::from_utf16_lossy(&buffer[..(size as usize / 2).saturating_sub(1)]);
    normalize_proxy_server(&value)
}

pub fn normalize_speed_limit(input: Option<String>) -> Option<String> {
    input
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn normalize_theme(input: String) -> String {
    if input.trim().eq_ignore_ascii_case("light") {
        "light".to_string()
    } else {
        "dark".to_string()
    }
}

pub fn normalize_language(input: String) -> String {
    if input.trim().eq_ignore_ascii_case("en") {
        "en".to_string()
    } else {
        "zh-CN".to_string()
    }
}

pub fn has_platform_auth_source(entry: &PlatformAuthSettings) -> bool {
    entry.status == "active"
        && entry
            .cookie_file
            .as_deref()
            .is_some_and(|path| Path::new(path).is_file())
}

pub fn has_auth_source(platform_auth: &BTreeMap<String, PlatformAuthSettings>) -> bool {
    AUTH_PLATFORM_IDS
        .iter()
        .any(|platform| has_platform_auth_source(&platform_auth_for(platform_auth, platform)))
}

pub fn auth_summary_label(platform_auth: &BTreeMap<String, PlatformAuthSettings>) -> String {
    let active = AUTH_PLATFORM_IDS
        .iter()
        .filter(|platform| has_platform_auth_source(&platform_auth_for(platform_auth, platform)))
        .count();
    match active {
        0 => "未登录".to_string(),
        1 => "1 个平台已授权".to_string(),
        count => format!("{count} 个平台已授权"),
    }
}

fn default_platform_auth() -> BTreeMap<String, PlatformAuthSettings> {
    AUTH_PLATFORM_IDS
        .iter()
        .map(|platform| (platform.to_string(), PlatformAuthSettings::default()))
        .collect()
}

pub(crate) fn home_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        env::var("USERPROFILE")
            .or_else(|_| env::var("HOME"))
            .unwrap_or_else(|_| ".".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        env::var("HOME").unwrap_or_else(|_| ".".to_string())
    }
}

pub(crate) fn app_data_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home_dir()))
            .join("StreamVerse")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from(home_dir())
            .join("Library")
            .join("Application Support")
            .join("StreamVerse")
    }
}

fn settings_path() -> PathBuf {
    app_data_root().join("settings-v2.json")
}

fn default_save_directory() -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{}\\Videos\\StreamVerse", home_dir())
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("{}/Movies/StreamVerse", home_dir())
    }
}

fn expand_home(input: &str) -> String {
    if input == "~" {
        return home_dir();
    }
    input
        .strip_prefix("~/")
        .map(|rest| format!("{}/{rest}", home_dir()))
        .unwrap_or_else(|| input.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_download_mode, normalize_max_concurrent, normalize_proxy_server,
        normalize_proxy_url, normalize_quality_preference,
    };

    #[test]
    fn settings_are_intentionally_breaking() {
        assert!(normalize_download_mode("auto".to_string()).is_err());
        assert_eq!(normalize_max_concurrent(99), 8);
        assert!(normalize_quality_preference("highest".to_string()).is_ok());
    }

    #[test]
    fn normalizes_windows_system_proxy_values() {
        assert_eq!(
            normalize_proxy_server("127.0.0.1:7897").as_deref(),
            Some("http://127.0.0.1:7897")
        );
        assert_eq!(
            normalize_proxy_server("http=127.0.0.1:80;https=127.0.0.1:443").as_deref(),
            Some("http://127.0.0.1:443")
        );
    }

    #[test]
    fn normalizes_and_validates_configured_proxy_values() {
        // 常见笔误：少一个斜杠
        assert_eq!(
            normalize_proxy_url(Some("http:/127.0.0.1:1082".to_string())).unwrap().as_deref(),
            Some("http://127.0.0.1:1082")
        );
        // 省略 scheme
        assert_eq!(
            normalize_proxy_url(Some("127.0.0.1:7897".to_string())).unwrap().as_deref(),
            Some("http://127.0.0.1:7897")
        );
        // 空值与空白视为不设置
        assert_eq!(normalize_proxy_url(Some("   ".to_string())).unwrap(), None);
        assert_eq!(normalize_proxy_url(None).unwrap(), None);
        // 无法解析的地址直接拒绝，不再透传给下载器
        assert!(normalize_proxy_url(Some("http://".to_string())).is_err());
        assert!(normalize_proxy_url(Some("ftp://127.0.0.1:21".to_string())).is_err());
    }
}
