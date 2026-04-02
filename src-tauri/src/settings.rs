use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const MODULE_IDS: [&str; 5] = [
    "douyin-single",
    "douyin-profile",
    "bilibili-single",
    "bilibili-profile",
    "youtube-single",
];

pub const AUTH_PLATFORM_IDS: [&str; 3] = ["douyin", "bilibili", "youtube"];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModuleSetting {
    pub installed: bool,
    pub enabled: bool,
}

impl Default for ModuleSetting {
    fn default() -> Self {
        Self {
            installed: false,
            enabled: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlatformAuthSettings {
    pub cookie_browser: Option<String>,
    pub cookie_file: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_browser: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_file: Option<String>,
    #[serde(default = "default_platform_auth")]
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
    pub modules: BTreeMap<String, ModuleSetting>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            cookie_browser: None,
            cookie_file: None,
            platform_auth: default_platform_auth(),
            save_directory: default_save_directory(),
            download_mode: "manual".to_string(),
            quality_preference: "recommended".to_string(),
            auto_reveal_in_finder: false,
            max_concurrent_downloads: 3,
            proxy_url: None,
            speed_limit: None,
            auto_update: false,
            theme: "dark".to_string(),
            notify_on_complete: true,
            language: "zh-CN".to_string(),
            modules: default_modules(),
        }
    }
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    let content = fs::read_to_string(path);

    let mut settings = match content {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    };

    let legacy_browser = normalize_cookie_browser(settings.cookie_browser.clone()).unwrap_or(None);
    let legacy_cookie_file = normalize_cookie_file(settings.cookie_file.clone()).unwrap_or(None);
    settings.platform_auth = normalize_platform_auths(
        std::mem::take(&mut settings.platform_auth),
        legacy_browser.as_deref(),
        legacy_cookie_file.as_deref(),
    )
    .unwrap_or_else(|_| default_platform_auth());
    settings.cookie_browser = None;
    settings.cookie_file = None;
    settings.download_mode = normalize_download_mode(settings.download_mode.clone())
        .unwrap_or_else(|_| "manual".to_string());
    settings.quality_preference = normalize_quality_preference(settings.quality_preference.clone())
        .unwrap_or_else(|_| "recommended".to_string());
    normalize_modules(&mut settings.modules);
    settings
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建设置目录失败：{error}"))?;
    }

    let content =
        serde_json::to_string_pretty(settings).map_err(|error| format!("序列化设置失败：{error}"))?;

    fs::write(path, content).map_err(|error| format!("写入设置失败：{error}"))
}

pub fn normalize_auth_platform_id(input: &str) -> Result<&'static str, String> {
    let normalized = input.trim().to_lowercase();
    AUTH_PLATFORM_IDS
        .iter()
        .copied()
        .find(|candidate| *candidate == normalized)
        .ok_or_else(|| format!("未知认证平台：{input}"))
}

pub fn normalize_platform_auths(
    input: BTreeMap<String, PlatformAuthSettings>,
    legacy_browser: Option<&str>,
    legacy_cookie_file: Option<&str>,
) -> Result<BTreeMap<String, PlatformAuthSettings>, String> {
    let mut keyed_entries = BTreeMap::new();
    for (platform, entry) in input {
        let normalized_platform = normalize_auth_platform_id(&platform)?;
        keyed_entries.insert(normalized_platform.to_string(), entry);
    }

    let has_explicit_platform_auth = keyed_entries
        .values()
        .any(|entry| entry.cookie_browser.is_some() || entry.cookie_file.is_some());

    let mut normalized = BTreeMap::new();
    for platform in AUTH_PLATFORM_IDS {
        let mut entry = keyed_entries.remove(platform).unwrap_or_default();
        if !has_explicit_platform_auth {
            entry.cookie_browser = entry
                .cookie_browser
                .or_else(|| legacy_browser.map(str::to_string));
            entry.cookie_file = entry
                .cookie_file
                .or_else(|| legacy_cookie_file.map(str::to_string));
        }

        normalized.insert(platform.to_string(), normalize_platform_auth_entry(entry)?);
    }

    Ok(normalized)
}

pub fn normalize_platform_auth_entry(entry: PlatformAuthSettings) -> Result<PlatformAuthSettings, String> {
    let cookie_browser = normalize_cookie_browser(entry.cookie_browser)?;
    let cookie_file = normalize_cookie_file(entry.cookie_file)?;

    #[cfg(target_os = "windows")]
    let cookie_browser = if cookie_browser.as_deref() == Some("chrome") && !chrome_cookie_db_exists() {
        None
    } else {
        cookie_browser
    };

    Ok(PlatformAuthSettings {
        cookie_browser,
        cookie_file,
    })
}

pub fn platform_auth_for(
    platform_auth: &BTreeMap<String, PlatformAuthSettings>,
    platform: &str,
) -> PlatformAuthSettings {
    platform_auth.get(platform).cloned().unwrap_or_default()
}

pub fn normalize_cookie_browser(input: Option<String>) -> Result<Option<String>, String> {
    let normalized = input
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase);

    match normalized.as_deref() {
        None => Ok(None),
        Some("chrome") => Ok(normalized),
        Some(other) => Err(format!("不支持的浏览器来源：{other}")),
    }
}

pub fn normalize_cookie_file(input: Option<String>) -> Result<Option<String>, String> {
    let normalized = input
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(expand_home);

    let Some(path) = normalized else {
        return Ok(None);
    };

    let resolved = PathBuf::from(&path);
    if !resolved.is_file() {
        return Err("Cookie 文件不存在，请重新选择一个有效的 cookies.txt 文件。".to_string());
    }

    Ok(Some(path))
}

pub fn normalize_cookie_text(input: Option<String>) -> Option<String> {
    input
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn import_cookie_text(platform: &str, input: &str) -> Result<String, String> {
    let content = normalize_imported_cookie_content(input)?;
    let path = managed_cookie_file_path(platform);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建 Cookie 目录失败：{error}"))?;
    }

    fs::write(&path, content).map_err(|error| format!("写入 Cookie 文件失败：{error}"))?;
    Ok(path.to_string_lossy().to_string())
}

pub fn validate_cookie_file_for_platform(path: &str, platform: &str) -> Result<(), String> {
    let Some(spec) = cookie_precheck_spec(platform) else {
        return Ok(());
    };

    let present_names = collect_cookie_names(Path::new(path), spec.domains)?;
    let has_required = spec.required_any.iter().any(|name| present_names.contains(*name));
    if has_required {
        return Ok(());
    }

    let required_label = spec.required_any.join(" / ");
    let mut message = format!(
        "当前 cookies.txt 里缺少 {} 登录关键 Cookie：{}。",
        platform_label(platform),
        required_label
    );
    let missing_recommended: Vec<&str> = spec
        .recommended
        .iter()
        .copied()
        .filter(|name| !present_names.contains(*name))
        .collect();
    if !missing_recommended.is_empty() {
        message.push_str(&format!(
            " 建议重新导出，并尽量包含 {}。",
            missing_recommended.join("、")
        ));
    }
    Err(message)
}

pub fn normalize_save_directory(input: String) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("下载目录不能为空。".to_string());
    }

    Ok(expand_home(trimmed))
}

pub fn normalize_download_mode(input: String) -> Result<String, String> {
    let normalized = input.trim().to_lowercase();

    match normalized.as_str() {
        "manual" => Ok(normalized),
        _ => Err("下载模式必须是 manual。".to_string()),
    }
}

pub fn normalize_quality_preference(input: String) -> Result<String, String> {
    let normalized = input.trim().to_lowercase();

    match normalized.as_str() {
        "recommended" | "highest" | "smallest" | "no_watermark" => Ok(normalized),
        _ => Err("默认清晰度策略无效。".to_string()),
    }
}

pub fn normalize_max_concurrent(input: u32) -> u32 {
    input.clamp(1, 10)
}

pub fn normalize_proxy_url(input: Option<String>) -> Option<String> {
    input
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn normalize_speed_limit(input: Option<String>) -> Option<String> {
    input
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn normalize_theme(input: String) -> String {
    let normalized = input.trim().to_lowercase();
    match normalized.as_str() {
        "dark" | "light" => normalized,
        _ => "dark".to_string(),
    }
}

pub fn normalize_language(input: String) -> String {
    let normalized = input.trim().to_lowercase();
    match normalized.as_str() {
        "zh-cn" => "zh-CN".to_string(),
        "en" => "en".to_string(),
        _ => "zh-CN".to_string(),
    }
}

pub fn normalize_module_id(input: &str) -> Result<&'static str, String> {
    let normalized = input.trim().to_lowercase();
    MODULE_IDS
        .iter()
        .copied()
        .find(|candidate| *candidate == normalized)
        .ok_or_else(|| format!("未知模块：{input}"))
}

pub fn has_platform_auth_source(entry: &PlatformAuthSettings) -> bool {
    entry
        .cookie_browser
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        || entry
            .cookie_file
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
}

pub fn has_auth_source(platform_auth: &BTreeMap<String, PlatformAuthSettings>) -> bool {
    AUTH_PLATFORM_IDS
        .iter()
        .any(|platform| has_platform_auth_source(&platform_auth_for(platform_auth, platform)))
}

pub fn auth_source_label(entry: &PlatformAuthSettings) -> String {
    if let Some(file) = entry
        .cookie_file
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if is_managed_cookie_file(file) {
            return "已保存登录态".to_string();
        }

        let label = Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file);
        return format!("Cookie 文件 · {label}");
    }

    match entry.cookie_browser.as_deref() {
        Some(value) => format!("浏览器 Cookie · {}", human_browser_name(value)),
        None => "未登录".to_string(),
    }
}

pub fn auth_summary_label(platform_auth: &BTreeMap<String, PlatformAuthSettings>) -> String {
    let active_platforms: Vec<&str> = AUTH_PLATFORM_IDS
        .iter()
        .copied()
        .filter(|platform| has_platform_auth_source(&platform_auth_for(platform_auth, platform)))
        .collect();

    match active_platforms.as_slice() {
        [] => "未登录".to_string(),
        [platform] => format!(
            "{} · {}",
            platform_label(platform),
            auth_source_label(&platform_auth_for(platform_auth, platform))
        ),
        many => format!("已配置 {} 个平台登录态", many.len()),
    }
}

fn default_platform_auth() -> BTreeMap<String, PlatformAuthSettings> {
    AUTH_PLATFORM_IDS
        .iter()
        .map(|platform| (platform.to_string(), PlatformAuthSettings::default()))
        .collect()
}

fn default_modules() -> BTreeMap<String, ModuleSetting> {
    MODULE_IDS
        .iter()
        .map(|id| (id.to_string(), ModuleSetting::default()))
        .collect()
}

fn normalize_modules(modules: &mut BTreeMap<String, ModuleSetting>) {
    modules.retain(|id, _| MODULE_IDS.contains(&id.as_str()));
    for id in MODULE_IDS {
        modules.entry(id.to_string()).or_default();
    }
}

fn collect_cookie_names(path: &Path, domains: &[&str]) -> Result<BTreeSet<String>, String> {
    let content = fs::read_to_string(path).map_err(|error| format!("读取 Cookie 文件失败：{error}"))?;
    let mut names = BTreeSet::new();

    for raw_line in content.lines() {
        if raw_line.is_empty() || raw_line.starts_with("# ") {
            continue;
        }
        let line = raw_line.strip_prefix("#HttpOnly_").unwrap_or(raw_line);
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 7 {
            continue;
        }
        let domain = parts[0].trim();
        let name = parts[5].trim();
        let value = parts[6..].join("\t");
        if !value.trim().is_empty() && domains.iter().any(|candidate| domain.ends_with(candidate)) {
            names.insert(name.to_string());
        }
    }

    Ok(names)
}

struct CookiePrecheckSpec {
    domains: &'static [&'static str],
    required_any: &'static [&'static str],
    recommended: &'static [&'static str],
}

fn cookie_precheck_spec(platform: &str) -> Option<CookiePrecheckSpec> {
    match platform {
        "douyin" => Some(CookiePrecheckSpec {
            domains: &["douyin.com", "iesdouyin.com"],
            required_any: &["sessionid", "sessionid_ss"],
            recommended: &["sid_tt", "uid_tt"],
        }),
        "bilibili" => Some(CookiePrecheckSpec {
            domains: &["bilibili.com", "b23.tv"],
            required_any: &["SESSDATA"],
            recommended: &["DedeUserID", "bili_jct"],
        }),
        _ => None,
    }
}

fn home_dir() -> String {
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

fn settings_path() -> PathBuf {
    PathBuf::from(home_dir())
        .join(".streamverse")
        .join("settings.json")
}

fn auth_root() -> PathBuf {
    PathBuf::from(home_dir()).join(".streamverse").join("auth")
}

fn managed_cookie_file_path(platform: &str) -> PathBuf {
    auth_root().join(format!("saved-{platform}-cookies.txt"))
}

fn default_save_directory() -> String {
    let home = home_dir();
    #[cfg(target_os = "windows")]
    {
        format!("{home}\\Videos\\StreamVerse")
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("{home}/Movies/StreamVerse")
    }
}

fn expand_home(input: &str) -> String {
    if input == "~" {
        return home_dir();
    }

    if let Some(rest) = input.strip_prefix("~/") {
        let home = home_dir();
        return format!("{home}/{rest}");
    }

    input.to_string()
}

fn human_browser_name(value: &str) -> &'static str {
    match value {
        "chrome" => "Chrome",
        _ => "Custom",
    }
}

fn platform_label(platform: &str) -> &'static str {
    match platform {
        "douyin" => "抖音",
        "bilibili" => "Bilibili",
        "youtube" => "YouTube",
        _ => "当前平台",
    }
}

fn is_managed_cookie_file(input: &str) -> bool {
    let candidate = PathBuf::from(expand_home(input));
    if let Some(file_name) = candidate.file_name().and_then(|name| name.to_str()) {
        return candidate.parent() == Some(auth_root().as_path())
            && (file_name == "saved-cookies.txt"
                || (file_name.starts_with("saved-") && file_name.ends_with("-cookies.txt")));
    }
    false
}

#[cfg(target_os = "windows")]
fn chrome_cookie_db_exists() -> bool {
    let Ok(local) = env::var("LOCALAPPDATA") else {
        return false;
    };
    Path::new(&local)
        .join("Google\\Chrome\\User Data\\Default\\Network\\Cookies")
        .exists()
}

fn normalize_imported_cookie_content(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Cookie 内容不能为空。".to_string());
    }

    if looks_like_netscape_cookie_text(trimmed) {
        return normalize_netscape_cookie_text(trimmed);
    }

    normalize_cookie_header_text(trimmed)
}

fn looks_like_netscape_cookie_text(input: &str) -> bool {
    input.lines().any(|line| {
        let candidate = line.trim();
        candidate.starts_with("# Netscape HTTP Cookie File")
            || candidate.starts_with("#HttpOnly_")
            || candidate.split('\t').count() >= 7
    })
}

fn normalize_netscape_cookie_text(input: &str) -> Result<String, String> {
    let mut lines = vec!["# Netscape HTTP Cookie File".to_string()];
    let mut valid = 0usize;

    for raw in input.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("# Netscape HTTP Cookie File") {
            continue;
        }
        if trimmed.starts_with('#') && !trimmed.starts_with("#HttpOnly_") {
            lines.push(trimmed.to_string());
            continue;
        }

        let line = if let Some(rest) = trimmed.strip_prefix("#HttpOnly_") {
            format!("#HttpOnly_{rest}")
        } else {
            trimmed.to_string()
        };

        if line.split('\t').count() >= 7 {
            valid += 1;
            lines.push(line);
        }
    }

    if valid == 0 {
        return Err("未识别到有效的 cookies.txt 内容，请粘贴 Netscape 格式文件内容或浏览器里的 Cookie 值。".to_string());
    }

    Ok(lines.join("\n") + "\n")
}

fn normalize_cookie_header_text(input: &str) -> Result<String, String> {
    let raw = input
        .strip_prefix("Cookie:")
        .or_else(|| input.strip_prefix("cookie:"))
        .unwrap_or(input)
        .trim();

    let mut pairs = Vec::<(String, String)>::new();
    for chunk in raw.split(';') {
        let candidate = chunk.trim();
        if candidate.is_empty() {
            continue;
        }
        let Some((name, value)) = candidate.split_once('=') else {
            continue;
        };
        let key = name.trim();
        let val = value.trim();
        if !key.is_empty() && !val.is_empty() {
            pairs.push((key.to_string(), val.to_string()));
        }
    }

    if pairs.is_empty() {
        return Err("未识别到可用的 Cookie 键值，请粘贴浏览器请求头里的完整 Cookie 值。".to_string());
    }

    let mut lines = vec!["# Netscape HTTP Cookie File".to_string()];
    let domains = [
        ".douyin.com",
        ".iesdouyin.com",
        ".bilibili.com",
        ".b23.tv",
        ".youtube.com",
        ".google.com",
    ];
    for domain in domains {
        for (name, value) in &pairs {
            lines.push(format!("{domain}\tTRUE\t/\tTRUE\t2147483647\t{name}\t{value}"));
        }
    }

    Ok(lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_auth_platform_id, normalize_cookie_browser, normalize_download_mode,
        normalize_quality_preference, normalize_save_directory,
    };

    #[test]
    fn rejects_empty_save_directory() {
        let result = normalize_save_directory("   ".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn expands_home_prefix_for_save_directory() {
        let path = normalize_save_directory("~/Movies/Test".to_string()).unwrap();
        assert!(path.contains("/Movies/Test"));
        assert!(!path.starts_with('~'));
    }

    #[test]
    fn normalizes_download_mode() {
        assert_eq!(
            normalize_download_mode(" manual ".to_string()).unwrap(),
            "manual"
        );
        assert!(normalize_download_mode("auto".to_string()).is_err());
        assert!(normalize_download_mode("smart".to_string()).is_err());
    }

    #[test]
    fn normalizes_quality_preference() {
        assert_eq!(
            normalize_quality_preference(" highest ".to_string()).unwrap(),
            "highest"
        );
        assert!(normalize_quality_preference("ultra".to_string()).is_err());
    }

    #[test]
    fn normalizes_supported_browser_source() {
        assert_eq!(
            normalize_cookie_browser(Some(" chrome ".to_string())).unwrap(),
            Some("chrome".to_string())
        );
        assert!(normalize_cookie_browser(Some("edge".to_string())).is_err());
    }

    #[test]
    fn normalizes_auth_platform_ids() {
        assert_eq!(normalize_auth_platform_id("douyin").unwrap(), "douyin");
        assert!(normalize_auth_platform_id("weibo").is_err());
    }
}
