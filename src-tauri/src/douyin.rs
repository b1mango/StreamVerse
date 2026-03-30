use crate::{formats, ProfileBatch, VideoAsset, VideoFormat, DEFAULT_GRADIENT};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeAsset {
    aweme_id: String,
    source_url: String,
    title: String,
    author: String,
    duration_seconds: u32,
    publish_date: String,
    caption: String,
    cover_url: Option<String>,
    cover_gradient: String,
    formats: Vec<BridgeFormat>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeProfileBatch {
    profile_title: String,
    source_url: String,
    total_available: u32,
    fetched_count: u32,
    skipped_count: u32,
    items: Vec<BridgeAsset>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeFormat {
    id: String,
    label: String,
    resolution: String,
    bitrate_kbps: u32,
    codec: String,
    container: String,
    no_watermark: bool,
    requires_login: bool,
    recommended: bool,
    direct_url: Option<String>,
    referer: Option<String>,
    user_agent: Option<String>,
}

pub fn is_douyin_url(url: &str) -> bool {
    ["douyin.com", "iesdouyin.com", "v.douyin.com"]
        .iter()
        .any(|domain| url.contains(domain))
}

pub fn analyze_url(source_url: &str, cookie_browser: &str) -> Result<VideoAsset, String> {
    let python_bin = ensure_helper_runtime()?;
    let cookie_file = export_browser_cookies(cookie_browser)?;
    let output = run_bridge_command(&python_bin, source_url, Some(&cookie_file));
    let _ = fs::remove_file(&cookie_file);
    let output = output?;

    if !output.status.success() {
        return Err(read_bridge_error(&output.stderr, "抖音复制链接解析失败"));
    }

    let bridge: BridgeAsset = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("解析抖音桥接结果失败：{error}"))?;

    Ok(VideoAsset {
        aweme_id: bridge.aweme_id,
        source_url: bridge.source_url,
        title: bridge.title,
        author: bridge.author,
        duration_seconds: bridge.duration_seconds,
        publish_date: bridge.publish_date,
        caption: bridge.caption,
        cover_url: normalize_optional_string(bridge.cover_url),
        cover_gradient: if bridge.cover_gradient.trim().is_empty() {
            DEFAULT_GRADIENT.to_string()
        } else {
            bridge.cover_gradient
        },
        formats: formats::dedupe_formats(bridge.formats.into_iter().map(map_format).collect()),
    })
}

pub fn analyze_profile(
    source_url: &str,
    cookie_browser: Option<&str>,
    limit: u32,
) -> Result<ProfileBatch, String> {
    let python_bin = ensure_helper_runtime()?;
    let cookie_file = match cookie_browser {
        Some(browser) => Some(export_browser_cookies(browser)?),
        None => None,
    };
    let output = run_bridge_profile_command(&python_bin, source_url, cookie_file.as_deref(), limit);
    if let Some(cookie_file) = cookie_file {
        let _ = fs::remove_file(cookie_file);
    }
    let output = output?;

    if !output.status.success() {
        return Err(read_bridge_error(&output.stderr, "抖音主页解析失败"));
    }

    let bridge: BridgeProfileBatch = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("解析主页批量结果失败：{error}"))?;

    Ok(ProfileBatch {
        profile_title: bridge.profile_title,
        source_url: bridge.source_url,
        total_available: bridge.total_available,
        fetched_count: bridge.fetched_count,
        skipped_count: bridge.skipped_count,
        items: bridge.items.into_iter().map(map_asset).collect(),
    })
}

fn map_format(format: BridgeFormat) -> VideoFormat {
    VideoFormat {
        id: format.id,
        label: format.label,
        resolution: format.resolution,
        bitrate_kbps: format.bitrate_kbps,
        codec: format.codec,
        container: format.container,
        no_watermark: format.no_watermark,
        requires_login: format.requires_login,
        recommended: format.recommended,
        direct_url: format.direct_url,
        referer: format.referer,
        user_agent: format.user_agent,
    }
}

fn map_asset(asset: BridgeAsset) -> VideoAsset {
    VideoAsset {
        aweme_id: asset.aweme_id,
        source_url: asset.source_url,
        title: asset.title,
        author: asset.author,
        duration_seconds: asset.duration_seconds,
        publish_date: asset.publish_date,
        caption: asset.caption,
        cover_url: normalize_optional_string(asset.cover_url),
        cover_gradient: if asset.cover_gradient.trim().is_empty() {
            DEFAULT_GRADIENT.to_string()
        } else {
            asset.cover_gradient
        },
        formats: formats::dedupe_formats(asset.formats.into_iter().map(map_format).collect()),
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn run_bridge_command(
    python_bin: &Path,
    source_url: &str,
    cookie_file: Option<&Path>,
) -> Result<std::process::Output, String> {
    let helper_script = helper_script_path();
    if !helper_script.exists() {
        return Err(format!(
            "未找到抖音桥接脚本：{}",
            helper_script.to_string_lossy()
        ));
    }

    let mut command = Command::new(python_bin);
    command
        .arg(&helper_script)
        .arg("analyze")
        .arg("--url")
        .arg(source_url);

    if let Some(cookie_file) = cookie_file {
        command.arg("--cookie-file").arg(cookie_file);
    }

    command
        .output()
        .map_err(|error| format!("启动抖音桥接脚本失败：{error}"))
}

fn run_bridge_profile_command(
    python_bin: &Path,
    source_url: &str,
    cookie_file: Option<&Path>,
    limit: u32,
) -> Result<std::process::Output, String> {
    let helper_script = helper_script_path();
    if !helper_script.exists() {
        return Err(format!(
            "未找到抖音桥接脚本：{}",
            helper_script.to_string_lossy()
        ));
    }

    let mut command = Command::new(python_bin);
    command
        .arg(&helper_script)
        .arg("profile")
        .arg("--url")
        .arg(source_url)
        .arg("--limit")
        .arg(limit.to_string());

    if let Some(cookie_file) = cookie_file {
        command.arg("--cookie-file").arg(cookie_file);
    }

    command
        .output()
        .map_err(|error| format!("启动抖音主页桥接脚本失败：{error}"))
}

fn export_browser_cookies(browser: &str) -> Result<PathBuf, String> {
    let cookie_file = env::temp_dir().join(format!(
        "streamverse-{}-{}.cookies.txt",
        browser,
        unique_suffix()
    ));

    let output = Command::new("yt-dlp")
        .arg("--cookies-from-browser")
        .arg(browser)
        .arg("--cookies")
        .arg(&cookie_file)
        .arg("--skip-download")
        .arg("https://www.douyin.com/")
        .output()
        .map_err(|error| format!("读取 {browser} 浏览器 Cookie 失败：{error}"))?;

    let has_cookie_dump = cookie_file
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);

    if has_cookie_dump {
        Ok(cookie_file)
    } else {
        let message = read_bridge_error(
            &output.stderr,
            "无法导出浏览器 Cookie，请确认浏览器已登录抖音并且当前会话有效。",
        );
        Err(message)
    }
}

fn ensure_helper_runtime() -> Result<PathBuf, String> {
    let workspace_root = workspace_root();
    let venv_dir = workspace_root.join(".venv");
    let venv_python = venv_dir.join("bin").join("python");

    if !venv_python.exists() {
        let output = Command::new("python3")
            .arg("-m")
            .arg("venv")
            .arg(&venv_dir)
            .output()
            .map_err(|error| format!("创建抖音解析环境失败：{error}"))?;

        if !output.status.success() {
            return Err(read_bridge_error(
                &output.stderr,
                "创建抖音解析环境失败，请确认系统已安装 python3。",
            ));
        }
    }

    let python_bin = if venv_python.exists() {
        venv_python
    } else {
        PathBuf::from("python3")
    };

    let check_output = Command::new(&python_bin)
        .arg("-c")
        .arg("import browser_cookie3, gmssl, httpx, yaml, pydantic, qrcode, rich")
        .output()
        .map_err(|error| format!("检测抖音解析依赖失败：{error}"))?;

    if check_output.status.success() {
        return Ok(python_bin);
    }

    let requirements = workspace_root.join("requirements-douyin-helper.txt");
    let install_output = Command::new(&python_bin)
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg("-r")
        .arg(&requirements)
        .output()
        .map_err(|error| format!("安装抖音解析依赖失败：{error}"))?;

    if install_output.status.success() {
        Ok(python_bin)
    } else {
        Err(read_bridge_error(
            &install_output.stderr,
            "安装抖音解析依赖失败，请检查网络或 Python 环境。",
        ))
    }
}

fn helper_script_path() -> PathBuf {
    workspace_root().join("scripts").join("douyin_bridge.py")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn read_bridge_error(stderr: &[u8], fallback: &str) -> String {
    let message = String::from_utf8_lossy(stderr);
    let trimmed = message.trim();
    if trimmed.contains("获取数据失败") || trimmed.contains("无效响应类型") {
        return "当前抖音主页请求被风控或返回空响应。请先在设置中选择 Chrome 等浏览器 Cookie 后，再重试主页批量下载。".to_string();
    }
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}
