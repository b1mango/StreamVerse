use crate::media_contract::{ProfileBatch, VideoAsset, VideoFormat, DEFAULT_GRADIENT};
use crate::settings;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

static RESOURCE_ROOT: OnceLock<PathBuf> = OnceLock::new();
// default 客户端集合里 web 客户端最慢（需要额外网页请求），排除后解析提速约 40%
// 且不损失清晰度与音频流；会员/受限内容仍由其余客户端覆盖
pub(crate) const YOUTUBE_EXTRACTOR_ARGS: &str = "youtube:player_client=default,-web";
const YOUTUBE_COLLECTION_EXTRACTOR_ARGS: &str = "youtubetab:skip=webpage,authcheck";
const INFO_CACHE_MAX_AGE: Duration = Duration::from_secs(30 * 60);

#[derive(Deserialize)]
struct RawInfo {
    id: Option<String>,
    title: Option<String>,
    uploader: Option<String>,
    creator: Option<String>,
    channel: Option<String>,
    duration: Option<f64>,
    upload_date: Option<String>,
    description: Option<String>,
    thumbnail: Option<String>,
    formats: Option<Vec<RawFormat>>,
}

#[derive(Deserialize)]
struct RawPlaylist {
    title: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    playlist_count: Option<u32>,
    entries: Option<Vec<RawPlaylistEntry>>,
}

#[derive(Deserialize)]
struct RawPlaylistEntry {
    id: Option<String>,
    title: Option<String>,
    url: Option<String>,
    duration: Option<f64>,
    upload_date: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    thumbnails: Option<Vec<RawThumbnail>>,
}

#[derive(Deserialize)]
struct RawThumbnail {
    url: Option<String>,
}

#[derive(Default, Deserialize)]
struct RawFormat {
    format_id: Option<String>,
    format_note: Option<String>,
    ext: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    vcodec: Option<String>,
    acodec: Option<String>,
    tbr: Option<f64>,
    filesize: Option<u64>,
    filesize_approx: Option<u64>,
    protocol: Option<String>,
}

pub fn set_resource_root(path: PathBuf) {
    let _ = RESOURCE_ROOT.set(path);
}

pub fn analyze_generic_url(
    platform: &str,
    source_url: &str,
    _cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    proxy_url: Option<&str>,
) -> Result<VideoAsset, String> {
    let mut command = silent_command(resolve_sidecar("yt-dlp")?);
    command.args([
        "--ignore-config",
        "--dump-single-json",
        "--no-playlist",
        "--no-warnings",
        "--socket-timeout",
        "30",
        "--retries",
        "5",
    ]);
    if let Some(file) = cookie_file.filter(|value| !value.trim().is_empty()) {
        command.arg("--cookies").arg(file);
    }
    if platform == "youtube" {
        append_youtube_extraction_args(&mut command)?;
        command.arg("--concurrent-fragments").arg("4");
        if let Some(proxy) = proxy_url.filter(|value| !value.trim().is_empty()) {
            command.arg("--proxy").arg(proxy);
        }
    }
    if platform == "bilibili" {
        // B 站风控校验 Origin/Referer，缺失时网页请求易被 412 拦截；
        // 重试退避限制在 2s，避免偶发 412 把解析拖到几十秒
        command
            .arg("--add-header")
            .arg("Origin: https://www.bilibili.com")
            .arg("--add-header")
            .arg("Referer: https://www.bilibili.com/")
            .arg("--retries")
            .arg("3")
            .arg("--retry-sleep")
            .arg("2");
    }
    let output = command
        .arg(source_url)
        .output()
        .map_err(|error| format!("启动固定 yt-dlp sidecar 失败：{error}"))?;
    if !output.status.success() {
        return Err(read_process_error(&output.stderr, "解析链接失败。"));
    }
    let raw: RawInfo = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("解析 yt-dlp 响应失败：{error}"))?;
    if matches!(platform, "youtube" | "bilibili") {
        if let Some(asset_id) = raw.id.as_deref() {
            let _ = cache_platform_info(platform, asset_id, &output.stdout);
        }
    }
    let formats = map_formats(platform, raw.formats.unwrap_or_default());
    Ok(VideoAsset {
        asset_id: raw.id.unwrap_or_else(|| source_url.to_string()),
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
        cover_url: raw.thumbnail.filter(|value| !value.trim().is_empty()),
        cover_urls: Vec::new(),
        cover_gradient: DEFAULT_GRADIENT.to_string(),
        image_urls: Vec::new(),
        formats,
    })
}

fn info_cache_path(platform: &str, asset_id: &str) -> Option<PathBuf> {
    let safe_id = asset_id
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
        .then_some(asset_id)?;
    Some(
        settings::app_data_root()
            .join(format!("{platform}-info"))
            .join(format!("{safe_id}.json")),
    )
}

fn cache_platform_info(platform: &str, asset_id: &str, payload: &[u8]) -> Result<(), String> {
    let Some(path) = info_cache_path(platform, asset_id) else {
        return Ok(());
    };
    let parent = path
        .parent()
        .ok_or_else(|| "无法确定解析缓存目录。".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建解析缓存失败：{error}"))?;
    fs::write(path, payload).map_err(|error| format!("保存解析缓存失败：{error}"))
}

/// 解析结果缓存（YouTube / B 站）：下载时直接 --load-info-json 复用，
/// 避免下载前再跑一遍完整解析（B 站还会因此再次暴露于 412 风控）
pub(crate) fn cached_platform_info_path(platform: &str, asset_id: &str) -> Option<PathBuf> {
    let path = info_cache_path(platform, asset_id)?;
    let age = path.metadata().ok()?.modified().ok()?.elapsed().ok()?;
    (age <= INFO_CACHE_MAX_AGE).then_some(path)
}

pub fn analyze_youtube_collection(
    source_url: &str,
    cookie_file: Option<&str>,
    proxy_url: Option<&str>,
) -> Result<ProfileBatch, String> {
    let normalized_url = normalize_youtube_collection_url(source_url);
    let mut command = silent_command(resolve_sidecar("yt-dlp")?);
    command.args([
        "--ignore-config",
        "--flat-playlist",
        "--dump-single-json",
        "--no-warnings",
        "--socket-timeout",
        "20",
        "--retries",
        "3",
        "--playlist-end",
        "2000",
        "--extractor-args",
        YOUTUBE_COLLECTION_EXTRACTOR_ARGS,
    ]);
    if let Some(file) = cookie_file.filter(|value| !value.trim().is_empty()) {
        command.arg("--cookies").arg(file);
    }
    if let Some(proxy) = proxy_url.filter(|value| !value.trim().is_empty()) {
        command.arg("--proxy").arg(proxy);
    }
    let output = command
        .arg(&normalized_url)
        .output()
        .map_err(|error| format!("启动固定 yt-dlp sidecar 失败：{error}"))?;
    if !output.status.success() {
        return Err(read_process_error(
            &output.stderr,
            "解析 YouTube 主页或合集失败。",
        ));
    }
    let raw: RawPlaylist = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("解析 YouTube 批量响应失败：{error}"))?;
    let profile_title = raw
        .title
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "YouTube 批量下载".to_string());
    let default_author = raw
        .uploader
        .or(raw.channel)
        .unwrap_or_else(|| profile_title.clone());
    let items = raw
        .entries
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            let asset_id = entry.id.filter(|value| !value.trim().is_empty())?;
            let source_url = entry
                .url
                .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
                .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={asset_id}"));
            let cover_urls: Vec<String> = entry
                .thumbnails
                .unwrap_or_default()
                .into_iter()
                .filter_map(|thumbnail| thumbnail.url)
                .filter(|value| !value.trim().is_empty())
                .collect();
            Some(VideoAsset {
                asset_id,
                platform: "youtube".to_string(),
                source_url,
                title: entry
                    .title
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "未命名视频".to_string()),
                author: entry
                    .uploader
                    .or(entry.channel)
                    .unwrap_or_else(|| default_author.clone()),
                duration_seconds: entry.duration.unwrap_or_default().round() as u32,
                publish_date: format_publish_date(entry.upload_date.as_deref()),
                caption: String::new(),
                category_label: Some("视频".to_string()),
                group_title: Some(profile_title.clone()),
                cover_url: cover_urls.last().cloned(),
                cover_urls,
                cover_gradient: DEFAULT_GRADIENT.to_string(),
                image_urls: Vec::new(),
                formats: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Err("该 YouTube 主页或合集没有读取到可下载的视频。".to_string());
    }
    let fetched_count = items.len() as u32;
    Ok(ProfileBatch {
        profile_title,
        source_url: normalized_url,
        total_available: raw
            .playlist_count
            .unwrap_or(fetched_count)
            .max(fetched_count),
        fetched_count,
        skipped_count: 0,
        items,
    })
}

pub(crate) fn append_youtube_extraction_args(command: &mut Command) -> Result<(), String> {
    let deno = resolve_sidecar("deno")?;
    command
        .arg("--force-ipv4")
        .arg("--js-runtimes")
        .arg(format!("deno:{}", deno.display()))
        .arg("--extractor-args")
        .arg(YOUTUBE_EXTRACTOR_ARGS);
    Ok(())
}

/// helper sidecar 调用的错误分类：
/// Infrastructure = sidecar 缺失 / 进程拉起失败 / 输出 JSON 无法解析（可由 yt-dlp 兜底）；
/// Business = helper 正常运行但以非零码退出并给出错误信息（登录态失效、403 等，需原样上报）
#[derive(Debug)]
pub enum HelperRunError {
    Infrastructure(String),
    Business(String),
}

impl HelperRunError {
    pub fn into_message(self) -> String {
        match self {
            HelperRunError::Infrastructure(message) | HelperRunError::Business(message) => message,
        }
    }
}

pub fn run_helper_json<T: DeserializeOwned>(
    action: &str,
    source_url: &str,
    cookie_file: Option<&str>,
    browser: Option<&str>,
    port: Option<u16>,
    progress_file: Option<&Path>,
) -> Result<T, String> {
    run_helper_json_classified(action, source_url, cookie_file, browser, port, progress_file)
        .map_err(HelperRunError::into_message)
}

pub fn run_helper_json_classified<T: DeserializeOwned>(
    action: &str,
    source_url: &str,
    cookie_file: Option<&str>,
    browser: Option<&str>,
    port: Option<u16>,
    progress_file: Option<&Path>,
) -> Result<T, HelperRunError> {
    let mut command = silent_command(
        resolve_sidecar("streamverse-helper").map_err(HelperRunError::Infrastructure)?,
    );
    command.arg(action).arg("--url").arg(source_url);
    if action == "douyin-profile" {
        command.arg("--limit").arg("2000");
    }
    let _ = port;
    if let Some(path) = cookie_file.filter(|value| !value.trim().is_empty()) {
        command.arg("--cookie-file").arg(path);
    }
    if let Some(browser) = browser.filter(|value| !value.trim().is_empty()) {
        command.arg("--browser").arg(browser);
    }
    if let Some(path) = progress_file {
        command.env("STREAMVERSE_PROGRESS_FILE", path);
    }
    let output = command.output().map_err(|error| {
        HelperRunError::Infrastructure(format!("启动固定 helper sidecar 失败：{error}"))
    })?;
    if !output.status.success() {
        let message = read_process_error(&output.stderr, "");
        // 非零退出但没有 stderr 信息时无法区分是业务失败还是 sidecar 本身损坏，
        // 归为基础环境错误，允许调用方回退 yt-dlp
        if message.is_empty() {
            return Err(HelperRunError::Infrastructure(
                "解析 helper 执行失败。".to_string(),
            ));
        }
        return Err(HelperRunError::Business(message));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let payload = stdout
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))
        .unwrap_or(stdout.trim());
    serde_json::from_str(payload)
        .map_err(|error| HelperRunError::Infrastructure(format!("解析 helper 输出失败：{error}")))
}

pub fn resolve_sidecar(name: &str) -> Result<PathBuf, String> {
    let plain = format!("{name}{}", executable_suffix());
    let target = format!("{name}-{}{}", target_triple(), executable_suffix());
    let onedir = format!("{name}-onedir");
    let mut candidates = Vec::new();
    if let Some(root) = RESOURCE_ROOT.get() {
        candidates.extend(sidecar_candidates(root, &onedir, &plain, &target));
    }
    if let Ok(current) = env::current_exe() {
        if let Some(parent) = current.parent() {
            candidates.extend(sidecar_candidates(parent, &onedir, &plain, &target));
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest.is_dir() {
        // macOS 上 yt-dlp / streamverse-helper 以 onedir 目录形式存在（避免 onefile 每次启动自解压 + 安全扫描）
        candidates.push(manifest.join("binaries").join(&onedir).join(&plain));
        candidates.push(manifest.join("binaries").join(&target));
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("缺少固定 sidecar：{target}。请重新安装 StreamVerse。"))
}

fn sidecar_candidates(root: &Path, onedir: &str, plain: &str, target: &str) -> Vec<PathBuf> {
    vec![
        root.join(onedir).join(plain),
        root.join(plain),
        root.join(target),
        root.join("binaries").join(onedir).join(plain),
        root.join("binaries").join(plain),
        root.join("binaries").join(target),
    ]
}

fn map_formats(platform: &str, formats: Vec<RawFormat>) -> Vec<VideoFormat> {
    // B 站 DASH 流是音视频分离的：给纯视频格式配上最佳音频流，
    // 否则 -f 只下载视频流，成品无声、MP3 提取也会失败
    let bilibili_best_audio_id = (platform == "bilibili")
        .then(|| {
            formats
                .iter()
                .filter(|raw| raw.vcodec.as_deref().is_none_or(|codec| codec == "none"))
                .filter(|raw| raw.acodec.as_deref().is_some_and(|codec| codec != "none"))
                .filter(|raw| raw.format_id.as_deref().is_some_and(|id| !id.is_empty()))
                .max_by(|left, right| {
                    left.tbr
                        .unwrap_or_default()
                        .partial_cmp(&right.tbr.unwrap_or_default())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .and_then(|raw| raw.format_id.clone())
        })
        .flatten();
    let mut best = BTreeMap::<String, (VideoFormat, bool)>::new();
    for raw in formats {
        let id = raw.format_id.unwrap_or_default();
        if id.is_empty() || raw.vcodec.as_deref().is_none_or(|codec| codec == "none") {
            continue;
        }
        let height = raw.height.unwrap_or_default();
        let width = raw.width.unwrap_or_default();
        let codec = normalize_codec(raw.vcodec.as_deref());
        let recommended = height == 1080 && codec == "H.264";
        let requires_processing = raw.acodec.as_deref().is_none_or(|codec| codec == "none");
        let uses_preferred_transport = raw.protocol.as_deref().is_some_and(|value| {
            if platform == "youtube" {
                matches!(value, "https" | "http")
            } else {
                value == "https"
            }
        });
        let format = VideoFormat {
            id: if platform == "youtube" && requires_processing {
                youtube_format_selector(&id)
            } else if platform == "bilibili" && requires_processing {
                match &bilibili_best_audio_id {
                    Some(audio_id) => format!("{id}+{audio_id}"),
                    None => id,
                }
            } else {
                id
            },
            label: raw
                .format_note
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    if height > 0 {
                        format!("{height}P")
                    } else {
                        "自动".to_string()
                    }
                }),
            resolution: if width > 0 && height > 0 {
                format!("{width}x{height}")
            } else {
                "Auto".to_string()
            },
            bitrate_kbps: raw.tbr.unwrap_or_default().round() as u32,
            codec,
            container: raw
                .ext
                .unwrap_or_else(|| "mp4".to_string())
                .to_ascii_uppercase(),
            no_watermark: platform != "douyin",
            requires_login: false,
            requires_processing,
            recommended,
            direct_url: None,
            referer: None,
            user_agent: None,
            audio_direct_url: None,
            audio_referer: None,
            audio_user_agent: None,
            file_size_bytes: raw.filesize.or(raw.filesize_approx),
        };
        let key = format!("{}:{}", height, format.codec);
        if best
            .get(&key)
            .is_none_or(|(existing, existing_is_preferred)| {
                uses_preferred_transport && !existing_is_preferred
                    || uses_preferred_transport == *existing_is_preferred
                        && format.bitrate_kbps > existing.bitrate_kbps
            })
        {
            best.insert(key, (format, uses_preferred_transport));
        }
    }
    let mut mapped: Vec<VideoFormat> = best.into_values().map(|(format, _)| format).collect();
    mapped.sort_by(|left, right| {
        format_height(&right.resolution)
            .cmp(&format_height(&left.resolution))
            .then_with(|| right.bitrate_kbps.cmp(&left.bitrate_kbps))
    });
    if mapped.is_empty() {
        mapped.push(VideoFormat {
            id: "best".to_string(),
            label: "自动".to_string(),
            resolution: "Auto".to_string(),
            bitrate_kbps: 0,
            codec: "AUTO".to_string(),
            container: "AUTO".to_string(),
            no_watermark: platform != "douyin",
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
        });
    } else if !mapped.iter().any(|format| format.recommended) {
        if let Some(first) = mapped.first_mut() {
            first.recommended = true;
        }
    }
    mapped.truncate(24);
    mapped
}

pub(crate) fn thumbnail_candidates(url: &str) -> Vec<String> {
    let video_id = ["/vi_webp/", "/vi/"]
        .iter()
        .find_map(|marker| url.split_once(marker).map(|(_, tail)| tail))
        .and_then(|tail| tail.split('/').next())
        .filter(|value| !value.trim().is_empty());
    let Some(video_id) = video_id else {
        return vec![url.to_string()];
    };
    let mut candidates = vec![
        format!("https://i.ytimg.com/vi/{video_id}/maxresdefault.jpg"),
        format!("https://i.ytimg.com/vi/{video_id}/sddefault.jpg"),
        format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg"),
    ];
    if !candidates.iter().any(|candidate| candidate == url) {
        candidates.push(url.to_string());
    }
    candidates
}

pub(crate) fn thumbnail_referer(url: &str) -> Option<&'static str> {
    let value = url.to_ascii_lowercase();
    if value.contains("douyin") || value.contains("byteimg") || value.contains("bytedance") {
        Some("https://www.douyin.com/")
    } else if value.contains("bilibili") || value.contains("hdslb") {
        Some("https://www.bilibili.com/")
    } else if value.contains("youtube") || value.contains("ytimg") {
        Some("https://www.youtube.com/")
    } else {
        None
    }
}

fn normalize_youtube_collection_url(source_url: &str) -> String {
    if source_url.contains("/playlist?") {
        return source_url.to_string();
    }
    let without_fragment = source_url.split('#').next().unwrap_or(source_url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let mut base = without_query.trim_end_matches('/');
    for suffix in ["/featured", "/videos", "/shorts", "/streams"] {
        if let Some(stripped) = base.strip_suffix(suffix) {
            base = stripped.trim_end_matches('/');
            break;
        }
    }
    format!("{base}/videos")
}

pub(crate) fn youtube_format_selector(format_id: &str) -> String {
    let selected = format_id.trim();
    if selected.contains('+') {
        return selected.to_string();
    }
    if selected == "best" {
        return "bestvideo+bestaudio[ext=m4a]/bestvideo+bestaudio/best".to_string();
    }
    format!("{selected}+bestaudio[ext=m4a]/{selected}+bestaudio/{selected}")
}

fn format_height(resolution: &str) -> u32 {
    resolution
        .split_once('x')
        .and_then(|(_, height)| height.parse().ok())
        .unwrap_or(0)
}

fn normalize_codec(codec: Option<&str>) -> String {
    let raw = codec.unwrap_or_default().to_ascii_lowercase();
    if raw.starts_with("avc") || raw.starts_with("h264") {
        "H.264".to_string()
    } else if raw.starts_with("hev") || raw.starts_with("h265") {
        "H.265".to_string()
    } else if raw.starts_with("av01") || raw.starts_with("av1") {
        "AV1".to_string()
    } else if raw.starts_with("vp9") {
        "VP9".to_string()
    } else if raw.is_empty() {
        "AUTO".to_string()
    } else {
        raw.to_ascii_uppercase()
    }
}

fn format_publish_date(raw: Option<&str>) -> String {
    match raw {
        Some(value) if value.len() == 8 => {
            format!("{}-{}-{}", &value[0..4], &value[4..6], &value[6..8])
        }
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn read_process_error(stderr: &[u8], fallback: &str) -> String {
    let message = String::from_utf8_lossy(stderr);
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    // yt-dlp 的 stderr 常在真正的错误之后还输出 WARNING/DEBUG 行，
    // 优先取最后一个 ERROR 行，避免把警告当成失败原因
    if let Some(line) = trimmed
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with("ERROR"))
    {
        return line.to_string();
    }
    if let Some(line) = trimmed
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("WARNING:") && !line.starts_with("DEBUG:"))
    {
        return line.to_string();
    }
    trimmed.to_string()
}

pub(crate) fn silent_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

fn target_triple() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
}

fn executable_suffix() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_height, info_cache_path, map_formats, normalize_codec, normalize_youtube_collection_url,
        read_process_error, target_triple, youtube_format_selector, RawFormat,
        YOUTUBE_COLLECTION_EXTRACTOR_ARGS, YOUTUBE_EXTRACTOR_ARGS,
    };

    #[test]
    fn read_process_error_prefers_last_error_line_over_trailing_warnings() {
        let stderr = b"WARNING: [youtube] falling back\nERROR: [youtube] HTTP Error 403: Forbidden\nWARNING: post-download notice\n";
        assert_eq!(
            read_process_error(stderr, "fallback"),
            "ERROR: [youtube] HTTP Error 403: Forbidden"
        );
    }

    #[test]
    fn read_process_error_skips_warning_and_debug_lines() {
        let stderr = "DEBUG: verbose detail\n真正的失败原因\nWARNING: trailing warning\n";
        assert_eq!(read_process_error(stderr.as_bytes(), "fallback"), "真正的失败原因");
    }

    #[test]
    fn read_process_error_falls_back_to_full_output() {
        assert_eq!(read_process_error(b"", "fallback"), "fallback");
        assert_eq!(
            read_process_error(b"WARNING: only warnings\nDEBUG: nothing else", "fallback"),
            "WARNING: only warnings\nDEBUG: nothing else"
        );
    }

    #[test]
    fn reports_supported_target_triple() {
        assert!(!target_triple().is_empty());
    }

    #[test]
    fn normalizes_media_metadata() {
        assert_eq!(format_height("1920x1080"), 1080);
        assert_eq!(normalize_codec(Some("avc1.640028")), "H.264");
        assert!(info_cache_path("youtube", "../outside").is_none());
        assert!(info_cache_path("youtube", "SnOckdip_cU").is_some());
        assert!(info_cache_path("bilibili", "BV1GJ411x7h7").is_some());
    }

    #[test]
    fn youtube_formats_keep_audio_and_use_the_same_client_for_download() {
        let formats = map_formats(
            "youtube",
            vec![RawFormat {
                format_id: Some("396".to_string()),
                format_note: Some("360p".to_string()),
                ext: Some("mp4".to_string()),
                width: Some(640),
                height: Some(360),
                vcodec: Some("av01.0.01M.08".to_string()),
                acodec: Some("none".to_string()),
                tbr: Some(288.0),
                filesize: Some(1024),
                filesize_approx: None,
                protocol: Some("https".to_string()),
            }],
        );

        assert_eq!(formats[0].id, "396+bestaudio[ext=m4a]/396+bestaudio/396");
        assert_eq!(
            youtube_format_selector("396"),
            "396+bestaudio[ext=m4a]/396+bestaudio/396"
        );
        assert_eq!(YOUTUBE_EXTRACTOR_ARGS, "youtube:player_client=default,-web");
    }

    #[test]
    fn bilibili_dash_formats_pair_best_audio_stream() {
        let formats = map_formats(
            "bilibili",
            vec![
                RawFormat {
                    format_id: Some("30080".to_string()),
                    height: Some(1080),
                    width: Some(1920),
                    vcodec: Some("avc1.640032".to_string()),
                    acodec: Some("none".to_string()),
                    tbr: Some(2509.0),
                    protocol: Some("https".to_string()),
                    ..RawFormat::default()
                },
                RawFormat {
                    format_id: Some("30216".to_string()),
                    vcodec: Some("none".to_string()),
                    acodec: Some("mp4a.40.5".to_string()),
                    tbr: Some(64.0),
                    protocol: Some("https".to_string()),
                    ..RawFormat::default()
                },
                RawFormat {
                    format_id: Some("30280".to_string()),
                    vcodec: Some("none".to_string()),
                    acodec: Some("mp4a.40.2".to_string()),
                    tbr: Some(319.0),
                    protocol: Some("https".to_string()),
                    ..RawFormat::default()
                },
            ],
        );
        assert_eq!(formats.len(), 1);
        assert_eq!(formats[0].id, "30080+30280");
        assert!(formats[0].requires_processing);
        assert!(formats[0].recommended);
        assert_eq!(formats[0].codec, "H.264");
    }

    #[test]
    fn youtube_formats_prefer_parallel_https_over_hls_duplicates() {
        let formats = map_formats(
            "youtube",
            vec![
                RawFormat {
                    format_id: Some("270".to_string()),
                    height: Some(1080),
                    vcodec: Some("avc1.640028".to_string()),
                    acodec: Some("none".to_string()),
                    tbr: Some(5510.0),
                    protocol: Some("m3u8_native".to_string()),
                    ..RawFormat::default()
                },
                RawFormat {
                    format_id: Some("137".to_string()),
                    height: Some(1080),
                    vcodec: Some("avc1.640028".to_string()),
                    acodec: Some("none".to_string()),
                    tbr: Some(2673.0),
                    protocol: Some("https".to_string()),
                    ..RawFormat::default()
                },
            ],
        );
        assert!(formats[0].id.starts_with("137+"));
        assert!(formats[0].recommended);
    }

    #[test]
    fn youtube_collections_skip_authcheck_when_webpage_is_skipped() {
        assert_eq!(
            YOUTUBE_COLLECTION_EXTRACTOR_ARGS,
            "youtubetab:skip=webpage,authcheck"
        );
    }

    #[test]
    fn youtube_channel_tabs_are_normalized_to_videos() {
        assert_eq!(
            normalize_youtube_collection_url("https://www.youtube.com/@stanley1510"),
            "https://www.youtube.com/@stanley1510/videos"
        );
        assert_eq!(
            normalize_youtube_collection_url("https://www.youtube.com/@stanley1510/featured"),
            "https://www.youtube.com/@stanley1510/videos"
        );
        assert_eq!(
            normalize_youtube_collection_url("https://www.youtube.com/playlist?list=PL123"),
            "https://www.youtube.com/playlist?list=PL123"
        );
    }
}
