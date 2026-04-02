#![allow(dead_code)]

#[path = "media_contract.rs"]
mod media_contract;

use media_contract::{VideoAsset, VideoFormat, DEFAULT_GRADIENT};
use serde::Deserialize;
use std::cmp::Reverse;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn silent_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd
}

pub fn configure_python_command(command: &mut Command) {
    command.env("PYTHONUTF8", "1");
    command.env("PYTHONIOENCODING", "utf-8");
}

const MIN_HELPER_PYTHON_MAJOR: u32 = 3;
const MIN_HELPER_PYTHON_MINOR: u32 = 10;
const YTDLP_PYPI_SPEC: &str = "yt-dlp[default]==2026.03.17";
const SINGLE_ANALYSIS_STAGES: u32 = 4;
const YOUTUBE_EXTRACTOR_ARGS: &str = "youtube:player_client=default,-ios,-android;player_skip=configs";

static PREFERRED_EXTERNAL_YTDLP_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static PREFERRED_JS_RUNTIME: OnceLock<Option<String>> = OnceLock::new();

#[derive(Deserialize)]
pub struct RawInfo {
    pub id: Option<String>,
    pub title: Option<String>,
    pub uploader: Option<String>,
    pub creator: Option<String>,
    pub channel: Option<String>,
    pub duration: Option<f64>,
    pub upload_date: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
    pub formats: Option<Vec<RawFormat>>,
}

#[derive(Default, Deserialize)]
pub struct RawFormat {
    pub format_id: Option<String>,
    pub format_note: Option<String>,
    pub ext: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub tbr: Option<f64>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub url: Option<String>,
    pub protocol: Option<String>,
    pub http_headers: Option<RawHeaders>,
}

#[derive(Clone, Default, Deserialize)]
pub struct RawHeaders {
    #[serde(rename = "Referer")]
    pub referer: Option<String>,
    #[serde(rename = "User-Agent")]
    pub user_agent: Option<String>,
}

pub fn analyze_generic_url(
    platform: &str,
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
) -> Result<VideoAsset, String> {
    write_progress(0, SINGLE_ANALYSIS_STAGES, "正在准备解析环境…");

    let mut command = prepare_ytdlp_command()?;
    command.args([
        "--dump-single-json",
        "--no-playlist",
        "--socket-timeout",
        "20",
    ]);
    append_platform_ytdlp_args(&mut command, platform);
    append_auth_args(&mut command, cookie_browser, cookie_file);

    if platform != "youtube" {
        command.arg("--proxy").arg("");
    }

    write_progress(1, SINGLE_ANALYSIS_STAGES, "正在读取视频信息…");

    let output = command
        .arg(source_url)
        .output()
        .map_err(|error| format!("启动 yt-dlp 失败：{error}"))?;

    if !output.status.success() {
        return Err(read_process_error(&output.stderr, "解析链接失败"));
    }

    let raw: RawInfo = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("解析 yt-dlp 响应失败：{error}"))?;

    write_progress(2, SINGLE_ANALYSIS_STAGES, "正在整理可用格式…");

    let mut formats = dedupe_formats(map_formats(platform, raw.formats.unwrap_or_default()));
    if formats.is_empty() && platform == "youtube" {
        formats.push(fallback_streaming_format("best", "自动选择"));
    }

    write_progress(4, SINGLE_ANALYSIS_STAGES, "作品解析完成。");

    Ok(VideoAsset {
        asset_id: raw.id.unwrap_or_else(|| "unknown".to_string()),
        platform: platform.to_string(),
        source_url: source_url.to_string(),
        title: raw.title.unwrap_or_else(|| "未命名作品".to_string()),
        author: raw
            .uploader
            .or(raw.creator)
            .or(raw.channel)
            .unwrap_or_else(|| "未知作者".to_string()),
        duration_seconds: raw.duration.unwrap_or_default().round() as u32,
        publish_date: format_publish_date(raw.upload_date.as_deref()),
        caption: raw.description.unwrap_or_default(),
        category_label: None,
        group_title: None,
        cover_url: raw.thumbnail.filter(|value| !value.trim().is_empty()).map(|url| {
            if url.starts_with("//") {
                format!("https:{url}")
            } else {
                url
            }
        }),
        cover_gradient: DEFAULT_GRADIENT.to_string(),
        formats,
    })
}

pub fn ensure_helper_runtime() -> Result<PathBuf, String> {
    let resource_root = browser_bridge_resource_root();
    let venv_dir = helper_runtime_root().join(".venv");
    let venv_python = venv_python_path(&venv_dir);
    let system_python = find_system_python()?;

    if !venv_python.exists() || !python_version_supported(&venv_python) {
        recreate_helper_runtime(&venv_dir, &system_python)?;
    }

    let python_bin = if venv_python.exists() {
        venv_python
    } else {
        system_python
    };

    if python_modules_available(
        &python_bin,
        &[
            "browser_cookie3",
            "gmssl",
            "httpx",
            "yaml",
            "pydantic",
            "qrcode",
            "rich",
            "playwright",
        ],
    )? {
        return Ok(python_bin);
    }

    let requirements = resource_root.join("requirements-douyin-helper.txt");
    let mut install_command = silent_command(&python_bin);
    configure_python_command(&mut install_command);
    let install_output = install_command
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg("-r")
        .arg(&requirements)
        .output()
        .map_err(|error| format!("安装 helper 依赖失败：{error}"))?;

    if install_output.status.success() {
        Ok(python_bin)
    } else {
        Err(read_process_error(
            &install_output.stderr,
            "安装 helper 依赖失败，请检查网络或 Python 环境。",
        ))
    }
}

fn ensure_ytdlp_runtime() -> Result<PathBuf, String> {
    let venv_dir = ytdlp_runtime_root().join(".venv");
    let venv_python = venv_python_path(&venv_dir);
    let system_python = find_system_python()?;

    if !venv_python.exists() || !python_version_supported(&venv_python) {
        recreate_helper_runtime(&venv_dir, &system_python)?;
    }

    let python_bin = if venv_python.exists() {
        venv_python
    } else {
        system_python
    };

    if python_modules_available(&python_bin, &["yt_dlp", "yt_dlp_ejs"])? {
        return Ok(python_bin);
    }

    let mut install_command = silent_command(&python_bin);
    configure_python_command(&mut install_command);
    let install_output = install_command
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg("-U")
        .arg(YTDLP_PYPI_SPEC)
        .output()
        .map_err(|error| format!("安装 yt-dlp 运行时失败：{error}"))?;

    if !install_output.status.success() {
        return Err(read_process_error(
            &install_output.stderr,
            "安装 yt-dlp 运行时失败，请检查网络或 Python 环境。",
        ));
    }

    if python_modules_available(&python_bin, &["yt_dlp", "yt_dlp_ejs"])? {
        Ok(python_bin)
    } else {
        Err("yt-dlp 运行时缺少必要组件，请稍后重试。".to_string())
    }
}

pub fn export_browser_cookies_for_url(browser: &str, url: &str) -> Result<PathBuf, String> {
    let cookie_file = env::temp_dir().join(format!(
        "streamverse-{}-{}.cookies.txt",
        browser,
        unique_suffix()
    ));

    let mut command = prepare_ytdlp_command()?;
    let browser_arg = normalize_cookie_browser_arg(browser);
    let output = command
        .arg("--cookies-from-browser")
        .arg(&browser_arg)
        .arg("--cookies")
        .arg(&cookie_file)
        .arg("--skip-download")
        .arg(url)
        .output()
        .map_err(|error| format!("读取 {browser} 浏览器 Cookie 失败：{error}"))?;

    let has_cookie_dump = cookie_file
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);

    if has_cookie_dump {
        Ok(cookie_file)
    } else {
        Err(read_process_error(
            &output.stderr,
            "无法导出浏览器 Cookie，请确认浏览器已登录对应平台。",
        ))
    }
}

pub fn cookie_file_contains_login_cookie(
    path: &Path,
    domains: &[&str],
    names: &[&str],
) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };

    content.lines().any(|raw_line| {
        if raw_line.is_empty() || raw_line.starts_with("# ") {
            return false;
        }

        let line = raw_line.strip_prefix("#HttpOnly_").unwrap_or(raw_line);
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 7 {
            return false;
        }

        let domain = parts[0].trim();
        let name = parts[5].trim();
        let value = parts[6..].join("\t");

        !value.trim().is_empty()
            && domains.iter().any(|item| domain.ends_with(item))
            && names.iter().any(|item| name == *item)
    })
}

fn append_platform_ytdlp_args(command: &mut Command, platform: &str) {
    if platform != "youtube" {
        return;
    }

    if let Some(runtime) = preferred_js_runtime() {
        command.arg("--js-runtimes").arg(runtime);
    }

    command
        .arg("--extractor-args")
        .arg(YOUTUBE_EXTRACTOR_ARGS)
        .arg("--concurrent-fragments")
        .arg("4");
}

fn fallback_streaming_format(id: &str, label: &str) -> VideoFormat {
    VideoFormat {
        id: id.to_string(),
        label: label.to_string(),
        resolution: "自动".to_string(),
        bitrate_kbps: 0,
        codec: "AUTO".to_string(),
        container: "AUTO".to_string(),
        no_watermark: false,
        requires_login: false,
        requires_processing: false,
        recommended: true,
        direct_url: None,
        referer: None,
        user_agent: None,
        audio_direct_url: None,
        audio_referer: None,
        audio_user_agent: None,
        file_size_bytes: None,
    }
}

fn recreate_helper_runtime(venv_dir: &Path, system_python: &Path) -> Result<(), String> {
    if venv_dir.exists() {
        fs::remove_dir_all(venv_dir).map_err(|error| format!("重建 helper 环境失败：{error}"))?;
    }

    let mut command = silent_command(system_python);
    configure_python_command(&mut command);
    let output = command
        .arg("-m")
        .arg("venv")
        .arg(venv_dir)
        .output()
        .map_err(|error| format!("创建 helper 环境失败：{error}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(read_process_error(
            &output.stderr,
            "创建 helper 环境失败，请确认系统已安装 python3。",
        ))
    }
}

fn python_version_supported(path: &Path) -> bool {
    detect_python_version(path)
        .map(|(major, minor, _)| {
            major > MIN_HELPER_PYTHON_MAJOR
                || (major == MIN_HELPER_PYTHON_MAJOR && minor >= MIN_HELPER_PYTHON_MINOR)
        })
        .unwrap_or(false)
}

fn detect_python_version(path: &Path) -> Option<(u32, u32, u32)> {
    let mut command = silent_command(path);
    configure_python_command(&mut command);
    let output = command
        .arg("-c")
        .arg("import sys; print(f'{sys.version_info[0]}.{sys.version_info[1]}.{sys.version_info[2]}')")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version = String::from_utf8(output.stdout).ok()?;
    let mut parts = version.trim().split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    Some((major, minor, patch))
}

fn resolve_ytdlp_path() -> Result<PathBuf, String> {
    if let Some(path) = preferred_external_ytdlp_path() {
        return Ok(path);
    }

    if let Some(path) = shared_pack_root("download-engine")
        .map(|root| root.join("bin").join(ytdlp_binary_name()))
        .filter(|path| path.is_file())
    {
        return Ok(path);
    }

    if let Some(path) = find_in_path(ytdlp_binary_name()) {
        return Ok(path);
    }

    Err("未检测到可用的 yt-dlp，请重新安装应用后再试。".to_string())
}

fn preferred_external_ytdlp_path() -> Option<PathBuf> {
    PREFERRED_EXTERNAL_YTDLP_PATH
        .get_or_init(|| {
            for candidate in [
                PathBuf::from("/opt/homebrew/bin/yt-dlp"),
                PathBuf::from("/usr/local/bin/yt-dlp"),
            ] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }

            find_in_path(ytdlp_binary_name())
        })
        .clone()
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

pub fn resource_root() -> PathBuf {
    installed_pack_root()
        .or_else(bundled_pack_resource_root)
        .unwrap_or_else(workspace_root)
}

pub fn shared_pack_root(pack_id: &str) -> Option<PathBuf> {
    if let Some(pack_root) = installed_pack_root() {
        let packs_root = pack_root.parent()?;
        let shared_root = packs_root.join(pack_id);
        if shared_root.exists() {
            return Some(shared_root);
        }
    }

    bundled_resources_root()
        .map(|root| root.join("pack-resources").join(pack_id))
        .filter(|path| path.exists())
}

pub fn browser_bridge_resource_root() -> PathBuf {
    shared_pack_root("browser-bridge").unwrap_or_else(resource_root)
}

fn helper_runtime_root() -> PathBuf {
    if let Ok(path) = env::var("STREAMVERSE_HELPER_ROOT") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    if let Some(pack_root) = installed_pack_root() {
        if let Some(app_root) = pack_root.parent().and_then(|path| path.parent()) {
            return app_root.join("runtime");
        }
    }

    app_data_root().join("runtime")
}

fn ytdlp_runtime_root() -> PathBuf {
    if let Ok(path) = env::var("STREAMVERSE_YTDLP_ROOT") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    if let Some(pack_root) = installed_pack_root() {
        if let Some(app_root) = pack_root.parent().and_then(|path| path.parent()) {
            return app_root.join("runtime").join("yt-dlp");
        }
    }

    app_data_root().join("runtime").join("yt-dlp")
}

fn installed_pack_root() -> Option<PathBuf> {
    let current_exe = env::current_exe().ok()?;
    let bin_dir = current_exe.parent()?;
    if bin_dir.file_name()?.to_str()? != "bin" {
        return None;
    }
    Some(bin_dir.parent()?.to_path_buf())
}

fn bundled_pack_resource_root() -> Option<PathBuf> {
    let pack_id = current_pack_id()?;
    bundled_resources_root()
        .map(|root| root.join("pack-resources").join(pack_id))
        .filter(|path| path.exists())
}

fn bundled_resources_root() -> Option<PathBuf> {
    let current_exe = env::current_exe().ok()?;
    let parent = current_exe.parent()?;

    if parent.file_name()?.to_str()? == "pack-binaries" {
        return parent.parent().map(Path::to_path_buf);
    }

    let resources = parent.join("resources");
    if resources.is_dir() {
        return Some(resources);
    }

    if let Some(contents_dir) = parent.parent() {
        let resources = contents_dir.join("Resources");
        if resources.is_dir() {
            return Some(resources);
        }
    }

    None
}

fn current_pack_id() -> Option<&'static str> {
    let current_exe = env::current_exe().ok()?;
    match current_exe.file_stem()?.to_str()? {
        "streamverse-pack-douyin" => Some("douyin-pack"),
        "streamverse-pack-bilibili" => Some("bilibili-pack"),
        "streamverse-pack-youtube" => Some("youtube-pack"),
        _ => None,
    }
}

fn app_data_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let root = env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(root).join("StreamVerse")
    }

    #[cfg(not(target_os = "windows"))]
    {
        let root = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(root).join(".streamverse")
    }
}

fn ytdlp_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for entry in env::split_paths(&path_var) {
        let candidate = entry.join(binary_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn extend_runtime_path(command: &mut Command) {
    let mut entries = Vec::<PathBuf>::new();

    for candidate in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let path = PathBuf::from(candidate);
        if path.is_dir() {
            entries.push(path);
        }
    }

    if let Some(path_var) = env::var_os("PATH") {
        entries.extend(env::split_paths(&path_var));
    }

    if let Ok(joined) = env::join_paths(entries) {
        command.env("PATH", joined);
    }
}

pub fn read_process_error(stderr: &[u8], fallback: &str) -> String {
    let message = String::from_utf8_lossy(stderr);
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }

    if trimmed.contains("Failed to decrypt with DPAPI") {
        return "Chrome 在 Windows 上启用了新的 Cookie 加密，当前无法直接从浏览器解密。请先使用浏览器扩展导出 Netscape 格式的 cookies.txt，再到设置中选择该文件后重试。".to_string();
    }

    trimmed.to_string()
}

pub fn cleanup_cookie_file(path: &Option<PathBuf>) {
    if let Some(path) = path {
        let _ = fs::remove_file(path);
    }
}

fn write_progress(current: u32, total: u32, message: &str) {
    let Some(path) = env::var_os("STREAMVERSE_PROGRESS_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let payload = serde_json::json!({
        "current": current.max(0),
        "total": total.max(current),
        "message": message,
    });
    let _ = fs::write(path, payload.to_string());
}

fn dedupe_formats(formats: Vec<VideoFormat>) -> Vec<VideoFormat> {
    let mut deduped = Vec::<VideoFormat>::new();

    for format in formats {
        if let Some(existing) = deduped
            .iter_mut()
            .find(|candidate| same_profile(candidate, &format))
        {
            *existing = merge_formats(existing.clone(), format);
        } else {
            deduped.push(format);
        }
    }

    deduped
}

fn same_profile(left: &VideoFormat, right: &VideoFormat) -> bool {
    normalized_quality_key(left) == normalized_quality_key(right)
        && left.requires_login == right.requires_login
}

fn should_replace(existing: &VideoFormat, candidate: &VideoFormat) -> bool {
    candidate.recommended && !existing.recommended
        || candidate.no_watermark && !existing.no_watermark
        || candidate.direct_url.is_some() && existing.direct_url.is_none()
        || candidate.audio_direct_url.is_some() && existing.audio_direct_url.is_none()
        || candidate.bitrate_kbps > existing.bitrate_kbps
        || (candidate.bitrate_kbps == existing.bitrate_kbps
            && codec_priority(&candidate.codec) < codec_priority(&existing.codec))
}

fn merge_formats(existing: VideoFormat, candidate: VideoFormat) -> VideoFormat {
    let prefer_candidate = should_replace(&existing, &candidate);

    let (mut primary, secondary) = if prefer_candidate {
        (candidate, existing)
    } else {
        (existing, candidate)
    };

    if !primary.recommended && secondary.recommended {
        primary.recommended = true;
    }
    if !primary.no_watermark && secondary.no_watermark {
        primary.no_watermark = true;
    }
    if primary.direct_url.is_none() {
        primary.direct_url = secondary.direct_url;
    }
    if primary.referer.is_none() {
        primary.referer = secondary.referer;
    }
    if primary.user_agent.is_none() {
        primary.user_agent = secondary.user_agent;
    }
    if primary.audio_direct_url.is_none() {
        primary.audio_direct_url = secondary.audio_direct_url;
    }
    if primary.audio_referer.is_none() {
        primary.audio_referer = secondary.audio_referer;
    }
    if primary.audio_user_agent.is_none() {
        primary.audio_user_agent = secondary.audio_user_agent;
    }
    if primary.bitrate_kbps == 0 {
        primary.bitrate_kbps = secondary.bitrate_kbps;
    }

    primary
}

fn map_formats(platform: &str, raw_formats: Vec<RawFormat>) -> Vec<VideoFormat> {
    match platform {
        "bilibili" | "youtube" => map_adaptive_formats(raw_formats),
        _ => map_generic_formats(raw_formats),
    }
}

fn map_generic_formats(raw_formats: Vec<RawFormat>) -> Vec<VideoFormat> {
    let mut formats = raw_formats
        .into_iter()
        .filter(has_muxed_video)
        .map(|format| VideoFormat {
            id: format
                .format_id
                .clone()
                .unwrap_or_else(|| "best".to_string()),
            label: build_label(&format),
            resolution: build_resolution(&format),
            bitrate_kbps: format.tbr.unwrap_or_default().round() as u32,
            codec: human_codec_name(format.vcodec.as_deref()),
            container: format
                .ext
                .as_deref()
                .map(|ext| ext.to_ascii_uppercase())
                .unwrap_or_else(|| "AUTO".to_string()),
            no_watermark: false,
            requires_login: false,
            requires_processing: false,
            recommended: false,
            direct_url: direct_media_url(&format),
            referer: format_referer(&format),
            user_agent: format_user_agent(&format),
            audio_direct_url: None,
            audio_referer: None,
            audio_user_agent: None,
            file_size_bytes: format.filesize.or(format.filesize_approx),
        })
        .collect::<Vec<_>>();

    formats.sort_by_key(|format| {
        let height = format
            .resolution
            .split('x')
            .nth(1)
            .and_then(|item| item.parse::<u32>().ok())
            .unwrap_or_default();
        (Reverse(height), Reverse(format.bitrate_kbps))
    });

    if let Some(first) = formats.first_mut() {
        first.recommended = true;
    }

    formats
}

fn map_adaptive_formats(raw_formats: Vec<RawFormat>) -> Vec<VideoFormat> {
    let muxed = raw_formats
        .iter()
        .filter(|format| has_muxed_video(format))
        .map(|format| build_video_format(format, false, None))
        .collect::<Vec<_>>();

    let best_audio = raw_formats
        .iter()
        .filter(|format| is_audio_only(format))
        .max_by_key(|format| format.tbr.unwrap_or_default().round() as u32);

    let mut formats = muxed;
    formats.extend(
        raw_formats
            .iter()
            .filter(|format| is_video_only(format))
            .map(|format| build_video_format(format, best_audio.is_some(), best_audio)),
    );

    finalize_formats(formats)
}

fn build_video_format(
    format: &RawFormat,
    requires_processing: bool,
    best_audio: Option<&RawFormat>,
) -> VideoFormat {
    let mut format_id = format
        .format_id
        .clone()
        .unwrap_or_else(|| "best".to_string());
    let audio_bitrate = best_audio
        .and_then(|audio| audio.tbr)
        .unwrap_or_default()
        .round() as u32;

    if requires_processing {
        if let Some(audio_id) = best_audio.and_then(|audio| audio.format_id.as_deref()) {
            format_id = format!("{format_id}+{audio_id}");
        }
    }

    let video_size = format.filesize.or(format.filesize_approx);
    let audio_size = best_audio.and_then(|a| a.filesize.or(a.filesize_approx));
    let combined_size = match (video_size, audio_size) {
        (Some(v), Some(a)) => Some(v + a),
        (Some(v), None) => Some(v),
        _ => None,
    };

    VideoFormat {
        id: format_id,
        label: build_label(format),
        resolution: build_resolution(format),
        bitrate_kbps: format.tbr.unwrap_or_default().round() as u32 + audio_bitrate,
        codec: human_codec_name(format.vcodec.as_deref()),
        container: format
            .ext
            .as_deref()
            .map(|ext| ext.to_ascii_uppercase())
            .unwrap_or_else(|| "AUTO".to_string()),
        no_watermark: false,
        requires_login: false,
        requires_processing,
        recommended: false,
        direct_url: direct_media_url(format),
        referer: format_referer(format),
        user_agent: format_user_agent(format),
        audio_direct_url: best_audio.and_then(direct_media_url),
        audio_referer: best_audio.and_then(format_referer),
        audio_user_agent: best_audio.and_then(format_user_agent),
        file_size_bytes: combined_size,
    }
}

fn finalize_formats(mut formats: Vec<VideoFormat>) -> Vec<VideoFormat> {
    formats.sort_by_key(|format| {
        let height = format
            .resolution
            .split('x')
            .nth(1)
            .and_then(|item| item.parse::<u32>().ok())
            .unwrap_or_default();
        (Reverse(height), Reverse(format.bitrate_kbps))
    });

    if let Some(first) = formats.first_mut() {
        first.recommended = true;
    }

    formats
}

fn has_muxed_video(format: &RawFormat) -> bool {
    let has_video = format
        .vcodec
        .as_deref()
        .is_some_and(|codec| codec != "none");
    let has_audio = format
        .acodec
        .as_deref()
        .is_some_and(|codec| codec != "none");
    has_video && has_audio
}

fn is_video_only(format: &RawFormat) -> bool {
    let has_video = format
        .vcodec
        .as_deref()
        .is_some_and(|codec| codec != "none");
    let has_audio = format
        .acodec
        .as_deref()
        .is_some_and(|codec| codec != "none");
    has_video && !has_audio
}

fn is_audio_only(format: &RawFormat) -> bool {
    let has_video = format
        .vcodec
        .as_deref()
        .is_some_and(|codec| codec != "none");
    let has_audio = format
        .acodec
        .as_deref()
        .is_some_and(|codec| codec != "none");
    !has_video && has_audio
}

fn direct_media_url(format: &RawFormat) -> Option<String> {
    match format.protocol.as_deref() {
        Some("http") | Some("https") | None => {
            format.url.clone().filter(|url| !url.trim().is_empty())
        }
        _ => None,
    }
}

fn normalized_quality_key(format: &VideoFormat) -> String {
    let height = format
        .resolution
        .split('x')
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_default();

    if height > 0 {
        return format!("H{height}");
    }

    let label = format
        .label
        .trim()
        .to_ascii_uppercase()
        .replace(|character: char| !character.is_ascii_alphanumeric(), "");
    let resolution = format
        .resolution
        .trim()
        .to_ascii_uppercase()
        .replace(|character: char| !character.is_ascii_alphanumeric(), "");
    format!("{label}|{resolution}")
}

fn codec_priority(codec: &str) -> u8 {
    let normalized = codec
        .trim()
        .to_ascii_uppercase()
        .replace(|character: char| !character.is_ascii_alphanumeric(), "");

    if normalized.starts_with("H264") || normalized.starts_with("AVC") {
        return 0;
    }
    if normalized.starts_with("H265") || normalized.starts_with("HEVC") {
        return 1;
    }
    if normalized.starts_with("AV1") {
        return 2;
    }
    if normalized.starts_with("VP9") {
        return 3;
    }
    4
}

fn format_referer(format: &RawFormat) -> Option<String> {
    format
        .http_headers
        .as_ref()
        .and_then(|headers| headers.referer.clone())
        .filter(|value| !value.trim().is_empty())
}

fn format_user_agent(format: &RawFormat) -> Option<String> {
    format
        .http_headers
        .as_ref()
        .and_then(|headers| headers.user_agent.clone())
        .filter(|value| !value.trim().is_empty())
}

fn build_label(format: &RawFormat) -> String {
    if let Some(height) = format.height {
        return format!("{height}P");
    }

    format
        .format_note
        .clone()
        .unwrap_or_else(|| "默认格式".to_string())
}

fn build_resolution(format: &RawFormat) -> String {
    match (format.width, format.height) {
        (Some(width), Some(height)) => format!("{width}x{height}"),
        _ => "Auto".to_string(),
    }
}

fn human_codec_name(codec: Option<&str>) -> String {
    match codec.unwrap_or_default() {
        raw if raw.starts_with("avc") || raw.starts_with("h264") => "H.264".to_string(),
        raw if raw.starts_with("hev") || raw.starts_with("h265") => "H.265".to_string(),
        raw if raw.starts_with("av01") || raw.starts_with("av1") => "AV1".to_string(),
        raw if raw.starts_with("vp9") => "VP9".to_string(),
        "" | "none" => "Auto".to_string(),
        raw => raw.to_string(),
    }
}

fn format_publish_date(raw: Option<&str>) -> String {
    match raw {
        Some(value) if value.len() == 8 => {
            format!("{}-{}-{}", &value[0..4], &value[4..6], &value[6..8])
        }
        Some(value) => value.to_string(),
        None => "未知".to_string(),
    }
}

fn append_auth_args(
    command: &mut Command,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
) {
    if let Some(file) = cookie_file.filter(|value| !value.trim().is_empty()) {
        command.arg("--cookies").arg(file);
        return;
    }

    if let Some(browser) = cookie_browser.filter(|value| !value.trim().is_empty()) {
        let browser_arg = normalize_cookie_browser_arg(browser);
        command.arg("--cookies-from-browser").arg(browser_arg);
    }
}

fn normalize_cookie_browser_arg(browser: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        if browser.to_lowercase() == "chrome" {
            ensure_chrome_cookie_unlock_plugin();
        }
    }
    browser.to_string()
}

/// On Windows, Chrome 127+ locks its cookie database exclusively via the
/// LockProfileCookieDatabase feature.  yt-dlp's `shutil.copy` cannot open the
/// file while Chrome is running.
///
/// The officially recommended workaround is the ChromeCookieUnlock yt-dlp
/// plugin (MIT — Charles Machalow / seproDev) which uses the Windows Restart
/// Manager API to temporarily release Chrome's lock on the Cookies file, then
/// retries the copy.
///
/// This function ensures the plugin is installed at `%APPDATA%\yt-dlp\plugins\`
/// so that yt-dlp picks it up automatically.
#[cfg(target_os = "windows")]
fn ensure_chrome_cookie_unlock_plugin() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Ok(appdata) = env::var("APPDATA") else { return };
        let dir = PathBuf::from(&appdata)
            .join("yt-dlp")
            .join("plugins")
            .join("ChromeCookieUnlock")
            .join("yt_dlp_plugins")
            .join("postprocessor");
        let target = dir.join("chrome_cookie_unlock.py");
        if target.exists() {
            return;
        }
        if fs::create_dir_all(&dir).is_err() {
            return;
        }
        let _ = fs::write(&target, CHROME_COOKIE_UNLOCK_PLUGIN_PY);
    });
}

#[cfg(target_os = "windows")]
const CHROME_COOKIE_UNLOCK_PLUGIN_PY: &str = r#"# yt-dlp plugin: ChromeCookieUnlock
# Adapted from https://github.com/seproDev/yt-dlp-ChromeCookieUnlock
# Original by Charles Machalow (MIT License)
import sys, os
if sys.platform == "win32":
    from ctypes import windll, byref, create_unicode_buffer, pointer, WINFUNCTYPE
    from ctypes.wintypes import DWORD, WCHAR, UINT
    ERROR_SUCCESS, ERROR_MORE_DATA, RmForceShutdown = 0, 234, 1
    @WINFUNCTYPE(None, UINT)
    def _rm_cb(pct):
        pass
    _rstrtmgr = windll.LoadLibrary("Rstrtmgr")
    def _unlock_file(path):
        sh = DWORD(0)
        result = DWORD(_rstrtmgr.RmStartSession(byref(sh), DWORD(0), (WCHAR * 256)())).value
        if result != ERROR_SUCCESS:
            return
        try:
            result = DWORD(_rstrtmgr.RmRegisterResources(sh, 1, byref(pointer(create_unicode_buffer(path))), 0, None, 0, None)).value
            if result != ERROR_SUCCESS:
                return
            needed = DWORD(0)
            result = DWORD(_rstrtmgr.RmGetList(sh, byref(needed), byref(DWORD(0)), None, byref(DWORD(0)))).value
            if result not in (ERROR_SUCCESS, ERROR_MORE_DATA):
                return
            if needed.value:
                _rstrtmgr.RmShutdown(sh, RmForceShutdown, _rm_cb)
        finally:
            _rstrtmgr.RmEndSession(sh)
    import yt_dlp.cookies
    _orig = yt_dlp.cookies._open_database_copy
    def _patched(db_path, tmpdir):
        try:
            return _orig(db_path, tmpdir)
        except PermissionError:
            print("[StreamVerse] Unlocking Chrome cookie database...", file=sys.stderr)
            _unlock_file(db_path)
            return _orig(db_path, tmpdir)
    yt_dlp.cookies._open_database_copy = _patched

from yt_dlp.postprocessor.common import PostProcessor
class ChromeCookieUnlockPP(PostProcessor):
    pass
"#;

fn venv_python_path(venv_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        venv_dir.join("Scripts").join("python.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        venv_dir.join("bin").join("python")
    }
}

fn find_system_python() -> Result<PathBuf, String> {
    let mut best_supported = None::<(PathBuf, (u32, u32, u32))>;
    let mut best_any = None::<(PathBuf, (u32, u32, u32))>;

    for candidate in [
        "/opt/homebrew/bin/python3",
        "/usr/local/bin/python3",
        "python3",
        "python",
    ] {
        let path = PathBuf::from(candidate);
        let Some(version) = detect_python_version(&path) else {
            continue;
        };

        if best_any
            .as_ref()
            .is_none_or(|(_, current)| version > *current)
        {
            best_any = Some((path.clone(), version));
        }

        if python_version_supported(&path)
            && best_supported
                .as_ref()
                .is_none_or(|(_, current)| version > *current)
        {
            best_supported = Some((path, version));
        }
    }

    if let Some((path, _)) = best_supported.or(best_any) {
        return Ok(path);
    }

    Err("未找到可用的 Python 解释器，请先安装 python3。".to_string())
}

fn prepare_ytdlp_command() -> Result<Command, String> {
    if let Some(path) = preferred_external_ytdlp_path() {
        let mut command = silent_command(path);
        extend_runtime_path(&mut command);
        return Ok(command);
    }

    if let Some(path) = shared_pack_root("download-engine")
        .map(|root| root.join("bin").join(ytdlp_binary_name()))
        .filter(|path| path.is_file())
    {
        let mut command = silent_command(path);
        extend_runtime_path(&mut command);
        return Ok(command);
    }

    if let Ok(python_bin) = ensure_ytdlp_runtime() {
        let mut command = silent_command(python_bin);
        configure_python_command(&mut command);
        command.arg("-m").arg("yt_dlp");
        extend_runtime_path(&mut command);
        return Ok(command);
    }

    if let Some(path) = find_in_path(ytdlp_binary_name()) {
        let mut command = silent_command(path);
        extend_runtime_path(&mut command);
        return Ok(command);
    }

    Err("未检测到可用的 yt-dlp，请重新安装应用后再试。".to_string())
}

fn python_modules_available(path: &Path, modules: &[&str]) -> Result<bool, String> {
    let imports = modules.join(", ");
    let mut command = silent_command(path);
    configure_python_command(&mut command);
    let output = command
        .arg("-c")
        .arg(format!("import {imports}"))
        .output()
        .map_err(|error| format!("检测 Python 依赖失败：{error}"))?;

    Ok(output.status.success())
}

fn preferred_js_runtime() -> Option<String> {
    PREFERRED_JS_RUNTIME
        .get_or_init(|| {
            for (kind, candidates) in [
                (
                    "deno",
                    vec!["/opt/homebrew/bin/deno", "/usr/local/bin/deno", "deno"],
                ),
                (
                    "node",
                    vec!["/opt/homebrew/bin/node", "/usr/local/bin/node", "node"],
                ),
            ] {
                for candidate in candidates {
                    let path = PathBuf::from(candidate);
                    if !candidate.contains('/') {
                        if let Some(found) = find_in_path(candidate) {
                            return Some(format!("{kind}:{}", found.display()));
                        }
                        continue;
                    }

                    if path.is_file() {
                        return Some(format!("{kind}:{}", path.display()));
                    }
                }
            }

            None
        })
        .clone()
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
