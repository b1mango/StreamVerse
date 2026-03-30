#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod douyin;
mod formats;
mod parser;
mod settings;
mod ytdlp;

use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub(crate) const DEFAULT_GRADIENT: &str =
    "linear-gradient(135deg, rgba(13, 190, 165, 0.95), rgba(97, 87, 255, 0.8))";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoFormat {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) resolution: String,
    pub(crate) bitrate_kbps: u32,
    pub(crate) codec: String,
    pub(crate) container: String,
    pub(crate) no_watermark: bool,
    pub(crate) requires_login: bool,
    pub(crate) recommended: bool,
    pub(crate) direct_url: Option<String>,
    pub(crate) referer: Option<String>,
    pub(crate) user_agent: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoAsset {
    pub(crate) aweme_id: String,
    pub(crate) source_url: String,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) duration_seconds: u32,
    pub(crate) publish_date: String,
    pub(crate) caption: String,
    pub(crate) cover_url: Option<String>,
    pub(crate) cover_gradient: String,
    pub(crate) formats: Vec<VideoFormat>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileBatch {
    pub(crate) profile_title: String,
    pub(crate) source_url: String,
    pub(crate) total_available: u32,
    pub(crate) fetched_count: u32,
    pub(crate) skipped_count: u32,
    pub(crate) items: Vec<VideoAsset>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadContentSelection {
    pub(crate) download_video: bool,
    pub(crate) download_cover: bool,
    pub(crate) download_caption: bool,
    pub(crate) download_metadata: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadTask {
    pub(crate) id: String,
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
struct BootstrapState {
    auth_state: String,
    account_label: String,
    cookie_browser: Option<String>,
    save_directory: String,
    download_mode: String,
    quality_preference: String,
    auto_reveal_in_finder: bool,
    metrics: Metrics,
    preview: VideoAsset,
    tasks: Vec<DownloadTask>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsProfile {
    auth_state: String,
    account_label: String,
    cookie_browser: Option<String>,
    save_directory: String,
    download_mode: String,
    quality_preference: String,
    auto_reveal_in_finder: bool,
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

#[derive(Clone)]
struct AppState {
    tasks: Arc<Mutex<Vec<DownloadTask>>>,
    settings: Arc<Mutex<settings::AppSettings>>,
    controllers: ytdlp::TaskControllerStore,
}

fn sample_preview() -> VideoAsset {
    VideoAsset {
        aweme_id: "7481035099182375478".into(),
        source_url: "https://v.douyin.com/XXXXXX/".into(),
        title: "春夜街景的风从镜头里吹过".into(),
        author: "镜头笔记".into(),
        duration_seconds: 42,
        publish_date: "2026-03-28".into(),
        caption: "浏览器预览模式下会显示这个占位作品；桌面应用里会改成实时解析结果。".into(),
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
                recommended: true,
                direct_url: None,
                referer: None,
                user_agent: None,
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
                recommended: false,
                direct_url: None,
                referer: None,
                user_agent: None,
            },
        ],
    }
}

#[tauri::command]
fn get_bootstrap_state(state: tauri::State<'_, AppState>) -> BootstrapState {
    build_bootstrap_state(&state)
}

#[tauri::command]
fn analyze_input(
    raw_input: String,
    state: tauri::State<'_, AppState>,
) -> Result<VideoAsset, String> {
    let source_url = parser::extract_first_url(raw_input.trim())
        .ok_or_else(|| "未在输入内容里找到可用链接，请粘贴完整分享文案或作品链接。".to_string())?;

    let cookie_browser = state.settings.lock().unwrap().cookie_browser.clone();
    ytdlp::analyze_url(&source_url, cookie_browser.as_deref())
}

#[tauri::command]
fn analyze_profile_input(
    raw_input: String,
    limit: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Result<ProfileBatch, String> {
    let source_url = parser::extract_first_url(raw_input.trim())
        .ok_or_else(|| "未在输入内容里找到可用主页链接，请粘贴完整主页分享文案或主页链接。".to_string())?;
    let settings = state.settings.lock().unwrap().clone();

    douyin::analyze_profile(
        &source_url,
        settings.cookie_browser.as_deref(),
        limit.unwrap_or(24),
    )
}

#[tauri::command]
fn create_download_task(
    state: tauri::State<'_, AppState>,
    aweme_id: String,
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
) -> Result<DownloadTask, String> {
    let settings = state.settings.lock().unwrap().clone();
    let save_directory = match save_directory_override {
        Some(path) => settings::normalize_save_directory(path)?,
        None => settings.save_directory.clone(),
    };

    ytdlp::download_video(
        Arc::clone(&state.tasks),
        Arc::clone(&state.controllers),
        &source_url,
        &aweme_id,
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
        settings.cookie_browser.as_deref(),
        direct_url.as_deref(),
        referer.as_deref(),
        user_agent.as_deref(),
    )
}

#[tauri::command]
fn create_profile_download_tasks(
    state: tauri::State<'_, AppState>,
    profile_title: String,
    source_url: String,
    items: Vec<VideoAsset>,
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
        && !download_options.download_cover
        && !download_options.download_caption
        && !download_options.download_metadata
    {
        return Err("至少要选择一种要保存的内容。".to_string());
    }

    let mut enqueued_count = 0u32;
    let mut skipped_count = 0u32;
    let mut first_error = None::<String>;

    for asset in &items {
        let selected_format = if download_options.download_video {
            formats::pick_preferred_format(
                &asset.formats,
                &settings.quality_preference,
                settings.cookie_browser.is_some(),
            )
        } else {
            None
        };

        if download_options.download_video && selected_format.is_none() {
            skipped_count += 1;
            continue;
        }

        match ytdlp::download_video(
            Arc::clone(&state.tasks),
            Arc::clone(&state.controllers),
            &asset.source_url,
            &asset.aweme_id,
            &asset.title,
            &asset.author,
            &asset.publish_date,
            &asset.caption,
            asset.cover_url.as_deref(),
            selected_format.as_ref().map(|format| format.id.as_str()),
            selected_format.as_ref().map(|format| format.label.as_str()),
            &save_directory,
            download_options.clone(),
            settings.auto_reveal_in_finder,
            settings.cookie_browser.as_deref(),
            selected_format.as_ref().and_then(|format| format.direct_url.as_deref()),
            selected_format.as_ref().and_then(|format| format.referer.as_deref()),
            selected_format.as_ref().and_then(|format| format.user_agent.as_deref()),
        ) {
            Ok(_) => enqueued_count += 1,
            Err(error) => {
                skipped_count += 1;
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if enqueued_count == 0 {
        return Err(first_error.unwrap_or_else(|| "主页作品批量下载入队失败。".to_string()));
    }

    Ok(BatchDownloadResult {
        profile_title,
        source_url,
        total_available: items.len() as u32,
        fetched_count: items.len() as u32,
        enqueued_count,
        skipped_count,
        message: format!(
            "已将 {} 个作品加入下载队列{}。",
            enqueued_count,
            if skipped_count > 0 {
                format!("，跳过 {} 个", skipped_count)
            } else {
                String::new()
            }
        ),
    })
}

#[tauri::command]
fn list_download_tasks(state: tauri::State<'_, AppState>) -> Vec<DownloadTask> {
    state.tasks.lock().unwrap().clone()
}

#[tauri::command]
fn pause_download_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DownloadTask, String> {
    ytdlp::pause_task(Arc::clone(&state.tasks), Arc::clone(&state.controllers), &task_id)
}

#[tauri::command]
fn resume_download_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DownloadTask, String> {
    ytdlp::resume_task(Arc::clone(&state.tasks), Arc::clone(&state.controllers), &task_id)
}

#[tauri::command]
fn cancel_download_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DownloadTask, String> {
    ytdlp::cancel_task(Arc::clone(&state.tasks), Arc::clone(&state.controllers), &task_id)
}

#[tauri::command]
fn save_settings(
    cookie_browser: Option<String>,
    save_directory: String,
    download_mode: String,
    quality_preference: String,
    auto_reveal_in_finder: bool,
    state: tauri::State<'_, AppState>,
) -> Result<SettingsProfile, String> {
    let normalized_browser = settings::normalize_cookie_browser(cookie_browser)?;
    let normalized_directory = settings::normalize_save_directory(save_directory)?;
    let normalized_mode = settings::normalize_download_mode(download_mode)?;
    let normalized_quality = settings::normalize_quality_preference(quality_preference)?;

    {
        let mut guard = state.settings.lock().unwrap();
        guard.cookie_browser = normalized_browser;
        guard.save_directory = normalized_directory;
        guard.download_mode = normalized_mode;
        guard.quality_preference = normalized_quality;
        guard.auto_reveal_in_finder = auto_reveal_in_finder;
        settings::save_settings(&guard)?;
    }

    Ok(build_settings_profile(&state.settings.lock().unwrap()))
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
fn open_in_file_manager(path: String, reveal_parent: bool) -> Result<(), String> {
    ytdlp::open_in_file_manager(&path, reveal_parent)
}

#[tauri::command]
fn clear_finished_tasks(state: tauri::State<'_, AppState>) -> Vec<DownloadTask> {
    let mut tasks = state.tasks.lock().unwrap();
    tasks.retain(|task| {
        task.status != "completed" && task.status != "failed" && task.status != "cancelled"
    });
    tasks.clone()
}

fn build_bootstrap_state(state: &tauri::State<'_, AppState>) -> BootstrapState {
    let settings = state.settings.lock().unwrap().clone();
    let tasks = state.tasks.lock().unwrap().clone();
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
        auth_state: if settings.cookie_browser.is_some() {
            "active".into()
        } else {
            "guest".into()
        },
        account_label: settings::cookie_browser_label(settings.cookie_browser.as_deref()),
        cookie_browser: settings.cookie_browser,
        save_directory: settings.save_directory,
        download_mode: settings.download_mode,
        quality_preference: settings.quality_preference,
        auto_reveal_in_finder: settings.auto_reveal_in_finder,
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

fn build_settings_profile(settings: &settings::AppSettings) -> SettingsProfile {
    SettingsProfile {
        auth_state: if settings.cookie_browser.is_some() {
            "active".into()
        } else {
            "guest".into()
        },
        account_label: settings::cookie_browser_label(settings.cookie_browser.as_deref()),
        cookie_browser: settings.cookie_browser.clone(),
        save_directory: settings.save_directory.clone(),
        download_mode: settings.download_mode.clone(),
        quality_preference: settings.quality_preference.clone(),
        auto_reveal_in_finder: settings.auto_reveal_in_finder,
    }
}

fn main() {
    let app_state = AppState {
        tasks: Arc::new(Mutex::new(Vec::new())),
        settings: Arc::new(Mutex::new(settings::load_settings())),
        controllers: ytdlp::new_task_controller_store(),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_state,
            analyze_input,
            analyze_profile_input,
            create_download_task,
            create_profile_download_tasks,
            list_download_tasks,
            pause_download_task,
            resume_download_task,
            cancel_download_task,
            save_settings,
            pick_save_directory,
            open_in_file_manager,
            clear_finished_tasks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
