use crate::{
    pack_manager, pack_registry, platforms, BrowserLaunchResult, ProfileBatch, VideoAsset,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Authoritative resource root captured from Tauri's `app.path().resource_dir()` during setup.
/// This avoids relying on brittle `current_exe` heuristics or the compile-time
/// `CARGO_MANIFEST_DIR` (which leaks the dev machine's path into release builds).
static RESOURCE_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Called once from `main.rs::setup()` with Tauri's authoritative resource directory.
pub fn set_resource_root(path: PathBuf) {
    // Broadcast to every sibling crate module and any spawned subprocess via env var.
    env::set_var("STREAMVERSE_RESOURCE_ROOT", &path);
    let _ = RESOURCE_ROOT.set(path);
}

pub fn resource_root() -> Option<PathBuf> {
    if let Some(path) = RESOURCE_ROOT.get() {
        return Some(path.clone());
    }
    env::var_os("STREAMVERSE_RESOURCE_ROOT")
        .map(PathBuf::from)
        .filter(|path| path.as_os_str().len() > 0)
}

fn silent_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd
}

pub fn analyze_single(
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    progress_file: Option<&Path>,
) -> Result<VideoAsset, String> {
    let platform = platforms::detect_platform(source_url);
    if let Some(pack) = pack_registry::local_pack_for_platform(platform) {
        if pack.supports_single {
            return run_pack_for_asset(
                pack.binary_name,
                "analyze-single",
                source_url,
                cookie_browser,
                cookie_file,
                progress_file,
            )
            .unwrap_or_else(|| Err(format!("未找到本地 pack：{}", pack.binary_name)));
        }
    }

    Err(format!(
        "{} 暂未开放单视频下载。",
        platforms::human_platform_name(platform)
    ))
}

pub fn analyze_profile(
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    progress_file: Option<&Path>,
) -> Result<ProfileBatch, String> {
    let platform = platforms::detect_platform(source_url);
    if let Some(pack) = pack_registry::local_pack_for_platform(platform) {
        if pack.supports_profile {
            return run_pack_for_profile(
                pack.binary_name,
                "analyze-profile",
                source_url,
                cookie_browser,
                cookie_file,
                progress_file,
            )
            .unwrap_or_else(|| Err(format!("未找到本地 pack：{}", pack.binary_name)));
        }
    }

    Err(format!(
        "{} 暂未开放批量主页下载，请先使用单视频下载。",
        platforms::human_platform_name(platform)
    ))
}

pub fn open_profile_browser(
    source_url: &str,
    cookie_browser: Option<&str>,
) -> Result<BrowserLaunchResult, String> {
    let platform = platforms::detect_platform(source_url);
    if let Some(pack) = pack_registry::local_pack_for_platform(platform) {
        if platform == "douyin" {
            return run_pack_for_browser_launch(
                pack.binary_name,
                "open-profile-browser",
                source_url,
                cookie_browser,
            )
            .unwrap_or_else(|| Err(format!("未找到本地 pack：{}", pack.binary_name)));
        }
    }

    Err(format!(
        "{} 暂未开放手动浏览器读取。",
        platforms::human_platform_name(platform)
    ))
}

pub fn collect_profile_browser(
    source_url: &str,
    port: u16,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    progress_file: Option<&Path>,
) -> Result<ProfileBatch, String> {
    let platform = platforms::detect_platform(source_url);
    if let Some(pack) = pack_registry::local_pack_for_platform(platform) {
        if platform == "douyin" {
            return run_pack_for_profile_collect(
                pack.binary_name,
                "collect-profile-browser",
                source_url,
                port,
                cookie_browser,
                cookie_file,
                progress_file,
            )
            .unwrap_or_else(|| Err(format!("未找到本地 pack：{}", pack.binary_name)));
        }
    }

    Err(format!(
        "{} 暂未开放手动浏览器读取。",
        platforms::human_platform_name(platform)
    ))
}

fn run_pack_for_asset(
    binary_name: &str,
    action: &str,
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    progress_file: Option<&Path>,
) -> Option<Result<VideoAsset, String>> {
    let output = run_pack(
        binary_name,
        action,
        source_url,
        cookie_browser,
        cookie_file,
        None,
        progress_file,
    )
    .ok()?;

    if !output.status.success() {
        let error = read_pack_error(&output.stderr, &format!("{binary_name} 返回了错误结果。"));
        return Some(Err(error));
    }

    Some(
        serde_json::from_slice::<VideoAsset>(&output.stdout)
            .map_err(|error| format!("解析 {binary_name} 输出失败：{error}")),
    )
}

fn run_pack_for_profile(
    binary_name: &str,
    action: &str,
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    progress_file: Option<&Path>,
) -> Option<Result<ProfileBatch, String>> {
    let output = run_pack(
        binary_name,
        action,
        source_url,
        cookie_browser,
        cookie_file,
        None,
        progress_file,
    )
    .ok()?;

    if !output.status.success() {
        let error = read_pack_error(&output.stderr, &format!("{binary_name} 返回了错误结果。"));
        return Some(Err(error));
    }

    Some(
        serde_json::from_slice::<ProfileBatch>(&output.stdout)
            .map_err(|error| format!("解析 {binary_name} 输出失败：{error}")),
    )
}

fn run_pack_for_browser_launch(
    binary_name: &str,
    action: &str,
    source_url: &str,
    cookie_browser: Option<&str>,
) -> Option<Result<BrowserLaunchResult, String>> {
    let output = run_pack(
        binary_name,
        action,
        source_url,
        cookie_browser,
        None,
        None,
        None,
    )
    .ok()?;

    if !output.status.success() {
        let error = read_pack_error(&output.stderr, &format!("{binary_name} 返回了错误结果。"));
        return Some(Err(error));
    }

    Some(
        serde_json::from_slice::<BrowserLaunchResult>(&output.stdout)
            .map_err(|error| format!("解析 {binary_name} 浏览器启动结果失败：{error}")),
    )
}

fn run_pack_for_profile_collect(
    binary_name: &str,
    action: &str,
    source_url: &str,
    port: u16,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    progress_file: Option<&Path>,
) -> Option<Result<ProfileBatch, String>> {
    let output = run_pack(
        binary_name,
        action,
        source_url,
        cookie_browser,
        cookie_file,
        Some(port),
        progress_file,
    )
    .ok()?;

    if !output.status.success() {
        let error = read_pack_error(&output.stderr, &format!("{binary_name} 返回了错误结果。"));
        return Some(Err(error));
    }

    Some(
        serde_json::from_slice::<ProfileBatch>(&output.stdout)
            .map_err(|error| format!("解析 {binary_name} 输出失败：{error}")),
    )
}

fn run_pack(
    binary_name: &str,
    action: &str,
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    port: Option<u16>,
    progress_file: Option<&Path>,
) -> Result<std::process::Output, String> {
    let binary_path = resolve_pack_binary(binary_name)
        .ok_or_else(|| format!("未找到本地 pack：{binary_name}"))?;

    let mut command = silent_command(&binary_path);
    command.arg(action).arg("--url").arg(source_url);

    if let Some(browser) = cookie_browser.filter(|value| !value.trim().is_empty()) {
        command.arg("--cookie-browser").arg(browser);
    }
    if let Some(file) = cookie_file.filter(|value| !value.trim().is_empty()) {
        command.arg("--cookie-file").arg(file);
    }
    if let Some(port) = port {
        command.arg("--port").arg(port.to_string());
    }
    if let Some(path) = progress_file {
        command.env("STREAMVERSE_PROGRESS_FILE", path);
    }
    let network_settings = crate::settings::load_settings();
    command.env(
        "STREAMVERSE_PROXY_URL",
        network_settings.proxy_url.unwrap_or_default(),
    );
    command
        .env("STREAMVERSE_LOG_DIR", writable_log_dir())
        .current_dir(writable_pack_work_dir());

    if let Some(root) = resource_root() {
        command.env("STREAMVERSE_RESOURCE_ROOT", root);
    }

    command
        .output()
        .map_err(|error| format!("启动本地 pack 失败：{error}"))
}

fn writable_log_dir() -> PathBuf {
    ensure_writable_dir(app_data_root().join("logs").join("helper")).unwrap_or_else(|| {
        ensure_writable_dir(env::temp_dir().join("streamverse-logs").join("helper"))
            .unwrap_or_else(|| env::temp_dir().join("streamverse-logs").join("helper"))
    })
}

fn writable_pack_work_dir() -> PathBuf {
    ensure_writable_dir(app_data_root().join("runtime").join("pack-work")).unwrap_or_else(|| {
        ensure_writable_dir(env::temp_dir().join("streamverse-pack-work"))
            .unwrap_or_else(|| env::temp_dir().join("streamverse-pack-work"))
    })
}

fn app_data_root() -> PathBuf {
    PathBuf::from(crate::settings::home_dir()).join(".streamverse")
}

fn ensure_writable_dir(path: PathBuf) -> Option<PathBuf> {
    fs::create_dir_all(&path).ok()?;
    Some(path)
}

fn resolve_pack_binary(binary_name: &str) -> Option<PathBuf> {
    let binary_file = binary_filename(binary_name);
    let mut candidates = Vec::new();

    // Highest priority: Tauri's authoritative resource_dir (set in main.rs setup()).
    if let Some(root) = resource_root() {
        candidates.push(root.join("pack-binaries").join(&binary_file));
        candidates.push(root.join(&binary_file));
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join(&binary_file));
            candidates.push(parent.join("pack-binaries").join(&binary_file));
            candidates.push(parent.join("binaries").join(&binary_file));

            if let Some(contents_dir) = parent.parent() {
                candidates.push(
                    contents_dir
                        .join("Resources")
                        .join("pack-binaries")
                        .join(&binary_file),
                );
                candidates.push(
                    contents_dir
                        .join("resources")
                        .join("pack-binaries")
                        .join(&binary_file),
                );
            }
        }
    }

    // Dev-time fallbacks: search target/debug, target/release, and src-tauri/binaries
    // via the workspace root. In release builds this still references the dev machine's
    // CARGO_MANIFEST_DIR, but since it's only a .is_file() probe the path simply won't
    // match on end-user machines — no user-visible error path is emitted.
    if let Some(workspace_root) = dev_workspace_root() {
        candidates.push(
            workspace_root
                .join("src-tauri")
                .join("target")
                .join("debug")
                .join(&binary_file),
        );
        candidates.push(
            workspace_root
                .join("src-tauri")
                .join("target")
                .join("release")
                .join(&binary_file),
        );
        candidates.push(
            workspace_root
                .join("src-tauri")
                .join("binaries")
                .join(&binary_file),
        );
    }

    if let Some(installed) = pack_manager::resolve_installed_binary(binary_name) {
        candidates.push(installed);
    }

    candidates.into_iter().find(|path| path.is_file())
}

fn binary_filename(binary_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{binary_name}.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        binary_name.to_string()
    }
}

/// Returns the workspace root only if the compile-time `CARGO_MANIFEST_DIR` still
/// exists on disk. This prevents the dev machine's path from leaking into user-
/// facing errors on end-user installs while keeping the dev workflow intact.
fn dev_workspace_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if !manifest_dir.exists() {
        return None;
    }
    Some(
        manifest_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(manifest_dir),
    )
}

fn read_pack_error(stderr: &[u8], fallback: &str) -> String {
    let message = String::from_utf8_lossy(stderr);
    let trimmed = message.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::binary_filename;

    #[test]
    fn adds_windows_extension_only_when_needed() {
        #[cfg(target_os = "windows")]
        assert_eq!(
            binary_filename("streamverse-pack-douyin"),
            "streamverse-pack-douyin.exe"
        );

        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            binary_filename("streamverse-pack-douyin"),
            "streamverse-pack-douyin"
        );
    }
}
