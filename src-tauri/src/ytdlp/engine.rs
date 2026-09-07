use super::artifact::{
    build_completion_message, persist_download_artifacts, prepare_output_layout, DownloadArtifacts,
    OutputLayout,
};
#[cfg(test)]
use super::controller::new_task_controller_store;
use super::controller::{
    acquire_download_slot, current_proxy_for_platform, current_speed_limit, register_controller,
    release_download_slot, spawn_download_job, unregister_controller, TaskController,
    TaskControllerStore,
};
use super::dash::dash_download_worker;
use super::direct::direct_download_worker;
use super::errors::normalize_download_failure;
use super::files::{
    ensure_ytdlp_available, ffmpeg_available, infer_extension, open_in_file_manager,
    resolve_downloaded_video_path, resolve_ytdlp_path,
};
use super::progress::{parse_progress_line, progress_task_message};
use crate::{
    download_history, parser, platforms, provider_runtime, task_store, DownloadContentSelection,
    DownloadRequest, DownloadTask,
};
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_TYPE, REFERER, USER_AGENT};
use reqwest::Proxy;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub(super) fn silent_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd
}

const ALBUM_IMAGE_CONCURRENCY: usize = 4;

impl DownloadContentSelection {
    fn has_any_selection(&self) -> bool {
        self.download_video || self.download_audio || self.download_cover || self.download_caption
    }

    fn selected_count(&self) -> usize {
        usize::from(self.download_video)
            + usize::from(self.download_audio)
            + usize::from(self.download_cover)
            + usize::from(self.download_caption)
    }

    pub(super) fn needs_bundle_directory(&self) -> bool {
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
        labels
    }
}

pub fn download_video(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    request: DownloadRequest,
) -> Result<DownloadTask, String> {
    let platform = request.platform.as_str();
    let source_url = request.source_url.as_str();
    let asset_id = request.asset_id.as_str();
    let title = request.title.as_str();
    let author = request.author.as_str();
    let publish_date = request.publish_date.as_str();
    let cover_url = request.cover_url.as_deref();
    let image_urls = request.image_urls.clone();
    let is_album = !image_urls.is_empty();
    let format_id = request.format_id.as_deref();
    let format_label = request.format_label.as_deref();
    let save_directory = request.save_directory.as_str();
    let download_options = request.download_options.clone();
    let auto_reveal_in_file_manager = request.auto_reveal_in_file_manager;
    let ffmpeg_path = request.ffmpeg_path.as_deref();
    let cookie_browser = request.cookie_browser.as_deref();
    let cookie_file = request.cookie_file.as_deref();
    let direct_url = request.direct_url.as_deref();
    let referer = request.referer.as_deref();
    let user_agent = request.user_agent.as_deref();
    let audio_direct_url = request.audio_direct_url.as_deref();
    let audio_referer = request.audio_referer.as_deref();
    let audio_user_agent = request.audio_user_agent.as_deref();
    if !download_options.has_any_selection() {
        return Err("至少要选择一种要保存的内容。".to_string());
    }

    if download_options.download_audio && !download_options.download_video {
        return Err("提取 MP3 音频需要同时勾选视频下载。".to_string());
    }

    // B 站未登录时只能拿到 ≤720P 的低清流且容易触发 412 风控：
    // 宁可明确报错，也不静默交付与用户所选清晰度不符的成品
    if platform == "bilibili"
        && download_options.download_video
        && !is_album
        && cookie_file.filter(|value| !value.trim().is_empty()).is_none()
    {
        return Err(
            "B 站视频下载需要登录态，未登录只能获得低清晰度且容易触发风控。请先在设置中导入 B 站登录 Cookie 后重试。"
                .to_string(),
        );
    }

    let output_dir = PathBuf::from(save_directory);
    fs::create_dir_all(&output_dir).map_err(|error| format!("创建下载目录失败：{error}"))?;

    let safe_title = parser::sanitize_filename(title);
    let output_layout = prepare_output_layout(
        &output_dir,
        &safe_title,
        asset_id,
        &download_options,
        is_album,
        request.is_retry,
    )?;
    let use_direct_download = platform != "youtube" && direct_url.is_some();
    let supports_pause = download_options.download_video && use_direct_download && !is_album;
    let supports_cancel = true;
    let format_display = build_task_format_label(format_label, &download_options);
    let task_id = if download_options.download_video && is_album {
        format!("task-{asset_id}-album")
    } else if download_options.download_video {
        format!(
            "task-{asset_id}-{}",
            format_id.unwrap_or("best").trim().replace(['/', ' '], "-")
        )
    } else {
        format!("task-{asset_id}-extras")
    };

    if download_options.download_video
        && !is_album
        && format_requires_processing(format_id)
        && !ffmpeg_available(ffmpeg_path)
    {
        return Err(format!(
            "{} 当前选中的清晰度需要 FFmpeg 合并音视频流。请先安装 FFmpeg，或切换到可直接保存的格式后再下载。",
            platforms::human_platform_name(platform)
        ));
    }

    if download_options.download_audio && !is_album && !ffmpeg_available(ffmpeg_path) {
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
        cover_url: request.cover_url.clone(),
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
        caption: request.caption.clone(),
        cover_url: cover_url.map(str::to_string),
        referer: referer.clone(),
        user_agent: user_agent.clone(),
    };
    let cookie_browser = cookie_browser.map(str::to_string);
    let cookie_file = cookie_file.map(str::to_string);
    let direct_url = direct_url.map(str::to_string);
    task_store::set_replay(&task_store, &task_id, request.clone());

    if download_options.download_video && is_album {
        spawn_download_job(move || {
            acquire_download_slot();
            album_download_worker(
                task_store,
                controller_store,
                task_id,
                title,
                format_display,
                artifacts,
                output_layout,
                download_options,
                image_urls,
                auto_reveal_in_file_manager,
                ffmpeg_path,
                controller,
            );
            release_download_slot();
        });
        return Ok(task);
    }

    if !download_options.download_video {
        spawn_download_job(move || {
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

    if platform_text != "youtube" {
        if let Some(direct_url) = direct_url {
            if let Some(audio_direct_url) = audio_direct_url {
                spawn_download_job(move || {
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
                spawn_download_job(move || {
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
    }

    ensure_ytdlp_available()?;
    let format_id_text = format_id_text
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "当前下载任务缺少可用的视频格式信息。".to_string())?;
    let format_id_text = if platform_text == "youtube" {
        provider_runtime::youtube_format_selector(&format_id_text)
    } else {
        format_id_text
    };
    let output_template = output_layout.asset_root().join(format!(
        "{}.%(ext)s",
        if output_layout.bundle_dir.is_some() {
            "video".to_string()
        } else {
            output_layout.single_stem.clone()
        }
    ));
    let output_template_text = output_template.to_string_lossy().to_string();

    spawn_download_job(move || {
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
            .arg("--ignore-config")
            .arg("--no-playlist")
            .arg("--continue")
            .arg("--socket-timeout")
            .arg("20")
            .arg("--progress")
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

        let proxy_url = current_proxy_for_platform(&platform_text);
        let speed_limit = current_speed_limit();
        if let Some(proxy) = proxy_url.as_deref() {
            if let Err(error) = ensure_local_proxy_available(proxy) {
                unregister_controller(&controller_store, &task_id);
                fail_task(&task_store, &task_id, error);
                release_download_slot();
                return;
            }
        }
        if let Err(error) = append_platform_ytdlp_args(&mut command, &platform_text) {
            fail_task(&task_store, &task_id, error);
            release_download_slot();
            return;
        }
        append_ffmpeg_args(&mut command, ffmpeg_path.as_deref());
        append_network_args(&mut command, proxy_url.as_deref(), speed_limit.as_deref());
        append_external_downloader_args(&mut command, &platform_text);
        command.arg("--format").arg(&format_id_text);
        append_auth_args(
            &mut command,
            cookie_browser.as_deref(),
            cookie_file.as_deref(),
        );

        let cached_info = matches!(platform_text.as_str(), "youtube" | "bilibili")
            .then(|| provider_runtime::cached_platform_info_path(&platform_text, &asset_id_text))
            .flatten();
        if let Some(path) = cached_info.as_deref() {
            command.arg("--load-info-json").arg(path);
        } else {
            command.arg(&source_url);
        }
        let spawn_result = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match spawn_result {
            Ok(child) => child,
            Err(error) => {
                unregister_controller(&controller_store, &task_id);
                fail_task(&task_store, &task_id, format!("启动下载失败：{error}"));
                release_download_slot();
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
                message: Some(if cached_info.is_some() {
                    "正在使用已解析的播放地址下载…".to_string()
                } else if platform_text == "youtube" {
                    "正在刷新 YouTube 播放地址…".to_string()
                } else {
                    "正在下载…".to_string()
                }),
                output_path: None,
                supports_pause: false,
                supports_cancel: true,
                can_retry: true,
                cover_url: artifacts.cover_url.clone(),
            },
        );

        let output_path = Arc::new(Mutex::new(None::<String>));
        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let task_platform = artifacts.platform.clone();
        let task_cover_url = artifacts.cover_url.clone();

        let stdout_handle = child.stdout.take().map(|stdout| {
            let task_store = Arc::clone(&task_store);
            let output_path = Arc::clone(&output_path);
            let task_id = task_id.clone();
            let title = title.clone();
            let format_display = format_display.clone();
            let task_platform = task_platform.clone();
            let task_cover_url = task_cover_url.clone();
            thread::spawn(move || {
                read_process_output_lines(stdout, |line| {
                    if let Some(progress) = parse_progress_line(&line) {
                        let message = progress_task_message(&task_platform, &progress.speed_text);
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
                                message: Some(message),
                                output_path: None,
                                supports_pause: false,
                                supports_cancel: true,
                                can_retry: true,
                                cover_url: task_cover_url.clone(),
                            },
                        );
                    } else if let Some(path) = line.strip_prefix("output:") {
                        let mut guard = output_path.lock().unwrap();
                        *guard = Some(path.trim().to_string());
                    }
                });
            })
        });

        let stderr_handle = child.stderr.take().map(|stderr| {
            let task_store = Arc::clone(&task_store);
            let stderr_lines = Arc::clone(&stderr_lines);
            let task_id = task_id.clone();
            let title = title.clone();
            let format_display = format_display.clone();
            let task_platform = task_platform.clone();
            let task_cover_url = task_cover_url.clone();

            thread::spawn(move || {
                read_process_output_lines(stderr, |line| {
                    if let Some(progress) = parse_progress_line(&line) {
                        let message = progress_task_message(&task_platform, &progress.speed_text);
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
                                message: Some(message),
                                output_path: None,
                                supports_pause: false,
                                supports_cancel: true,
                                can_retry: true,
                                cover_url: task_cover_url.clone(),
                            },
                        );
                    } else {
                        let mut guard = stderr_lines.lock().unwrap();
                        guard.push(line.clone());
                        if guard.len() > 12 {
                            let _ = guard.remove(0);
                        }
                    }
                });
            })
        });

        let mut cancelled = false;
        let status = loop {
            if controller.is_cancel_requested() {
                cancelled = true;
                terminate_process_tree(&mut child);
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
                    release_download_slot();
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
            release_download_slot();
            return;
        }

        if status.success() {
            let reported_path = output_path.lock().unwrap().clone();
            let Some(saved_path) =
                resolve_downloaded_video_path(reported_path.as_deref(), &output_layout)
            else {
                unregister_controller(&controller_store, &task_id);
                fail_task(
                    &task_store,
                    &task_id,
                    "yt-dlp 已退出，但没有找到可播放的最终视频文件。".to_string(),
                );
                release_download_slot();
                return;
            };
            let mut artifact_summary = persist_download_artifacts(
                &output_layout,
                Some(&saved_path),
                &artifacts,
                format_label_text.as_deref(),
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
                    cover_url: artifacts.cover_url.clone(),
                },
            );

            if auto_reveal_in_file_manager {
                if let Some(path) = artifact_summary.output_path.clone() {
                    let _ = open_in_file_manager(&path, output_layout.bundle_dir.is_none());
                } else {
                    let _ = open_in_file_manager(&artifact_summary.destination_path, false);
                }
            }

            download_history::record_download(
                &task_platform,
                &artifacts.asset_id,
                &title,
                artifacts.cover_url.clone(),
                artifact_summary.output_path.clone(),
            );
        } else {
            let reason = stderr_lines
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "下载失败".to_string());
            let reason = normalize_download_failure(&task_platform, reason);

            unregister_controller(&controller_store, &task_id);
            fail_task(&task_store, &task_id, reason);
        }
        release_download_slot();
    });

    Ok(task)
}

#[allow(clippy::too_many_arguments)]
fn album_download_worker(
    task_store: task_store::TaskStore,
    controller_store: TaskControllerStore,
    task_id: String,
    title: String,
    format_label: String,
    artifacts: DownloadArtifacts,
    output_layout: OutputLayout,
    download_options: DownloadContentSelection,
    image_urls: Vec<String>,
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
            message: Some(format!("正在下载图册，共 {} 张图片…", image_urls.len())),
            output_path: None,
            supports_pause: false,
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
    let referer = artifacts
        .referer
        .as_deref()
        .or_else(|| (artifacts.platform == "douyin").then_some("https://www.douyin.com/"));
    let user_agent = artifacts.user_agent.as_deref().unwrap_or(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/137.0.0.0 Safari/537.36",
    );
    let mut saved_count = 0usize;
    let mut warning_slots: Vec<Option<String>> = (0..image_urls.len()).map(|_| None).collect();

    // 有界并发下载图片：网络请求按 4 路并行，文件写入与进度上报仍按原始顺序进行
    for (chunk_index, chunk) in image_urls.chunks(ALBUM_IMAGE_CONCURRENCY).enumerate() {
        if controller.is_cancel_requested() {
            let _ = fs::remove_dir_all(output_layout.asset_root());
            unregister_controller(&controller_store, &task_id);
            cancel_task_update(&task_store, &task_id);
            return;
        }

        let chunk_start = chunk_index * ALBUM_IMAGE_CONCURRENCY;
        let results = thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .enumerate()
                .map(|(offset, image_url)| {
                    let client = &client;
                    scope.spawn(move || {
                        let index = chunk_start + offset;
                        (
                            index,
                            fetch_album_image(client, image_url, referer, user_agent, index),
                        )
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap_or_else(|_| (usize::MAX, Err("图片下载线程异常退出。".to_string()))))
                .collect::<Vec<_>>()
        });

        for (index, result) in results {
            if index == usize::MAX {
                continue;
            }
            let (extension, bytes) = match result {
                Ok(payload) => payload,
                Err(warning) => {
                    warning_slots[index] = Some(warning);
                    continue;
                }
            };
            let path = output_layout.image_path(index + 1, &extension);
            let mut file = match File::create(&path) {
                Ok(file) => file,
                Err(error) => {
                    warning_slots[index] =
                        Some(format!("第 {} 张图片创建失败：{error}", index + 1));
                    continue;
                }
            };
            if let Err(error) = file.write_all(&bytes) {
                let _ = fs::remove_file(&path);
                warning_slots[index] = Some(format!("第 {} 张图片保存失败：{error}", index + 1));
                continue;
            }
            if let Err(error) = file.flush() {
                let _ = fs::remove_file(&path);
                warning_slots[index] = Some(format!("第 {} 张图片刷新失败：{error}", index + 1));
                continue;
            }

            saved_count += 1;
            let progress = ((index + 1) * 94 / image_urls.len().max(1)) as u32;
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    platform: artifacts.platform.clone(),
                    title: title.clone(),
                    progress,
                    speed_text: "-".to_string(),
                    format_label: format_label.clone(),
                    status: "downloading".to_string(),
                    eta_text: format!("{}/{}", index + 1, image_urls.len()),
                    message: Some(format!("已保存 {saved_count} 张图片。")),
                    output_path: None,
                    supports_pause: false,
                    supports_cancel: true,
                    can_retry: true,
                    cover_url: artifacts.cover_url.clone(),
                },
            );
        }
    }

    let warnings: Vec<String> = warning_slots.into_iter().flatten().collect();

    if saved_count == 0 {
        let _ = fs::remove_dir_all(output_layout.asset_root());
        unregister_controller(&controller_store, &task_id);
        fail_task(
            &task_store,
            &task_id,
            warnings
                .into_iter()
                .next()
                .unwrap_or_else(|| "图册没有返回可保存的图片。".to_string()),
        );
        return;
    }

    let mut summary = persist_download_artifacts(
        &output_layout,
        None,
        &artifacts,
        Some(&format_label),
        &download_options,
        ffmpeg_path.as_deref(),
    );
    summary.image_count = saved_count;
    summary.warnings.extend(warnings);
    summary.output_path = output_layout.bundle_entry_path();
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
            cover_url: artifacts.cover_url.clone(),
        },
    );

    if auto_reveal_in_file_manager {
        if let Some(path) = summary.output_path.as_deref() {
            let _ = open_in_file_manager(path, false);
        }
    }
    download_history::record_download(
        &artifacts.platform,
        &artifacts.asset_id,
        &artifacts.title,
        artifacts.cover_url.clone(),
        summary.output_path.clone(),
    );
}

/// 抓取单张图册图片，Ok 为（扩展名, 字节），Err 为带序号的告警文案
fn fetch_album_image(
    client: &Client,
    image_url: &str,
    referer: Option<&str>,
    user_agent: &str,
    index: usize,
) -> Result<(String, Vec<u8>), String> {
    let mut request = client.get(image_url).header(USER_AGENT, user_agent);
    if let Some(value) = referer {
        request = request.header(REFERER, value);
    }
    let response = match request.send() {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return Err(format!("第 {} 张图片返回 {}", index + 1, response.status()));
        }
        Err(error) => {
            return Err(format!("第 {} 张图片请求失败：{error}", index + 1));
        }
    };
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !content_type.is_empty() && !content_type.starts_with("image/") {
        return Err(format!("第 {} 张资源不是图片：{content_type}", index + 1));
    }

    let extension = infer_extension(&response, image_url);
    let bytes = response
        .bytes()
        .map_err(|error| format!("第 {} 张图片保存失败：{error}", index + 1))?;
    Ok((extension, bytes.to_vec()))
}

#[allow(clippy::too_many_arguments)]
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
            cover_url: artifacts.cover_url.clone(),
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
            cover_url: artifacts.cover_url.clone(),
        },
    );

    if auto_reveal_in_file_manager {
        if let Some(path) = revealed_path.as_deref() {
            let _ = open_in_file_manager(path, output_layout.bundle_dir.is_none());
        }
    }

    download_history::record_download(
        &artifacts.platform,
        &artifacts.asset_id,
        &artifacts.title,
        artifacts.cover_url.clone(),
        summary.output_path.clone(),
    );
}

fn append_ffmpeg_args(command: &mut Command, ffmpeg_path: Option<&str>) {
    if let Some(path) = ffmpeg_path.filter(|value| !value.trim().is_empty()) {
        command.arg("--ffmpeg-location").arg(path);
    }
}

fn format_requires_processing(format_id: Option<&str>) -> bool {
    format_id.is_some_and(|value| value.contains('+'))
}

pub(super) fn build_http_client(platform: &str) -> Result<Client, String> {
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

fn ensure_local_proxy_available(proxy_url: &str) -> Result<(), String> {
    let parsed =
        reqwest::Url::parse(proxy_url).map_err(|error| format!("YouTube 代理地址无效：{error}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "YouTube 代理地址缺少主机名。".to_string())?;
    let is_local = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    if !is_local {
        return Ok(());
    }

    let port = parsed.port().or_else(|| match parsed.scheme() {
        "http" => Some(80),
        "https" => Some(443),
        "socks4" | "socks4a" | "socks5" | "socks5h" => Some(1080),
        _ => None,
    });
    let port = port.ok_or_else(|| "YouTube 本地代理地址缺少端口。".to_string())?;
    let ip = if host.eq_ignore_ascii_case("localhost") {
        IpAddr::from([127, 0, 0, 1])
    } else {
        host.parse::<IpAddr>()
            .map_err(|_| "无法识别 YouTube 本地代理地址。".to_string())?
    };
    let endpoint = SocketAddr::new(ip, port);
    TcpStream::connect_timeout(&endpoint, Duration::from_secs(2))
        .map(|_| ())
        .map_err(|_| {
            format!(
                "无法连接本地 YouTube 代理 {host}:{port}。请先启动代理客户端，或修正设置中的代理端口。"
            )
        })
}

fn terminate_process_tree(child: &mut Child) {
    let process_id = child.id().to_string();
    #[cfg(target_os = "windows")]
    {
        let _ = silent_command("taskkill")
            .args(["/PID", process_id.as_str(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = silent_command("pkill")
            .args(["-TERM", "-P", process_id.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

fn append_auth_args(
    command: &mut Command,
    _cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
) {
    if let Some(file) = cookie_file.filter(|value| !value.trim().is_empty()) {
        command.arg("--cookies").arg(file);
    }
}

fn append_platform_ytdlp_args(command: &mut Command, platform: &str) -> Result<(), String> {
    if platform == "bilibili" {
        // B 站风控校验 Origin/Referer；重试退避限制在 2s，避免偶发 412 卡住下载起点。
        // http-chunk-size 让下载按 10M 分块重连，绕过 CDN 单连接限速
        command
            .arg("--add-header")
            .arg("Origin: https://www.bilibili.com")
            .arg("--add-header")
            .arg("Referer: https://www.bilibili.com/")
            .arg("--retries")
            .arg("5")
            .arg("--retry-sleep")
            .arg("2")
            .arg("--http-chunk-size")
            .arg("10485760");
        return Ok(());
    }
    if platform != "youtube" {
        return Ok(());
    }

    provider_runtime::append_youtube_extraction_args(command)?;
    command
        .arg("--merge-output-format")
        .arg("mp4")
        .arg("--concurrent-fragments")
        .arg("8")
        .arg("--http-chunk-size")
        .arg("10485760")
        .arg("--retries")
        .arg("10")
        .arg("--fragment-retries")
        .arg("10")
        .arg("--retry-sleep")
        .arg("http:3")
        .arg("--buffer-size")
        .arg("1M");
    Ok(())
}

// B 站 CDN 对单连接限速，改用 aria2c 多连接分段下载；
// 分段数取保守的 8，避免并发过高触发 B 站风控 412。
// yt-dlp 会把 --add-header/--cookies/--limit-rate 分别映射为
// aria2c 的 --header/--load-cookies/--max-overall-download-limit，
// Referer、Origin、User-Agent、Cookie 与限速语义保持不变。
const BILIBILI_ARIA2C_ARGS: &str =
    "-x8 -s8 -j8 --min-split-size=1M --file-allocation=none --summary-interval=1 --console-log-level=warn";
const YOUTUBE_ARIA2C_ARGS: &str =
    "-x16 -s16 -j16 --min-split-size=1M --file-allocation=none --summary-interval=1 --console-log-level=warn";

fn aria2c_connection_args(platform: &str) -> Option<&'static str> {
    match platform {
        "youtube" => Some(YOUTUBE_ARIA2C_ARGS),
        "bilibili" => Some(BILIBILI_ARIA2C_ARGS),
        _ => None,
    }
}

fn append_external_downloader_args(command: &mut Command, platform: &str) {
    let Some(connection_args) = aria2c_connection_args(platform) else {
        return;
    };

    if let Ok(aria2_path) = provider_runtime::resolve_sidecar("aria2c") {
        command
            .arg("--downloader")
            .arg(aria2_path)
            .arg("--downloader-args")
            .arg(format!("aria2c:{connection_args}"));
    }
}

fn append_network_args(command: &mut Command, proxy_url: Option<&str>, speed_limit: Option<&str>) {
    if let Some(proxy) = proxy_url.filter(|value| !value.trim().is_empty()) {
        command.arg("--proxy").arg(proxy);
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

pub(super) fn upsert_task(task_store: &task_store::TaskStore, next: DownloadTask) {
    task_store::upsert_task(task_store, next);
}

pub(super) fn fail_task(task_store: &task_store::TaskStore, task_id: &str, reason: String) {
    let _ = task_store::mutate_task(task_store, task_id, |task| {
        task.status = "failed".to_string();
        task.eta_text = "失败".to_string();
        task.message = Some(reason);
    });
}

pub(super) fn cancel_task_update(task_store: &task_store::TaskStore, task_id: &str) {
    let _ = task_store::mutate_task(task_store, task_id, |task| {
        task.status = "cancelled".to_string();
        task.eta_text = "已取消".to_string();
        task.message = Some("下载已取消，临时文件已清理。".to_string());
    });
}

fn read_process_output_lines<R, F>(mut reader: R, mut on_line: F)
where
    R: Read,
    F: FnMut(String),
{
    let mut buffer = [0u8; 8192];
    let mut pending = Vec::<u8>::new();

    loop {
        let bytes_read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => bytes_read,
            Err(_) => break,
        };

        for byte in &buffer[..bytes_read] {
            if matches!(*byte, b'\n' | b'\r') {
                if !pending.is_empty() {
                    on_line(String::from_utf8_lossy(&pending).to_string());
                    pending.clear();
                }
            } else {
                pending.push(*byte);
            }
        }
    }

    if !pending.is_empty() {
        on_line(String::from_utf8_lossy(&pending).to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::{
        album_download_worker, append_external_downloader_args, append_platform_ytdlp_args,
        aria2c_connection_args, dash_download_worker, direct_download_worker,
        ensure_local_proxy_available, ffmpeg_available, new_task_controller_store,
        persist_download_artifacts, prepare_output_layout, read_process_output_lines,
        silent_command, DownloadArtifacts, TaskController,
    };
    use super::super::controller::parse_speed_limit_bytes;
    use crate::{provider_runtime, task_store, DownloadContentSelection};
    use std::fs;
    use std::io::Cursor;
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
    fn parses_decimal_speed_limits() {
        assert_eq!(parse_speed_limit_bytes(Some("1.5M")), Some(1_572_864));
        assert_eq!(parse_speed_limit_bytes(Some("256K")), Some(262_144));
    }

    #[test]
    fn youtube_download_uses_stable_parallel_transport_without_restart_threshold() {
        let mut command = silent_command("yt-dlp");
        append_platform_ytdlp_args(&mut command, "youtube").unwrap();
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let joined = args.join(" ");
        assert!(joined.contains("--concurrent-fragments 8"));
        assert!(joined.contains("--http-chunk-size 10485760"));
        assert!(!joined.contains("--throttled-rate"));
    }

    #[test]
    fn youtube_download_uses_aria2_when_the_fixed_sidecar_is_available() {
        let mut command = silent_command("yt-dlp");
        append_external_downloader_args(&mut command, "youtube");
        let joined = command
            .get_args()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        if provider_runtime::resolve_sidecar("aria2c").is_ok() {
            assert!(joined.contains("--downloader"));
            assert!(joined.contains("-x16 -s16 -j16"));
        }
    }

    #[test]
    fn aria2c_connection_args_cover_bilibili_with_conservative_split_count() {
        let bilibili = aria2c_connection_args("bilibili").expect("bilibili args");
        assert!(bilibili.contains("-x8"));
        assert!(bilibili.contains("-s8"));
        assert!(aria2c_connection_args("youtube")
            .expect("youtube args")
            .contains("-x16"));
        assert!(aria2c_connection_args("douyin").is_none());
    }

    #[test]
    fn bilibili_download_uses_aria2_when_the_fixed_sidecar_is_available() {
        let mut command = silent_command("yt-dlp");
        append_external_downloader_args(&mut command, "bilibili");
        let joined = command
            .get_args()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        if provider_runtime::resolve_sidecar("aria2c").is_ok() {
            assert!(joined.contains("--downloader"));
            assert!(joined.contains("aria2c:-x8 -s8"));
        } else {
            assert!(!joined.contains("--downloader"));
        }
    }

    #[test]
    fn validates_local_proxy_ports_without_probing_remote_proxies() {
        assert!(ensure_local_proxy_available("http://proxy.example:7890").is_ok());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(ensure_local_proxy_available(&format!("http://127.0.0.1:{port}")).is_ok());
        drop(listener);

        // 并行测试可能在 drop 后立刻复用该临时端口，改用不会被内核临时分配的
        // 周知关闭端口（discard/9）来验证关闭端口的快速失败
        let closed_port = 9;
        let error = ensure_local_proxy_available(&format!("http://127.0.0.1:{closed_port}"))
            .expect_err("closed local proxy port must fail fast");
        assert!(error.contains("代理客户端"));
        assert!(error.contains(&closed_port.to_string()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn terminates_windows_download_process_tree() {
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "ping", "-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        thread::sleep(Duration::from_millis(150));

        super::terminate_process_tree(&mut child);
        let status = child.wait().unwrap();

        assert!(!status.success());
    }

    #[test]
    fn ignores_invalid_speed_limits() {
        assert_eq!(parse_speed_limit_bytes(Some("0")), None);
        assert_eq!(parse_speed_limit_bytes(Some("abc")), None);
    }

    #[test]
    fn retry_layout_reuses_existing_bundle_directory_for_resume() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        let options = bundle_options();

        let first =
            prepare_output_layout(&output_dir, "测试重试", "aweme-9", &options, false, false)
                .unwrap();
        let first_dir = first.bundle_dir.clone().unwrap();
        fs::write(first_dir.join("video.mp4.part"), b"partial").unwrap();

        let retry =
            prepare_output_layout(&output_dir, "测试重试", "aweme-9", &options, false, true)
                .unwrap();
        assert_eq!(retry.bundle_dir.as_deref(), Some(first_dir.as_path()));

        let fresh =
            prepare_output_layout(&output_dir, "测试重试", "aweme-9", &options, false, false)
                .unwrap();
        assert_ne!(fresh.bundle_dir, Some(first_dir));

        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn splits_process_output_on_carriage_returns() {
        let mut lines = Vec::<String>::new();
        read_process_output_lines(
            Cursor::new(b"download:progress:1.0%|-|--\rdownload:progress:2.0%|1MiB/s|00:10\n"),
            |line| lines.push(line),
        );

        assert_eq!(
            lines,
            vec![
                "download:progress:1.0%|-|--".to_string(),
                "download:progress:2.0%|1MiB/s|00:10".to_string()
            ]
        );
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
        let layout =
            prepare_output_layout(&output_dir, "测试下载", "aweme-1", &options, false, false).unwrap();

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
    fn album_download_worker_saves_every_image_in_one_directory() {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind test listener: {error}"),
        };
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            // 图片是并发下载的，accept 顺序不保证等于图片顺序：按请求路径分发响应
            let mut served = 0;
            while served < 2 {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut request_buffer = [0u8; 1024];
                    let bytes = stream.read(&mut request_buffer).unwrap_or(0);
                    let request = String::from_utf8_lossy(&request_buffer[..bytes]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");
                    let payload: &[u8] = if path == "/2.jpg" { b"second" } else { b"first" };
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        payload.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(payload);
                    served += 1;
                }
            }
        });

        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        let task_store = task_store::new_empty_task_store();
        let controller_store = new_task_controller_store();
        let options = video_only_options();
        let layout =
            prepare_output_layout(&output_dir, "测试图册", "album-1", &options, true, false).unwrap();

        album_download_worker(
            Arc::clone(&task_store),
            controller_store,
            "task-album-1".to_string(),
            "测试图册".to_string(),
            "图册 · 2 张".to_string(),
            sample_artifacts(None),
            layout,
            options,
            vec![
                format!("http://{address}/1.jpg"),
                format!("http://{address}/2.jpg"),
            ],
            false,
            None,
            Arc::new(TaskController::new(false, true)),
        );

        server.join().unwrap();
        let tasks = task_store::list_tasks(&task_store);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, "completed");
        let bundle_dir = PathBuf::from(tasks[0].output_path.clone().unwrap());
        assert_eq!(
            fs::read(bundle_dir.join("image-001.jpg")).unwrap(),
            b"first"
        );
        assert_eq!(
            fs::read(bundle_dir.join("image-002.jpg")).unwrap(),
            b"second"
        );
        assert!(tasks[0]
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("2 张"));

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
            prepare_output_layout(&output_dir, "测试合流下载", "aweme-2", &options, false, false).unwrap();
        let ffmpeg_path = provider_runtime::resolve_sidecar("ffmpeg")
            .ok()
            .and_then(|path| path.to_str().map(str::to_string));

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
        let layout =
            prepare_output_layout(&output_dir, "测试下载", "aweme-1", &options, false, false).unwrap();
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
        assert!(summary.cover_written);
        assert!(summary.warnings.is_empty());
        let bundle_dir = video_path.parent().unwrap().to_path_buf();
        assert!(bundle_dir.join("caption.txt").exists());
        assert!(bundle_dir.join("cover.jpg").exists());
        let caption = fs::read_to_string(bundle_dir.join("caption.txt")).unwrap();
        assert!(caption.contains("测试下载"));
        assert!(caption.contains("来源链接：https://example.com/video"));
        assert!(caption.contains("文案："));
        assert!(caption.contains("这是一段测试文案"));

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
        }
    }

    fn bundle_options() -> DownloadContentSelection {
        DownloadContentSelection {
            download_video: true,
            download_audio: false,
            download_cover: true,
            download_caption: true,
        }
    }

}
