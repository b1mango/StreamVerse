use crate::{
    douyin, formats, parser, DownloadContentSelection, DownloadTask, VideoAsset, VideoFormat,
    DEFAULT_GRADIENT,
};
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_TYPE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::cmp::Reverse;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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

#[derive(Clone)]
struct DownloadArtifacts {
    source_url: String,
    aweme_id: String,
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
    aweme_id: &'a str,
    title: &'a str,
    author: &'a str,
    publish_date: &'a str,
    caption: &'a str,
    source_url: &'a str,
    format_label: &'a str,
    cover_url: Option<&'a str>,
    generated_by: &'static str,
}

#[derive(Deserialize)]
struct RawFormat {
    format_id: Option<String>,
    format_note: Option<String>,
    ext: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    vcodec: Option<String>,
    acodec: Option<String>,
    tbr: Option<f64>,
}

impl DownloadContentSelection {
    fn has_any_selection(&self) -> bool {
        self.download_video
            || self.download_cover
            || self.download_caption
            || self.download_metadata
    }

    fn selected_count(&self) -> usize {
        usize::from(self.download_video)
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
}

pub fn new_task_controller_store() -> TaskControllerStore {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn analyze_url(source_url: &str, cookie_browser: Option<&str>) -> Result<VideoAsset, String> {
    ensure_ytdlp_available()?;

    let mut douyin_bridge_error = None::<String>;
    if douyin::is_douyin_url(source_url) {
        if let Some(browser) = cookie_browser {
            match douyin::analyze_url(source_url, browser) {
                Ok(asset) => return Ok(asset),
                Err(error) => douyin_bridge_error = Some(error),
            }
        }
    }

    let mut command = Command::new("yt-dlp");
    command.args(["--dump-single-json", "--no-playlist"]);
    append_cookie_args(&mut command, cookie_browser);

    let output = command
        .arg(source_url)
        .output()
        .map_err(|error| format!("启动 yt-dlp 失败：{error}"))?;

    if !output.status.success() {
        let fallback_error = readable_error(&output.stderr, "解析链接失败");
        return Err(match douyin_bridge_error {
            Some(bridge_error) => format!("{bridge_error}\n\n备用解析器返回：{fallback_error}"),
            None => fallback_error,
        });
    }

    let raw: RawInfo = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("解析 yt-dlp 响应失败：{error}"))?;

    let formats = formats::dedupe_formats(map_formats(raw.formats.unwrap_or_default()));

    Ok(VideoAsset {
        aweme_id: raw.id.unwrap_or_else(|| "unknown".to_string()),
        source_url: source_url.to_string(),
        title: raw.title.unwrap_or_else(|| "未命名作品".to_string()),
        author: raw
            .uploader
            .or(raw.creator)
            .or(raw.channel)
            .unwrap_or_else(|| "未知作者".to_string()),
        duration_seconds: raw.duration.unwrap_or_default().round() as u32,
        publish_date: format_publish_date(raw.upload_date.as_deref()),
        caption: raw
            .description
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| match cookie_browser {
                Some(browser) => format!("已使用 {browser} 的浏览器 Cookie 完成解析。"),
                None => "解析完成，可以直接选择格式开始下载。".to_string(),
            }),
        cover_url: raw.thumbnail.filter(|url| !url.trim().is_empty()),
        cover_gradient: DEFAULT_GRADIENT.to_string(),
        formats,
    })
}

pub fn download_video(
    task_store: Arc<Mutex<Vec<DownloadTask>>>,
    controller_store: TaskControllerStore,
    source_url: &str,
    aweme_id: &str,
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
    cookie_browser: Option<&str>,
    direct_url: Option<&str>,
    referer: Option<&str>,
    user_agent: Option<&str>,
) -> Result<DownloadTask, String> {
    if !download_options.has_any_selection() {
        return Err("至少要选择一种要保存的内容。".to_string());
    }

    let output_dir = PathBuf::from(save_directory);
    fs::create_dir_all(&output_dir).map_err(|error| format!("创建下载目录失败：{error}"))?;

    let safe_title = parser::sanitize_filename(title);
    let output_layout = prepare_output_layout(&output_dir, &safe_title, aweme_id, &download_options)?;
    let supports_pause = download_options.download_video && direct_url.is_some();
    let supports_cancel = true;
    let format_display = build_task_format_label(format_label, &download_options);
    let task_id = if download_options.download_video {
        format!(
            "task-{aweme_id}-{}",
            format_id.unwrap_or("best").trim().replace(['/', ' '], "-")
        )
    } else {
        format!("task-{aweme_id}-extras")
    };
    let controller = Arc::new(TaskController::new(supports_pause, supports_cancel));
    register_controller(&controller_store, &task_id, Arc::clone(&controller));
    let task = DownloadTask {
        id: task_id.clone(),
        title: title.to_string(),
        progress: 0,
        speed_text: "-".to_string(),
        format_label: format_display.clone(),
        status: "queued".to_string(),
        eta_text: "等待中".to_string(),
        message: Some("下载任务已开始。".to_string()),
        output_path: None,
        supports_pause,
        supports_cancel,
    };

    upsert_task(&task_store, task.clone());

    let source_url = source_url.to_string();
    let title = title.to_string();
    let format_id_text = format_id.map(str::to_string);
    let format_label_text = format_label.map(str::to_string);
    let aweme_id_text = aweme_id.to_string();
    let referer = referer.map(str::to_string);
    let user_agent = user_agent.map(str::to_string);
    let artifacts = DownloadArtifacts {
        source_url: source_url.to_string(),
        aweme_id: aweme_id_text.clone(),
        title: title.clone(),
        author: author.to_string(),
        publish_date: publish_date.to_string(),
        caption: caption.to_string(),
        cover_url: cover_url.map(str::to_string),
        referer: referer.clone(),
        user_agent: user_agent.clone(),
    };
    let cookie_browser = cookie_browser.map(str::to_string);
    let direct_url = direct_url.map(str::to_string);

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
                controller,
            );
        });

        return Ok(task);
    }

    if let Some(direct_url) = direct_url {
        thread::spawn(move || {
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
                controller,
            );
        });

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
        let artifacts = artifacts.clone();
        let mut command = Command::new("yt-dlp");
        command
            .arg("--no-playlist")
            .arg("--newline")
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

        command.arg("--format").arg(&format_id_text);
        append_cookie_args(&mut command, cookie_browser.as_deref());

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
                title: title.clone(),
                progress: 0,
                speed_text: "-".to_string(),
                format_label: format_display.clone(),
                status: "downloading".to_string(),
                eta_text: "准备中".to_string(),
                message: Some("正在下载…".to_string()),
                output_path: None,
                supports_pause: false,
                supports_cancel: true,
            },
        );

        let output_path = Arc::new(Mutex::new(None::<String>));
        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));

        let stdout_handle = child.stdout.take().map(|stdout| {
            let output_path = Arc::clone(&output_path);
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(path) = line.strip_prefix("output:") {
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
                    );
                    if output_layout.bundle_dir.is_some() {
                        summary.output_path = output_layout.bundle_entry_path();
                    }
                    summary
                })
                .unwrap_or_else(|| ArtifactSummary {
                    destination_path: output_layout.destination_path(),
                    warnings: vec!["已下载视频，但未能确认最终文件路径，未生成封面和文案文件。".to_string()],
                    ..ArtifactSummary::default()
                });
            unregister_controller(&controller_store, &task_id);
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
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
                },
            );

            if auto_reveal_in_file_manager {
                if let Some(path) = artifact_summary.output_path.clone() {
                    let _ = open_in_file_manager(&path, output_layout.bundle_dir.is_none());
                } else {
                    let _ = open_in_file_manager(&artifact_summary.destination_path, false);
                }
            }
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
    });

    Ok(task)
}

fn metadata_only_worker(
    task_store: Arc<Mutex<Vec<DownloadTask>>>,
    controller_store: TaskControllerStore,
    task_id: String,
    title: String,
    format_label: String,
    artifacts: DownloadArtifacts,
    output_layout: OutputLayout,
    download_options: DownloadContentSelection,
    auto_reveal_in_file_manager: bool,
    controller: Arc<TaskController>,
) {
    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id.clone(),
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
        },
    );

    if auto_reveal_in_file_manager {
        if let Some(path) = revealed_path.as_deref() {
            let _ = open_in_file_manager(path, output_layout.bundle_dir.is_none());
        }
    }
}

fn direct_download_worker(
    task_store: Arc<Mutex<Vec<DownloadTask>>>,
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
    controller: Arc<TaskController>,
) {
    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id.clone(),
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
        },
    );

    let client = match build_http_client() {
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

    let mut response = match request.send() {
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

    let mut file = match File::create(&temp_path) {
        Ok(file) => file,
        Err(error) => {
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, format!("创建临时文件失败：{error}"));
            return;
        }
    };

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded_bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    let started_at = Instant::now();
    let mut last_report = Instant::now();
    let mut paused_for = Duration::ZERO;

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
                    title: title.clone(),
                    progress: compute_percent(downloaded_bytes, total_bytes),
                    speed_text: human_speed(
                        downloaded_bytes,
                        started_at.elapsed().saturating_sub(paused_for),
                    ),
                    format_label: format_label.clone(),
                    status: "paused".to_string(),
                    eta_text: human_eta(
                        downloaded_bytes,
                        total_bytes,
                        started_at.elapsed().saturating_sub(paused_for),
                    ),
                    message: Some("已暂停，可以继续或取消。".to_string()),
                    output_path: None,
                    supports_pause: true,
                    supports_cancel: true,
                },
            );

            let paused_at = Instant::now();
            while controller.is_pause_requested() {
                if controller.is_cancel_requested() {
                    let _ = fs::remove_file(&temp_path);
                    unregister_controller(&controller_store, &task_id);
                    cancel_task_update(&task_store, &task_id);
                    return;
                }
                thread::sleep(Duration::from_millis(180));
            }

            paused_for += paused_at.elapsed();
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    title: title.clone(),
                    progress: compute_percent(downloaded_bytes, total_bytes),
                    speed_text: human_speed(
                        downloaded_bytes,
                        started_at.elapsed().saturating_sub(paused_for),
                    ),
                    format_label: format_label.clone(),
                    status: "downloading".to_string(),
                    eta_text: human_eta(
                        downloaded_bytes,
                        total_bytes,
                        started_at.elapsed().saturating_sub(paused_for),
                    ),
                    message: Some("继续下载…".to_string()),
                    output_path: None,
                    supports_pause: true,
                    supports_cancel: true,
                },
            );
        }

        let bytes_read = match response.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                unregister_controller(&controller_store, &task_id);
                fail_task(&task_store, &task_id, format!("读取下载响应失败：{error}"));
                return;
            }
        };

        if bytes_read == 0 {
            break;
        }

        if let Err(error) = file.write_all(&buffer[..bytes_read]) {
            let _ = fs::remove_file(&temp_path);
            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, format!("写入下载文件失败：{error}"));
            return;
        }

        downloaded_bytes += bytes_read as u64;
        if last_report.elapsed() >= Duration::from_millis(250) {
            let active_elapsed = started_at.elapsed().saturating_sub(paused_for);
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    title: title.clone(),
                    progress: compute_percent(downloaded_bytes, total_bytes),
                    speed_text: human_speed(downloaded_bytes, active_elapsed),
                    format_label: format_label.clone(),
                    status: "downloading".to_string(),
                    eta_text: human_eta(downloaded_bytes, total_bytes, active_elapsed),
                    message: Some("正在下载…".to_string()),
                    output_path: None,
                    supports_pause: true,
                    supports_cancel: true,
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
    );
    if output_layout.bundle_dir.is_some() {
        artifact_summary.output_path = output_layout.bundle_entry_path();
    }
    unregister_controller(&controller_store, &task_id);

    upsert_task(
        &task_store,
        DownloadTask {
            id: task_id,
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
        },
    );

    if auto_reveal_in_file_manager {
        if let Some(path) = artifact_summary.output_path.as_deref() {
            let _ = open_in_file_manager(path, output_layout.bundle_dir.is_none());
        }
    }
}

fn ensure_ytdlp_available() -> Result<(), String> {
    let output = Command::new("yt-dlp")
        .arg("--version")
        .output()
        .map_err(|_| "未检测到 yt-dlp，请先安装后再继续。".to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err("yt-dlp 不可用，请检查安装是否完整。".to_string())
    }
}

fn prepare_output_layout(
    base_dir: &Path,
    safe_title: &str,
    aweme_id: &str,
    download_options: &DownloadContentSelection,
) -> Result<OutputLayout, String> {
    if download_options.needs_bundle_directory() {
        let bundle_dir = unique_output_dir(base_dir.join(safe_title));
        fs::create_dir_all(&bundle_dir).map_err(|error| format!("创建作品文件夹失败：{error}"))?;

        return Ok(OutputLayout {
            base_dir: base_dir.to_path_buf(),
            bundle_dir: Some(bundle_dir),
            single_stem: format!("{safe_title} [{aweme_id}]"),
        });
    }

    Ok(OutputLayout {
        base_dir: base_dir.to_path_buf(),
        bundle_dir: None,
        single_stem: format!("{safe_title} [{aweme_id}]"),
    })
}

fn persist_download_artifacts(
    output_layout: &OutputLayout,
    video_path: Option<&Path>,
    artifacts: &DownloadArtifacts,
    format_label: Option<&str>,
    download_options: &DownloadContentSelection,
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
            aweme_id: &artifacts.aweme_id,
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
            summary.warnings.push("当前作品没有可用封面地址。".to_string());
        }
    }

    summary
}

fn build_text_sidecar(artifacts: &DownloadArtifacts, format_label: Option<&str>) -> String {
    let mut sections = vec![
        format!("标题：{}", artifacts.title),
        format!("作者：{}", artifacts.author),
        format!("发布日期：{}", artifacts.publish_date),
        format!("作品 ID：{}", artifacts.aweme_id),
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
    output_layout: &OutputLayout,
    cover_url: &str,
    referer: Option<&str>,
    user_agent: Option<&str>,
) -> Result<PathBuf, String> {
    let client = build_http_client()?;
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

fn build_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
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
    task_store: Arc<Mutex<Vec<DownloadTask>>>,
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
    task_store: Arc<Mutex<Vec<DownloadTask>>>,
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
    task_store: Arc<Mutex<Vec<DownloadTask>>>,
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
    task_store: &Arc<Mutex<Vec<DownloadTask>>>,
    task_id: &str,
    mutator: F,
) -> Result<DownloadTask, String>
where
    F: FnOnce(&mut DownloadTask),
{
    let mut guard = task_store.lock().unwrap();
    let task = guard
        .iter_mut()
        .find(|task| task.id == task_id)
        .ok_or_else(|| "未找到对应的下载任务。".to_string())?;
    mutator(task);
    Ok(task.clone())
}

fn infer_extension(response: &reqwest::blocking::Response, direct_url: &str) -> String {
    if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
        if let Ok(content_type) = content_type.to_str() {
            if content_type.contains("video/mp4") {
                return "mp4".to_string();
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
        return 0;
    }

    ((downloaded_bytes as f64 / total_bytes as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u32
}

fn human_speed(downloaded_bytes: u64, elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return "-".to_string();
    }

    human_bytes(downloaded_bytes as f64 / seconds, "/s")
}

fn human_eta(downloaded_bytes: u64, total_bytes: u64, elapsed: Duration) -> String {
    if total_bytes == 0 || downloaded_bytes >= total_bytes {
        return "—".to_string();
    }

    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return "—".to_string();
    }

    let speed = downloaded_bytes as f64 / seconds;
    if speed <= 0.0 {
        return "—".to_string();
    }

    let remaining = ((total_bytes - downloaded_bytes) as f64 / speed).round() as u64;
    let minutes = remaining / 60;
    let seconds = remaining % 60;
    format!("{minutes:02}:{seconds:02}")
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

fn append_cookie_args(command: &mut Command, cookie_browser: Option<&str>) {
    if let Some(browser) = cookie_browser {
        command.arg("--cookies-from-browser").arg(browser);
    }
}

pub fn open_in_file_manager(path: &str, reveal_parent: bool) -> Result<(), String> {
    let target = PathBuf::from(path);
    if !target.exists() && !reveal_parent {
        fs::create_dir_all(&target).map_err(|error| format!("创建目录失败：{error}"))?;
    }

    if !target.exists() {
        return Err("目标路径不存在，无法在文件管理器中打开。".to_string());
    }

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        if reveal_parent && !target.is_dir() {
            command.arg("-R");
        }
        command.arg(&target);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        if reveal_parent && !target.is_dir() {
            command.arg(format!("/select,{}", target.to_string_lossy()));
        } else {
            command.arg(&target);
        }
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        if reveal_parent && !target.is_dir() {
            command.arg(
                target
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| target.clone()),
            );
        } else {
            command.arg(&target);
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

fn upsert_task(task_store: &Arc<Mutex<Vec<DownloadTask>>>, next: DownloadTask) {
    let mut guard = task_store.lock().unwrap();
    if let Some(existing) = guard.iter_mut().find(|task| task.id == next.id) {
        *existing = next;
    } else {
        guard.insert(0, next);
    }
}

fn fail_task(task_store: &Arc<Mutex<Vec<DownloadTask>>>, task_id: &str, reason: String) {
    let mut guard = task_store.lock().unwrap();
    if let Some(task) = guard.iter_mut().find(|task| task.id == task_id) {
        task.status = "failed".to_string();
        task.eta_text = "失败".to_string();
        task.message = Some(reason);
    }
}

fn cancel_task_update(task_store: &Arc<Mutex<Vec<DownloadTask>>>, task_id: &str) {
    let mut guard = task_store.lock().unwrap();
    if let Some(task) = guard.iter_mut().find(|task| task.id == task_id) {
        task.status = "cancelled".to_string();
        task.eta_text = "已取消".to_string();
        task.message = Some("下载已取消，临时文件已清理。".to_string());
    }
}

fn map_formats(raw_formats: Vec<RawFormat>) -> Vec<VideoFormat> {
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
            recommended: false,
            direct_url: None,
            referer: None,
            user_agent: None,
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
            recommended: true,
            direct_url: None,
            referer: None,
            user_agent: None,
        });
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
    let payload = line.strip_prefix("progress:")?;
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
        compute_percent, direct_download_worker, new_task_controller_store,
        persist_download_artifacts, prepare_output_layout, DownloadArtifacts, TaskController,
    };
    use crate::{DownloadContentSelection, DownloadTask};
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn computes_percent_safely() {
        assert_eq!(compute_percent(50, 100), 50);
        assert_eq!(compute_percent(0, 0), 0);
        assert_eq!(compute_percent(101, 100), 100);
    }

    #[test]
    fn direct_download_worker_saves_file_and_updates_task() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
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
        let task_store = Arc::new(Mutex::new(Vec::<DownloadTask>::new()));
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
            Arc::new(TaskController::new(true, true)),
        );

        server.join().unwrap();

        let tasks = task_store.lock().unwrap().clone();
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
    fn persist_download_artifacts_writes_selected_sidecars_into_bundle_folder() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
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
            source_url: "https://example.com/video".to_string(),
            aweme_id: "aweme-1".to_string(),
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
            download_cover: false,
            download_caption: false,
            download_metadata: false,
        }
    }

    fn bundle_options() -> DownloadContentSelection {
        DownloadContentSelection {
            download_video: true,
            download_cover: true,
            download_caption: true,
            download_metadata: true,
        }
    }
}
