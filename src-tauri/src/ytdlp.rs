#[cfg(test)]
use crate::VideoFormat;
use crate::{
    download_history, pack_manager, parser, platforms, settings, task_store,
    DownloadContentSelection, DownloadTask, TaskReplayRequest,
};
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_TYPE, REFERER, USER_AGENT};
use reqwest::Proxy;
#[cfg(test)]
use serde::Deserialize;
use serde::Serialize;
#[cfg(test)]
use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime};

fn silent_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd
}

const YOUTUBE_EXTRACTOR_ARGS: &str = "youtube:player_client=android_vr;player_skip=configs";

static PREFERRED_EXTERNAL_YTDLP_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static PREFERRED_JS_RUNTIME: OnceLock<Option<String>> = OnceLock::new();
static DOWNLOAD_SEMAPHORE: OnceLock<Arc<(Mutex<u32>, std::sync::Condvar)>> = OnceLock::new();
static MAX_CONCURRENT: OnceLock<AtomicU64> = OnceLock::new();
static NETWORK_PROXY: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static NETWORK_SPEED_LIMIT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[derive(Clone)]
struct DownloadArtifacts {
    platform: String,
    source_url: String,
    asset_id: String,
    title: String,
    author: String,
    publish_date: String,
    caption: String,
    cover_url: Option<String>,
    referer: Option<String>,
    user_agent: Option<String>,
}

#[derive(Default)]
struct ArtifactSummary {
    video_written: bool,
    audio_written: bool,
    text_written: bool,
    metadata_written: bool,
    cover_written: bool,
    output_path: Option<String>,
    destination_path: String,
    warnings: Vec<String>,
}

#[derive(Clone)]
struct OutputLayout {
    base_dir: PathBuf,
    bundle_dir: Option<PathBuf>,
    single_stem: String,
}

#[derive(Clone)]
pub struct TaskController {
    supports_pause: bool,
    supports_cancel: bool,
    pause_requested: Arc<AtomicBool>,
    cancel_requested: Arc<AtomicBool>,
}

pub type TaskControllerStore = Arc<Mutex<HashMap<String, Arc<TaskController>>>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MetadataSidecar<'a> {
    asset_id: &'a str,
    platform: &'a str,
    title: &'a str,
    author: &'a str,
    publish_date: &'a str,
    caption: &'a str,
    source_url: &'a str,
    format_label: &'a str,
    cover_url: Option<&'a str>,
    generated_by: &'static str,
}

#[derive(Clone, Copy)]
enum StreamKind {
    Video,
    Audio,
}

enum StreamWorkerOutcome {
    Completed,
    Cancelled,
    Failed(String),
}

#[cfg(test)]
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
    url: Option<String>,
    protocol: Option<String>,
    http_headers: Option<RawHeaders>,
}

#[cfg(test)]
#[derive(Clone, Default, Deserialize)]
struct RawHeaders {
    #[serde(rename = "Referer")]
    referer: Option<String>,
    #[serde(rename = "User-Agent")]
    user_agent: Option<String>,
}

impl DownloadContentSelection {
    fn has_any_selection(&self) -> bool {
        self.download_video
            || self.download_audio
            || self.download_cover
            || self.download_caption
            || self.download_metadata
    }

    fn selected_count(&self) -> usize {
        usize::from(self.download_video)
            + usize::from(self.download_audio)
            + usize::from(self.download_cover)
            + usize::from(self.download_caption)
            + usize::from(self.download_metadata)
    }

    fn needs_bundle_directory(&self) -> bool {
        self.selected_count() > 1
    }

    fn selected_labels(&self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.download_video {
            labels.push("视频");
        }
        if self.download_audio {
            labels.push("音频");
        }
        if self.download_cover {
            labels.push("封面");
        }
        if self.download_caption {
            labels.push("文案");
        }
        if self.download_metadata {
            labels.push("元数据");
        }
        labels
    }
}

impl TaskController {
    fn new(supports_pause: bool, supports_cancel: bool) -> Self {
        Self {
            supports_pause,
            supports_cancel,
            pause_requested: Arc::new(AtomicBool::new(false)),
            cancel_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    fn is_pause_requested(&self) -> bool {
        self.pause_requested.load(Ordering::Relaxed)
    }

    fn is_cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Relaxed)
    }

    fn request_pause(&self) {
        if self.supports_pause {
            self.pause_requested.store(true, Ordering::Relaxed);
        }
    }

    fn request_resume(&self) {
        self.pause_requested.store(false, Ordering::Relaxed);
    }

    fn request_cancel(&self) {
        if self.supports_cancel {
            self.cancel_requested.store(true, Ordering::Relaxed);
        }
    }
}

impl OutputLayout {
    fn asset_root(&self) -> &Path {
        self.bundle_dir.as_deref().unwrap_or(&self.base_dir)
    }

    fn destination_path(&self) -> String {
        self.asset_root().to_string_lossy().to_string()
    }

    fn bundle_entry_path(&self) -> Option<String> {
        self.bundle_dir
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
    }

    fn video_path(&self, extension: &str) -> PathBuf {
        let file_name = if self.bundle_dir.is_some() {
            format!("video.{extension}")
        } else {
            format!("{}.{extension}", self.single_stem)
        };
        unique_output_path(self.asset_root().join(file_name))
    }

    fn caption_path(&self) -> PathBuf {
        if self.bundle_dir.is_some() {
            self.asset_root().join("caption.txt")
        } else {
            unique_output_path(self.asset_root().join(format!("{}.txt", self.single_stem)))
        }
    }

    fn metadata_path(&self) -> PathBuf {
        if self.bundle_dir.is_some() {
            self.asset_root().join("metadata.json")
        } else {
            unique_output_path(self.asset_root().join(format!("{}.json", self.single_stem)))
        }
    }

    fn cover_path(&self, extension: &str) -> PathBuf {
        let file_name = if self.bundle_dir.is_some() {
            format!("cover.{extension}")
        } else {
            format!("{} cover.{extension}", self.single_stem)
        };
        unique_output_path(self.asset_root().join(file_name))
    }

    fn audio_path(&self) -> PathBuf {
        let file_name = if self.bundle_dir.is_some() {
            "audio.mp3".to_string()
        } else {
            format!("{}.mp3", self.single_stem)
        };
        unique_output_path(self.asset_root().join(file_name))
    }
}

pub fn new_task_controller_store() -> TaskControllerStore {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn set_max_concurrent_downloads(max: u32) {
    let max = max.max(1).min(10) as u64;
    MAX_CONCURRENT
        .get_or_init(|| AtomicU64::new(max))
        .store(max, Ordering::Relaxed);
    let _ = DOWNLOAD_SEMAPHORE.get_or_init(|| Arc::new((Mutex::new(0), std::sync::Condvar::new())));
}

pub fn set_network_settings(proxy: Option<String>, speed_limit: Option<String>) {
    *NETWORK_PROXY
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = proxy;
    *NETWORK_SPEED_LIMIT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = speed_limit;
}

fn current_proxy() -> Option<String> {
    NETWORK_PROXY
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|g| g.clone())
}

fn current_proxy_for_platform(platform: &str) -> Option<String> {
    if platform == "youtube" {
        current_proxy()
    } else {
        None
    }
}

fn current_speed_limit() -> Option<String> {
    NETWORK_SPEED_LIMIT
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|g| g.clone())
}

/// Parse a speed limit string like "500K" or "10M" into bytes per second.
fn parse_speed_limit_bytes(limit: Option<&str>) -> Option<u64> {
    let limit = limit?.trim();
    if limit.is_empty() {
        return None;
    }
    let (num_part, unit) = if limit.ends_with('M') || limit.ends_with('m') {
        (&limit[..limit.len() - 1], 1024u64 * 1024)
    } else if limit.ends_with('K') || limit.ends_with('k') {
        (&limit[..limit.len() - 1], 1024u64)
    } else {
        (limit, 1u64) // raw bytes
    };
    let value: f64 = num_part.parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let bytes = (value * unit as f64).round();
    if bytes <= 0.0 {
        return None;
    }
    Some(bytes as u64)
}

/// Sleep to enforce speed limit. Returns the updated window state.
fn throttle_transfer(
    window_start: Instant,
    window_bytes: u64,
    bytes_per_sec: u64,
) -> (Instant, u64) {
    let elapsed = window_start.elapsed();
    let expected = Duration::from_secs_f64(window_bytes as f64 / bytes_per_sec as f64);
    if expected > elapsed {
        thread::sleep(expected - elapsed);
    }
    // Reset window every second to avoid drift
    if window_start.elapsed() >= Duration::from_secs(1) {
        (Instant::now(), 0)
    } else {
        (window_start, window_bytes)
    }
}

fn acquire_download_slot() {
    let max = MAX_CONCURRENT
        .get()
        .map(|v| v.load(Ordering::Relaxed) as u32)
        .unwrap_or(3);
    if let Some(sem) = DOWNLOAD_SEMAPHORE.get() {
        let (lock, cvar) = sem.as_ref();
        let mut active = lock.lock().unwrap();
        while *active >= max {
            active = cvar.wait(active).unwrap();
        }
        *active += 1;
    }
}

fn release_download_slot() {
    if let Some(sem) = DOWNLOAD_SEMAPHORE.get() {
        let (lock, cvar) = sem.as_ref();
        let mut active = lock.lock().unwrap();
        *active = active.saturating_sub(1);
        cvar.notify_one();
    }
}

pub fn download_video(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    platform: &str,
    source_url: &str,
    asset_id: &str,
    title: &str,
    author: &str,
    publish_date: &str,
    caption: &str,
    cover_url: Option<&str>,
    format_id: Option<&str>,
    format_label: Option<&str>,
    save_directory: &str,
    download_options: DownloadContentSelection,
    auto_reveal_in_file_manager: bool,
    ffmpeg_path: Option<&str>,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    direct_url: Option<&str>,
    referer: Option<&str>,
    user_agent: Option<&str>,
    audio_direct_url: Option<&str>,
    audio_referer: Option<&str>,
    audio_user_agent: Option<&str>,
) -> Result<DownloadTask, String> {
    if !download_options.has_any_selection() {
        return Err("至少要选择一种要保存的内容。".to_string());
    }

    if download_options.download_audio && !download_options.download_video {
        return Err("提取 MP3 音频需要同时勾选视频下载。".to_string());
    }

    let output_dir = PathBuf::from(save_directory);
    fs::create_dir_all(&output_dir).map_err(|error| format!("创建下载目录失败：{error}"))?;

    let safe_title = parser::sanitize_filename(title);
    let output_layout =
        prepare_output_layout(&output_dir, &safe_title, asset_id, &download_options)?;
    let supports_pause = download_options.download_video && direct_url.is_some();
    let supports_cancel = true;
    let format_display = build_task_format_label(format_label, &download_options);
    let task_id = if download_options.download_video {
        format!(
            "task-{asset_id}-{}",
            format_id.unwrap_or("best").trim().replace(['/', ' '], "-")
        )
    } else {
        format!("task-{asset_id}-extras")
    };

    if download_options.download_video
        && format_requires_processing(format_id)
        && !ffmpeg_available(ffmpeg_path)
    {
        return Err(format!(
            "{} 当前选中的清晰度需要 FFmpeg 合并音视频流。请先安装 FFmpeg，或切换到可直接保存的格式后再下载。",
            platforms::human_platform_name(platform)
        ));
    }

    if download_options.download_audio && !ffmpeg_available(ffmpeg_path) {
        return Err("提取 MP3 音频需要 FFmpeg。请先在设置中安装媒体引擎组件。".to_string());
    }

    let controller = Arc::new(TaskController::new(supports_pause, supports_cancel));
    register_controller(&controller_store, &task_id, Arc::clone(&controller));
    let task = DownloadTask {
        id: task_id.clone(),
        platform: platform.to_string(),
        title: title.to_string(),
        progress: if download_options.download_video {
            1
        } else {
            0
        },
        speed_text: "-".to_string(),
        format_label: format_display.clone(),
        status: "queued".to_string(),
        eta_text: "等待中".to_string(),
        message: Some("下载任务已开始。".to_string()),
        output_path: None,
        supports_pause,
        supports_cancel,
        can_retry: true,
    };

    upsert_task(&task_store, task.clone());

    let source_url = source_url.to_string();
    let platform_text = platform.to_string();
    let title = title.to_string();
    let format_id_text = format_id.map(str::to_string);
    let format_label_text = format_label.map(str::to_string);
    let asset_id_text = asset_id.to_string();
    let referer = referer.map(str::to_string);
    let user_agent = user_agent.map(str::to_string);
    let audio_direct_url = audio_direct_url.map(str::to_string);
    let audio_referer = audio_referer.map(str::to_string);
    let audio_user_agent = audio_user_agent.map(str::to_string);
    let ffmpeg_path = ffmpeg_path.map(str::to_string);
    let artifacts = DownloadArtifacts {
        platform: platform.to_string(),
        source_url: source_url.to_string(),
        asset_id: asset_id_text.clone(),
        title: title.clone(),
        author: author.to_string(),
        publish_date: publish_date.to_string(),
        caption: caption.to_string(),
        cover_url: cover_url.map(str::to_string),
        referer: referer.clone(),
        user_agent: user_agent.clone(),
    };
    let cookie_browser = cookie_browser.map(str::to_string);
    let cookie_file = cookie_file.map(str::to_string);
    let direct_url = direct_url.map(str::to_string);
    task_store::set_replay(
        &task_store,
        &task_id,
        TaskReplayRequest {
            platform: platform.to_string(),
            source_url: source_url.clone(),
            asset_id: asset_id_text.clone(),
            title: title.clone(),
            author: author.to_string(),
            publish_date: publish_date.to_string(),
            caption: caption.to_string(),
            cover_url: cover_url.map(str::to_string),
            format_id: format_id_text.clone(),
            format_label: format_label_text.clone(),
            save_directory: save_directory.to_string(),
            download_options: download_options.clone(),
            auto_reveal_in_file_manager,
            cookie_browser: cookie_browser.clone(),
            cookie_file: cookie_file.clone(),
            direct_url: direct_url.clone(),
            referer: referer.clone(),
            user_agent: user_agent.clone(),
            audio_direct_url: audio_direct_url.clone(),
            audio_referer: audio_referer.clone(),
            audio_user_agent: audio_user_agent.clone(),
        },
    );

    if !download_options.download_video {
        thread::spawn(move || {
            metadata_only_worker(
                task_store,
                controller_store,
                task_id,
                title,
                format_display,
                artifacts,
                output_layout,
                download_options,
                auto_reveal_in_file_manager,
                ffmpeg_path,
                controller,
            );
        });

        return Ok(task);
    }

    if let Some(direct_url) = direct_url {
        if let Some(audio_direct_url) = audio_direct_url {
            thread::spawn(move || {
                acquire_download_slot();
                dash_download_worker(
                    task_store,
                    controller_store,
                    task_id,
                    title,
                    format_display,
                    artifacts,
                    output_layout,
                    download_options,
                    direct_url,
                    audio_direct_url,
                    auto_reveal_in_file_manager,
                    referer,
                    user_agent,
                    audio_referer,
                    audio_user_agent,
                    ffmpeg_path,
                    controller,
                );
                release_download_slot();
            });
        } else {
            thread::spawn(move || {
                acquire_download_slot();
                direct_download_worker(
                    task_store,
                    controller_store,
                    task_id,
                    title,
                    format_display,
                    artifacts,
                    output_layout,
                    download_options,
                    direct_url,
                    auto_reveal_in_file_manager,
                    referer,
                    user_agent,
                    ffmpeg_path,
                    controller,
                );
                release_download_slot();
            });
        }

        return Ok(task);
    }

    ensure_ytdlp_available()?;
    let format_id_text = format_id_text
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "当前下载任务缺少可用的视频格式信息。".to_string())?;
    let output_template = output_layout.asset_root().join(format!(
        "{}.%(ext)s",
        if output_layout.bundle_dir.is_some() {
            "video".to_string()
        } else {
            output_layout.single_stem.clone()
        }
    ));
    let output_template_text = output_template.to_string_lossy().to_string();

    thread::spawn(move || {
        acquire_download_slot();
        let artifacts = artifacts.clone();
        let ytdlp_binary = match resolve_ytdlp_path() {
            Ok(path) => path,
            Err(error) => {
                fail_task(&task_store, &task_id, error);
                release_download_slot();
                return;
            }
        };
        let mut command = silent_command(ytdlp_binary);
        extend_runtime_path(&mut command);
        command
            .arg("--no-playlist")
            .arg("--socket-timeout")
            .arg("20")
            .arg("--newline")
            .arg("--progress-delta")
            .arg("0.5")
            .arg("--progress-template")
            .arg("download:progress:%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s")
            .arg("--output")
            .arg(&output_template_text)
            .arg("--print")
            .arg("after_move:output:%(filepath)s");

        if let Some(referer) = referer.as_deref() {
            command
                .arg("--add-header")
                .arg(format!("Referer: {referer}"));
        }

        if let Some(user_agent) = user_agent.as_deref() {
            command
                .arg("--add-header")
                .arg(format!("User-Agent: {user_agent}"));
        }

        append_platform_ytdlp_args(&mut command, &platform_text);
        append_ffmpeg_args(&mut command, ffmpeg_path.as_deref());
        append_network_args(
            &mut command,
            current_proxy_for_platform(&platform_text).as_deref(),
            current_speed_limit().as_deref(),
        );
        command.arg("--format").arg(&format_id_text);
        append_auth_args(
            &mut command,
            cookie_browser.as_deref(),
            cookie_file.as_deref(),
        );

        let spawn_result = command
            .arg(&source_url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match spawn_result {
            Ok(child) => child,
            Err(error) => {
                fail_task(&task_store, &task_id, format!("启动下载失败：{error}"));
                return;
            }
        };

        upsert_task(
            &task_store,
            DownloadTask {
                id: task_id.clone(),
                platform: artifacts.platform.clone(),
                title: title.clone(),
                progress: 1,
                speed_text: "-".to_string(),
                format_label: format_display.clone(),
                status: "downloading".to_string(),
                eta_text: "准备中".to_string(),
                message: Some("正在下载…".to_string()),
                output_path: None,
                supports_pause: false,
                supports_cancel: true,
                can_retry: true,
            },
        );

        let output_path = Arc::new(Mutex::new(None::<String>));
        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let task_platform = artifacts.platform.clone();

        let stdout_handle = child.stdout.take().map(|stdout| {
            let task_store = Arc::clone(&task_store);
            let output_path = Arc::clone(&output_path);
            let task_id = task_id.clone();
            let title = title.clone();
            let format_display = format_display.clone();
            let task_platform = task_platform.clone();
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(progress) = parse_progress_line(&line) {
                        upsert_task(
                            &task_store,
                            DownloadTask {
                                id: task_id.clone(),
                                platform: task_platform.clone(),
                                title: title.clone(),
                                progress: progress.percent,
                                speed_text: progress.speed_text,
                                format_label: format_display.clone(),
                                status: "downloading".to_string(),
                                eta_text: progress.eta_text,
                                message: Some("正在下载…".to_string()),
                                output_path: None,
                                supports_pause: false,
                                supports_cancel: true,
                                can_retry: true,
                            },
                        );
                    } else if let Some(path) = line.strip_prefix("output:") {
                        let mut guard = output_path.lock().unwrap();
                        *guard = Some(path.trim().to_string());
                    }
                }
            })
        });

        let stderr_handle = child.stderr.take().map(|stderr| {
            let task_store = Arc::clone(&task_store);
            let stderr_lines = Arc::clone(&stderr_lines);
            let task_id = task_id.clone();
            let title = title.clone();
            let format_display = format_display.clone();
            let task_platform = task_platform.clone();

            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    {
                        let mut guard = stderr_lines.lock().unwrap();
                        guard.push(line.clone());
                        if guard.len() > 12 {
                            let _ = guard.remove(0);
                        }
                    }

                    if let Some(progress) = parse_progress_line(&line) {
                        upsert_task(
                            &task_store,
                            DownloadTask {
                                id: task_id.clone(),
                                platform: task_platform.clone(),
                                title: title.clone(),
                                progress: progress.percent,
                                speed_text: progress.speed_text,
                                format_label: format_display.clone(),
                                status: "downloading".to_string(),
                                eta_text: progress.eta_text,
                                message: Some("正在下载…".to_string()),
                                output_path: None,
                                supports_pause: false,
                                supports_cancel: true,
                                can_retry: true,
                            },
                        );
                    }
                }
            })
        });

        let mut cancelled = false;
        let status = loop {
            if controller.is_cancel_requested() {
                cancelled = true;
                let _ = child.kill();
            }

            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(180)),
                Err(error) => {
                    unregister_controller(&controller_store, &task_id);
                    fail_task(
                        &task_store,
                        &task_id,
                        format!("等待下载进程结束失败：{error}"),
                    );
                    return;
                }
            }
        };

        if let Some(handle) = stdout_handle {
            let _ = handle.join();
        }
        if let Some(handle) = stderr_handle {
            let _ = handle.join();
        }

        if cancelled {
            unregister_controller(&controller_store, &task_id);
            cancel_task_update(&task_store, &task_id);
            return;
        }

        if status.success() {
            let saved_path = output_path.lock().unwrap().clone();
            let artifact_summary = saved_path
                .as_deref()
                .map(|path| {
                    let mut summary = persist_download_artifacts(
                        &output_layout,
                        Some(Path::new(path)),
                        &artifacts,
                        format_label_text.as_deref(),
                        &download_options,
                        ffmpeg_path.as_deref(),
                    );
                    if output_layout.bundle_dir.is_some() {
                        summary.output_path = output_layout.bundle_entry_path();
                    }
                    summary
                })
                .unwrap_or_else(|| ArtifactSummary {
                    output_path: Some(output_layout.destination_path()),
                    destination_path: output_layout.destination_path(),
                    warnings: vec![
                        "已下载视频，但未能确认最终文件路径，未生成封面和文案文件。".to_string()
                    ],
                    ..ArtifactSummary::default()
                });
            unregister_controller(&controller_store, &task_id);
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    platform: task_platform.clone(),
                    title: title.clone(),
                    progress: 100,
                    speed_text: "-".to_string(),
                    format_label: format_display.clone(),
                    status: "completed".to_string(),
                    eta_text: "已完成".to_string(),
                    message: Some(build_completion_message(
                        &artifact_summary.destination_path,
                        &artifact_summary,
                    )),
                    output_path: artifact_summary.output_path.clone(),
                    supports_pause: false,
                    supports_cancel: true,
                    can_retry: true,
                },
            );

            if auto_reveal_in_file_manager {
                if let Some(path) = artifact_summary.output_path.clone() {
                    let _ = open_in_file_manager(&path, output_layout.bundle_dir.is_none());
                } else {
                    let _ = open_in_file_manager(&artifact_summary.destination_path, false);
                }
            }

            download_history::record_download(&task_platform, &artifacts.asset_id, &title);
        } else {
            let reason = stderr_lines
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "下载失败".to_string());

            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, reason);
        }
        release_download_slot();
    });

    Ok(task)
}

fn metadata_only_worker(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    task_id: String,
    title: String,
    format_label: String,
    artifacts: DownloadArtifacts,
    output_layout: OutputLayout,
    download_options: DownloadContentSelection,
    auto_reveal_in_file_manager: bool,
    ffmpeg_path: Option<String>,
    controller: Arc<TaskController>,
) {
    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id.clone(),
            platform: artifacts.platform.clone(),
            title: title.clone(),
            progress: 0,
            speed_text: "-".to_string(),
            format_label: format_label.clone(),
            status: "downloading".to_string(),
            eta_text: "准备中".to_string(),
            message: Some("正在整理文案和封面…".to_string()),
            output_path: None,
            supports_pause: false,
            supports_cancel: true,
            can_retry: true,
        },
    );

    if controller.is_cancel_requested() {
        unregister_controller(&controller_store, &task_id);
        cancel_task_update(&task_store, &task_id);
        return;
    }

    let summary = persist_download_artifacts(
        &output_layout,
        None,
        &artifacts,
        None,
        &download_options,
        ffmpeg_path.as_deref(),
    );

    if controller.is_cancel_requested() {
        unregister_controller(&controller_store, &task_id);
        cancel_task_update(&task_store, &task_id);
        return;
    }

    let revealed_path = summary.output_path.clone();
    unregister_controller(&controller_store, &task_id);
    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id,
            platform: artifacts.platform.clone(),
            title,
            progress: 100,
            speed_text: "-".to_string(),
            format_label,
            status: "completed".to_string(),
            eta_text: "已完成".to_string(),
            message: Some(build_completion_message(
                &summary.destination_path,
                &summary,
            )),
            output_path: summary.output_path.clone(),
            supports_pause: false,
            supports_cancel: true,
            can_retry: true,
        },
    );

    if auto_reveal_in_file_manager {
        if let Some(path) = revealed_path.as_deref() {
            let _ = open_in_file_manager(path, output_layout.bundle_dir.is_none());
        }
    }

    download_history::record_download(&artifacts.platform, &artifacts.asset_id, &artifacts.title);
}

fn dash_message(video_finished: bool, audio_finished: bool) -> &'static str {
    match (video_finished, audio_finished) {
        (false, false) => "正在下载音视频流…",
        (true, false) => "正在下载音频流…",
        (false, true) => "正在下载视频流…",
        (true, true) => "正在合并音视频…",
    }
}

fn stream_label(kind: StreamKind) -> &'static str {
    match kind {
        StreamKind::Video => "视频",
        StreamKind::Audio => "音频",
    }
}

fn spawn_stream_download_worker(
    kind: StreamKind,
    mut response: reqwest::blocking::Response,
    mut file: File,
    temp_path: PathBuf,
    controller: Arc<TaskController>,
    progress: Arc<AtomicU64>,
    sender: mpsc::Sender<(StreamKind, StreamWorkerOutcome)>,
    rate_limit_bytes: Option<u64>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let outcome = stream_response_to_file(
            kind,
            &mut response,
            &mut file,
            &temp_path,
            &controller,
            &progress,
            rate_limit_bytes,
        );
        let _ = sender.send((kind, outcome));
    })
}

fn stream_response_to_file(
    kind: StreamKind,
    response: &mut reqwest::blocking::Response,
    file: &mut File,
    temp_path: &Path,
    controller: &TaskController,
    progress: &AtomicU64,
    rate_limit_bytes: Option<u64>,
) -> StreamWorkerOutcome {
    let mut buffer = [0u8; 1024 * 1024];
    let mut throttle_window_start = Instant::now();
    let mut throttle_window_bytes: u64 = 0;

    loop {
        if controller.is_cancel_requested() {
            let _ = fs::remove_file(temp_path);
            return StreamWorkerOutcome::Cancelled;
        }

        while controller.is_pause_requested() {
            if controller.is_cancel_requested() {
                let _ = fs::remove_file(temp_path);
                return StreamWorkerOutcome::Cancelled;
            }
            thread::sleep(Duration::from_millis(180));
        }

        let bytes_read = match response.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                let _ = fs::remove_file(temp_path);
                return StreamWorkerOutcome::Failed(format!(
                    "读取{}流失败：{error}",
                    stream_label(kind)
                ));
            }
        };

        if bytes_read == 0 {
            break;
        }

        if let Err(error) = file.write_all(&buffer[..bytes_read]) {
            let _ = fs::remove_file(temp_path);
            return StreamWorkerOutcome::Failed(format!(
                "写入{}临时文件失败：{error}",
                stream_label(kind)
            ));
        }

        progress.fetch_add(bytes_read as u64, Ordering::Relaxed);

        if let Some(limit) = rate_limit_bytes {
            throttle_window_bytes += bytes_read as u64;
            let (new_start, new_bytes) =
                throttle_transfer(throttle_window_start, throttle_window_bytes, limit);
            throttle_window_start = new_start;
            throttle_window_bytes = new_bytes;
        }
    }

    if let Err(error) = file.flush() {
        let _ = fs::remove_file(temp_path);
        return StreamWorkerOutcome::Failed(format!(
            "刷新{}文件缓存失败：{error}",
            stream_label(kind)
        ));
    }

    StreamWorkerOutcome::Completed
}

fn direct_download_worker(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    task_id: String,
    title: String,
    format_label: String,
    artifacts: DownloadArtifacts,
    output_layout: OutputLayout,
    download_options: DownloadContentSelection,
    direct_url: String,
    auto_reveal_in_file_manager: bool,
    referer: Option<String>,
    user_agent: Option<String>,
    ffmpeg_path: Option<String>,
    controller: Arc<TaskController>,
) {
    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id.clone(),
            platform: artifacts.platform.clone(),
            title: title.clone(),
            progress: 0,
            speed_text: "-".to_string(),
            format_label: format_label.clone(),
            status: "downloading".to_string(),
            eta_text: "准备中".to_string(),
            message: Some("正在下载…".to_string()),
            output_path: None,
            supports_pause: true,
            supports_cancel: true,
            can_retry: true,
        },
    );

    let client = match build_http_client(&artifacts.platform) {
        Ok(client) => client,
        Err(error) => {
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, error);
            return;
        }
    };

    if controller.is_cancel_requested() {
        unregister_controller(&controller_store, &task_id);
        cancel_task_update(&task_store, &task_id);
        return;
    }

    let mut request = client.get(&direct_url);
    if let Some(referer) = referer.as_deref() {
        request = request.header(REFERER, referer);
    }
    if let Some(user_agent) = user_agent.as_deref() {
        request = request.header(USER_AGENT, user_agent);
    }

    let response = match request.send() {
        Ok(response) => response,
        Err(error) => {
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, format!("请求下载直链失败：{error}"));
            return;
        }
    };

    if !response.status().is_success() {
        unregister_controller(&controller_store, &task_id);
        fail_task(
            &task_store,
            &task_id,
            format!("下载直链返回异常状态：{}", response.status()),
        );
        return;
    }

    let extension = infer_extension(&response, &direct_url);
    let final_path = output_layout.video_path(&extension);
    let temp_path = final_path.with_extension(format!("{extension}.download"));

    // Resume support: check for existing partial download
    let existing_bytes = fs::metadata(&temp_path).map(|m| m.len()).unwrap_or(0);
    let mut resumed = false;

    let (mut response, mut downloaded_bytes) = if existing_bytes > 0 {
        // Try Range request for resume
        let mut resume_request = client.get(&direct_url);
        if let Some(referer) = referer.as_deref() {
            resume_request = resume_request.header(REFERER, referer);
        }
        if let Some(user_agent) = user_agent.as_deref() {
            resume_request = resume_request.header(USER_AGENT, user_agent);
        }
        resume_request = resume_request.header("Range", format!("bytes={}-", existing_bytes));

        match resume_request.send() {
            Ok(range_resp) if range_resp.status() == reqwest::StatusCode::PARTIAL_CONTENT => {
                resumed = true;
                (range_resp, existing_bytes)
            }
            _ => {
                // Server doesn't support Range — start over
                let _ = fs::remove_file(&temp_path);
                (response, 0u64)
            }
        }
    } else {
        (response, 0u64)
    };

    let mut file = if resumed {
        match fs::OpenOptions::new().append(true).open(&temp_path) {
            Ok(file) => file,
            Err(error) => {
                unregister_controller(&controller_store, &task_id);
                fail_task(&task_store, &task_id, format!("打开续传文件失败：{error}"));
                return;
            }
        }
    } else {
        match File::create(&temp_path) {
            Ok(file) => file,
            Err(error) => {
                unregister_controller(&controller_store, &task_id);
                fail_task(&task_store, &task_id, format!("创建临时文件失败：{error}"));
                return;
            }
        }
    };

    let total_bytes = if resumed {
        existing_bytes + response.content_length().unwrap_or(0)
    } else {
        response.content_length().unwrap_or(0)
    };
    let mut buffer = [0u8; 1024 * 1024];
    let mut last_report = Instant::now();
    let mut speed_tracker = SpeedTracker::new();
    speed_tracker.record(Instant::now(), 0);
    let rate_limit_bytes = parse_speed_limit_bytes(current_speed_limit().as_deref());
    let mut throttle_window_start = Instant::now();
    let mut throttle_window_bytes: u64 = 0;

    loop {
        if controller.is_cancel_requested() {
            let _ = fs::remove_file(&temp_path);
            unregister_controller(&controller_store, &task_id);
            cancel_task_update(&task_store, &task_id);
            return;
        }

        if controller.is_pause_requested() {
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    platform: artifacts.platform.clone(),
                    title: title.clone(),
                    progress: compute_percent(downloaded_bytes, total_bytes),
                    speed_text: speed_tracker.human_speed_text(),
                    format_label: format_label.clone(),
                    status: "paused".to_string(),
                    eta_text: speed_tracker.human_eta_text(downloaded_bytes, total_bytes),
                    message: Some("已暂停，可以继续或取消。".to_string()),
                    output_path: None,
                    supports_pause: true,
                    supports_cancel: true,
                    can_retry: true,
                },
            );

            let _paused_at = Instant::now();
            while controller.is_pause_requested() {
                if controller.is_cancel_requested() {
                    let _ = fs::remove_file(&temp_path);
                    unregister_controller(&controller_store, &task_id);
                    cancel_task_update(&task_store, &task_id);
                    return;
                }
                thread::sleep(Duration::from_millis(180));
            }

            speed_tracker.record(Instant::now(), downloaded_bytes);
            throttle_window_start = Instant::now();
            throttle_window_bytes = 0;
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    platform: artifacts.platform.clone(),
                    title: title.clone(),
                    progress: compute_percent(downloaded_bytes, total_bytes),
                    speed_text: speed_tracker.human_speed_text(),
                    format_label: format_label.clone(),
                    status: "downloading".to_string(),
                    eta_text: speed_tracker.human_eta_text(downloaded_bytes, total_bytes),
                    message: Some("继续下载…".to_string()),
                    output_path: None,
                    supports_pause: true,
                    supports_cancel: true,
                    can_retry: true,
                },
            );
        }

        let bytes_read = match response.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                // Keep temp file for resume on retry
                unregister_controller(&controller_store, &task_id);
                fail_task(&task_store, &task_id, format!("读取下载响应失败：{error}"));
                return;
            }
        };

        if bytes_read == 0 {
            break;
        }

        if let Err(error) = file.write_all(&buffer[..bytes_read]) {
            // Keep temp file for resume on retry
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, format!("写入下载文件失败：{error}"));
            return;
        }

        downloaded_bytes += bytes_read as u64;

        if let Some(limit) = rate_limit_bytes {
            throttle_window_bytes += bytes_read as u64;
            let (new_start, new_bytes) =
                throttle_transfer(throttle_window_start, throttle_window_bytes, limit);
            throttle_window_start = new_start;
            throttle_window_bytes = new_bytes;
        }

        if last_report.elapsed() >= Duration::from_millis(250) {
            speed_tracker.record(Instant::now(), downloaded_bytes);
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    platform: artifacts.platform.clone(),
                    title: title.clone(),
                    progress: compute_percent(downloaded_bytes, total_bytes),
                    speed_text: speed_tracker.human_speed_text(),
                    format_label: format_label.clone(),
                    status: "downloading".to_string(),
                    eta_text: speed_tracker.human_eta_text(downloaded_bytes, total_bytes),
                    message: Some("正在下载…".to_string()),
                    output_path: None,
                    supports_pause: true,
                    supports_cancel: true,
                    can_retry: true,
                },
            );
            last_report = Instant::now();
        }
    }

    if let Err(error) = file.flush() {
        let _ = fs::remove_file(&temp_path);
        unregister_controller(&controller_store, &task_id);
        fail_task(&task_store, &task_id, format!("刷新文件缓存失败：{error}"));
        return;
    }

    if let Err(error) = fs::rename(&temp_path, &final_path) {
        let _ = fs::remove_file(&temp_path);
        unregister_controller(&controller_store, &task_id);
        fail_task(&task_store, &task_id, format!("保存下载文件失败：{error}"));
        return;
    }

    let mut artifact_summary = persist_download_artifacts(
        &output_layout,
        Some(&final_path),
        &artifacts,
        Some(&format_label),
        &download_options,
        ffmpeg_path.as_deref(),
    );
    if output_layout.bundle_dir.is_some() {
        artifact_summary.output_path = output_layout.bundle_entry_path();
    }
    unregister_controller(&controller_store, &task_id);

    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id,
            platform: artifacts.platform.clone(),
            title,
            progress: 100,
            speed_text: "-".to_string(),
            format_label,
            status: "completed".to_string(),
            eta_text: "已完成".to_string(),
            message: Some(build_completion_message(
                &artifact_summary.destination_path,
                &artifact_summary,
            )),
            output_path: artifact_summary.output_path.clone(),
            supports_pause: true,
            supports_cancel: true,
            can_retry: true,
        },
    );

    if auto_reveal_in_file_manager {
        if let Some(path) = artifact_summary.output_path.as_deref() {
            let _ = open_in_file_manager(path, output_layout.bundle_dir.is_none());
        }
    }

    download_history::record_download(&artifacts.platform, &artifacts.asset_id, &artifacts.title);
}

fn dash_download_worker(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    task_id: String,
    title: String,
    format_label: String,
    artifacts: DownloadArtifacts,
    output_layout: OutputLayout,
    download_options: DownloadContentSelection,
    video_direct_url: String,
    audio_direct_url: String,
    auto_reveal_in_file_manager: bool,
    video_referer: Option<String>,
    video_user_agent: Option<String>,
    audio_referer: Option<String>,
    audio_user_agent: Option<String>,
    ffmpeg_path: Option<String>,
    controller: Arc<TaskController>,
) {
    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id.clone(),
            platform: artifacts.platform.clone(),
            title: title.clone(),
            progress: 0,
            speed_text: "-".to_string(),
            format_label: format_label.clone(),
            status: "downloading".to_string(),
            eta_text: "准备中".to_string(),
            message: Some("正在下载视频流…".to_string()),
            output_path: None,
            supports_pause: true,
            supports_cancel: true,
            can_retry: true,
        },
    );

    let client = match build_http_client(&artifacts.platform) {
        Ok(client) => client,
        Err(error) => {
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, error);
            return;
        }
    };

    if controller.is_cancel_requested() {
        unregister_controller(&controller_store, &task_id);
        cancel_task_update(&task_store, &task_id);
        return;
    }

    let (video_head_total, audio_head_total) = std::thread::scope(|s| {
        let v = s.spawn(|| {
            probe_content_length(
                &client,
                &video_direct_url,
                video_referer.as_deref(),
                video_user_agent.as_deref(),
            )
        });
        let a = s.spawn(|| {
            probe_content_length(
                &client,
                &audio_direct_url,
                audio_referer.as_deref(),
                audio_user_agent.as_deref(),
            )
        });
        (v.join().unwrap_or(0), a.join().unwrap_or(0))
    });

    let mut video_request = client.get(&video_direct_url);
    if let Some(referer) = video_referer.as_deref() {
        video_request = video_request.header(REFERER, referer);
    }
    if let Some(user_agent) = video_user_agent.as_deref() {
        video_request = video_request.header(USER_AGENT, user_agent);
    }

    let video_response = match video_request.send() {
        Ok(response) => response,
        Err(error) => {
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, format!("请求视频流失败：{error}"));
            return;
        }
    };

    if !video_response.status().is_success() {
        unregister_controller(&controller_store, &task_id);
        fail_task(
            &task_store,
            &task_id,
            format!("视频流返回异常状态：{}", video_response.status()),
        );
        return;
    }

    let video_total = video_head_total.max(video_response.content_length().unwrap_or(0));
    let video_extension = infer_extension(&video_response, &video_direct_url);
    let mut audio_request = client.get(&audio_direct_url);
    if let Some(referer) = audio_referer.as_deref() {
        audio_request = audio_request.header(REFERER, referer);
    }
    if let Some(user_agent) = audio_user_agent.as_deref() {
        audio_request = audio_request.header(USER_AGENT, user_agent);
    }

    let audio_response = match audio_request.send() {
        Ok(response) => response,
        Err(error) => {
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, format!("请求音频流失败：{error}"));
            return;
        }
    };

    if !audio_response.status().is_success() {
        unregister_controller(&controller_store, &task_id);
        fail_task(
            &task_store,
            &task_id,
            format!("音频流返回异常状态：{}", audio_response.status()),
        );
        return;
    }

    let audio_total = audio_head_total.max(audio_response.content_length().unwrap_or(0));
    let audio_extension = infer_extension(&audio_response, &audio_direct_url);
    let dash_root = output_layout.asset_root().to_path_buf();
    let temp_stem = if output_layout.bundle_dir.is_some() {
        "video".to_string()
    } else {
        output_layout.single_stem.clone()
    };
    let video_temp_path =
        unique_output_path(dash_root.join(format!("{temp_stem}.video.{video_extension}.download")));
    let video_file = match File::create(&video_temp_path) {
        Ok(file) => file,
        Err(error) => {
            unregister_controller(&controller_store, &task_id);
            fail_task(
                &task_store,
                &task_id,
                format!("创建视频临时文件失败：{error}"),
            );
            return;
        }
    };
    let merged_extension = dash_output_extension(&video_extension, &audio_extension);
    let final_path = output_layout.video_path(&merged_extension);
    let audio_temp_path =
        unique_output_path(dash_root.join(format!("{temp_stem}.audio.{audio_extension}.download")));
    let audio_file = match File::create(&audio_temp_path) {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(&video_temp_path);
            unregister_controller(&controller_store, &task_id);
            fail_task(
                &task_store,
                &task_id,
                format!("创建音频临时文件失败：{error}"),
            );
            return;
        }
    };

    let (sender, receiver) = mpsc::channel::<(StreamKind, StreamWorkerOutcome)>();
    let video_progress = Arc::new(AtomicU64::new(0));
    let audio_progress = Arc::new(AtomicU64::new(0));
    let rate_limit_bytes = parse_speed_limit_bytes(current_speed_limit().as_deref());
    // Split rate limit between two streams
    let per_stream_limit = rate_limit_bytes.map(|l| (l / 2).max(1));
    let video_handle = spawn_stream_download_worker(
        StreamKind::Video,
        video_response,
        video_file,
        video_temp_path.clone(),
        Arc::clone(&controller),
        Arc::clone(&video_progress),
        sender.clone(),
        per_stream_limit,
    );
    let audio_handle = spawn_stream_download_worker(
        StreamKind::Audio,
        audio_response,
        audio_file,
        audio_temp_path.clone(),
        Arc::clone(&controller),
        Arc::clone(&audio_progress),
        sender,
        per_stream_limit,
    );

    let mut last_report = Instant::now();
    let mut speed_tracker = SpeedTracker::new();
    speed_tracker.record(Instant::now(), 0);
    let mut video_finished = false;
    let mut audio_finished = false;
    let mut cancelled = false;
    let mut failure = None::<String>;
    let mut finished_count = 0usize;

    while finished_count < 2 {
        while let Ok((kind, outcome)) = receiver.try_recv() {
            finished_count += 1;
            match kind {
                StreamKind::Video => video_finished = true,
                StreamKind::Audio => audio_finished = true,
            }

            match outcome {
                StreamWorkerOutcome::Completed => {}
                StreamWorkerOutcome::Cancelled => {
                    cancelled = true;
                }
                StreamWorkerOutcome::Failed(error) => {
                    if failure.is_none() {
                        failure = Some(error);
                        controller.request_cancel();
                    }
                }
            }
        }

        if finished_count >= 2 {
            break;
        }

        if controller.is_cancel_requested() {
            cancelled = true;
        }

        let video_downloaded = video_progress.load(Ordering::Relaxed);
        let audio_downloaded = audio_progress.load(Ordering::Relaxed);
        let downloaded_known = video_downloaded.saturating_add(audio_downloaded);
        let total_known = video_total.saturating_add(audio_total);

        if controller.is_pause_requested() {
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    platform: artifacts.platform.clone(),
                    title: title.clone(),
                    progress: if total_known > 0 {
                        dash_combined_progress(downloaded_known, total_known)
                    } else if video_finished {
                        dash_phase_progress(audio_downloaded, audio_total, 85, 96)
                    } else {
                        dash_phase_progress(video_downloaded, video_total, 0, 85)
                    },
                    speed_text: speed_tracker.human_speed_text(),
                    format_label: format_label.clone(),
                    status: "paused".to_string(),
                    eta_text: speed_tracker.human_eta_text(downloaded_known, total_known),
                    message: Some("已暂停，可以继续或取消。".to_string()),
                    output_path: None,
                    supports_pause: true,
                    supports_cancel: true,
                    can_retry: true,
                },
            );

            let _paused_at = Instant::now();
            while controller.is_pause_requested() {
                while let Ok((kind, outcome)) = receiver.try_recv() {
                    finished_count += 1;
                    match kind {
                        StreamKind::Video => video_finished = true,
                        StreamKind::Audio => audio_finished = true,
                    }

                    match outcome {
                        StreamWorkerOutcome::Completed => {}
                        StreamWorkerOutcome::Cancelled => {
                            cancelled = true;
                        }
                        StreamWorkerOutcome::Failed(error) => {
                            if failure.is_none() {
                                failure = Some(error);
                                controller.request_cancel();
                            }
                        }
                    }
                }

                if finished_count >= 2 {
                    break;
                }

                if controller.is_cancel_requested() {
                    cancelled = true;
                    break;
                }

                thread::sleep(Duration::from_millis(180));
            }

            last_report = Instant::now();
            continue;
        }

        if last_report.elapsed() >= Duration::from_millis(180) {
            speed_tracker.record(Instant::now(), downloaded_known);
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    platform: artifacts.platform.clone(),
                    title: title.clone(),
                    progress: if total_known > 0 {
                        dash_combined_progress(downloaded_known, total_known)
                    } else if video_finished {
                        dash_phase_progress(audio_downloaded, audio_total, 85, 96)
                    } else {
                        dash_phase_progress(video_downloaded, video_total, 0, 85)
                    },
                    speed_text: speed_tracker.human_speed_text(),
                    format_label: format_label.clone(),
                    status: "downloading".to_string(),
                    eta_text: speed_tracker.human_eta_text(downloaded_known, total_known),
                    message: Some(dash_message(video_finished, audio_finished).to_string()),
                    output_path: None,
                    supports_pause: true,
                    supports_cancel: true,
                    can_retry: true,
                },
            );
            last_report = Instant::now();
        }

        thread::sleep(Duration::from_millis(120));
    }

    let _ = video_handle.join();
    let _ = audio_handle.join();

    if let Some(error) = failure {
        let _ = fs::remove_file(&video_temp_path);
        let _ = fs::remove_file(&audio_temp_path);
        unregister_controller(&controller_store, &task_id);
        fail_task(&task_store, &task_id, error);
        return;
    }

    if controller.is_cancel_requested() {
        cancelled = true;
    }

    if cancelled {
        let _ = fs::remove_file(&video_temp_path);
        let _ = fs::remove_file(&audio_temp_path);
        unregister_controller(&controller_store, &task_id);
        cancel_task_update(&task_store, &task_id);
        return;
    }

    let ffmpeg_binary = ffmpeg_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("ffmpeg");
    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id.clone(),
            platform: artifacts.platform.clone(),
            title: title.clone(),
            progress: 98,
            speed_text: "-".to_string(),
            format_label: format_label.clone(),
            status: "downloading".to_string(),
            eta_text: "处理中".to_string(),
            message: Some("正在合并音视频…".to_string()),
            output_path: None,
            supports_pause: true,
            supports_cancel: true,
            can_retry: true,
        },
    );

    let merge_status = silent_command(ffmpeg_binary)
        .arg("-y")
        .arg("-i")
        .arg(&video_temp_path)
        .arg("-i")
        .arg(&audio_temp_path)
        .arg("-c")
        .arg("copy")
        .arg(&final_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();

    let merge_output = match merge_status {
        Ok(output) => output,
        Err(error) => {
            let _ = fs::remove_file(&video_temp_path);
            let _ = fs::remove_file(&audio_temp_path);
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, format!("启动 FFmpeg 失败：{error}"));
            return;
        }
    };

    if controller.is_cancel_requested() {
        let _ = fs::remove_file(&video_temp_path);
        let _ = fs::remove_file(&audio_temp_path);
        let _ = fs::remove_file(&final_path);
        unregister_controller(&controller_store, &task_id);
        cancel_task_update(&task_store, &task_id);
        return;
    }

    if !merge_output.status.success() {
        let _ = fs::remove_file(&video_temp_path);
        let _ = fs::remove_file(&audio_temp_path);
        let _ = fs::remove_file(&final_path);
        unregister_controller(&controller_store, &task_id);
        fail_task(
            &task_store,
            &task_id,
            readable_error(&merge_output.stderr, "FFmpeg 合并失败"),
        );
        return;
    }

    let _ = fs::remove_file(&video_temp_path);
    let _ = fs::remove_file(&audio_temp_path);

    let mut artifact_summary = persist_download_artifacts(
        &output_layout,
        Some(&final_path),
        &artifacts,
        Some(&format_label),
        &download_options,
        ffmpeg_path.as_deref(),
    );
    if output_layout.bundle_dir.is_some() {
        artifact_summary.output_path = output_layout.bundle_entry_path();
    }
    unregister_controller(&controller_store, &task_id);

    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id,
            platform: artifacts.platform.clone(),
            title,
            progress: 100,
            speed_text: "-".to_string(),
            format_label,
            status: "completed".to_string(),
            eta_text: "已完成".to_string(),
            message: Some(build_completion_message(
                &artifact_summary.destination_path,
                &artifact_summary,
            )),
            output_path: artifact_summary.output_path.clone(),
            supports_pause: true,
            supports_cancel: true,
            can_retry: true,
        },
    );

    if auto_reveal_in_file_manager {
        if let Some(path) = artifact_summary.output_path.as_deref() {
            let _ = open_in_file_manager(path, output_layout.bundle_dir.is_none());
        }
    }

    download_history::record_download(&artifacts.platform, &artifacts.asset_id, &artifacts.title);
}

fn ensure_ytdlp_available() -> Result<(), String> {
    let ytdlp_binary = resolve_ytdlp_path()?;
    let output = silent_command(ytdlp_binary)
        .arg("--version")
        .output()
        .map_err(|_| "未检测到可用的 yt-dlp，请重新安装应用后再试。".to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err("yt-dlp 不可用，请重新安装应用后再试。".to_string())
    }
}

pub fn resolve_ffmpeg_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    first_existing_path(ffmpeg_candidates(app))
}

pub fn ffmpeg_available(ffmpeg_path: Option<&str>) -> bool {
    ffmpeg_path
        .map(PathBuf::from)
        .filter(|path| can_execute_ffmpeg(path))
        .is_some()
        || pack_manager::resolve_shared_pack_file(
            "media-engine",
            PathBuf::from("bin").join(ffmpeg_binary_name()),
        )
        .as_ref()
        .is_some_and(|path| can_execute_ffmpeg(path))
        || can_execute_ffmpeg(Path::new("ffmpeg"))
}

fn append_ffmpeg_args(command: &mut Command, ffmpeg_path: Option<&str>) {
    if let Some(path) = ffmpeg_path.filter(|value| !value.trim().is_empty()) {
        command.arg("--ffmpeg-location").arg(path);
    }
}

fn ffmpeg_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

fn ytdlp_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn resolve_ytdlp_path() -> Result<PathBuf, String> {
    if let Some(path) = preferred_external_ytdlp_path() {
        return Ok(path);
    }

    if let Some(path) = pack_manager::ensure_download_engine_installed()? {
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

fn ffmpeg_candidates<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = pack_manager::resolve_shared_pack_file(
        "media-engine",
        PathBuf::from("bin").join(ffmpeg_binary_name()),
    ) {
        candidates.push(path);
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("bin").join(ffmpeg_binary_name()));
        candidates.push(resource_dir.join(ffmpeg_binary_name()));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest_dir.exists() {
        let workspace_root = manifest_dir.join("..");
        candidates.push(
            workspace_root
                .join("node_modules")
                .join("ffmpeg-static")
                .join(ffmpeg_binary_name()),
        );
        candidates.push(
            workspace_root
                .join("src-tauri")
                .join("resources")
                .join("bin")
                .join(ffmpeg_binary_name()),
        );
    }

    candidates
}

fn first_existing_path<I>(paths: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    paths.into_iter().find(|path| path.is_file())
}

fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path_var) {
        let candidate = entry.join(binary_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn can_execute_ffmpeg(path: &Path) -> bool {
    silent_command(path)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn format_requires_processing(format_id: Option<&str>) -> bool {
    format_id.is_some_and(|value| value.contains('+'))
}

fn prepare_output_layout(
    base_dir: &Path,
    safe_title: &str,
    asset_id: &str,
    download_options: &DownloadContentSelection,
) -> Result<OutputLayout, String> {
    if download_options.needs_bundle_directory() {
        let bundle_dir = unique_output_dir(base_dir.join(safe_title));
        fs::create_dir_all(&bundle_dir).map_err(|error| format!("创建作品文件夹失败：{error}"))?;

        return Ok(OutputLayout {
            base_dir: base_dir.to_path_buf(),
            bundle_dir: Some(bundle_dir),
            single_stem: format!("{safe_title} [{asset_id}]"),
        });
    }

    Ok(OutputLayout {
        base_dir: base_dir.to_path_buf(),
        bundle_dir: None,
        single_stem: format!("{safe_title} [{asset_id}]"),
    })
}

fn persist_download_artifacts(
    output_layout: &OutputLayout,
    video_path: Option<&Path>,
    artifacts: &DownloadArtifacts,
    format_label: Option<&str>,
    download_options: &DownloadContentSelection,
    ffmpeg_path: Option<&str>,
) -> ArtifactSummary {
    let mut summary = ArtifactSummary {
        video_written: video_path.is_some(),
        output_path: if output_layout.bundle_dir.is_some() {
            output_layout.bundle_entry_path()
        } else {
            video_path.map(|path| path.to_string_lossy().to_string())
        },
        destination_path: output_layout.destination_path(),
        warnings: Vec::new(),
        ..ArtifactSummary::default()
    };

    if download_options.download_audio {
        if let Some(source_video) = video_path {
            let audio_output = output_layout.audio_path();
            let ffmpeg_binary = ffmpeg_path
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("ffmpeg");
            let result = silent_command(ffmpeg_binary)
                .arg("-y")
                .arg("-i")
                .arg(source_video)
                .arg("-vn")
                .arg("-acodec")
                .arg("libmp3lame")
                .arg("-q:a")
                .arg("2")
                .arg(&audio_output)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output();
            match result {
                Ok(output) if output.status.success() => {
                    summary.audio_written = true;
                    if summary.output_path.is_none() {
                        summary.output_path = Some(audio_output.to_string_lossy().to_string());
                    }
                }
                Ok(output) => {
                    let stderr_text = String::from_utf8_lossy(&output.stderr);
                    summary.warnings.push(format!(
                        "MP3 提取失败：{}",
                        stderr_text.lines().last().unwrap_or("未知错误")
                    ));
                }
                Err(error) => {
                    summary
                        .warnings
                        .push(format!("启动 FFmpeg 提取音频失败：{error}"));
                }
            }
        } else {
            summary
                .warnings
                .push("没有可用的视频文件，无法提取音频。".to_string());
        }
    }

    if download_options.download_caption {
        let text_path = output_layout.caption_path();
        let text_content = build_text_sidecar(artifacts, format_label);
        match fs::write(&text_path, text_content) {
            Ok(_) => {
                summary.text_written = true;
                if summary.output_path.is_none() {
                    summary.output_path = Some(text_path.to_string_lossy().to_string());
                }
            }
            Err(error) => summary.warnings.push(format!("文案保存失败：{error}")),
        }
    }

    if download_options.download_metadata {
        let json_path = output_layout.metadata_path();
        let metadata = MetadataSidecar {
            asset_id: &artifacts.asset_id,
            platform: &artifacts.platform,
            title: &artifacts.title,
            author: &artifacts.author,
            publish_date: &artifacts.publish_date,
            caption: &artifacts.caption,
            source_url: &artifacts.source_url,
            format_label: format_label.unwrap_or("未下载视频"),
            cover_url: artifacts.cover_url.as_deref(),
            generated_by: "StreamVerse",
        };
        match serde_json::to_vec_pretty(&metadata)
            .map_err(|error| error.to_string())
            .and_then(|payload| fs::write(&json_path, payload).map_err(|error| error.to_string()))
        {
            Ok(_) => {
                summary.metadata_written = true;
                if summary.output_path.is_none() {
                    summary.output_path = Some(json_path.to_string_lossy().to_string());
                }
            }
            Err(error) => summary.warnings.push(format!("元数据保存失败：{error}")),
        }
    }

    if download_options.download_cover {
        if let Some(cover_url) = artifacts.cover_url.as_deref() {
            match download_cover_image_to_path(
                &artifacts.platform,
                output_layout,
                cover_url,
                artifacts.referer.as_deref(),
                artifacts.user_agent.as_deref(),
            ) {
                Ok(path) => {
                    summary.cover_written = true;
                    if summary.output_path.is_none() {
                        summary.output_path = Some(path.to_string_lossy().to_string());
                    }
                }
                Err(error) => summary.warnings.push(format!("封面下载失败：{error}")),
            }
        } else {
            summary
                .warnings
                .push("当前作品没有可用封面地址。".to_string());
        }
    }

    summary
}

fn build_text_sidecar(artifacts: &DownloadArtifacts, format_label: Option<&str>) -> String {
    let mut sections = vec![
        format!(
            "平台：{}",
            platforms::human_platform_name(&artifacts.platform)
        ),
        format!("标题：{}", artifacts.title),
        format!("作者：{}", artifacts.author),
        format!("发布日期：{}", artifacts.publish_date),
        format!("资源 ID：{}", artifacts.asset_id),
        format!("来源链接：{}", artifacts.source_url),
    ];

    if let Some(format_label) = format_label {
        sections.push(format!("下载格式：{format_label}"));
    }

    if let Some(cover_url) = artifacts.cover_url.as_deref() {
        sections.push(format!("封面链接：{cover_url}"));
    }

    sections.push(String::new());
    sections.push("文案：".to_string());
    sections.push(if artifacts.caption.trim().is_empty() {
        "（无）".to_string()
    } else {
        artifacts.caption.trim().to_string()
    });

    sections.join("\n")
}

fn download_cover_image_to_path(
    platform: &str,
    output_layout: &OutputLayout,
    cover_url: &str,
    referer: Option<&str>,
    user_agent: Option<&str>,
) -> Result<PathBuf, String> {
    let client = build_http_client(platform)?;
    let mut request = client.get(cover_url);
    if let Some(referer) = referer {
        request = request.header(REFERER, referer);
    }
    if let Some(user_agent) = user_agent {
        request = request.header(USER_AGENT, user_agent);
    }

    let response = request
        .send()
        .map_err(|error| format!("请求封面直链失败：{error}"))?;

    if !response.status().is_success() {
        return Err(format!("封面直链返回异常状态：{}", response.status()));
    }

    let extension = infer_extension(&response, cover_url);
    let bytes = response
        .bytes()
        .map_err(|error| format!("读取封面响应失败：{error}"))?;
    let cover_path = output_layout.cover_path(&extension);
    fs::write(&cover_path, bytes).map_err(|error| format!("写入封面文件失败：{error}"))?;
    Ok(cover_path)
}

fn build_completion_message(output_dir_text: &str, summary: &ArtifactSummary) -> String {
    let mut items = Vec::new();
    if summary.video_written {
        items.push("视频".to_string());
    }
    if summary.audio_written {
        items.push("音频".to_string());
    }
    if summary.text_written {
        items.push("文案".to_string());
    }
    if summary.metadata_written {
        items.push("元数据".to_string());
    }
    if summary.cover_written {
        items.push("封面".to_string());
    }

    let label = if items.is_empty() {
        "内容".to_string()
    } else {
        items.join("、")
    };

    let mut message = format!("下载完成，{}已保存到 {output_dir_text}。", label);
    if !summary.warnings.is_empty() {
        message.push(' ');
        message.push_str(&summary.warnings.join("；"));
    }

    message
}

fn build_http_client(platform: &str) -> Result<Client, String> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .tcp_nodelay(true)
        .pool_max_idle_per_host(8);

    match current_proxy_for_platform(platform).filter(|value| !value.trim().is_empty()) {
        Some(proxy) => {
            let parsed_proxy =
                Proxy::all(&proxy).map_err(|error| format!("代理地址无效：{error}"))?;
            builder = builder.proxy(parsed_proxy);
        }
        None => {
            builder = builder.no_proxy();
        }
    }

    builder
        .build()
        .map_err(|error| format!("创建下载客户端失败：{error}"))
}

fn build_task_format_label(
    format_label: Option<&str>,
    download_options: &DownloadContentSelection,
) -> String {
    let labels = download_options.selected_labels();
    if download_options.download_video {
        let base = format_label.unwrap_or("视频");
        if labels.len() > 1 {
            format!("{base} · {}", labels.join(" + "))
        } else {
            base.to_string()
        }
    } else {
        labels.join(" + ")
    }
}

fn register_controller(
    controller_store: &TaskControllerStore,
    task_id: &str,
    controller: Arc<TaskController>,
) {
    controller_store
        .lock()
        .unwrap()
        .insert(task_id.to_string(), controller);
}

fn unregister_controller(controller_store: &TaskControllerStore, task_id: &str) {
    controller_store.lock().unwrap().remove(task_id);
}

pub fn pause_task(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    task_id: &str,
) -> Result<DownloadTask, String> {
    let controller = controller_store
        .lock()
        .unwrap()
        .get(task_id)
        .cloned()
        .ok_or_else(|| "当前任务已经结束，无法暂停。".to_string())?;

    if !controller.supports_pause {
        return Err("当前任务暂不支持暂停。".to_string());
    }

    controller.request_pause();
    mutate_task(&task_store, task_id, |task| {
        task.status = "paused".to_string();
        task.message = Some("已暂停，可以继续或取消。".to_string());
    })
}

pub fn resume_task(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    task_id: &str,
) -> Result<DownloadTask, String> {
    let controller = controller_store
        .lock()
        .unwrap()
        .get(task_id)
        .cloned()
        .ok_or_else(|| "当前任务已经结束，无法继续。".to_string())?;

    if !controller.supports_pause {
        return Err("当前任务暂不支持继续。".to_string());
    }

    controller.request_resume();
    mutate_task(&task_store, task_id, |task| {
        task.status = "downloading".to_string();
        task.message = Some("继续下载…".to_string());
    })
}

pub fn cancel_task(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    task_id: &str,
) -> Result<DownloadTask, String> {
    let controller = controller_store
        .lock()
        .unwrap()
        .get(task_id)
        .cloned()
        .ok_or_else(|| "当前任务已经结束，无法取消。".to_string())?;

    if !controller.supports_cancel {
        return Err("当前任务暂不支持取消。".to_string());
    }

    controller.request_cancel();
    mutate_task(&task_store, task_id, |task| {
        task.message = Some("正在取消…".to_string());
    })
}

fn mutate_task<F>(
    task_store: &task_store::TaskStore,
    task_id: &str,
    mutator: F,
) -> Result<DownloadTask, String>
where
    F: FnOnce(&mut DownloadTask),
{
    task_store::mutate_task(task_store, task_id, mutator)
}

fn infer_extension(response: &reqwest::blocking::Response, direct_url: &str) -> String {
    if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
        if let Ok(content_type) = content_type.to_str() {
            if content_type.contains("video/mp4") || content_type.contains("audio/mp4") {
                return "mp4".to_string();
            }
            if content_type.contains("video/webm") || content_type.contains("audio/webm") {
                return "webm".to_string();
            }
            if content_type.contains("audio/mp3") || content_type.contains("audio/mpeg") {
                return "mp3".to_string();
            }
            if content_type.contains("image/jpeg") {
                return "jpg".to_string();
            }
            if content_type.contains("image/png") {
                return "png".to_string();
            }
            if content_type.contains("image/webp") {
                return "webp".to_string();
            }
        }
    }

    let direct_url = direct_url.split('?').next().unwrap_or(direct_url);
    Path::new(direct_url)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("mp4")
        .to_string()
}

fn unique_output_path(base_path: PathBuf) -> PathBuf {
    if !base_path.exists() {
        return base_path;
    }

    let stem = base_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let extension = base_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let parent = base_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    for index in 1..1000 {
        let candidate_name = if extension.is_empty() {
            format!("{stem} ({index})")
        } else {
            format!("{stem} ({index}).{extension}")
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    base_path
}

fn unique_output_dir(base_path: PathBuf) -> PathBuf {
    if !base_path.exists() {
        return base_path;
    }

    let parent = base_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let name = base_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("download");

    for index in 1..1000 {
        let candidate = parent.join(format!("{name} ({index})"));
        if !candidate.exists() {
            return candidate;
        }
    }

    base_path
}

fn compute_percent(downloaded_bytes: u64, total_bytes: u64) -> u32 {
    if total_bytes == 0 {
        // Unknown total: show indeterminate-style progress capped at 90%
        if downloaded_bytes == 0 {
            return 0;
        }
        // Asymptotic curve: quickly rises then flattens near 90%
        let mb = downloaded_bytes as f64 / (1024.0 * 1024.0);
        return (90.0 * (1.0 - (-mb / 50.0).exp())).round().clamp(1.0, 90.0) as u32;
    }

    ((downloaded_bytes as f64 / total_bytes as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u32
}

fn dash_combined_progress(downloaded_bytes: u64, total_bytes: u64) -> u32 {
    if total_bytes == 0 {
        if downloaded_bytes == 0 {
            return 0;
        }
        let mb = downloaded_bytes as f64 / (1024.0 * 1024.0);
        return (90.0 * (1.0 - (-mb / 100.0).exp()))
            .round()
            .clamp(1.0, 90.0) as u32;
    }

    ((downloaded_bytes as f64 / total_bytes as f64) * 96.0)
        .round()
        .clamp(0.0, 96.0) as u32
}

fn dash_phase_progress(downloaded_bytes: u64, total_bytes: u64, start: u32, end: u32) -> u32 {
    if end <= start {
        return start;
    }

    if total_bytes == 0 {
        if downloaded_bytes == 0 {
            return start.max(1);
        }
        // Unknown total: asymptotic progress within [start, end)
        let span = (end - start) as f64;
        let mb = downloaded_bytes as f64 / (1024.0 * 1024.0);
        let offset = (span * (1.0 - (-mb / 50.0).exp()))
            .round()
            .clamp(0.0, span - 1.0) as u32;
        return (start + offset).clamp(start, end - 1);
    }

    let span = (end - start) as f64;
    let offset = ((downloaded_bytes as f64 / total_bytes as f64) * span)
        .round()
        .clamp(0.0, span) as u32;
    (start + offset).clamp(start, end)
}

fn dash_output_extension(video_extension: &str, audio_extension: &str) -> String {
    let video = video_extension.trim().to_ascii_lowercase();
    let audio = audio_extension.trim().to_ascii_lowercase();

    if matches!(video.as_str(), "m4s" | "mp4" | "m4v")
        || matches!(audio.as_str(), "m4s" | "m4a" | "aac")
    {
        return "mp4".to_string();
    }

    if video == "webm" || audio == "webm" {
        return "webm".to_string();
    }

    if !video.is_empty() {
        return video;
    }

    "mp4".to_string()
}

fn probe_content_length(
    client: &Client,
    url: &str,
    referer: Option<&str>,
    user_agent: Option<&str>,
) -> u64 {
    let mut request = client.head(url);
    if let Some(referer) = referer {
        request = request.header(REFERER, referer);
    }
    if let Some(user_agent) = user_agent {
        request = request.header(USER_AGENT, user_agent);
    }

    request
        .send()
        .ok()
        .and_then(|response| response.content_length())
        .unwrap_or(0)
}

/// 3-second sliding window speed tracker for direct downloads.
struct SpeedTracker {
    samples: VecDeque<(Instant, u64)>,
    window: Duration,
}

impl SpeedTracker {
    fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            window: Duration::from_secs(3),
        }
    }

    fn record(&mut self, now: Instant, bytes: u64) {
        self.samples.push_back((now, bytes));
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        while self.samples.front().is_some_and(|(t, _)| *t < cutoff) {
            self.samples.pop_front();
        }
    }

    fn speed_bps(&self) -> Option<f64> {
        if self.samples.len() < 2 {
            return None;
        }
        let (first_t, first_b) = self.samples.front().unwrap();
        let (last_t, last_b) = self.samples.back().unwrap();
        let dt = last_t.duration_since(*first_t).as_secs_f64();
        if dt <= 0.0 {
            return None;
        }
        Some((*last_b - *first_b) as f64 / dt)
    }

    fn human_speed_text(&self) -> String {
        match self.speed_bps() {
            Some(bps) if bps > 0.0 => human_bytes(bps, "/s"),
            _ => "-".to_string(),
        }
    }

    fn human_eta_text(&self, downloaded_bytes: u64, total_bytes: u64) -> String {
        if total_bytes == 0 || downloaded_bytes >= total_bytes {
            return "—".to_string();
        }
        match self.speed_bps() {
            Some(bps) if bps > 0.0 => {
                let remaining = ((total_bytes - downloaded_bytes) as f64 / bps).round() as u64;
                let minutes = remaining / 60;
                let seconds = remaining % 60;
                format!("{minutes:02}:{seconds:02}")
            }
            _ => "—".to_string(),
        }
    }
}

fn human_bytes(bytes: f64, suffix: &str) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];

    let mut value = bytes;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{:.0} {}{}", value, UNITS[unit_index], suffix)
    } else {
        format!("{value:.2} {}{}", UNITS[unit_index], suffix)
    }
}

/// Use the `rookie` crate to extract cookies directly from a browser's cookie database.
/// This bypasses Chrome's App-Bound Encryption (DPAPI) on Windows.
fn extract_cookies_via_rookie(browser: &str, platform: &str) -> Result<String, String> {
    let domains: Vec<String> = match platform {
        "douyin" => vec![".douyin.com".into(), ".iesdouyin.com".into()],
        "bilibili" => vec![".bilibili.com".into()],
        "youtube" => vec![".youtube.com".into(), ".google.com".into()],
        _ => return Err(format!("不支持的平台：{platform}")),
    };

    let cookies = match browser.to_lowercase().as_str() {
        "chrome" => rookie::chrome(Some(domains)),
        "edge" => rookie::edge(Some(domains)),
        "firefox" => rookie::firefox(Some(domains)),
        "safari" => return Err("rookie 不支持 Safari，跳转 yt-dlp".into()),
        _ => return Err(format!("rookie 不支持的浏览器：{browser}")),
    }
    .map_err(|e| format!("从 {} 读取 Cookie 失败：{e}", browser.to_uppercase()))?;

    if cookies.is_empty() {
        return Err(format!(
            "从 {} 未读取到 {} 的 Cookie，请先在浏览器中登录该平台。",
            browser.to_uppercase(),
            platform
        ));
    }

    // Convert to Netscape cookie format
    let mut lines = vec!["# Netscape HTTP Cookie File".to_string()];
    for c in &cookies {
        let http_only_prefix = if c.http_only { "#HttpOnly_" } else { "" };
        let domain = &c.domain;
        let flag = if domain.starts_with('.') {
            "TRUE"
        } else {
            "FALSE"
        };
        let secure = if c.secure { "TRUE" } else { "FALSE" };
        let expires = c.expires.unwrap_or(0);
        lines.push(format!(
            "{http_only_prefix}{domain}\t{flag}\t{path}\t{secure}\t{expires}\t{name}\t{value}",
            path = c.path,
            name = c.name,
            value = c.value,
        ));
    }
    let content = lines.join("\n") + "\n";

    let home = settings::home_dir();
    let cookie_dir = PathBuf::from(&home).join(".streamverse").join("auth");
    if let Err(e) = fs::create_dir_all(&cookie_dir) {
        return Err(format!("创建 Cookie 目录失败：{e}"));
    }
    let cookie_output = cookie_dir.join(format!("saved-{platform}-cookies.txt"));
    fs::write(&cookie_output, content).map_err(|e| format!("写入 Cookie 文件失败：{e}"))?;

    Ok(cookie_output.to_string_lossy().to_string())
}

/// Extract cookies from a browser: try `rookie` first (native, bypasses DPAPI),
/// fall back to yt-dlp `--cookies-from-browser` if rookie fails.
pub fn extract_browser_cookies(browser: &str, platform: &str) -> Result<String, String> {
    // Try rookie first — works even with Chrome App-Bound Encryption
    match extract_cookies_via_rookie(browser, platform) {
        Ok(path) => return Ok(path),
        Err(rookie_err) => {
            eprintln!("[rookie] Cookie extraction failed: {rookie_err}");
            // Fall through to yt-dlp method
        }
    }

    let ytdlp_binary = resolve_ytdlp_path()?;

    let test_url = match platform {
        "douyin" => "https://www.douyin.com",
        "bilibili" => "https://www.bilibili.com",
        "youtube" => "https://www.youtube.com",
        _ => return Err(format!("不支持的平台：{platform}")),
    };

    let home = settings::home_dir();
    let cookie_dir = PathBuf::from(&home).join(".streamverse").join("auth");
    if let Err(e) = fs::create_dir_all(&cookie_dir) {
        return Err(format!("创建 Cookie 目录失败：{e}"));
    }
    let cookie_output = cookie_dir.join(format!("saved-{platform}-cookies.txt"));

    let browser_arg = normalize_cookie_browser_arg(browser);

    let mut cmd = silent_command(&ytdlp_binary);
    cmd.arg("--cookies-from-browser")
        .arg(&browser_arg)
        .arg("--cookies")
        .arg(&cookie_output)
        .arg("--skip-download")
        .arg("--no-warnings")
        .arg("--quiet")
        .arg(test_url);

    extend_runtime_path(&mut cmd);

    let output = cmd.output().map_err(|e| format!("启动 yt-dlp 失败：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Failed to decrypt")
            || stderr.contains("DPAPI")
            || stderr.contains("Keychain")
            || stderr.contains("Could not get key")
        {
            return Err(format!(
                "无法自动获取 {} 的 Cookie（浏览器安全限制）。请使用「手动粘贴」方式：在浏览器中按 F12 打开开发者工具，从网络请求头中复制 Cookie 值，粘贴到设置里的 Cookie 文本框中保存即可。",
                browser.to_uppercase()
            ));
        }
        return Err(format!(
            "从 {} 提取 Cookie 失败：{}",
            browser,
            stderr.trim()
        ));
    }

    if !cookie_output.exists() || fs::metadata(&cookie_output).map(|m| m.len()).unwrap_or(0) == 0 {
        return Err(format!(
            "Cookie 文件为空，请先在 {} 中登录 {} 后再试。",
            browser.to_uppercase(),
            platform
        ));
    }

    Ok(cookie_output.to_string_lossy().to_string())
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

    if let Some(browser) = cookie_browser {
        let browser_arg = normalize_cookie_browser_arg(browser);
        command.arg("--cookies-from-browser").arg(browser_arg);
    }
}

fn normalize_cookie_browser_arg(browser: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        let lower = browser.to_lowercase();
        if matches!(lower.as_str(), "chrome" | "edge") {
            ensure_chrome_cookie_unlock_plugin();
        }
    }
    browser.to_string()
}

/// Ensure the ChromeCookieUnlock yt-dlp plugin is installed so that
/// yt-dlp can read Chrome cookies even when Chrome is running.
/// See pack_common.rs for the full documentation.
#[cfg(target_os = "windows")]
fn ensure_chrome_cookie_unlock_plugin() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Ok(appdata) = env::var("APPDATA") else {
            return;
        };
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
        let _ = fs::write(&target, include_str!("chrome_cookie_unlock_plugin.py"));
    });
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
        .arg("8")
        .arg("--http-chunk-size")
        .arg("10485760")
        .arg("--retries")
        .arg("10")
        .arg("--fragment-retries")
        .arg("10")
        .arg("--buffer-size")
        .arg("1M");
}

fn append_network_args(command: &mut Command, proxy_url: Option<&str>, speed_limit: Option<&str>) {
    match proxy_url.filter(|v| !v.trim().is_empty()) {
        Some(proxy) => {
            command.arg("--proxy").arg(proxy);
        }
        None => {
            command.arg("--proxy").arg("");
        }
    }
    if let Some(limit) = speed_limit.filter(|v| !v.trim().is_empty()) {
        command.arg("--limit-rate").arg(limit);
    }
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

pub fn open_in_file_manager(path: &str, reveal_parent: bool) -> Result<(), String> {
    let target = PathBuf::from(path);
    if !target.exists() && !reveal_parent {
        fs::create_dir_all(&target).map_err(|error| format!("创建目录失败：{error}"))?;
    }

    let resolved_target = if target.exists() {
        target
    } else if reveal_parent {
        target
            .parent()
            .map(Path::to_path_buf)
            .filter(|parent| parent.exists())
            .ok_or_else(|| "目标路径不存在，无法在文件管理器中打开。".to_string())?
    } else {
        return Err("目标路径不存在，无法在文件管理器中打开。".to_string());
    };

    #[cfg(target_os = "windows")]
    {
        return open_in_file_manager_windows(&resolved_target, reveal_parent);
    }

    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = Command::new("open");
            if reveal_parent && !resolved_target.is_dir() {
                command.arg("-R");
            }
            command.arg(&resolved_target);
            command
        };

        #[cfg(all(unix, not(target_os = "macos")))]
        let mut command = {
            let mut command = Command::new("xdg-open");
            if reveal_parent && !resolved_target.is_dir() {
                command.arg(
                    resolved_target
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| resolved_target.clone()),
                );
            } else {
                command.arg(&resolved_target);
            }
            command
        };

        let status = command
            .status()
            .map_err(|error| format!("打开文件管理器失败：{error}"))?;

        if status.success() {
            Ok(())
        } else {
            Err("文件管理器没有成功打开目标路径。".to_string())
        }
    }
}

#[cfg(target_os = "windows")]
fn open_in_file_manager_windows(target: &Path, reveal_parent: bool) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
    };
    use windows_sys::Win32::UI::Shell::{
        ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems, ShellExecuteW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW;

    fn to_wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }

    unsafe {
        let com_result = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED as u32);
        let should_uninitialize = com_result >= 0;

        let result = if reveal_parent && !target.is_dir() {
            let wide_target = to_wide(target.as_os_str());
            let item_id_list = ILCreateFromPathW(wide_target.as_ptr());
            if item_id_list.is_null() {
                Err("无法解析目标路径，文件管理器未能打开。".to_string())
            } else {
                let status = SHOpenFolderAndSelectItems(item_id_list, 0, std::ptr::null(), 0);
                ILFree(item_id_list as _);
                if status >= 0 {
                    Ok(())
                } else {
                    Err("文件管理器没有成功打开目标路径。".to_string())
                }
            }
        } else {
            let open = to_wide(OsStr::new("open"));
            let directory = if target.is_dir() {
                target
            } else {
                target.parent().unwrap_or(target)
            };
            let wide_target = to_wide(directory.as_os_str());
            let handle = ShellExecuteW(
                std::ptr::null_mut(),
                open.as_ptr(),
                wide_target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOW,
            ) as isize;
            if handle > 32 {
                Ok(())
            } else {
                Err("文件管理器没有成功打开目标路径。".to_string())
            }
        };

        if should_uninitialize {
            CoUninitialize();
        }

        result
    }
}

fn upsert_task(task_store: &task_store::TaskStore, next: DownloadTask) {
    task_store::upsert_task(task_store, next);
}

fn fail_task(task_store: &task_store::TaskStore, task_id: &str, reason: String) {
    let _ = task_store::mutate_task(task_store, task_id, |task| {
        task.status = "failed".to_string();
        task.eta_text = "失败".to_string();
        task.message = Some(reason);
    });
}

fn cancel_task_update(task_store: &task_store::TaskStore, task_id: &str) {
    let _ = task_store::mutate_task(task_store, task_id, |task| {
        task.status = "cancelled".to_string();
        task.eta_text = "已取消".to_string();
        task.message = Some("下载已取消，临时文件已清理。".to_string());
    });
}

#[cfg(test)]
fn map_formats(platform: &str, raw_formats: Vec<RawFormat>) -> Vec<VideoFormat> {
    match platform {
        "bilibili" => map_bilibili_formats(raw_formats),
        _ => map_generic_formats(raw_formats),
    }
}

#[cfg(test)]
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
            file_size_bytes: None,
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

    if formats.is_empty() {
        formats.push(VideoFormat {
            id: "best".to_string(),
            label: "Best Available".to_string(),
            resolution: "Auto".to_string(),
            bitrate_kbps: 0,
            codec: "Auto".to_string(),
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
        });
    }

    formats
}

#[cfg(test)]
fn map_bilibili_formats(raw_formats: Vec<RawFormat>) -> Vec<VideoFormat> {
    let muxed = raw_formats
        .iter()
        .filter(|format| has_muxed_video(format))
        .map(|format| build_video_format(format, false, None))
        .collect::<Vec<_>>();

    if !muxed.is_empty() {
        return finalize_mapped_formats(muxed);
    }

    let best_audio = raw_formats
        .iter()
        .filter(|format| is_audio_only(format))
        .max_by_key(|format| format.tbr.unwrap_or_default().round() as u32);

    let mut dash_formats = raw_formats
        .iter()
        .filter(|format| is_video_only(format))
        .map(|format| build_video_format(format, best_audio.is_some(), best_audio))
        .collect::<Vec<_>>();

    if dash_formats.is_empty() {
        dash_formats.push(VideoFormat {
            id: "best".to_string(),
            label: "自动选择".to_string(),
            resolution: "Auto".to_string(),
            bitrate_kbps: 0,
            codec: "Auto".to_string(),
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
        });
    }

    finalize_mapped_formats(dash_formats)
}

#[cfg(test)]
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
        file_size_bytes: None,
    }
}

#[cfg(test)]
fn finalize_mapped_formats(mut formats: Vec<VideoFormat>) -> Vec<VideoFormat> {
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn direct_media_url(format: &RawFormat) -> Option<String> {
    match format.protocol.as_deref() {
        Some("http") | Some("https") | None => {
            format.url.clone().filter(|url| !url.trim().is_empty())
        }
        _ => None,
    }
}

#[cfg(test)]
fn format_referer(format: &RawFormat) -> Option<String> {
    format
        .http_headers
        .as_ref()
        .and_then(|headers| headers.referer.clone())
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
fn format_user_agent(format: &RawFormat) -> Option<String> {
    format
        .http_headers
        .as_ref()
        .and_then(|headers| headers.user_agent.clone())
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
fn build_label(format: &RawFormat) -> String {
    if let Some(height) = format.height {
        return format!("{height}P");
    }

    format
        .format_note
        .clone()
        .unwrap_or_else(|| "默认格式".to_string())
}

#[cfg(test)]
fn build_resolution(format: &RawFormat) -> String {
    match (format.width, format.height) {
        (Some(width), Some(height)) => format!("{width}x{height}"),
        _ => "Auto".to_string(),
    }
}

#[cfg(test)]
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

fn readable_error(stderr: &[u8], fallback: &str) -> String {
    let message = String::from_utf8_lossy(stderr);
    let trimmed = message.trim();

    if trimmed.contains("Fresh cookies") {
        return "当前抖音链接需要新鲜浏览器 Cookie。请在应用顶部选择 Chrome、Safari 等浏览器来源后重新解析。".to_string();
    }

    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

struct ProgressLine {
    percent: u32,
    speed_text: String,
    eta_text: String,
}

fn parse_progress_line(line: &str) -> Option<ProgressLine> {
    let payload = line
        .strip_prefix("download:progress:")
        .or_else(|| line.strip_prefix("progress:"))?;
    let mut parts = payload.split('|');
    let percent_text = parts.next()?.replace('%', "").trim().to_string();
    let speed_text = parts.next()?.trim().to_string();
    let eta_text = parts.next()?.trim().to_string();
    let percent = percent_text.parse::<f32>().ok()?.round() as u32;

    Some(ProgressLine {
        percent,
        speed_text: if speed_text.is_empty() {
            "-".to_string()
        } else {
            speed_text
        },
        eta_text: if eta_text.is_empty() {
            "—".to_string()
        } else {
            eta_text
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{
        compute_percent, dash_download_worker, direct_download_worker, ffmpeg_available,
        first_existing_path, map_formats, new_task_controller_store, parse_progress_line,
        parse_speed_limit_bytes, persist_download_artifacts, prepare_output_layout,
        DownloadArtifacts, RawFormat, RawHeaders, TaskController,
    };
    use crate::{pack_manager, task_store, DownloadContentSelection};
    use std::fs;
    use std::io::ErrorKind;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn computes_percent_safely() {
        assert_eq!(compute_percent(50, 100), 50);
        assert_eq!(compute_percent(0, 0), 0);
        assert_eq!(compute_percent(101, 100), 100);
    }

    #[test]
    fn parses_decimal_speed_limits() {
        assert_eq!(parse_speed_limit_bytes(Some("1.5M")), Some(1_572_864));
        assert_eq!(parse_speed_limit_bytes(Some("256K")), Some(262_144));
    }

    #[test]
    fn ignores_invalid_speed_limits() {
        assert_eq!(parse_speed_limit_bytes(Some("0")), None);
        assert_eq!(parse_speed_limit_bytes(Some("abc")), None);
    }

    #[test]
    fn parses_download_prefixed_progress_lines() {
        let line = parse_progress_line("download:progress:42.5%|3.1MiB/s|00:19").unwrap();
        assert_eq!(line.percent, 43);
        assert_eq!(line.speed_text, "3.1MiB/s");
        assert_eq!(line.eta_text, "00:19");
    }

    #[test]
    fn direct_download_worker_saves_file_and_updates_task() {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind test listener: {error}"),
        };
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request_buffer = [0u8; 1024];
                let _ = stream.read(&mut request_buffer);
                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: video/mp4\r\n",
                    "Content-Length: 5\r\n",
                    "\r\n",
                    "hello"
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        let task_store = task_store::new_empty_task_store();
        let controller_store = new_task_controller_store();
        let options = video_only_options();
        let layout = prepare_output_layout(&output_dir, "测试下载", "aweme-1", &options).unwrap();

        direct_download_worker(
            Arc::clone(&task_store),
            controller_store,
            "task-1".to_string(),
            "测试下载".to_string(),
            "720P".to_string(),
            sample_artifacts(None),
            layout,
            options,
            format!("http://{address}/demo.mp4"),
            false,
            None,
            None,
            None,
            Arc::new(TaskController::new(true, true)),
        );

        server.join().unwrap();

        let tasks = task_store::list_tasks(&task_store);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, "completed");

        let output_path = PathBuf::from(tasks[0].output_path.clone().unwrap());
        let file_content = fs::read(&output_path).unwrap();
        assert_eq!(file_content, b"hello");
        assert!(!output_path.with_extension("txt").exists());
        assert!(!output_path.with_extension("json").exists());

        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn dash_download_worker_reports_intermediate_progress() {
        if !ffmpeg_available(None) {
            return;
        }

        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind test listener: {error}"),
        };
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() <= deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request_buffer = [0u8; 1024];
                        let bytes = stream.read(&mut request_buffer).unwrap_or(0);
                        let request = String::from_utf8_lossy(&request_buffer[..bytes]);
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("/");

                        match path {
                            "/audio.m4a" if request.starts_with("HEAD ") => {
                                let response = concat!(
                                    "HTTP/1.1 200 OK\r\n",
                                    "Connection: close\r\n",
                                    "Content-Type: audio/mp4\r\n",
                                    "Content-Length: 131072\r\n",
                                    "\r\n"
                                );
                                let _ = stream.write_all(response.as_bytes());
                            }
                            "/video.m4s" => {
                                let header = concat!(
                                    "HTTP/1.1 200 OK\r\n",
                                    "Connection: close\r\n",
                                    "Content-Type: video/mp4\r\n",
                                    "Content-Length: 524288\r\n",
                                    "\r\n"
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let chunk = vec![b'v'; 32 * 1024];
                                for _ in 0..16 {
                                    let _ = stream.write_all(&chunk);
                                    let _ = stream.flush();
                                    thread::sleep(Duration::from_millis(30));
                                }
                            }
                            "/audio.m4a" => {
                                let header = concat!(
                                    "HTTP/1.1 200 OK\r\n",
                                    "Connection: close\r\n",
                                    "Content-Type: audio/mp4\r\n",
                                    "Content-Length: 131072\r\n",
                                    "\r\n"
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let chunk = vec![b'a'; 16 * 1024];
                                for _ in 0..8 {
                                    let _ = stream.write_all(&chunk);
                                    let _ = stream.flush();
                                    thread::sleep(Duration::from_millis(30));
                                }
                            }
                            _ => {
                                let response = "HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
                                let _ = stream.write_all(response.as_bytes());
                            }
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });

        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        let task_store = task_store::new_empty_task_store();
        let controller_store = new_task_controller_store();
        let options = video_only_options();
        let layout =
            prepare_output_layout(&output_dir, "测试合流下载", "aweme-2", &options).unwrap();
        let ffmpeg_path = match pack_manager::ensure_media_engine_installed() {
            Ok(Some(path)) => path.to_str().map(|value| value.to_string()),
            Ok(None) => None,
            Err(_) => None,
        };

        let worker_store = Arc::clone(&task_store);
        let worker_controllers = Arc::clone(&controller_store);
        let worker = thread::spawn(move || {
            dash_download_worker(
                worker_store,
                worker_controllers,
                "task-dash".to_string(),
                "测试合流下载".to_string(),
                "1080P".to_string(),
                sample_artifacts(None),
                layout,
                options,
                format!("http://{address}/video.m4s"),
                format!("http://{address}/audio.m4a"),
                false,
                None,
                None,
                None,
                None,
                ffmpeg_path,
                Arc::new(TaskController::new(true, true)),
            );
        });

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut saw_intermediate_progress = false;
        loop {
            let tasks = task_store::list_tasks(&task_store);
            if let Some(task) = tasks.iter().find(|task| task.id == "task-dash") {
                if task.progress > 0 && task.progress < 100 {
                    saw_intermediate_progress = true;
                }

                if task.status == "completed" {
                    assert!(saw_intermediate_progress);
                    let output_path = PathBuf::from(task.output_path.clone().unwrap());
                    assert!(output_path.exists());
                    assert_eq!(
                        output_path.extension().and_then(|value| value.to_str()),
                        Some("mp4")
                    );
                    break;
                }

                if task.status == "failed" {
                    assert!(saw_intermediate_progress);
                    assert!(
                        task.message
                            .as_deref()
                            .is_some_and(|message| message
                                .contains("Invalid data found when processing input")),
                        "dash task failed unexpectedly: {:?}",
                        task.message
                    );
                    break;
                }
            }

            if Instant::now() > deadline {
                panic!("dash task did not complete before deadline");
            }
            thread::sleep(Duration::from_millis(50));
        }

        worker.join().unwrap();
        server.join().unwrap();
        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn persist_download_artifacts_writes_selected_sidecars_into_bundle_folder() {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind test listener: {error}"),
        };
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request_buffer = [0u8; 1024];
                let _ = stream.read(&mut request_buffer);
                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: image/jpeg\r\n",
                    "Content-Length: 5\r\n",
                    "\r\n",
                    "cover"
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        let options = bundle_options();
        let layout = prepare_output_layout(&output_dir, "测试下载", "aweme-1", &options).unwrap();
        let video_path = layout.video_path("mp4");
        fs::write(&video_path, b"hello").unwrap();

        let summary = persist_download_artifacts(
            &layout,
            Some(&video_path),
            &sample_artifacts(Some(format!("http://{address}/cover.jpg"))),
            Some("1080P"),
            &options,
            None,
        );

        server.join().unwrap();

        assert!(summary.text_written);
        assert!(summary.metadata_written);
        assert!(summary.cover_written);
        assert!(summary.warnings.is_empty());
        let bundle_dir = video_path.parent().unwrap().to_path_buf();
        assert!(bundle_dir.join("caption.txt").exists());
        assert!(bundle_dir.join("metadata.json").exists());
        assert!(bundle_dir.join("cover.jpg").exists());

        let json = fs::read_to_string(bundle_dir.join("metadata.json")).unwrap();
        assert!(json.contains("测试下载"));
        assert!(json.contains("这是一段测试文案"));

        let _ = fs::remove_dir_all(output_dir);
    }

    fn unique_test_dir() -> PathBuf {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "streamverse-test-{}-{counter}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn sample_artifacts(cover_url: Option<String>) -> DownloadArtifacts {
        DownloadArtifacts {
            platform: "douyin".to_string(),
            source_url: "https://example.com/video".to_string(),
            asset_id: "asset-1".to_string(),
            title: "测试下载".to_string(),
            author: "测试作者".to_string(),
            publish_date: "2026-03-30".to_string(),
            caption: "这是一段测试文案".to_string(),
            cover_url,
            referer: Some("https://www.douyin.com/".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
        }
    }

    fn video_only_options() -> DownloadContentSelection {
        DownloadContentSelection {
            download_video: true,
            download_audio: false,
            download_cover: false,
            download_caption: false,
            download_metadata: false,
        }
    }

    fn bundle_options() -> DownloadContentSelection {
        DownloadContentSelection {
            download_video: true,
            download_audio: false,
            download_cover: true,
            download_caption: true,
            download_metadata: true,
        }
    }

    #[test]
    fn builds_bilibili_dash_formats_with_audio_pairing() {
        let formats = map_formats(
            "bilibili",
            vec![
                RawFormat {
                    format_id: Some("30080".to_string()),
                    format_note: Some("1080P".to_string()),
                    ext: Some("mp4".to_string()),
                    width: Some(1920),
                    height: Some(1080),
                    vcodec: Some("avc1.640032".to_string()),
                    acodec: Some("none".to_string()),
                    tbr: Some(2509.0),
                    url: Some("https://example.com/video.m4s".to_string()),
                    protocol: Some("https".to_string()),
                    http_headers: Some(RawHeaders {
                        referer: Some("https://www.bilibili.com/".to_string()),
                        user_agent: Some("Mozilla/5.0".to_string()),
                    }),
                },
                RawFormat {
                    format_id: Some("30280".to_string()),
                    format_note: Some("高码率音频".to_string()),
                    ext: Some("m4a".to_string()),
                    width: None,
                    height: None,
                    vcodec: Some("none".to_string()),
                    acodec: Some("mp4a.40.2".to_string()),
                    tbr: Some(319.0),
                    url: Some("https://example.com/audio.m4s".to_string()),
                    protocol: Some("https".to_string()),
                    http_headers: Some(RawHeaders {
                        referer: Some("https://www.bilibili.com/".to_string()),
                        user_agent: Some("Mozilla/5.0".to_string()),
                    }),
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
    fn first_existing_path_skips_missing_candidates() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).unwrap();
        let existing = temp_dir.join("ffmpeg");
        fs::write(&existing, b"binary").unwrap();

        let selected = first_existing_path([
            temp_dir.join("missing-1"),
            existing.clone(),
            temp_dir.join("missing-2"),
        ]);

        assert_eq!(selected, Some(existing));
        let _ = fs::remove_dir_all(temp_dir);
    }
}
