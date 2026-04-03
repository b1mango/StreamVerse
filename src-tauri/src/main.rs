#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod download_history;
mod formats;
mod media_contract;
mod pack_host;
mod pack_manager;
mod pack_registry;
mod parser;
mod platforms;
mod providers;
mod settings;
mod task_store;
mod ytdlp;

pub(crate) use media_contract::{
    BatchItemSelection, BrowserLaunchResult, DownloadContentSelection, ProfileBatch, VideoAsset,
    VideoFormat, DEFAULT_GRADIENT,
};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
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
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleRuntimeState {
    pub(crate) id: String,
    pub(crate) installed: bool,
    pub(crate) enabled: bool,
    pub(crate) pack_id: Option<String>,
    pub(crate) current_version: Option<String>,
    pub(crate) latest_version: Option<String>,
    pub(crate) size_bytes: Option<u64>,
    pub(crate) source_kind: Option<String>,
    pub(crate) update_available: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskReplayRequest {
    pub(crate) platform: String,
    pub(crate) source_url: String,
    pub(crate) asset_id: String,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) publish_date: String,
    pub(crate) caption: String,
    pub(crate) cover_url: Option<String>,
    pub(crate) format_id: Option<String>,
    pub(crate) format_label: Option<String>,
    pub(crate) save_directory: String,
    pub(crate) download_options: DownloadContentSelection,
    pub(crate) auto_reveal_in_file_manager: bool,
    pub(crate) cookie_browser: Option<String>,
    pub(crate) cookie_file: Option<String>,
    pub(crate) direct_url: Option<String>,
    pub(crate) referer: Option<String>,
    pub(crate) user_agent: Option<String>,
    pub(crate) audio_direct_url: Option<String>,
    pub(crate) audio_referer: Option<String>,
    pub(crate) audio_user_agent: Option<String>,
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
    auth_state: String,
    account_label: String,
    cookie_browser: Option<String>,
    cookie_file: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PlatformAuthInput {
    cookie_browser: Option<String>,
    cookie_file: Option<String>,
    cookie_text: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapState {
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
    modules: Vec<ModuleRuntimeState>,
    preview: VideoAsset,
    tasks: Vec<DownloadTask>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsProfile {
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
    PathBuf::from(settings::home_dir())
        .join(".streamverse")
        .join("analysis-progress")
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

    Ok(pack_manager::ensure_media_engine_installed()?
        .and_then(|path| path.to_str().map(|value| value.to_string()))
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
        cover_gradient: DEFAULT_GRADIENT.into(),
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

#[tauri::command]
fn get_bootstrap_state(
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
) -> BootstrapState {
    build_bootstrap_state(&state, tooling.ffmpeg_path.as_deref())
}

#[tauri::command]
fn analyze_input(
    raw_input: String,
    session_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<VideoAsset, String> {
    let settings = state.settings.lock().unwrap().clone();
    let progress_file = session_id.as_deref().map(analysis_progress_path);
    let _ = write_analysis_progress(session_id.as_deref(), 0, 1, "正在解析作品链接…");
    let result = providers::analyze_input(&raw_input, &settings.platform_auth, progress_file.as_deref());
    match &result {
        Ok(_) => {
            let _ = write_analysis_progress(session_id.as_deref(), 1, 1, "作品解析完成。");
        }
        Err(error) => {
            let _ = write_analysis_progress(session_id.as_deref(), 0, 1, error);
        }
    }
    result
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
    let progress_file = session_id.as_deref().map(analysis_progress_path);
    let _ = write_analysis_progress(session_id.as_deref(), 0, 0, "正在读取主页视频…");

    tauri::async_runtime::spawn_blocking(move || {
        let result = providers::analyze_profile_input(
            &raw_input,
            &settings.platform_auth,
            progress_file.as_deref(),
        );
        match &result {
            Ok(batch) => {
                let _ = write_analysis_progress(
                    sid.as_deref(),
                    batch.fetched_count,
                    batch.total_available,
                    "主页视频解析完成。",
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
fn open_profile_browser(
    raw_input: String,
    state: tauri::State<'_, AppState>,
) -> Result<BrowserLaunchResult, String> {
    let settings = state.settings.lock().unwrap().clone();
    providers::open_profile_browser(&raw_input, &settings.platform_auth)
}

#[tauri::command]
async fn collect_profile_browser(
    raw_input: String,
    port: u16,
    session_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ProfileBatch, String> {
    let settings = state.settings.lock().unwrap().clone();
    let sid = session_id.clone();
    let progress_file = session_id.as_deref().map(analysis_progress_path);
    let _ = write_analysis_progress(session_id.as_deref(), 0, 0, "正在连接浏览器并读取作品…");

    tauri::async_runtime::spawn_blocking(move || {
        let result = providers::collect_profile_browser(
            &raw_input,
            port,
            &settings.platform_auth,
            progress_file.as_deref(),
        );
        match &result {
            Ok(batch) => {
                let _ = write_analysis_progress(
                    sid.as_deref(),
                    batch.fetched_count,
                    batch.total_available,
                    "主页视频解析完成。",
                );
            }
            Err(error) => {
                let _ = write_analysis_progress(sid.as_deref(), 0, 0, error);
            }
        }
        result
    })
    .await
    .map_err(|_| "浏览器读取线程异常退出".to_string())?
}

#[tauri::command]
fn create_download_task(
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
    asset_id: String,
    platform: String,
    source_url: String,
    title: String,
    author: String,
    publish_date: String,
    caption: String,
    cover_url: Option<String>,
    format_id: Option<String>,
    format_label: Option<String>,
    save_directory_override: Option<String>,
    download_options: DownloadContentSelection,
    direct_url: Option<String>,
    referer: Option<String>,
    user_agent: Option<String>,
    audio_direct_url: Option<String>,
    audio_referer: Option<String>,
    audio_user_agent: Option<String>,
) -> Result<DownloadTask, String> {
    let settings = state.settings.lock().unwrap().clone();
    let save_directory = match save_directory_override {
        Some(path) => settings::normalize_save_directory(path)?,
        None => settings.save_directory.clone(),
    };
    let ffmpeg_path = ensure_ffmpeg_path(
        tooling.ffmpeg_path.as_deref(),
        format_id
            .as_deref()
            .is_some_and(|value| value.contains('+'))
            || audio_direct_url.is_some()
            || download_options.download_audio,
    )?;
    let auth = settings::platform_auth_for(&settings.platform_auth, &platform);
    let cookie_browser = auth.cookie_browser.as_deref();
    let cookie_file = auth.cookie_file.as_deref();

    ytdlp::download_video(
        Arc::clone(&state.tasks),
        Arc::clone(&state.controllers),
        &platform,
        &source_url,
        &asset_id,
        &title,
        &author,
        &publish_date,
        &caption,
        cover_url.as_deref(),
        format_id.as_deref(),
        format_label.as_deref(),
        &save_directory,
        download_options,
        settings.auto_reveal_in_finder,
        ffmpeg_path.as_deref(),
        cookie_browser,
        cookie_file,
        direct_url.as_deref(),
        referer.as_deref(),
        user_agent.as_deref(),
        audio_direct_url.as_deref(),
        audio_referer.as_deref(),
        audio_user_agent.as_deref(),
    )
}

#[tauri::command]
fn create_profile_download_tasks(
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
    profile_title: String,
    source_url: String,
    items: Vec<BatchItemSelection>,
    session_cookie_file: Option<String>,
    save_directory_override: Option<String>,
    download_options: DownloadContentSelection,
) -> Result<BatchDownloadResult, String> {
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
        && !download_options.download_metadata
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
    let auto_reveal_in_finder = settings.auto_reveal_in_finder;
    let save_directory_for_thread = save_directory.clone();
    let profile_title_for_thread = profile_title.clone();
    let source_url_for_thread = source_url.clone();

    thread::spawn(move || {
        let mut skipped_count = 0u32;
        let mut first_error = None::<String>;
        let mut runtime_ffmpeg_path = ffmpeg_path.clone();
        let batch_cookie_file = session_cookie_file
            .as_deref()
            .or(default_cookie_file.as_deref());
        let has_auth = cookie_browser.is_some() || batch_cookie_file.is_some();

        for item in items {
            let fallback_format = if download_options.download_video {
                fallback_profile_format(&item.asset, item.selected_format_id.as_deref())
            } else {
                None
            };

            let resolved_asset = if download_options.download_video
                && item.asset.formats.is_empty()
                && fallback_format.is_none()
            {
                match pack_host::analyze_single(
                    &item.asset.source_url,
                    cookie_browser.as_deref(),
                    batch_cookie_file,
                    None,
                ) {
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
                            },
                        );
                        continue;
                    }
                }
            } else {
                item.asset.clone()
            };
            let asset = &resolved_asset;
            let selected_format = if download_options.download_video {
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

            if download_options.download_video && selected_format.is_none() {
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
                    },
                );
                continue;
            }

            if (selected_format
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
                            },
                        );
                        continue;
                    }
                }
            }

            if let Err(error) = ytdlp::download_video(
                Arc::clone(&tasks),
                Arc::clone(&controllers),
                &asset.platform,
                &asset.source_url,
                &asset.asset_id,
                &asset.title,
                &asset.author,
                &asset.publish_date,
                &asset.caption,
                asset.cover_url.as_deref(),
                selected_format.as_ref().map(|format| format.id.as_str()),
                selected_format.as_ref().map(|format| format.label.as_str()),
                &save_directory_for_thread,
                download_options.clone(),
                auto_reveal_in_finder,
                runtime_ffmpeg_path.as_deref(),
                cookie_browser.as_deref(),
                batch_cookie_file,
                selected_format
                    .as_ref()
                    .and_then(|format| format.direct_url.as_deref()),
                selected_format
                    .as_ref()
                    .and_then(|format| format.referer.as_deref()),
                selected_format
                    .as_ref()
                    .and_then(|format| format.user_agent.as_deref()),
                selected_format
                    .as_ref()
                    .and_then(|format| format.audio_direct_url.as_deref()),
                selected_format
                    .as_ref()
                    .and_then(|format| format.audio_referer.as_deref()),
                selected_format
                    .as_ref()
                    .and_then(|format| format.audio_user_agent.as_deref()),
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
    let replay = task_store::replay_for_task(&state.tasks, &task_id)
        .ok_or_else(|| "当前任务缺少可重试的上下文，请重新解析后再下载。".to_string())?;
    let ffmpeg_path = ensure_ffmpeg_path(
        tooling.ffmpeg_path.as_deref(),
        replay
            .format_id
            .as_deref()
            .is_some_and(|value| value.contains('+'))
            || replay.audio_direct_url.is_some()
            || replay.download_options.download_audio,
    )?;

    ytdlp::download_video(
        Arc::clone(&state.tasks),
        Arc::clone(&state.controllers),
        &replay.platform,
        &replay.source_url,
        &replay.asset_id,
        &replay.title,
        &replay.author,
        &replay.publish_date,
        &replay.caption,
        replay.cover_url.as_deref(),
        replay.format_id.as_deref(),
        replay.format_label.as_deref(),
        &replay.save_directory,
        replay.download_options,
        replay.auto_reveal_in_file_manager,
        ffmpeg_path.as_deref(),
        replay.cookie_browser.as_deref(),
        replay.cookie_file.as_deref(),
        replay.direct_url.as_deref(),
        replay.referer.as_deref(),
        replay.user_agent.as_deref(),
        replay.audio_direct_url.as_deref(),
        replay.audio_referer.as_deref(),
        replay.audio_user_agent.as_deref(),
    )
}

#[tauri::command]
async fn detect_browser_cookies(platform: String, browser: String) -> Result<String, String> {
    let _ = settings::normalize_auth_platform_id(&platform)?;
    let _ = settings::normalize_cookie_browser(Some(browser.clone()))?;

    // Extract cookies from the selected browser using yt-dlp --cookies-from-browser
    // No need to open the platform URL — yt-dlp reads the browser's cookie database directly
    let cookie_file = tauri::async_runtime::spawn_blocking(move || {
        ytdlp::extract_browser_cookies(&browser, &platform)
    })
    .await
    .map_err(|e| format!("任务执行失败：{e}"))??;

    Ok(cookie_file)
}

#[tauri::command]
async fn save_settings(
    platform_auth: BTreeMap<String, PlatformAuthInput>,
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
    state: tauri::State<'_, AppState>,
    tooling: tauri::State<'_, ToolingState>,
) -> Result<SettingsProfile, String> {
    let mut normalized_platform_auth = BTreeMap::new();
    for platform in settings::AUTH_PLATFORM_IDS {
        let input = platform_auth.get(platform).cloned().unwrap_or_default();
        let cookie_browser = settings::normalize_cookie_browser(input.cookie_browser)?;
        let cookie_text = settings::normalize_cookie_text(input.cookie_text);
        let cookie_file = if let Some(cookie_text) = cookie_text.as_deref() {
            Some(settings::import_cookie_text(platform, cookie_text)?)
        } else {
            settings::normalize_cookie_file(input.cookie_file)?
        };
        normalized_platform_auth.insert(
            platform.to_string(),
            settings::PlatformAuthSettings {
                cookie_browser,
                cookie_file,
            },
        );
    }
    let normalized_directory = settings::normalize_save_directory(save_directory)?;
    let normalized_mode = settings::normalize_download_mode(download_mode)?;
    let normalized_quality = settings::normalize_quality_preference(quality_preference)?;
    let normalized_max_concurrent = settings::normalize_max_concurrent(max_concurrent_downloads);
    let normalized_proxy = settings::normalize_proxy_url(proxy_url);
    let normalized_speed_limit = settings::normalize_speed_limit(speed_limit);
    let normalized_theme = settings::normalize_theme(theme);
    let normalized_language = settings::normalize_language(language);

    {
        let mut guard = state.settings.lock().unwrap();
        guard.cookie_browser = None;
        guard.cookie_file = None;
        guard.platform_auth = normalized_platform_auth;
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
    ytdlp::set_network_settings(normalized_proxy, normalized_speed_limit);

    let profile = {
        let guard = state.settings.lock().unwrap();
        build_settings_profile(&guard, tooling.ffmpeg_path.as_deref())
    };

    Ok(profile)
}

#[tauri::command]
fn set_module_enabled(
    module_id: String,
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ModuleRuntimeState>, String> {
    let normalized_id = settings::normalize_module_id(&module_id)?.to_string();

    let modules = {
        let mut guard = state.settings.lock().unwrap();
        pack_manager::refresh_installed_state(&mut guard);
        let installed = pack_manager::is_module_installed(&normalized_id);
        if enabled && !installed {
            return Err("请先安装这个模块。".to_string());
        }
        let module = guard.modules.entry(normalized_id).or_default();
        module.installed = installed;
        module.enabled = installed && enabled;
        settings::save_settings(&guard)?;
        build_module_runtime_states(&guard)
    };

    Ok(modules)
}

#[tauri::command]
fn install_module_pack(
    module_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ModuleRuntimeState>, String> {
    let modules = {
        let mut guard = state.settings.lock().unwrap();
        pack_manager::install_pack_for_module(&module_id, &mut guard)?;
        pack_manager::refresh_installed_state(&mut guard);
        settings::save_settings(&guard)?;
        build_module_runtime_states(&guard)
    };

    Ok(modules)
}

#[tauri::command]
fn uninstall_module_pack(
    module_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ModuleRuntimeState>, String> {
    let modules = {
        let mut guard = state.settings.lock().unwrap();
        pack_manager::uninstall_pack_for_module(&module_id, &mut guard)?;
        pack_manager::refresh_installed_state(&mut guard);
        settings::save_settings(&guard)?;
        build_module_runtime_states(&guard)
    };

    Ok(modules)
}

#[tauri::command]
fn update_module_pack(
    module_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ModuleRuntimeState>, String> {
    let modules = {
        let mut guard = state.settings.lock().unwrap();
        pack_manager::update_pack_for_module(&module_id, &mut guard)?;
        pack_manager::refresh_installed_state(&mut guard);
        settings::save_settings(&guard)?;
        build_module_runtime_states(&guard)
    };

    Ok(modules)
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
fn install_download_engine() -> Result<(), String> {
    pack_manager::ensure_download_engine_installed().map(|_| ())
}

#[tauri::command]
fn clear_finished_tasks(state: tauri::State<'_, AppState>) -> Vec<DownloadTask> {
    task_store::clear_finished(&state.tasks)
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
fn get_download_history_count(state: tauri::State<'_, AppState>) -> usize {
    state.history.lock().unwrap().total_count()
}

fn build_bootstrap_state(
    state: &tauri::State<'_, AppState>,
    ffmpeg_path: Option<&str>,
) -> BootstrapState {
    let settings = state.settings.lock().unwrap().clone();
    let platform_auth = build_platform_auth_profiles(&settings.platform_auth);
    let modules = build_module_runtime_states(&settings);
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
        modules,
        preview: sample_preview(),
        tasks,
    }
}

fn build_module_runtime_states(settings: &settings::AppSettings) -> Vec<ModuleRuntimeState> {
    settings::MODULE_IDS
        .iter()
        .filter_map(|id| {
            settings.modules.get(*id).map(|_module| {
                let installed = pack_manager::is_module_installed(id);
                let pack_info = pack_manager::module_runtime_info(id);
                ModuleRuntimeState {
                    id: (*id).to_string(),
                    installed,
                    enabled: installed,
                    pack_id: pack_info.as_ref().map(|info| info.pack_id.clone()),
                    current_version: pack_info
                        .as_ref()
                        .and_then(|info| info.current_version.clone()),
                    latest_version: pack_info
                        .as_ref()
                        .and_then(|info| info.latest_version.clone()),
                    size_bytes: pack_info.as_ref().and_then(|info| info.size_bytes),
                    source_kind: pack_info.as_ref().map(|info| info.source_kind.clone()),
                    update_available: pack_info
                        .as_ref()
                        .map(|info| info.update_available)
                        .unwrap_or(false),
                }
            })
        })
        .collect()
}

fn build_settings_profile(
    settings: &settings::AppSettings,
    ffmpeg_path: Option<&str>,
) -> SettingsProfile {
    SettingsProfile {
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
            (
                (*platform).to_string(),
                PlatformAuthProfile {
                    auth_state: if settings::has_platform_auth_source(&auth) {
                        "active".to_string()
                    } else {
                        "guest".to_string()
                    },
                    account_label: settings::auth_source_label(&auth),
                    cookie_browser: auth.cookie_browser,
                    cookie_file: auth.cookie_file,
                },
            )
        })
        .collect()
}

#[tauri::command]
async fn fetch_thumbnail(url: String) -> Result<String, String> {
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败: {e}"))?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("缩略图请求失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("缩略图请求返回 {}", response.status()));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取缩略图失败: {e}"))?;

    let mut encoded = String::from("data:");
    encoded.push_str(&content_type);
    encoded.push_str(";base64,");

    const CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut buf = Vec::with_capacity(bytes.len() * 4 / 3 + 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        buf.push(CHARS[((triple >> 18) & 0x3F) as usize]);
        buf.push(CHARS[((triple >> 12) & 0x3F) as usize]);
        if chunk.len() > 1 {
            buf.push(CHARS[((triple >> 6) & 0x3F) as usize]);
        } else {
            buf.push(b'=');
        }
        if chunk.len() > 2 {
            buf.push(CHARS[(triple & 0x3F) as usize]);
        } else {
            buf.push(b'=');
        }
    }
    encoded.push_str(&String::from_utf8_lossy(&buf));

    Ok(encoded)
}

fn main() {
    let tasks = task_store::load_task_store();
    task_store::normalize_interrupted_tasks(&tasks);
    let history = download_history::load_history_store();
    let mut loaded_settings = settings::load_settings();
    if pack_manager::refresh_installed_state(&mut loaded_settings) {
        let _ = settings::save_settings(&loaded_settings);
    }

    ytdlp::set_max_concurrent_downloads(loaded_settings.max_concurrent_downloads);
    ytdlp::set_network_settings(
        loaded_settings.proxy_url.clone(),
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
            open_profile_browser,
            collect_profile_browser,
            create_download_task,
            create_profile_download_tasks,
            list_download_tasks,
            pause_download_task,
            resume_download_task,
            cancel_download_task,
            retry_download_task,
            save_settings,
            detect_browser_cookies,
            set_module_enabled,
            install_module_pack,
            uninstall_module_pack,
            update_module_pack,
            pick_save_directory,
            pick_cookie_file,
            open_in_file_manager,
            install_download_engine,
            clear_finished_tasks,
            check_download_history,
            get_download_history_count,
            fetch_thumbnail
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
