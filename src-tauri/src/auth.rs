use rookie::enums::Cookie;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSource {
    pub id: String,
    pub label: String,
    pub is_default: bool,
    pub profiles: Vec<BrowserProfile>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfile {
    pub id: String,
    pub label: String,
    pub is_default: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieImportRequest {
    pub platform: String,
    pub browser_id: String,
    pub profile_id: Option<String>,
    pub consent: String,
    #[serde(default)]
    pub allow_elevation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_account: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieImportResult {
    pub platform: String,
    pub browser_id: String,
    pub profile_id: Option<String>,
    pub status: String,
    pub imported_count: usize,
    pub requires_elevation: bool,
    pub message: String,
}

pub fn list_browser_sources() -> Result<Vec<BrowserSource>, String> {
    let default = default_browser_id();
    let supported =
        rookie::supported_browsers().map_err(|error| format!("读取浏览器列表失败：{error}"))?;
    let mut sources = Vec::new();
    for browser in supported {
        let id = browser.id.as_str().to_string();
        if !matches!(id.as_str(), "chrome" | "edge" | "firefox" | "safari") {
            continue;
        }
        let profiles = match if id == "chrome" {
            rookie::chrome_profiles()
        } else {
            rookie::browser_profiles(&id)
        } {
            Ok(profiles) => profiles,
            Err(_) => continue,
        };
        if profiles.is_empty() {
            continue;
        }
        sources.push(BrowserSource {
            is_default: default.as_deref() == Some(id.as_str()),
            id: id.clone(),
            label: browser.display_name,
            profiles: profiles
                .into_iter()
                .enumerate()
                .map(|(index, profile)| BrowserProfile {
                    id: profile.profile.profile_id.as_str().to_string(),
                    label: profile.profile.display_name,
                    is_default: preferred_profile(&id, index, profile.is_default),
                })
                .collect(),
        });
    }
    sources.sort_by_key(|source| !source.is_default);
    Ok(sources)
}

pub fn import_browser_cookies(request: &CookieImportRequest) -> Result<CookieImportResult, String> {
    let mut request = request.clone();
    request.owner_account = None;
    let request = &request;
    validate_request(request)?;
    match extract_browser_cookies(request) {
        Ok(cookies) => persist_import(request, cookies),
        Err(error) if needs_elevation(&error) && !request.allow_elevation => {
            Ok(CookieImportResult {
                platform: request.platform.clone(),
                browser_id: request.browser_id.clone(),
                profile_id: request.profile_id.clone(),
                status: "needsElevation".to_string(),
                imported_count: 0,
                requires_elevation: true,
                message: "浏览器启用了 App-Bound Encryption。再次确认后，StreamVerse 只会为本次 Cookie 读取启动一次提权 helper。".to_string(),
            })
        }
        Err(_) if request.allow_elevation && cfg!(target_os = "windows") => {
            run_elevated_import(request)
        }
        Err(error) => Err(error),
    }
}

pub fn save_manual_cookies(platform: &str, content: &str) -> Result<CookieImportResult, String> {
    let platform = validate_platform(platform)?;
    let cookies = parse_manual_cookies(platform, content)?;
    validate_critical_cookies(platform, &cookies)?;
    write_cookie_file(platform, &cookies, None)?;
    Ok(CookieImportResult {
        platform: platform.to_string(),
        browser_id: "manual".to_string(),
        profile_id: None,
        status: "active".to_string(),
        imported_count: cookies.len(),
        requires_elevation: false,
        message: "平台登录态已安全保存。".to_string(),
    })
}

pub fn import_cookie_file(platform: &str, source: &Path) -> Result<CookieImportResult, String> {
    if source.extension().and_then(|value| value.to_str()) != Some("txt") {
        return Err("Cookie 文件必须是 .txt 格式。".to_string());
    }
    let content =
        fs::read_to_string(source).map_err(|error| format!("读取 cookies.txt 失败：{error}"))?;
    save_manual_cookies(platform, &content)
}

pub fn clear_platform_auth(platform: &str) -> Result<(), String> {
    let platform = validate_platform(platform)?;
    let path = cookie_file_path(platform);
    if path.is_file() {
        fs::remove_file(path).map_err(|error| format!("删除平台登录态失败：{error}"))?;
    }
    Ok(())
}

pub fn cookie_file_for(platform: &str) -> Option<String> {
    let path = cookie_file_path(platform);
    path.is_file().then(|| path.to_string_lossy().to_string())
}

pub fn run_elevated_cookie_helper_from_args() -> bool {
    let args: Vec<String> = env::args().collect();
    let Some(index) = args.iter().position(|arg| arg == "--cookie-helper") else {
        return false;
    };
    let Some(request_path) = args.get(index + 1) else {
        return true;
    };
    let Some(result_path) = args.get(index + 2) else {
        return true;
    };
    let result = fs::read_to_string(request_path)
        .map_err(|error| format!("读取 helper 请求失败：{error}"))
        .and_then(|raw| {
            serde_json::from_str::<CookieImportRequest>(&raw)
                .map_err(|error| format!("解析 helper 请求失败：{error}"))
        })
        .and_then(|request| {
            validate_request(&request)?;
            extract_browser_cookies(&request).and_then(|cookies| persist_import(&request, cookies))
        });
    let payload = match result {
        Ok(result) => serde_json::to_string(&result),
        Err(error) => serde_json::to_string(&CookieImportResult {
            platform: String::new(),
            browser_id: String::new(),
            profile_id: None,
            status: "failed".to_string(),
            imported_count: 0,
            requires_elevation: false,
            message: error,
        }),
    };
    if let Ok(payload) = payload {
        let _ = fs::write(result_path, payload);
    }
    true
}

fn validate_request(request: &CookieImportRequest) -> Result<(), String> {
    validate_platform(&request.platform)?;
    if !matches!(
        request.browser_id.as_str(),
        "chrome" | "edge" | "firefox" | "safari"
    ) {
        return Err("不支持的浏览器来源。".to_string());
    }
    if !matches!(request.consent.as_str(), "once" | "always") {
        return Err("Cookie 授权范围无效。".to_string());
    }
    Ok(())
}

fn extract_browser_cookies(request: &CookieImportRequest) -> Result<Vec<Cookie>, String> {
    let report = rookie::browser_report(
        &request.browser_id,
        request.profile_id.as_deref(),
        Some(
            domain_whitelist(&request.platform)
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        ),
    )
    .map_err(|error| cookie_extraction_error(&request.browser_id, &error.to_string()))?;
    let mut cookies = Vec::new();
    let mut issue_text = String::new();
    let mut has_decryption_issue = false;
    let mut selected_profile = None;
    for profile in report.profiles {
        for issue in profile.issues {
            record_extraction_issue(&issue, &mut issue_text, &mut has_decryption_issue);
        }
        selected_profile = Some(profile.profile.display_name);
        for source in profile.sources {
            for issue in source.issues {
                record_extraction_issue(&issue, &mut issue_text, &mut has_decryption_issue);
            }
            if source.selected {
                cookies.extend(source.cookies);
            }
        }
    }
    for issue in report.issues {
        record_extraction_issue(&issue, &mut issue_text, &mut has_decryption_issue);
    }
    if cookies.is_empty() {
        return Err(cookie_extraction_error(&request.browser_id, &issue_text));
    }
    if let Err(error) = validate_critical_cookies(&request.platform, &cookies) {
        if request.browser_id == "chrome" && has_decryption_issue {
            return Err(cookie_extraction_error(&request.browser_id, &issue_text));
        }
        let profile = selected_profile.as_deref().unwrap_or("所选 Profile");
        return Err(format!(
            "{error} 当前读取的是“{profile}”；如登录账号位于其他 Chrome Profile，请在下拉框中切换后重试。"
        ));
    }
    Ok(cookies)
}

fn preferred_profile(browser_id: &str, index: usize, declared_default: bool) -> bool {
    if browser_id == "chrome" {
        index == 0
    } else {
        declared_default
    }
}

fn record_extraction_issue(
    issue: &rookie::report::ExtractionIssue,
    issue_text: &mut String,
    has_decryption_issue: &mut bool,
) {
    let code = issue.code.as_str();
    if is_decryption_issue_code(code) {
        *has_decryption_issue = true;
    }
    issue_text.push(' ');
    issue_text.push_str(code);
    issue_text.push_str(": ");
    issue_text.push_str(&issue.message);
}

fn is_decryption_issue_code(code: &str) -> bool {
    matches!(
        code,
        "decrypt_failed" | "provider_unavailable" | "provider_failed"
    )
}

fn persist_import(
    request: &CookieImportRequest,
    cookies: Vec<Cookie>,
) -> Result<CookieImportResult, String> {
    write_cookie_file(
        &request.platform,
        &cookies,
        request.owner_account.as_deref(),
    )?;
    Ok(CookieImportResult {
        platform: request.platform.clone(),
        browser_id: request.browser_id.clone(),
        profile_id: request.profile_id.clone(),
        status: "active".to_string(),
        imported_count: cookies.len(),
        requires_elevation: false,
        message: "浏览器登录态已导入。".to_string(),
    })
}

#[cfg(target_os = "windows")]
fn run_elevated_import(request: &CookieImportRequest) -> Result<CookieImportResult, String> {
    let root = auth_root();
    fs::create_dir_all(&root).map_err(|error| format!("创建认证目录失败：{error}"))?;
    let nonce = now_millis();
    let request_path = root.join(format!("elevated-request-{nonce}.json"));
    let result_path = root.join(format!("elevated-result-{nonce}.json"));
    let mut helper_request = request.clone();
    helper_request.owner_account = Some(current_windows_account()?);
    fs::write(
        &request_path,
        serde_json::to_vec(&helper_request).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("写入 helper 请求失败：{error}"))?;
    let executable =
        env::current_exe().map_err(|error| format!("定位 Cookie helper 失败：{error}"))?;
    let operation = (|| {
        launch_elevated_cookie_helper(&executable, &request_path, &result_path)?;
        if !result_path.is_file() {
            return Err("提权 Cookie helper 已退出，但没有返回结果。".to_string());
        }
        let raw = fs::read_to_string(&result_path)
            .map_err(|error| format!("读取 helper 结果失败：{error}"))?;
        let result: CookieImportResult =
            serde_json::from_str(&raw).map_err(|error| format!("解析 helper 结果失败：{error}"))?;
        if result.status == "active" {
            Ok(result)
        } else {
            Err(result.message)
        }
    })();
    let _ = fs::remove_file(&request_path);
    let _ = fs::remove_file(&result_path);
    operation
}

#[cfg(target_os = "windows")]
fn launch_elevated_cookie_helper(
    executable: &Path,
    request_path: &Path,
    result_path: &Path,
) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn to_wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }

    let verb = to_wide(OsStr::new("runas"));
    let executable = to_wide(executable.as_os_str());
    let parameters = elevated_helper_parameters(request_path, result_path)?;
    let mut execute_info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    execute_info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    execute_info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    execute_info.lpVerb = verb.as_ptr();
    execute_info.lpFile = executable.as_ptr();
    execute_info.lpParameters = parameters.as_ptr();
    execute_info.nShow = SW_HIDE;

    if unsafe { ShellExecuteExW(&mut execute_info) } == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            return Err("已取消 Windows UAC 授权。".to_string());
        }
        return Err(format!("启动提权 Cookie helper 失败：{error}"));
    }
    if execute_info.hProcess.is_null() {
        return Err("Windows 未返回 Cookie helper 进程句柄。".to_string());
    }

    let process = execute_info.hProcess;
    let wait_result = unsafe { WaitForSingleObject(process, INFINITE) };
    if wait_result != WAIT_OBJECT_0 {
        unsafe { CloseHandle(process) };
        return Err(format!("等待提权 Cookie helper 失败：{wait_result}"));
    }
    let mut exit_code = 0u32;
    let exit_code_read = unsafe { GetExitCodeProcess(process, &mut exit_code) };
    unsafe { CloseHandle(process) };
    if exit_code_read == 0 {
        return Err(format!(
            "读取提权 Cookie helper 退出码失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    if exit_code != 0 {
        return Err(format!("提权 Cookie helper 异常退出，代码：{exit_code}"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn elevated_helper_parameters(request_path: &Path, result_path: &Path) -> Result<Vec<u16>, String> {
    use std::os::windows::ffi::OsStrExt;

    let mut parameters: Vec<u16> = "--cookie-helper ".encode_utf16().collect();
    for (index, path) in [request_path, result_path].into_iter().enumerate() {
        if index > 0 {
            parameters.push(' ' as u16);
        }
        parameters.push('"' as u16);
        for unit in path.as_os_str().encode_wide() {
            if unit == '"' as u16 {
                return Err("Cookie helper 路径包含无效引号。".to_string());
            }
            parameters.push(unit);
        }
        parameters.push('"' as u16);
    }
    parameters.push(0);
    Ok(parameters)
}

#[cfg(not(target_os = "windows"))]
fn run_elevated_import(_request: &CookieImportRequest) -> Result<CookieImportResult, String> {
    Err("当前系统不需要 Windows 提权 Cookie helper。".to_string())
}

fn write_cookie_file(
    platform: &str,
    cookies: &[Cookie],
    owner_account: Option<&str>,
) -> Result<(), String> {
    let root = auth_root();
    fs::create_dir_all(&root).map_err(|error| format!("创建认证目录失败：{error}"))?;
    let target = cookie_file_path(platform);
    let temporary = root.join(format!(".{platform}-{}.tmp", now_millis()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| format!("创建临时 Cookie 文件失败：{error}"))?;
    file.write_all(b"# Netscape HTTP Cookie File\n")
        .map_err(|error| format!("写入 Cookie 文件失败：{error}"))?;
    for cookie in cookies {
        if !domain_allowed(platform, &cookie.domain) || cookie.value.trim().is_empty() {
            continue;
        }
        let prefix = if cookie.http_only { "#HttpOnly_" } else { "" };
        let include_subdomains = if cookie.domain.starts_with('.') {
            "TRUE"
        } else {
            "FALSE"
        };
        let secure = if cookie.secure { "TRUE" } else { "FALSE" };
        let expires = cookie.expires.unwrap_or(0);
        writeln!(
            file,
            "{prefix}{}\t{include_subdomains}\t{}\t{secure}\t{expires}\t{}\t{}",
            cookie.domain, cookie.path, cookie.name, cookie.value
        )
        .map_err(|error| format!("写入 Cookie 文件失败：{error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("同步 Cookie 文件失败：{error}"))?;
    atomic_replace(&temporary, &target)?;
    restrict_to_current_user(&target, owner_account)?;
    Ok(())
}

fn parse_manual_cookies(platform: &str, content: &str) -> Result<Vec<Cookie>, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("Cookie 内容不能为空。".to_string());
    }
    let mut cookies = Vec::new();
    if trimmed.lines().any(|line| line.split('\t').count() >= 7) {
        for raw in trimmed.lines() {
            let line = raw.strip_prefix("#HttpOnly_").unwrap_or(raw);
            let columns: Vec<&str> = line.split('\t').collect();
            if columns.len() < 7 || !domain_allowed(platform, columns[0]) {
                continue;
            }
            cookies.push(Cookie {
                domain: columns[0].to_string(),
                path: columns[2].to_string(),
                secure: columns[3].eq_ignore_ascii_case("TRUE"),
                expires: columns[4].parse().ok(),
                name: columns[5].to_string(),
                value: columns[6..].join("\t"),
                http_only: raw.starts_with("#HttpOnly_"),
                same_site: -1,
            });
        }
    } else {
        let domain = domain_whitelist(platform)[0];
        for pair in trimmed
            .strip_prefix("Cookie:")
            .or_else(|| trimmed.strip_prefix("cookie:"))
            .unwrap_or(trimmed)
            .split(';')
        {
            let Some((name, value)) = pair.trim().split_once('=') else {
                continue;
            };
            if name.trim().is_empty() || value.trim().is_empty() {
                continue;
            }
            cookies.push(Cookie {
                domain: format!(".{domain}"),
                path: "/".to_string(),
                secure: true,
                expires: Some(2_147_483_647),
                name: name.trim().to_string(),
                value: value.trim().to_string(),
                http_only: false,
                same_site: -1,
            });
        }
    }
    if cookies.is_empty() {
        return Err("未识别到当前平台的有效 Cookie。".to_string());
    }
    Ok(cookies)
}

fn validate_critical_cookies(platform: &str, cookies: &[Cookie]) -> Result<(), String> {
    let has = |name: &str| {
        cookies
            .iter()
            .any(|cookie| cookie.name == name && !cookie.value.trim().is_empty())
    };
    let valid = match platform {
        "douyin" => has("sessionid") || has("sessionid_ss"),
        "bilibili" => has("SESSDATA"),
        "youtube" => {
            has("LOGIN_INFO")
                && (has("SAPISID") || has("__Secure-1PAPISID") || has("__Secure-3PAPISID"))
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "登录态缺少 {} 的关键 Cookie，请确认所选 profile 已登录。",
            platform_label(platform)
        ))
    }
}

fn validate_platform(platform: &str) -> Result<&str, String> {
    match platform {
        "douyin" | "bilibili" | "youtube" => Ok(platform),
        _ => Err("不支持的平台。".to_string()),
    }
}

fn domain_whitelist(platform: &str) -> &'static [&'static str] {
    match platform {
        "douyin" => &["douyin.com", "iesdouyin.com"],
        "bilibili" => &["bilibili.com", "b23.tv"],
        "youtube" => &["youtube.com", "google.com"],
        _ => &[],
    }
}

fn domain_allowed(platform: &str, domain: &str) -> bool {
    let normalized = domain
        .trim_start_matches("#HttpOnly_")
        .trim_start_matches('.');
    domain_whitelist(platform)
        .iter()
        .any(|allowed| normalized == *allowed || normalized.ends_with(&format!(".{allowed}")))
}

fn platform_label(platform: &str) -> &'static str {
    match platform {
        "douyin" => "抖音",
        "bilibili" => "Bilibili",
        "youtube" => "YouTube",
        _ => "平台",
    }
}

fn auth_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(crate::settings::home_dir()));
    #[cfg(not(target_os = "windows"))]
    let base = PathBuf::from(crate::settings::home_dir())
        .join("Library")
        .join("Application Support");
    base.join("StreamVerse").join("auth-v2")
}

fn cookie_file_path(platform: &str) -> PathBuf {
    auth_root().join(format!("{platform}.cookies.txt"))
}

#[cfg(target_os = "windows")]
fn atomic_replace(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(format!(
            "原子替换 Cookie 文件失败：{}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|error| format!("原子替换 Cookie 文件失败：{error}"))
}

#[cfg(target_os = "windows")]
fn restrict_to_current_user(path: &Path, owner_account: Option<&str>) -> Result<(), String> {
    let account = match owner_account {
        Some(account) if !account.trim().is_empty() => account.to_string(),
        _ => current_windows_account()?,
    };
    let status = Command::new("icacls.exe")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &format!("{account}:(F)")])
        .status()
        .map_err(|error| format!("限制 Cookie 文件权限失败：{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("限制 Cookie 文件权限失败。".to_string())
    }
}

#[cfg(target_os = "windows")]
fn current_windows_account() -> Result<String, String> {
    let username = env::var("USERNAME").map_err(|_| "无法确定当前 Windows 用户。".to_string())?;
    let domain = env::var("USERDOMAIN").unwrap_or_default();
    Ok(qualify_windows_account(&domain, &username))
}

fn qualify_windows_account(domain: &str, username: &str) -> String {
    if domain.trim().is_empty() {
        username.to_string()
    } else {
        format!(r"{}\{}", domain.trim(), username)
    }
}

#[cfg(not(target_os = "windows"))]
fn restrict_to_current_user(path: &Path, _owner_account: Option<&str>) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("限制 Cookie 文件权限失败：{error}"))
}

fn needs_elevation(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("app-bound")
        || lower.contains("appbound")
        || lower.contains("elevation")
        || lower.contains("decrypt")
        || lower.contains("provider_failed")
        || lower.contains("provider_unavailable")
        || lower.contains("access denied")
}

fn cookie_extraction_error(browser_id: &str, issue_text: &str) -> String {
    let browser = match browser_id {
        "chrome" => "Google Chrome",
        "edge" => "Microsoft Edge",
        "firefox" => "Firefox",
        "safari" => "Safari",
        _ => "所选浏览器",
    };
    let lower = issue_text.to_ascii_lowercase();

    if lower.contains("share-locked")
        || lower.contains("os error 32")
        || lower.contains("being used by another process")
    {
        return format!(
            "{browser} 正在占用 Cookie 数据库。请完全退出浏览器（包括后台进程）后重试；StreamVerse 不会强制关闭浏览器。也可以改用 cookies.txt 导入。"
        );
    }
    if lower.contains("app-bound") || lower.contains("appbound") {
        return format!("{browser} 使用了 App-Bound Encryption，普通权限无法解密 Cookie。");
    }
    if lower.contains("decrypt")
        || lower.contains("provider_failed")
        || lower.contains("provider_unavailable")
        || lower.contains("access denied")
        || lower.contains("elevation")
    {
        return format!("{browser} 的 Cookie 解密被系统拒绝，需要 elevation 权限。");
    }

    format!(
        "未读取到 {browser} 中当前平台的登录 Cookie。请确认所选 Profile 已登录，或改用 cookies.txt 导入。"
    )
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(target_os = "windows")]
fn default_browser_id() -> Option<String> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let key: Vec<u16> = std::ffi::OsStr::new(
        "Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\https\\UserChoice",
    )
    .encode_wide()
    .chain(Some(0))
    .collect();
    let value: Vec<u16> = std::ffi::OsStr::new("ProgId")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut buffer = vec![0u16; 256];
    let mut size = (buffer.len() * 2) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut size,
        )
    };
    if result != 0 {
        return None;
    }
    let prog_id = String::from_utf16_lossy(&buffer[..(size as usize / 2).saturating_sub(1)])
        .to_ascii_lowercase();
    if prog_id.contains("chrome") {
        Some("chrome".to_string())
    } else if prog_id.contains("edge") {
        Some("edge".to_string())
    } else if prog_id.contains("firefox") {
        Some("firefox".to_string())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn default_browser_id() -> Option<String> {
    let output = Command::new("defaults")
        .args([
            "read",
            "com.apple.LaunchServices/com.apple.launchservices.secure",
            "LSHandlers",
        ])
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if value.contains("com.google.chrome") {
        Some("chrome".to_string())
    } else if value.contains("org.mozilla.firefox") {
        Some("firefox".to_string())
    } else if value.contains("com.apple.safari") {
        Some("safari".to_string())
    } else {
        None
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn default_browser_id() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::{
        cookie_extraction_error, domain_allowed, parse_manual_cookies, qualify_windows_account,
        validate_critical_cookies,
    };

    #[test]
    fn manual_cookie_header_is_scoped_to_one_platform_domain() {
        let cookies = parse_manual_cookies("douyin", "sessionid=abc; sid_tt=def").unwrap();
        assert!(cookies.iter().all(|cookie| cookie.domain == ".douyin.com"));
        assert!(cookies
            .iter()
            .all(|cookie| !cookie.domain.contains("bilibili")));
    }

    #[test]
    fn rejects_cross_platform_netscape_rows() {
        let content = ".bilibili.com\tTRUE\t/\tTRUE\t0\tSESSDATA\tsecret";
        assert!(parse_manual_cookies("douyin", content).is_err());
    }

    #[test]
    fn critical_cookie_check_is_platform_specific() {
        let cookies = parse_manual_cookies("bilibili", "SESSDATA=secret").unwrap();
        assert!(validate_critical_cookies("bilibili", &cookies).is_ok());
        assert!(!domain_allowed("youtube", ".bilibili.com"));
    }

    #[test]
    fn youtube_cookie_check_matches_ytdlp_auth_requirements() {
        let valid =
            parse_manual_cookies("youtube", "LOGIN_INFO=login; __Secure-1PAPISID=account").unwrap();
        assert!(validate_critical_cookies("youtube", &valid).is_ok());

        let missing_login_info =
            parse_manual_cookies("youtube", "SAPISID=account; SID=legacy").unwrap();
        assert!(validate_critical_cookies("youtube", &missing_login_info).is_err());
    }

    #[test]
    fn chrome_prefers_the_recent_profile_exposed_first_by_rookie() {
        assert!(super::preferred_profile("chrome", 0, false));
        assert!(!super::preferred_profile("chrome", 1, true));
        assert!(super::preferred_profile("edge", 1, true));
    }

    #[test]
    fn locked_browser_database_has_actionable_error() {
        let error = cookie_extraction_error(
            "chrome",
            "Windows browser database is share-locked (OS error 32)",
        );
        assert!(error.contains("完全退出浏览器"));
        assert!(error.contains("不会强制关闭"));
        assert!(!error.contains("ExtractionIssue"));
    }

    #[test]
    fn app_bound_error_still_requests_elevation() {
        let error = cookie_extraction_error("chrome", "App-Bound Encryption blocked decrypt");
        assert!(super::needs_elevation(&error));
        assert!(super::is_decryption_issue_code("decrypt_failed"));
        assert!(super::is_decryption_issue_code("provider_failed"));
        assert!(!super::is_decryption_issue_code("decode_failed"));
        assert!(super::needs_elevation(&cookie_extraction_error(
            "chrome",
            "provider_failed: protected key unavailable"
        )));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn elevated_helper_parameters_quote_paths() {
        use std::path::Path;

        let parameters = super::elevated_helper_parameters(
            Path::new(r"C:\Users\Test User\request.json"),
            Path::new(r"C:\Users\Test User\result.json"),
        )
        .unwrap();
        let decoded = String::from_utf16(&parameters[..parameters.len() - 1]).unwrap();
        assert_eq!(
            decoded,
            r#"--cookie-helper "C:\Users\Test User\request.json" "C:\Users\Test User\result.json""#
        );
    }

    #[test]
    fn windows_account_is_qualified_with_domain() {
        assert_eq!(
            qualify_windows_account("DC-MYJ", "DC-MYJ"),
            r"DC-MYJ\DC-MYJ"
        );
        assert_eq!(qualify_windows_account("", "user"), "user");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn restricted_file_remains_readable_by_original_user() {
        let path =
            std::env::temp_dir().join(format!("streamverse-auth-acl-{}.txt", super::now_millis()));
        std::fs::write(&path, b"acl-smoke-test").unwrap();
        let account = super::current_windows_account().unwrap();
        super::restrict_to_current_user(&path, Some(&account)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"acl-smoke-test");
        std::fs::remove_file(path).unwrap();
    }
}
