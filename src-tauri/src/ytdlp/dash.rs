use super::artifact::{
    build_completion_message, persist_download_artifacts, DownloadArtifacts, OutputLayout,
};
use super::controller::{
    current_speed_limit, parse_speed_limit_bytes, throttle_transfer, unregister_controller,
    TaskController, TaskControllerStore,
};
use super::engine::{build_http_client, cancel_task_update, fail_task, silent_command, upsert_task};
use super::errors::readable_error;
use super::files::{infer_extension, open_in_file_manager, unique_output_path};
use super::progress::{dash_combined_progress, dash_phase_progress, SpeedTracker};
use crate::{download_history, task_store, DownloadContentSelection, DownloadTask};
use reqwest::blocking::Client;
use reqwest::header::{REFERER, USER_AGENT};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

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

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]

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

#[allow(clippy::too_many_arguments)]
pub(super) fn dash_download_worker(
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
            cover_url: artifacts.cover_url.clone(),
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
                    cover_url: artifacts.cover_url.clone(),
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
                    cover_url: artifacts.cover_url.clone(),
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
            cover_url: artifacts.cover_url.clone(),
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
            cover_url: artifacts.cover_url.clone(),
        },
    );

    if auto_reveal_in_file_manager {
        if let Some(path) = artifact_summary.output_path.as_deref() {
            let _ = open_in_file_manager(path, output_layout.bundle_dir.is_none());
        }
    }

    download_history::record_download(
        &artifacts.platform,
        &artifacts.asset_id,
        &artifacts.title,
        artifacts.cover_url.clone(),
        artifact_summary.output_path.clone(),
    );
}
