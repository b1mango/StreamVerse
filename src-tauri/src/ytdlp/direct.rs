use super::artifact::{
    build_completion_message, persist_download_artifacts, DownloadArtifacts, OutputLayout,
};
use super::controller::{
    current_speed_limit, parse_speed_limit_bytes, throttle_transfer, unregister_controller,
    TaskController, TaskControllerStore,
};
use super::engine::{build_http_client, cancel_task_update, fail_task, upsert_task};
use super::files::{infer_extension, open_in_file_manager};
use super::progress::{compute_percent, SpeedTracker};
use crate::{download_history, task_store, DownloadContentSelection, DownloadTask};
use reqwest::header::{REFERER, USER_AGENT};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[allow(clippy::too_many_arguments)]
pub(super) fn direct_download_worker(
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
                    cover_url: artifacts.cover_url.clone(),
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
                    cover_url: artifacts.cover_url.clone(),
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
                    cover_url: artifacts.cover_url.clone(),
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
