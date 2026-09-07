use crate::media_contract::{
    BatchItemSelection, DownloadContentSelection, ProfileBatch, VideoAsset, VideoFormat,
    DEFAULT_GRADIENT,
};
use crate::{
    auth, download_history, formats, parser, platforms, provider_runtime, providers, settings,
    task_store, ytdlp,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::header::{ACCEPT, REFERER, USER_AGENT};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::Manager;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadTask {
    pub(crate) id: String,
    pub(crate) platform: String,
    pub(crate) title: String,
    pub(crate) progress: u32,
    pub(crate) speed_text: String,
    pub(crate) format_label: String,
    pub(crate) status: String,
    pub(crate) eta_text: String,
    pub(crate) message: Option<String>,
    pub(crate) output_path: Option<String>,
    pub(crate) supports_pause: bool,
    pub(crate) supports_cancel: bool,
    #[serde(default)]
    pub(crate) can_retry: bool,
    #[serde(default)]
    pub(crate) cover_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadRequest {
    pub(crate) platform: String,
    pub(crate) source_url: String,
    pub(crate) asset_id: String,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) publish_date: String,
    pub(crate) caption: String,
    pub(crate) cover_url: Option<String>,
    #[serde(default)]
    pub(crate) image_urls: Vec<String>,
    pub(crate) format_id: Option<String>,
    pub(crate) format_label: Option<String>,
    pub(crate) save_directory: String,
    pub(crate) download_options: DownloadContentSelection,
    pub(crate) auto_reveal_in_file_manager: bool,
    #[serde(skip)]
    pub(crate) cookie_browser: Option<String>,
    #[serde(skip)]
    pub(crate) cookie_file: Option<String>,
    #[serde(skip)]
    pub(crate) ffmpeg_path: Option<String>,
    pub(crate) direct_url: Option<String>,
    pub(crate) referer: Option<String>,
    pub(crate) user_agent: Option<String>,
    pub(crate) audio_direct_url: Option<String>,
    pub(crate) audio_referer: Option<String>,
    pub(crate) audio_user_agent: Option<String>,
    /// 仅后端内部使用：重试任务时复用原有输出目录，让 yt-dlp 的 .part 分片可以断点续传
    #[serde(skip)]
    pub(crate) is_retry: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metrics {
    today_downloads: u32,
    success_rate: String,
    available_formats: u32,
    max_quality: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformAuthProfile {
    mode: String,
    browser_id: Option<String>,
    profile_id: Option<String>,
    consented_at: Option<u64>,
    status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapState {
    version: String,
    auth_state: String,
    account_label: String,
    is_windows: bool,
    platform_auth: BTreeMap<String, PlatformAuthProfile>,
    save_directory: String,
    download_mode: String,
    quality_preference: String,
    auto_reveal_in_finder: bool,
    max_concurrent_downloads: u32,
    proxy_url: Option<String>,
    speed_limit: Option<String>,
    auto_update: bool,
    theme: String,
    notify_on_complete: bool,
    language: String,
    ffmpeg_available: bool,
    metrics: Metrics,
    preview: VideoAsset,
    tasks: Vec<DownloadTask>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsProfile {
    version: String,
    auth_state: String,
    account_label: String,
    platform_auth: BTreeMap<String, PlatformAuthProfile>,
    save_directory: String,
    download_mode: String,
    quality_preference: String,
    auto_reveal_in_finder: bool,
    max_concurrent_downloads: u32,
    proxy_url: Option<String>,
    speed_limit: Option<String>,
    auto_update: bool,
    theme: String,
    notify_on_complete: bool,
    language: String,
    ffmpeg_available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchDownloadResult {
    profile_title: String,
    source_url: String,
    total_available: u32,
    fetched_count: u32,
    enqueued_count: u32,
    skipped_count: u32,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileDownloadRequest {
    profile_title: String,
    source_url: String,
    items: Vec<BatchItemSelection>,
    save_directory_override: Option<String>,
    download_options: DownloadContentSelection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveSettingsRequest {
    save_directory: String,
    download_mode: String,
    quality_preference: String,
    auto_reveal_in_finder: bool,
    max_concurrent_downloads: u32,
    proxy_url: Option<String>,
    speed_limit: Option<String>,
    auto_update: bool,
    theme: String,
    notify_on_complete: bool,
    language: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisProgress {
    current: u32,
    total: u32,
    message: String,
}

#[derive(Clone)]
struct AppState {
    tasks: task_store::TaskStore,
    settings: Arc<Mutex<settings::AppSettings>>,
    controllers: ytdlp::TaskControllerStore,
    history: download_history::DownloadHistoryStore,
}

#[derive(Clone)]
struct ToolingState {
    ffmpeg_path: Option<String>,
}

fn analysis_progress_dir() -> PathBuf {
    settings::app_data_root().join("analysis-progress")
}

fn analysis_progress_path(session_id: &str) -> PathBuf {
    analysis_progress_dir().join(format!("{session_id}.json"))
}

fn write_analysis_progress(
    session_id: Option<&str>,
    current: u32,
    total: u32,
    message: &str,
) -> Result<(), String> {
    let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };

    let path = analysis_progress_path(session_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建解析进度目录失败：{error}"))?;
    }

    let payload = AnalysisProgress {
        current,
        total: total.max(current),
        message: message.to_string(),
    };
    let content =
        serde_json::to_vec(&payload).map_err(|error| format!("序列化解析进度失败：{error}"))?;
    fs::write(path, content).map_err(|error| format!("写入解析进度失败：{error}"))
}

#[tauri::command]
fn get_analysis_progress(session_id: String) -> Result<Option<AnalysisProgress>, String> {
    let path = analysis_progress_path(&session_id);
    if !path.is_file() {
        return Ok(None);
    }

    let raw = fs::read(&path).map_err(|error| format!("读取解析进度失败：{error}"))?;
    let payload = serde_json::from_slice::<AnalysisProgress>(&raw)
        .map_err(|error| format!("解析进度内容损坏：{error}"))?;
    Ok(Some(payload))
}

#[tauri::command]
fn clear_analysis_progress(session_id: String) -> Result<(), String> {
    let path = analysis_progress_path(&session_id);
    if path.is_file() {
        fs::remove_file(path).map_err(|error| format!("清理解析进度失败：{error}"))?;
    }
    Ok(())
}

fn ensure_ffmpeg_path(
    current: Option<&str>,
    requires_processing: bool,
) -> Result<Option<String>, String> {
    if !requires_processing || ytdlp::ffmpeg_available(current) {
        return Ok(current.map(str::to_string));
    }

    Ok(provider_runtime::resolve_sidecar("ffmpeg")
        .ok()
        .and_then(|path| path.to_str().map(str::to_string))
        .or_else(|| current.map(str::to_string)))
}

fn fallback_profile_format(
    asset: &VideoAsset,
    selected_format_id: Option<&str>,
) -> Option<VideoFormat> {
    if asset.platform != "bilibili" || !asset.formats.is_empty() {
        return None;
    }

    Some(VideoFormat {
        id: selected_format_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("best")
            .to_string(),
        label: "自动选择".to_string(),
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
    })
}

fn sample_preview() -> VideoAsset {
    VideoAsset {
        asset_id: "7481035099182375478".into(),
        platform: "douyin".into(),
        source_url: "https://v.douyin.com/XXXXXX/".into(),
        title: "春夜街景的风从镜头里吹过".into(),
        author: "镜头笔记".into(),
        duration_seconds: 42,
        publish_date: "2026-03-28".into(),
        caption: "支持分享文本、短链与作品链接解析。".into(),
        category_label: None,
        group_title: None,
        cover_url: None,
        cover_urls: Vec::new(),
        cover_gradient: DEFAULT_GRADIENT.into(),
        image_urls: Vec::new(),
        formats: vec![
            VideoFormat {
                id: "fhd_nowm".into(),
                label: "1080P".into(),
                resolution: "1920x1080".into(),
                bitrate_kbps: 4200,
                codec: "H.264".into(),
                container: "MP4".into(),
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
            },
            VideoFormat {
                id: "uhd_plus".into(),
                label: "2K 超清".into(),
                resolution: "2560x1440".into(),
                bitrate_kbps: 9100,
                codec: "H.265".into(),
                container: "MP4".into(),
                no_watermark: true,
                requires_login: true,
                requires_processing: false,
                recommended: false,
                direct_url: None,
                referer: None,
                user_agent: None,
                audio_direct_url: None,
                audio_referer: None,
                audio_user_agent: None,
                file_size_bytes: None,
            },
        ],
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheckResult {
    current_version: String,
    latest_version: String,
    has_update: bool,
    release_url: String,
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let parse = |value: &str| {
        value
            .trim_start_matches('v')
            .split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    let latest = parse(latest);
    let current = parse(current);
    for index in 0..latest.len().max(current.len()) {
        let lhs = latest.get(index).copied().unwrap_or(0);
        let rhs = current.get(index).copied().unwrap_or(0);
        if lhs != rhs {
            return lhs > rhs;
        }
    }
    false
}

#[tauri::command]
async fn check_for_update(state: tauri::State<'_, AppState>) -> Result<UpdateCheckResult, String> {
    let proxy_url = state.settings.lock().unwrap().proxy_url.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut builder = reqwest::blocking::Client::builder()
            .user_agent(concat!("StreamVerse/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(12));
        if let Some(proxy) = settings::effective_proxy_url(proxy_url.as_deref()) {
            builder = builder
                .proxy(reqwest::Proxy::all(&proxy).map_err(|error| format!("代理配置无效：{error}"))?);
        }
        let client = builder.build().map_err(|error| format!("初始化更新检查失败：{error}"))?;
        let response = client
            .get("https://api.github.com/repos/b1mango/StreamVerse/releases/latest")
            .header("Accept", "application/vnd.github+json")
            .send()
            .map_err(|error| format!("无法连接更新服务器：{error}"))?;
        if !response.status().is_success() {
            return Err(format!("更新服务器返回错误（HTTP {}）。", response.status()));
        }
        let payload: serde_json::Value = serde_json::from_str(
            &response
                .text()
                .map_err(|error| format!("读取更新信息失败：{error}"))?,
        )
        .map_err(|error| format!("解析更新信息失败：{error}"))?;
        let latest = payload
            .get("tag_name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim_start_matches('v')
            .to_string();
        if latest.is_empty() {
            return Err("更新信息中缺少版本号。".to_string());
        }
        let release_url = payload
            .get("html_url")
            .and_then(|value| value.as_str())
            .unwrap_or("https://github.com/b1mango/StreamVerse/releases")
            .to_string();
        let current = env!("CARGO_PKG_VERSION").to_string();
        Ok(UpdateCheckResult {
            has_update: version_is_newer(&latest, &current),
            latest_version: latest,
            current_version: current,
            release_url,
        })
    })
    .await
    .map_err(|error| format!("更新检查任务异常退出：{error}"))?
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("仅支持打开 http(s) 链接。".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        provider_runtime::silent_command("rundll32")
            .args(["url.dll,FileProtocolHandler", &url])
            .spawn()
            .map_err(|error| format!("打开链接失败：{error}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        provider_runtime::silent_command("open")
            .arg(&url)
            .spawn()
            .map_err(|error| format!("打开链接失败：{error}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        provider_runtime::silent_command("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|error| format!("打开链接失败：{error}"))?;
    }
    Ok(())
}

#[tauri::command]
fn get_bootstrap_state(
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
) -> BootstrapState {
    build_bootstrap_state(&state, tooling.ffmpeg_path.as_deref())
}

/// 从输入文本推断目标平台（用于登录态失效后的定向续期）。
fn infer_platform_from_input(raw_input: &str) -> Option<&'static str> {
    let lower = raw_input.to_ascii_lowercase();
    if lower.contains("youtube.com") || lower.contains("youtu.be") {
        Some("youtube")
    } else if lower.contains("douyin") || lower.contains("iesdouyin") {
        Some("douyin")
    } else if lower.contains("bilibili") || lower.contains("b23.tv") {
        Some("bilibili")
    } else {
        None
    }
}

/// 判断解析报错是否属于登录态失效（Cookie 过期/被服务端轮换）。
fn is_auth_class_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    const PATTERNS: &[&str] = &[
        "sign in to confirm",
        "sign in to your account",
        "login required",
        "http error 401",
        "http error 403",
        "only available to registered",
        "use --cookies",
        "cookies are needed",
        "关键 cookie",
        "登录态已失效",
        "需要登录",
    ];
    PATTERNS.iter().any(|pattern| lower.contains(pattern))
}

/// 用设置里“始终授权”的浏览器来源静默重取 Cookie；未授权过则返回 Err。
fn try_refresh_browser_auth(
    platform_auth: &BTreeMap<String, settings::PlatformAuthSettings>,
    platform: &str,
) -> Result<(), String> {
    let entry = platform_auth
        .get(platform)
        .ok_or_else(|| "该平台未导入浏览器登录态。".to_string())?;
    if entry.mode != "browser" || entry.consented_at.is_none() {
        return Err("未开启浏览器登录态长期授权。".to_string());
    }
    let browser_id = entry
        .browser_id
        .clone()
        .ok_or_else(|| "缺少浏览器来源。".to_string())?;
    let result = auth::refresh_browser_cookies(platform, &browser_id, entry.profile_id.clone())?;
    if result.status == "active" {
        Ok(())
    } else {
        Err(result.message)
    }
}

#[tauri::command]
async fn analyze_input(
    raw_input: String,
    session_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<VideoAsset, String> {
    let settings = state.settings.lock().unwrap().clone();
    let proxy_url = settings::effective_proxy_url(settings.proxy_url.as_deref());
    let sid = session_id.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let progress_file = sid.as_deref().map(analysis_progress_path);
        let _ = write_analysis_progress(sid.as_deref(), 0, 1, "正在解析作品链接…");
        let run = || {
            providers::analyze_input(
                &raw_input,
                &settings.platform_auth,
                progress_file.as_deref(),
                proxy_url.as_deref(),
            )
        };
        // YouTube 的 Cookie 会被服务端频繁轮换而失效：命中登录态类报错时，
        // 若用户开启过“始终授权”，自动重取浏览器 Cookie 并重试一次
        let result = match run() {
            Err(error)
                if is_auth_class_error(&error)
                    && infer_platform_from_input(&raw_input).is_some_and(|platform| {
                        try_refresh_browser_auth(&settings.platform_auth, platform).is_ok()
                    }) =>
            {
                let _ =
                    write_analysis_progress(sid.as_deref(), 0, 1, "登录态已自动更新，正在重试解析…");
                run()
            }
            other => other,
        };
        match &result {
            Ok(_) => {
                let _ = write_analysis_progress(sid.as_deref(), 1, 1, "作品解析完成。");
            }
            Err(error) => {
                let _ = write_analysis_progress(sid.as_deref(), 0, 1, error);
            }
        }
        result
    })
    .await
    .map_err(|_| "解析线程异常退出".to_string())?
}

#[tauri::command]
async fn analyze_profile_input(
    raw_input: String,
    _limit: Option<u32>,
    session_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ProfileBatch, String> {
    let settings = state.settings.lock().unwrap().clone();
    let sid = session_id.clone();
    let proxy_url = settings::effective_proxy_url(settings.proxy_url.as_deref());
    let progress_file = session_id.as_deref().map(analysis_progress_path);
    // YouTube 合集与频道主页共用这个入口，进度文案按链接形态区分
    let is_playlist = raw_input.contains("playlist");
    let reading_message = if is_playlist { "正在读取合集视频…" } else { "正在读取主页视频…" };
    let done_message = if is_playlist { "合集视频解析完成。" } else { "主页视频解析完成。" };
    let _ = write_analysis_progress(session_id.as_deref(), 0, 0, reading_message);

    tauri::async_runtime::spawn_blocking(move || {
        let run = || {
            providers::analyze_profile_input(
                &raw_input,
                &settings.platform_auth,
                progress_file.as_deref(),
                proxy_url.as_deref(),
            )
        };
        // 登录态失效时自动续期后重试一次（与单视频解析同一策略）
        let result = match run() {
            Err(error)
                if is_auth_class_error(&error)
                    && infer_platform_from_input(&raw_input).is_some_and(|platform| {
                        try_refresh_browser_auth(&settings.platform_auth, platform).is_ok()
                    }) =>
            {
                let _ = write_analysis_progress(
                    sid.as_deref(),
                    0,
                    0,
                    "登录态已自动更新，正在重试解析…",
                );
                run()
            }
            other => other,
        };
        match &result {
            Ok(batch) => {
                let _ = write_analysis_progress(
                    sid.as_deref(),
                    batch.fetched_count,
                    batch.total_available,
                    done_message,
                );
            }
            Err(error) => {
                let _ = write_analysis_progress(sid.as_deref(), 0, 0, error);
            }
        }
        result
    })
    .await
    .map_err(|_| "批量解析线程异常退出".to_string())?
}

#[tauri::command]
fn create_download_task(
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
    mut request: DownloadRequest,
) -> Result<DownloadTask, String> {
    let settings = state.settings.lock().unwrap().clone();
    request.save_directory = settings::normalize_save_directory(request.save_directory)?;
    request.auto_reveal_in_file_manager = settings.auto_reveal_in_finder;
    let needs_youtube_merge =
        request.platform == "youtube" && request.download_options.download_video;
    let ffmpeg_path = ensure_ffmpeg_path(
        tooling.ffmpeg_path.as_deref(),
        request
            .format_id
            .as_deref()
            .is_some_and(|value| value.contains('+'))
            || request.audio_direct_url.is_some()
            || request.download_options.download_audio
            || needs_youtube_merge,
    )?;
    let auth = settings::platform_auth_for(&settings.platform_auth, &request.platform);
    request.cookie_browser = auth.cookie_browser;
    request.cookie_file = auth.cookie_file;
    request.ffmpeg_path = ffmpeg_path;

    ytdlp::download_video(
        Arc::clone(&state.tasks),
        Arc::clone(&state.controllers),
        request,
    )
}

#[tauri::command]
fn create_profile_download_tasks(
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
    request: ProfileDownloadRequest,
) -> Result<BatchDownloadResult, String> {
    let ProfileDownloadRequest {
        profile_title,
        source_url,
        items,
        save_directory_override,
        download_options,
    } = request;
    let settings = state.settings.lock().unwrap().clone();
    let save_directory = match save_directory_override {
        Some(path) => settings::normalize_save_directory(path)?,
        None => settings.save_directory.clone(),
    };

    if items.is_empty() {
        return Err("请先在主页列表里至少勾选一个作品。".to_string());
    }

    if !download_options.download_video
        && !download_options.download_audio
        && !download_options.download_cover
        && !download_options.download_caption
    {
        return Err("至少要选择一种要保存的内容。".to_string());
    }

    let total_requested = items.len() as u32;
    let tasks = Arc::clone(&state.tasks);
    let controllers = Arc::clone(&state.controllers);
    let ffmpeg_path = tooling.ffmpeg_path.clone();
    let platform = items
        .first()
        .map(|item| item.asset.platform.clone())
        .unwrap_or_else(|| "douyin".to_string());
    let platform_auth = settings::platform_auth_for(&settings.platform_auth, &platform);
    let cookie_browser = platform_auth.cookie_browser.clone();
    let default_cookie_file = platform_auth.cookie_file.clone();
    let quality_preference = settings.quality_preference.clone();
    let effective_proxy_url = settings::effective_proxy_url(settings.proxy_url.as_deref());
    let auto_reveal_in_finder = settings.auto_reveal_in_finder;
    let save_directory_for_thread = save_directory.clone();
    let profile_title_for_thread = profile_title.clone();
    let source_url_for_thread = source_url.clone();

    thread::spawn(move || {
        let mut skipped_count = 0u32;
        let mut first_error = None::<String>;
        let mut runtime_ffmpeg_path = ffmpeg_path.clone();
        let batch_cookie_file = default_cookie_file.as_deref();
        let has_auth = cookie_browser.is_some() || batch_cookie_file.is_some();
        let mut first_item = true;

        for item in items {
            let is_album = !item.asset.image_urls.is_empty();
            let fallback_format = if download_options.download_video && !is_album {
                fallback_profile_format(&item.asset, item.selected_format_id.as_deref())
            } else {
                None
            };
            // 只有需要逐项重新解析的条目（抖音/YouTube 无格式条目）才限速，
            // B 站等自带格式的批次不再在入队阶段白等 800ms/条
            let needs_reparse = download_options.download_video
                && !is_album
                && item.asset.formats.is_empty()
                && fallback_format.is_none();
            if !first_item && needs_reparse {
                std::thread::sleep(std::time::Duration::from_millis(800));
            }
            first_item = false;

            let resolved_asset = if download_options.download_video
                && !is_album
                && item.asset.formats.is_empty()
                && fallback_format.is_none()
            {
                let analysis = if item.asset.platform == "douyin" {
                    provider_runtime::run_helper_json(
                        "douyin-analyze",
                        &item.asset.source_url,
                        batch_cookie_file,
                        cookie_browser.as_deref(),
                        None,
                        None,
                    )
                } else {
                    provider_runtime::analyze_generic_url(
                        &item.asset.platform,
                        &item.asset.source_url,
                        cookie_browser.as_deref(),
                        batch_cookie_file,
                        if item.asset.platform == "youtube" {
                            effective_proxy_url.as_deref()
                        } else {
                            None
                        },
                    )
                };
                match analysis {
                    Ok(asset) => asset,
                    Err(error) => {
                        skipped_count += 1;
                        if first_error.is_none() {
                            first_error = Some(error.clone());
                        }
                        task_store::upsert_task(
                            &tasks,
                            DownloadTask {
                                id: format!("task-prepare-{}", item.asset.asset_id),
                                platform: item.asset.platform.clone(),
                                title: item.asset.title.clone(),
                                progress: 0,
                                speed_text: "-".to_string(),
                                format_label: "准备失败".to_string(),
                                status: "failed".to_string(),
                                eta_text: "失败".to_string(),
                                message: Some(error),
                                output_path: None,
                                supports_pause: false,
                                supports_cancel: false,
                                can_retry: false,
                                cover_url: item.asset.cover_url.clone(),
                            },
                        );
                        continue;
                    }
                }
            } else {
                item.asset.clone()
            };
            let asset = &resolved_asset;
            let selected_format = if download_options.download_video && !is_album {
                item.selected_format_id
                    .as_deref()
                    .and_then(|format_id| {
                        asset
                            .formats
                            .iter()
                            .find(|format| format.id == format_id)
                            .cloned()
                    })
                    .or_else(|| {
                        formats::pick_preferred_format(
                            &asset.formats,
                            &quality_preference,
                            has_auth,
                        )
                    })
                    .or(fallback_format)
            } else {
                None
            };

            if download_options.download_video && !is_album && selected_format.is_none() {
                skipped_count += 1;
                task_store::upsert_task(
                    &tasks,
                    DownloadTask {
                        id: format!("task-prepare-{}", asset.asset_id),
                        platform: asset.platform.clone(),
                        title: asset.title.clone(),
                        progress: 0,
                        speed_text: "-".to_string(),
                        format_label: "未找到可用格式".to_string(),
                        status: "failed".to_string(),
                        eta_text: "失败".to_string(),
                        message: Some("当前作品没有可用的下载格式。".to_string()),
                        output_path: None,
                        supports_pause: false,
                        supports_cancel: false,
                        can_retry: false,
                        cover_url: asset.cover_url.clone(),
                    },
                );
                continue;
            }

            if !is_album
                && (selected_format
                    .as_ref()
                    .is_some_and(|format| format.requires_processing)
                    || download_options.download_audio)
                && !ytdlp::ffmpeg_available(runtime_ffmpeg_path.as_deref())
            {
                match ensure_ffmpeg_path(runtime_ffmpeg_path.as_deref(), true) {
                    Ok(path) => runtime_ffmpeg_path = path,
                    Err(error) => {
                        skipped_count += 1;
                        if first_error.is_none() {
                            first_error = Some(error.clone());
                        }
                        task_store::upsert_task(
                            &tasks,
                            DownloadTask {
                                id: format!("task-prepare-{}", asset.asset_id),
                                platform: asset.platform.clone(),
                                title: asset.title.clone(),
                                progress: 0,
                                speed_text: "-".to_string(),
                                format_label: selected_format
                                    .as_ref()
                                    .map(|format| format.label.clone())
                                    .unwrap_or_else(|| "准备失败".to_string()),
                                status: "failed".to_string(),
                                eta_text: "失败".to_string(),
                                message: Some(error),
                                output_path: None,
                                supports_pause: false,
                                supports_cancel: false,
                                can_retry: false,
                                cover_url: asset.cover_url.clone(),
                            },
                        );
                        continue;
                    }
                }
            }

            if let Err(error) = ytdlp::download_video(
                Arc::clone(&tasks),
                Arc::clone(&controllers),
                DownloadRequest {
                    platform: asset.platform.clone(),
                    source_url: asset.source_url.clone(),
                    asset_id: asset.asset_id.clone(),
                    title: asset.title.clone(),
                    author: asset.author.clone(),
                    publish_date: asset.publish_date.clone(),
                    caption: asset.caption.clone(),
                    cover_url: asset.cover_url.clone(),
                    image_urls: asset.image_urls.clone(),
                    format_id: selected_format.as_ref().map(|format| format.id.clone()),
                    format_label: if is_album {
                        Some(format!("图册 · {} 张", asset.image_urls.len()))
                    } else {
                        selected_format.as_ref().map(|format| format.label.clone())
                    },
                    save_directory: save_directory_for_thread.clone(),
                    download_options: download_options.clone(),
                    auto_reveal_in_file_manager: auto_reveal_in_finder,
                    cookie_browser: cookie_browser.clone(),
                    cookie_file: batch_cookie_file.map(str::to_string),
                    ffmpeg_path: runtime_ffmpeg_path.clone(),
                    direct_url: selected_format
                        .as_ref()
                        .and_then(|format| format.direct_url.clone()),
                    referer: selected_format
                        .as_ref()
                        .and_then(|format| format.referer.clone()),
                    user_agent: selected_format
                        .as_ref()
                        .and_then(|format| format.user_agent.clone()),
                    audio_direct_url: selected_format
                        .as_ref()
                        .and_then(|format| format.audio_direct_url.clone()),
                    audio_referer: selected_format
                        .as_ref()
                        .and_then(|format| format.audio_referer.clone()),
                    audio_user_agent: selected_format
                        .as_ref()
                        .and_then(|format| format.audio_user_agent.clone()),
                    is_retry: false,
                },
            ) {
                skipped_count += 1;
                if first_error.is_none() {
                    first_error = Some(error.clone());
                }
                task_store::upsert_task(
                    &tasks,
                    DownloadTask {
                        id: format!("task-prepare-{}", asset.asset_id),
                        platform: asset.platform.clone(),
                        title: asset.title.clone(),
                        progress: 0,
                        speed_text: "-".to_string(),
                        format_label: selected_format
                            .as_ref()
                            .map(|format| format.label.clone())
                            .unwrap_or_else(|| "准备失败".to_string()),
                        status: "failed".to_string(),
                        eta_text: "失败".to_string(),
                        message: Some(error),
                        output_path: None,
                        supports_pause: false,
                        supports_cancel: false,
                        can_retry: false,
                        cover_url: asset.cover_url.clone(),
                    },
                );
            }
        }

        if skipped_count >= total_requested {
            task_store::upsert_task(
                &tasks,
                DownloadTask {
                    id: format!(
                        "task-batch-summary-{}",
                        parser::sanitize_filename(&profile_title_for_thread)
                    ),
                    platform: platforms::detect_platform(&source_url_for_thread).to_string(),
                    title: profile_title_for_thread,
                    progress: 0,
                    speed_text: "-".to_string(),
                    format_label: "批量入队失败".to_string(),
                    status: "failed".to_string(),
                    eta_text: "失败".to_string(),
                    message: Some(
                        first_error.unwrap_or_else(|| "主页作品批量下载入队失败。".to_string()),
                    ),
                    output_path: None,
                    supports_pause: false,
                    supports_cancel: false,
                    can_retry: false,
                    cover_url: None,
                },
            );
        }
    });

    Ok(BatchDownloadResult {
        profile_title,
        source_url,
        total_available: total_requested,
        fetched_count: total_requested,
        enqueued_count: total_requested,
        skipped_count: 0,
        message: format!(
            "正在将 {} 个作品加入下载队列，你可以先去查看任务进度。",
            total_requested
        ),
    })
}

#[tauri::command]
fn list_download_tasks(state: tauri::State<'_, AppState>) -> Vec<DownloadTask> {
    task_store::list_tasks(&state.tasks)
}

#[tauri::command]
fn pause_download_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DownloadTask, String> {
    ytdlp::pause_task(
        Arc::clone(&state.tasks),
        Arc::clone(&state.controllers),
        &task_id,
    )
}

#[tauri::command]
fn resume_download_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DownloadTask, String> {
    ytdlp::resume_task(
        Arc::clone(&state.tasks),
        Arc::clone(&state.controllers),
        &task_id,
    )
}

#[tauri::command]
fn cancel_download_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DownloadTask, String> {
    ytdlp::cancel_task(
        Arc::clone(&state.tasks),
        Arc::clone(&state.controllers),
        &task_id,
    )
}

#[tauri::command]
fn retry_download_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
) -> Result<DownloadTask, String> {
    let mut replay = task_store::replay_for_task(&state.tasks, &task_id)
        .ok_or_else(|| "当前任务缺少可重试的上下文，请重新解析后再下载。".to_string())?;
    let ffmpeg_path = ensure_ffmpeg_path(
        tooling.ffmpeg_path.as_deref(),
        replay
            .format_id
            .as_deref()
            .is_some_and(|value| value.contains('+'))
            || replay.audio_direct_url.is_some()
            || replay.download_options.download_audio
            || (replay.platform == "youtube" && replay.download_options.download_video),
    )?;

    let settings = state.settings.lock().unwrap().clone();
    let auth = settings::platform_auth_for(&settings.platform_auth, &replay.platform);
    replay.cookie_browser = auth.cookie_browser;
    replay.cookie_file = auth.cookie_file;
    replay.ffmpeg_path = ffmpeg_path;
    replay.is_retry = true;
    ytdlp::download_video(
        Arc::clone(&state.tasks),
        Arc::clone(&state.controllers),
        replay,
    )
}

#[tauri::command]
fn list_browser_sources() -> Result<Vec<auth::BrowserSource>, String> {
    auth::list_browser_sources()
}

#[tauri::command]
async fn import_browser_cookies(
    request: auth::CookieImportRequest,
    state: tauri::State<'_, AppState>,
) -> Result<auth::CookieImportResult, String> {
    let request_for_worker = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        auth::import_browser_cookies(&request_for_worker)
    })
    .await
    .map_err(|error| format!("Cookie 导入任务异常退出：{error}"))??;
    if result.status == "active" && request.consent == "always" {
        let mut guard = state.settings.lock().unwrap();
        let entry = guard
            .platform_auth
            .entry(request.platform.clone())
            .or_default();
        entry.mode = "browser".to_string();
        entry.browser_id = Some(request.browser_id.clone());
        entry.profile_id = request.profile_id.clone();
        entry.consented_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        entry.status = "active".to_string();
        entry.cookie_browser = None;
        entry.cookie_file = auth::cookie_file_for(&request.platform);
        settings::save_settings(&guard)?;
    } else if result.status == "active" {
        let mut guard = state.settings.lock().unwrap();
        let entry = guard
            .platform_auth
            .entry(request.platform.clone())
            .or_default();
        entry.mode = "browser".to_string();
        entry.browser_id = Some(request.browser_id);
        entry.profile_id = request.profile_id;
        entry.status = "active".to_string();
        entry.cookie_file = auth::cookie_file_for(&request.platform);
    }
    Ok(result)
}

#[tauri::command]
fn save_manual_cookies(
    platform: String,
    cookie_text: Option<String>,
    cookie_file: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<auth::CookieImportResult, String> {
    let result = match (cookie_text, cookie_file) {
        (Some(content), None) => auth::save_manual_cookies(&platform, &content)?,
        (None, Some(path)) => auth::import_cookie_file(&platform, Path::new(&path))?,
        _ => return Err("请选择 cookies.txt，或粘贴 Cookie 内容。".to_string()),
    };
    let mut guard = state.settings.lock().unwrap();
    let entry = guard.platform_auth.entry(platform.clone()).or_default();
    entry.mode = "manual".to_string();
    entry.browser_id = None;
    entry.profile_id = None;
    entry.consented_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );
    entry.status = "active".to_string();
    entry.cookie_browser = None;
    entry.cookie_file = auth::cookie_file_for(&platform);
    settings::save_settings(&guard)?;
    Ok(result)
}

#[tauri::command]
fn clear_platform_auth(platform: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    auth::clear_platform_auth(&platform)?;
    let mut guard = state.settings.lock().unwrap();
    guard
        .platform_auth
        .insert(platform, settings::PlatformAuthSettings::default());
    settings::save_settings(&guard)
}

#[tauri::command]
async fn save_settings(
    request: SaveSettingsRequest,
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
) -> Result<SettingsProfile, String> {
    let SaveSettingsRequest {
        save_directory,
        download_mode,
        quality_preference,
        auto_reveal_in_finder,
        max_concurrent_downloads,
        proxy_url,
        speed_limit,
        auto_update,
        theme,
        notify_on_complete,
        language,
    } = request;
    let normalized_directory = settings::normalize_save_directory(save_directory)?;
    let normalized_mode = settings::normalize_download_mode(download_mode)?;
    let normalized_quality = settings::normalize_quality_preference(quality_preference)?;
    let normalized_max_concurrent = settings::normalize_max_concurrent(max_concurrent_downloads);
    let normalized_proxy = settings::normalize_proxy_url(proxy_url)?;
    let normalized_speed_limit = settings::normalize_speed_limit(speed_limit);
    let normalized_theme = settings::normalize_theme(theme);
    let normalized_language = settings::normalize_language(language);

    {
        let mut guard = state.settings.lock().unwrap();
        guard.save_directory = normalized_directory;
        guard.download_mode = normalized_mode;
        guard.quality_preference = normalized_quality;
        guard.auto_reveal_in_finder = auto_reveal_in_finder;
        guard.max_concurrent_downloads = normalized_max_concurrent;
        guard.proxy_url = normalized_proxy.clone();
        guard.speed_limit = normalized_speed_limit.clone();
        guard.auto_update = auto_update;
        guard.theme = normalized_theme;
        guard.notify_on_complete = notify_on_complete;
        guard.language = normalized_language;
        settings::save_settings(&guard)?;
    }

    ytdlp::set_max_concurrent_downloads(normalized_max_concurrent);
    ytdlp::set_network_settings(
        settings::effective_proxy_url(normalized_proxy.as_deref()),
        normalized_speed_limit,
    );

    let profile = {
        let guard = state.settings.lock().unwrap();
        build_settings_profile(&guard, tooling.ffmpeg_path.as_deref())
    };

    Ok(profile)
}

#[tauri::command]
fn pick_save_directory(current_directory: Option<String>) -> Option<String> {
    let mut dialog = FileDialog::new();

    if let Some(current_directory) = current_directory {
        if let Ok(normalized) = settings::normalize_save_directory(current_directory) {
            dialog = dialog.set_directory(normalized);
        }
    }

    dialog
        .pick_folder()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
fn pick_cookie_file(current_file: Option<String>) -> Option<String> {
    let mut dialog = FileDialog::new().add_filter("Cookies", &["txt"]);

    if let Some(current_file) = current_file {
        if let Ok(Some(file)) = settings::normalize_cookie_file(Some(current_file)) {
            if let Some(parent) = PathBuf::from(file).parent() {
                dialog = dialog.set_directory(parent);
            }
        }
    }

    dialog
        .pick_file()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
fn open_in_file_manager(path: String, reveal_parent: bool) -> Result<(), String> {
    ytdlp::open_in_file_manager(&path, reveal_parent)
}

#[tauri::command]
fn clear_finished_tasks(state: tauri::State<'_, AppState>) -> Vec<DownloadTask> {
    task_store::clear_finished(&state.tasks)
}

#[tauri::command]
fn remove_download_task(task_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    task_store::remove_task(&state.tasks, &task_id)
}

#[tauri::command]
fn check_download_history(
    platform: String,
    asset_ids: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Vec<String> {
    let guard = state.history.lock().unwrap();
    guard.check_downloaded(&platform, &asset_ids)
}

#[tauri::command]
fn list_download_history(
    limit: Option<usize>,
    platform: Option<String>,
    _state: tauri::State<'_, AppState>,
) -> Vec<download_history::DownloadHistoryEntry> {
    download_history::list_history(limit.unwrap_or(100), platform)
}

#[tauri::command]
fn search_download_history(
    query: String,
    limit: Option<usize>,
) -> Vec<download_history::DownloadHistoryEntry> {
    download_history::search_history(query, limit.unwrap_or(50))
}

#[tauri::command]
fn get_download_history_count(state: tauri::State<'_, AppState>) -> usize {
    state.history.lock().unwrap().total_count()
}

fn build_bootstrap_state(
    state: &tauri::State<'_, AppState>,
    ffmpeg_path: Option<&str>,
) -> BootstrapState {
    let settings = state.settings.lock().unwrap().clone();
    let platform_auth = build_platform_auth_profiles(&settings.platform_auth);
    let tasks = task_store::list_tasks(&state.tasks);
    let completed = tasks
        .iter()
        .filter(|task| task.status == "completed")
        .count() as u32;
    let failed = tasks.iter().filter(|task| task.status == "failed").count() as u32;
    let finished = completed + failed;
    let success_rate = if finished == 0 {
        "—".to_string()
    } else {
        format!("{:.0}%", (completed as f32 / finished as f32) * 100.0)
    };

    BootstrapState {
        version: env!("CARGO_PKG_VERSION").to_string(),
        auth_state: if settings::has_auth_source(&settings.platform_auth) {
            "active".into()
        } else {
            "guest".into()
        },
        account_label: settings::auth_summary_label(&settings.platform_auth),
        is_windows: cfg!(target_os = "windows"),
        platform_auth,
        save_directory: settings.save_directory,
        download_mode: settings.download_mode,
        quality_preference: settings.quality_preference,
        auto_reveal_in_finder: settings.auto_reveal_in_finder,
        max_concurrent_downloads: settings.max_concurrent_downloads,
        proxy_url: settings.proxy_url,
        speed_limit: settings.speed_limit,
        auto_update: settings.auto_update,
        theme: settings.theme,
        notify_on_complete: settings.notify_on_complete,
        language: settings.language,
        ffmpeg_available: ytdlp::ffmpeg_available(ffmpeg_path),
        metrics: Metrics {
            today_downloads: completed,
            success_rate,
            available_formats: 0,
            max_quality: "等待解析".into(),
        },
        preview: sample_preview(),
        tasks,
    }
}

fn build_settings_profile(
    settings: &settings::AppSettings,
    ffmpeg_path: Option<&str>,
) -> SettingsProfile {
    SettingsProfile {
        version: env!("CARGO_PKG_VERSION").to_string(),
        auth_state: if settings::has_auth_source(&settings.platform_auth) {
            "active".into()
        } else {
            "guest".into()
        },
        account_label: settings::auth_summary_label(&settings.platform_auth),
        platform_auth: build_platform_auth_profiles(&settings.platform_auth),
        save_directory: settings.save_directory.clone(),
        download_mode: settings.download_mode.clone(),
        quality_preference: settings.quality_preference.clone(),
        auto_reveal_in_finder: settings.auto_reveal_in_finder,
        max_concurrent_downloads: settings.max_concurrent_downloads,
        proxy_url: settings.proxy_url.clone(),
        speed_limit: settings.speed_limit.clone(),
        auto_update: settings.auto_update,
        theme: settings.theme.clone(),
        notify_on_complete: settings.notify_on_complete,
        language: settings.language.clone(),
        ffmpeg_available: ytdlp::ffmpeg_available(ffmpeg_path),
    }
}

fn build_platform_auth_profiles(
    platform_auth: &BTreeMap<String, settings::PlatformAuthSettings>,
) -> BTreeMap<String, PlatformAuthProfile> {
    settings::AUTH_PLATFORM_IDS
        .iter()
        .map(|platform| {
            let auth = settings::platform_auth_for(platform_auth, platform);
            let status = if settings::has_platform_auth_source(&auth) {
                "active".to_string()
            } else {
                auth.status.clone()
            };
            (
                (*platform).to_string(),
                PlatformAuthProfile {
                    mode: auth.mode,
                    browser_id: auth.browser_id,
                    profile_id: auth.profile_id,
                    consented_at: auth.consented_at,
                    status,
                },
            )
        })
        .collect()
}

#[tauri::command]
async fn fetch_thumbnail(url: String) -> Result<String, String> {
    let cache_dir = settings::app_data_root().join("thumbnails");
    let url_hash = format!("{:x}", Sha256::digest(format!("v3:{url}").as_bytes()));
    let cache_path = cache_dir.join(&url_hash);
    if let Ok(cached) = fs::read_to_string(&cache_path) {
        if cached.starts_with("data:") {
            return Ok(cached);
        }
    }

    let candidates = provider_runtime::thumbnail_candidates(&url);
    let is_youtube = candidates.iter().any(|candidate| {
        let value = candidate.to_ascii_lowercase();
        value.contains("youtube") || value.contains("ytimg")
    });
    let loaded_settings = settings::load_settings();
    let proxy_url = if is_youtube {
        settings::effective_proxy_url(loaded_settings.proxy_url.as_deref())
    } else {
        None
    };
    let mut client_builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .timeout(std::time::Duration::from_secs(15));
    if let Some(proxy_url) = proxy_url {
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|error| format!("缩略图代理地址无效：{error}"))?;
        client_builder = client_builder.proxy(proxy);
    } else {
        client_builder = client_builder.no_proxy();
    }
    let client = client_builder
        .build()
        .map_err(|error| format!("HTTP 客户端创建失败: {error}"))?;
    let mut last_error = "没有可用的缩略图地址。".to_string();

    for candidate in candidates {
        let mut request = client
            .get(&candidate)
            .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124.0.0.0 Safari/537.36")
            .header(ACCEPT, "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8");
        if let Some(referer) = provider_runtime::thumbnail_referer(&candidate) {
            request = request.header(REFERER, referer);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("请求 {candidate} 失败：{error}");
                continue;
            }
        };
        if !response.status().is_success() {
            last_error = format!("请求 {candidate} 返回 {}", response.status());
            continue;
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();
        if !content_type.to_ascii_lowercase().starts_with("image/") {
            last_error = format!("请求 {candidate} 返回了 {content_type}");
            continue;
        }
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                last_error = format!("读取 {candidate} 失败：{error}");
                continue;
            }
        };
        if bytes.len() > 12 * 1024 * 1024 {
            last_error = format!("缩略图 {candidate} 超过 12 MB 限制。");
            continue;
        }
        let encoded = format!(
            "data:{content_type};base64,{}",
            BASE64_STANDARD.encode(bytes)
        );
        let _ = fs::create_dir_all(&cache_dir);
        let _ = fs::write(&cache_path, &encoded);
        return Ok(encoded);
    }

    Err(format!("缩略图加载失败：{last_error}"))
}

pub(crate) fn run() {
    if auth::run_elevated_cookie_helper_from_args() {
        return;
    }
    let tasks = task_store::load_task_store();
    task_store::normalize_interrupted_tasks(&tasks);
    let history = download_history::load_history_store();
    let loaded_settings = settings::load_settings();

    ytdlp::set_max_concurrent_downloads(loaded_settings.max_concurrent_downloads);
    ytdlp::set_network_settings(
        settings::effective_proxy_url(loaded_settings.proxy_url.as_deref()),
        loaded_settings.speed_limit.clone(),
    );

    let app_state = AppState {
        tasks,
        settings: Arc::new(Mutex::new(loaded_settings)),
        controllers: ytdlp::new_task_controller_store(),
        history,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(app_state)
        .setup(|app| {
            // Capture Tauri's authoritative resource directory as early as possible so
            // every downstream path lookup (including pack subprocesses) can rely on it
            // instead of brittle `current_exe` heuristics or compile-time paths.
            if let Ok(resource_dir) = app.path().resource_dir() {
                provider_runtime::set_resource_root(resource_dir);
            }

            let ffmpeg_path = ytdlp::resolve_ffmpeg_path(app.handle())
                .map(|path| path.to_string_lossy().to_string());
            app.manage(ToolingState { ffmpeg_path });

            let state: tauri::State<'_, AppState> = app.state();
            task_store::set_app_handle(&state.tasks, app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_state,
            get_analysis_progress,
            clear_analysis_progress,
            analyze_input,
            analyze_profile_input,
            create_download_task,
            create_profile_download_tasks,
            list_download_tasks,
            pause_download_task,
            resume_download_task,
            cancel_download_task,
            retry_download_task,
            save_settings,
            list_browser_sources,
            import_browser_cookies,
            save_manual_cookies,
            clear_platform_auth,
            pick_save_directory,
            pick_cookie_file,
            open_in_file_manager,
            clear_finished_tasks,
            remove_download_task,
            list_download_history,
            search_download_history,
            check_download_history,
            get_download_history_count,
            fetch_thumbnail,
            check_for_update,
            open_external_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{fallback_profile_format, sample_preview};
    use crate::provider_runtime::{thumbnail_candidates, thumbnail_referer};

    #[test]
    fn update_check_compares_versions() {
        assert!(super::version_is_newer("1.0.1", "1.0.0"));
        assert!(super::version_is_newer("v2.0", "1.9.9"));
        assert!(!super::version_is_newer("1.0.0", "1.0.0"));
        assert!(!super::version_is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn youtube_batch_items_are_reanalyzed_instead_of_using_best_placeholder() {
        let mut asset = sample_preview();
        asset.platform = "youtube".to_string();
        asset.formats.clear();

        assert!(fallback_profile_format(&asset, None).is_none());

        asset.platform = "bilibili".to_string();
        assert!(fallback_profile_format(&asset, None).is_some());
    }

    #[test]
    fn thumbnail_referer_matches_platform_cdn_hosts() {
        assert_eq!(
            thumbnail_referer("https://p3-sign.douyinpic.com/example.webp"),
            Some("https://www.douyin.com/")
        );
        assert_eq!(
            thumbnail_referer("https://i0.hdslb.com/example.jpg"),
            Some("https://www.bilibili.com/")
        );
        assert_eq!(
            thumbnail_referer("https://i.ytimg.com/example.jpg"),
            Some("https://www.youtube.com/")
        );
    }

    #[test]
    fn youtube_thumbnail_candidates_prefer_jpeg_fallbacks() {
        let candidates =
            thumbnail_candidates("https://i.ytimg.com/vi_webp/SnOckdip_cU/maxresdefault.webp");
        assert_eq!(
            candidates[0],
            "https://i.ytimg.com/vi/SnOckdip_cU/maxresdefault.jpg"
        );
        assert!(candidates
            .iter()
            .any(|candidate| candidate.ends_with("/hqdefault.jpg")));
    }
}
