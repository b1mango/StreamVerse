use crate::{douyin, formats, parser, DownloadTask, VideoAsset, VideoFormat, DEFAULT_GRADIENT};
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_TYPE, REFERER, USER_AGENT};
use serde::Deserialize;
use std::cmp::Reverse;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    formats: Option<Vec<RawFormat>>,
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
        cover_gradient: DEFAULT_GRADIENT.to_string(),
        formats,
    })
}

pub fn download_video(
    task_store: Arc<Mutex<Vec<DownloadTask>>>,
    source_url: &str,
    aweme_id: &str,
    title: &str,
    format_id: &str,
    format_label: &str,
    save_directory: &str,
    auto_reveal_in_file_manager: bool,
    cookie_browser: Option<&str>,
    direct_url: Option<&str>,
    referer: Option<&str>,
    user_agent: Option<&str>,
) -> Result<DownloadTask, String> {
    let output_dir = PathBuf::from(save_directory);
    fs::create_dir_all(&output_dir).map_err(|error| format!("创建下载目录失败：{error}"))?;

    let safe_title = parser::sanitize_filename(title);
    let task_id = format!("task-{aweme_id}-{format_id}");
    let task = DownloadTask {
        id: task_id.clone(),
        title: title.to_string(),
        progress: 0,
        speed_text: "-".to_string(),
        format_label: format_label.to_string(),
        status: "queued".to_string(),
        eta_text: "等待中".to_string(),
        message: Some("下载任务已开始。".to_string()),
        output_path: None,
    };

    upsert_task(&task_store, task.clone());

    let source_url = source_url.to_string();
    let title = title.to_string();
    let format_id = format_id.to_string();
    let format_label = format_label.to_string();
    let output_dir_text = save_directory.to_string();
    let output_dir_path = output_dir.clone();
    let cookie_browser = cookie_browser.map(str::to_string);
    let direct_url = direct_url.map(str::to_string);
    let referer = referer.map(str::to_string);
    let user_agent = user_agent.map(str::to_string);
    let aweme_id_text = aweme_id.to_string();

    if let Some(direct_url) = direct_url {
        let output_dir = output_dir.clone();
        thread::spawn(move || {
            direct_download_worker(
                task_store,
                task_id,
                title,
                format_label,
                output_dir,
                output_dir_text,
                safe_title,
                aweme_id_text,
                direct_url,
                auto_reveal_in_file_manager,
                referer,
                user_agent,
            );
        });

        return Ok(task);
    }

    ensure_ytdlp_available()?;
    let output_template = output_dir.join(format!("{safe_title} [{aweme_id}].%(ext)s"));
    let output_template_text = output_template.to_string_lossy().to_string();

    thread::spawn(move || {
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

        command.arg("--format").arg(&format_id);
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
                format_label: format_label.clone(),
                status: "downloading".to_string(),
                eta_text: "准备中".to_string(),
                message: Some("正在下载…".to_string()),
                output_path: None,
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
            let format_label = format_label.clone();

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
                                format_label: format_label.clone(),
                                status: "downloading".to_string(),
                                eta_text: progress.eta_text,
                                message: Some("正在下载…".to_string()),
                                output_path: None,
                            },
                        );
                    }
                }
            })
        });

        let status = match child.wait() {
            Ok(status) => status,
            Err(error) => {
                fail_task(
                    &task_store,
                    &task_id,
                    format!("等待下载进程结束失败：{error}"),
                );
                return;
            }
        };

        if let Some(handle) = stdout_handle {
            let _ = handle.join();
        }
        if let Some(handle) = stderr_handle {
            let _ = handle.join();
        }

        if status.success() {
            let saved_path = output_path.lock().unwrap().clone();
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    title: title.clone(),
                    progress: 100,
                    speed_text: "-".to_string(),
                    format_label: format_label.clone(),
                    status: "completed".to_string(),
                    eta_text: "已完成".to_string(),
                    message: Some(format!("下载完成，文件已保存到 {output_dir_text}。")),
                    output_path: saved_path,
                },
            );

            if auto_reveal_in_file_manager {
                if let Some(path) = output_path.lock().unwrap().clone() {
                    let _ = open_in_file_manager(&path, true);
                } else {
                    let _ =
                        open_in_file_manager(output_dir_path.to_string_lossy().as_ref(), false);
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

            fail_task(&task_store, &task_id, reason);
        }
    });

    Ok(task)
}

fn direct_download_worker(
    task_store: Arc<Mutex<Vec<DownloadTask>>>,
    task_id: String,
    title: String,
    format_label: String,
    output_dir: PathBuf,
    output_dir_text: String,
    safe_title: String,
    aweme_id: String,
    direct_url: String,
    auto_reveal_in_file_manager: bool,
    referer: Option<String>,
    user_agent: Option<String>,
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
        },
    );

    let client = match Client::builder().timeout(Duration::from_secs(120)).build() {
        Ok(client) => client,
        Err(error) => {
            fail_task(
                &task_store,
                &task_id,
                format!("创建下载客户端失败：{error}"),
            );
            return;
        }
    };

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
            fail_task(&task_store, &task_id, format!("请求下载直链失败：{error}"));
            return;
        }
    };

    if !response.status().is_success() {
        fail_task(
            &task_store,
            &task_id,
            format!("下载直链返回异常状态：{}", response.status()),
        );
        return;
    }

    let extension = infer_extension(&response, &direct_url);
    let final_path =
        unique_output_path(output_dir.join(format!("{safe_title} [{aweme_id}].{extension}")));
    let temp_path = final_path.with_extension(format!("{extension}.download"));

    let mut file = match File::create(&temp_path) {
        Ok(file) => file,
        Err(error) => {
            fail_task(&task_store, &task_id, format!("创建临时文件失败：{error}"));
            return;
        }
    };

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded_bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    let started_at = Instant::now();
    let mut last_report = Instant::now();

    loop {
        let bytes_read = match response.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                fail_task(&task_store, &task_id, format!("读取下载响应失败：{error}"));
                return;
            }
        };

        if bytes_read == 0 {
            break;
        }

        if let Err(error) = file.write_all(&buffer[..bytes_read]) {
            let _ = fs::remove_file(&temp_path);
            fail_task(&task_store, &task_id, format!("写入下载文件失败：{error}"));
            return;
        }

        downloaded_bytes += bytes_read as u64;
        if last_report.elapsed() >= Duration::from_millis(250) {
            upsert_task(
                &task_store,
                DownloadTask {
                    id: task_id.clone(),
                    title: title.clone(),
                    progress: compute_percent(downloaded_bytes, total_bytes),
                    speed_text: human_speed(downloaded_bytes, started_at.elapsed()),
                    format_label: format_label.clone(),
                    status: "downloading".to_string(),
                    eta_text: human_eta(downloaded_bytes, total_bytes, started_at.elapsed()),
                    message: Some("正在下载…".to_string()),
                    output_path: None,
                },
            );
            last_report = Instant::now();
        }
    }

    if let Err(error) = file.flush() {
        let _ = fs::remove_file(&temp_path);
        fail_task(&task_store, &task_id, format!("刷新文件缓存失败：{error}"));
        return;
    }

    if let Err(error) = fs::rename(&temp_path, &final_path) {
        let _ = fs::remove_file(&temp_path);
        fail_task(&task_store, &task_id, format!("保存下载文件失败：{error}"));
        return;
    }

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
            message: Some(format!("下载完成，文件已保存到 {output_dir_text}。")),
            output_path: Some(final_path.to_string_lossy().to_string()),
        },
    );

    if auto_reveal_in_file_manager {
        let _ = open_in_file_manager(final_path.to_string_lossy().as_ref(), true);
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
    use super::{compute_percent, direct_download_worker};
    use crate::DownloadTask;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

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

        direct_download_worker(
            Arc::clone(&task_store),
            "task-1".to_string(),
            "测试下载".to_string(),
            "720P".to_string(),
            output_dir.clone(),
            output_dir.to_string_lossy().to_string(),
            "测试下载".to_string(),
            "aweme-1".to_string(),
            format!("http://{address}/demo.mp4"),
            false,
            None,
            None,
        );

        server.join().unwrap();

        let tasks = task_store.lock().unwrap().clone();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, "completed");

        let output_path = PathBuf::from(tasks[0].output_path.clone().unwrap());
        let file_content = fs::read(&output_path).unwrap();
        assert_eq!(file_content, b"hello");

        let _ = fs::remove_file(output_path);
        let _ = fs::remove_dir_all(output_dir);
    }

    fn unique_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "streamverse-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ))
    }
}
