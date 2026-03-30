#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod douyin;
mod formats;
mod parser;
mod settings;
mod ytdlp;

use rfd::FileDialog;
use serde::Serialize;
use std::sync::{Arc, Mutex};

pub(crate) const DEFAULT_GRADIENT: &str =
    "linear-gradient(135deg, rgba(13, 190, 165, 0.95), rgba(97, 87, 255, 0.8))";

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoAsset {
    pub(crate) aweme_id: String,
    pub(crate) source_url: String,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) duration_seconds: u32,
    pub(crate) publish_date: String,
    pub(crate) caption: String,
    pub(crate) cover_gradient: String,
    pub(crate) formats: Vec<VideoFormat>,
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

#[derive(Clone)]
struct AppState {
    tasks: Arc<Mutex<Vec<DownloadTask>>>,
    settings: Arc<Mutex<settings::AppSettings>>,
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
fn create_download_task(
    state: tauri::State<'_, AppState>,
    aweme_id: String,
    source_url: String,
    title: String,
    format_id: String,
    format_label: String,
    save_directory_override: Option<String>,
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
        &source_url,
        &aweme_id,
        &title,
        &format_id,
        &format_label,
        &save_directory,
        settings.auto_reveal_in_finder,
        settings.cookie_browser.as_deref(),
        direct_url.as_deref(),
        referer.as_deref(),
        user_agent.as_deref(),
    )
}

#[tauri::command]
fn list_download_tasks(state: tauri::State<'_, AppState>) -> Vec<DownloadTask> {
    state.tasks.lock().unwrap().clone()
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
    tasks.retain(|task| task.status != "completed" && task.status != "failed");
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
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_state,
            analyze_input,
            create_download_task,
            list_download_tasks,
            save_settings,
            pick_save_directory,
            open_in_file_manager,
            clear_finished_tasks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
