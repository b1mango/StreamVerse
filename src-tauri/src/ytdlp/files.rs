use super::artifact::OutputLayout;
use super::engine::silent_command;
use crate::provider_runtime;
use reqwest::header::CONTENT_TYPE;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager, Runtime};

static CACHED_YTDLP_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

pub(super) fn ensure_ytdlp_available() -> Result<(), String> {
    resolve_ytdlp_path().map(|_| ())
}

pub fn resolve_ffmpeg_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    first_existing_path(ffmpeg_candidates(app))
}

pub fn ffmpeg_available(ffmpeg_path: Option<&str>) -> bool {
    // 探测要真实启动 ffmpeg 进程：批量链路会反复调用，缓存命中过的可用结果；
    // 只缓存阳性结果，运行中途新装/修复的 ffmpeg 不会被阴性缓存挡住
    static FFMPEG_OK_CACHE: OnceLock<Mutex<Vec<Option<String>>>> = OnceLock::new();
    let cache = FFMPEG_OK_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let key = ffmpeg_path.map(str::to_string);
    if cache.lock().unwrap().contains(&key) {
        return true;
    }
    let available = ffmpeg_path
        .map(PathBuf::from)
        .filter(|path| can_execute_ffmpeg(path))
        .is_some()
        || provider_runtime::resolve_sidecar("ffmpeg")
            .as_ref()
            .is_ok_and(|path| can_execute_ffmpeg(path))
        || can_execute_ffmpeg(Path::new("ffmpeg"));
    if available {
        cache.lock().unwrap().push(key);
    }
    available
}

fn ffmpeg_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

pub(super) fn resolve_ytdlp_path() -> Result<PathBuf, String> {
    if let Some(cached) = CACHED_YTDLP_PATH.get() {
        return cached.clone().ok_or_else(|| "yt-dlp 不可用".to_string());
    }

    let resolved = provider_runtime::resolve_sidecar("yt-dlp").ok();

    let _ = CACHED_YTDLP_PATH.set(resolved.clone());
    resolved.ok_or_else(|| "未检测到可用的 yt-dlp，请重新安装应用后再试。".to_string())
}

fn ffmpeg_candidates<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(path) = provider_runtime::resolve_sidecar("ffmpeg") {
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

fn can_execute_ffmpeg(path: &Path) -> bool {
    silent_command(path)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(super) fn infer_extension(response: &reqwest::blocking::Response, direct_url: &str) -> String {
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

pub(super) fn unique_output_path(base_path: PathBuf) -> PathBuf {
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

pub(super) fn unique_output_dir(base_path: PathBuf) -> PathBuf {
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
        open_in_file_manager_windows(&resolved_target, reveal_parent)
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

pub(super) fn resolve_downloaded_video_path(
    reported_path: Option<&str>,
    output_layout: &OutputLayout,
) -> Option<PathBuf> {
    if let Some(reported) = reported_path {
        let path = PathBuf::from(reported.trim().trim_matches('"'));
        if path.is_file() {
            return Some(path);
        }
    }

    let prefix = if output_layout.bundle_dir.is_some() {
        "video.".to_string()
    } else {
        format!("{}.", output_layout.single_stem)
    };
    fs::read_dir(output_layout.asset_root())
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let name_matches = path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with(&prefix));
            let extension_matches = path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| {
                    matches!(
                        value.to_ascii_lowercase().as_str(),
                        "mp4" | "mkv" | "webm" | "mov"
                    )
                });
            path.is_file() && name_matches && extension_matches
        })
        .max_by_key(|path| {
            fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0)
        })
}

#[cfg(test)]
mod tests {
    use super::{first_existing_path, resolve_downloaded_video_path};
    use super::super::artifact::prepare_output_layout;
    use crate::DownloadContentSelection;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

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

    fn video_only_options() -> DownloadContentSelection {
        DownloadContentSelection {
            download_video: true,
            download_audio: false,
            download_cover: false,
            download_caption: false,
        }
    }

    #[test]
    fn recovers_final_video_path_when_reported_path_is_missing() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        let layout = prepare_output_layout(
            &output_dir,
            "路径恢复",
            "video-1",
            &video_only_options(),
            false,
            false,
        )
        .unwrap();
        let expected = layout.video_path("mp4");
        fs::write(&expected, b"valid-video").unwrap();

        let resolved =
            resolve_downloaded_video_path(Some("C:\\missing\\video.mp4"), &layout).unwrap();
        assert_eq!(resolved, expected);
        let _ = fs::remove_dir_all(output_dir);
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
