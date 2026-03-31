use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

pub const MODULE_IDS: [&str; 5] = [
    "douyin-single",
    "douyin-profile",
    "bilibili-single",
    "bilibili-profile",
    "youtube-single",
];

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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub cookie_browser: Option<String>,
    pub save_directory: String,
    pub download_mode: String,
    pub quality_preference: String,
    pub auto_reveal_in_finder: bool,
    pub modules: BTreeMap<String, ModuleSetting>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            cookie_browser: None,
            save_directory: default_save_directory(),
            download_mode: "manual".to_string(),
            quality_preference: "recommended".to_string(),
            auto_reveal_in_finder: false,
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
    settings.cookie_browser =
        normalize_cookie_browser(settings.cookie_browser.clone()).unwrap_or(None);
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

    let content = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("序列化设置失败：{error}"))?;

    fs::write(path, content).map_err(|error| format!("写入设置失败：{error}"))
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

pub fn normalize_module_id(input: &str) -> Result<&'static str, String> {
    let normalized = input.trim().to_lowercase();
    MODULE_IDS
        .iter()
        .copied()
        .find(|candidate| *candidate == normalized)
        .ok_or_else(|| format!("未知模块：{input}"))
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

pub fn cookie_browser_label(cookie_browser: Option<&str>) -> String {
    match cookie_browser {
        Some(value) => format!("浏览器 Cookie · {}", human_browser_name(value)),
        None => "未登录".to_string(),
    }
}

fn settings_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".streamverse")
        .join("settings.json")
}

fn default_save_directory() -> String {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("{home}/Movies/StreamVerse")
}

fn expand_home(input: &str) -> String {
    if input == "~" {
        return env::var("HOME").unwrap_or_else(|_| ".".to_string());
    }

    if let Some(rest) = input.strip_prefix("~/") {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
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

#[cfg(test)]
mod tests {
    use super::{
        normalize_cookie_browser, normalize_download_mode, normalize_quality_preference,
        normalize_save_directory,
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
}
