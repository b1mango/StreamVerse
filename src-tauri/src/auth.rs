use rookie::enums::Cookie;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File, OpenOptions};
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
    /// true = 未能枚举真实 Profile，列表仅含展示用兜底入口。
    /// 批量同步必须重新枚举并选择真实 Profile ID。
    pub degraded: bool,
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
pub struct BrowserSyncRequest {
    pub browser_id: String,
    pub profile_id: Option<String>,
    pub consent: String,
    pub platforms: Vec<String>,
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
        let _access = crate::browser_access::restore(&id);
        let enumerated = if id == "chrome" {
            rookie::chrome_profiles()
        } else {
            rookie::browser_profiles(&id)
        };
        // 枚举失败或为空的原因尚不确定；数据目录存在时保留展示入口供重试。
        let (profiles, degraded) = match enumerated {
            Ok(profiles) if !profiles.is_empty() => (
                profiles
                    .into_iter()
                    .enumerate()
                    .map(|(index, profile)| BrowserProfile {
                        id: profile.profile.profile_id.as_str().to_string(),
                        label: profile.profile.display_name,
                        is_default: preferred_profile(&id, index, profile.is_default),
                    })
                    .collect(),
                false,
            ),
            _ => match fallback_profile_if_installed(&id) {
                Some(profile) => (vec![profile], true),
                None => continue,
            },
        };
        sources.push(BrowserSource {
            is_default: default.as_deref() == Some(id.as_str()),
            id: id.clone(),
            label: browser.display_name,
            profiles,
            degraded,
        });
    }
    sources.sort_by_key(|source| !source.is_default);
    Ok(sources)
}

/// 浏览器已安装（数据目录存在）但 profile 枚举失败时的兜底入口。
/// "Default" 仅为展示占位，不是 browser_report 可用的真实 Profile ID；
/// 仅 Chromium 系（chrome/edge）适用，firefox/safari 无此目录约定。
fn fallback_profile_if_installed(browser_id: &str) -> Option<BrowserProfile> {
    fallback_profile(browser_id, &PathBuf::from(crate::settings::home_dir()))
}

fn fallback_profile(browser_id: &str, home: &Path) -> Option<BrowserProfile> {
    let root = browser_data_root(browser_id, home)?;
    root.is_dir().then(|| BrowserProfile {
        id: "Default".to_string(),
        label: "默认".to_string(),
        is_default: true,
    })
}

fn browser_data_root(browser_id: &str, home: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let rel = match browser_id {
            "chrome" => "Library/Application Support/Google/Chrome",
            "edge" => "Library/Application Support/Microsoft Edge",
            _ => return None,
        };
        Some(home.join(rel))
    }
    #[cfg(target_os = "windows")]
    {
        let rel = match browser_id {
            "chrome" => "Google/Chrome/User Data",
            "edge" => "Microsoft/Edge/User Data",
            _ => return None,
        };
        let base = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Local"));
        Some(base.join(rel))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (browser_id, home);
        None
    }
}

pub fn import_browser_cookies(request: &CookieImportRequest) -> Result<CookieImportResult, String> {
    let mut request = request.clone();
    request.owner_account = None;
    let request = &request;
    validate_request(request)?;
    match extract_browser_cookies(request) {
        Ok((profile_id, cookies)) => {
            let mut resolved = request.clone();
            resolved.profile_id = Some(profile_id);
            persist_import(&resolved, cookies)
        }
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
        Err(error) if request.allow_elevation && needs_elevation(&error) => {
            run_elevated_import(request)
        }
        Err(error) => Err(error),
    }
}

/// 从同一真实 Profile 读取一次，各平台校验成功后才替换各自的 Cookie。
pub fn sync_browser_cookies(
    request: &BrowserSyncRequest,
) -> Result<Vec<CookieImportResult>, String> {
    let platforms = validate_sync_request(request)?;
    let _access = crate::browser_access::restore(&request.browser_id);
    let domains = platforms
        .iter()
        .flat_map(|platform| domain_whitelist(platform).iter().copied())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect();
    let report = rookie::browser_report(
        &request.browser_id,
        request.profile_id.as_deref(),
        Some(domains),
    )
    .map_err(|error| cookie_extraction_error(&request.browser_id, &error.to_string()))?;
    let extracted =
        unpack_browser_report(&request.browser_id, request.profile_id.as_deref(), report)?;
    Ok(persist_platform_cookies(
        request,
        &platforms,
        extracted,
        persist_import,
    ))
}

pub(crate) fn validate_sync_request(request: &BrowserSyncRequest) -> Result<Vec<String>, String> {
    if request.platforms.is_empty() {
        return Err("请至少选择一个同步平台。".to_string());
    }
    let mut platforms = Vec::new();
    for platform in &request.platforms {
        validate_request(&CookieImportRequest {
            platform: platform.clone(),
            browser_id: request.browser_id.clone(),
            profile_id: request.profile_id.clone(),
            consent: request.consent.clone(),
            allow_elevation: false,
            owner_account: None,
        })?;
        if !platforms.contains(platform) {
            platforms.push(platform.clone());
        }
    }
    Ok(platforms)
}

fn persist_platform_cookies(
    request: &BrowserSyncRequest,
    platforms: &[String],
    extracted: ExtractedProfile,
    mut persist: impl FnMut(&CookieImportRequest, Vec<Cookie>) -> Result<CookieImportResult, String>,
) -> Vec<CookieImportResult> {
    // 平台域名集合互不重叠；再过滤一次，不能依赖提取库的域名匹配语义。
    let mut buckets: std::collections::BTreeMap<_, Vec<Cookie>> = platforms
        .iter()
        .map(|platform| (platform.as_str(), Vec::new()))
        .collect();
    let now = now_seconds();
    for cookie in extracted.cookies {
        if !cookie_is_current(&cookie, now) {
            continue;
        }
        if let Some(platform) = platforms.iter().find(|p| domain_allowed(p, &cookie.domain)) {
            buckets.get_mut(platform.as_str()).unwrap().push(cookie);
        }
    }
    platforms
        .iter()
        .map(|platform| {
            let cookies = buckets.remove(platform.as_str()).unwrap_or_default();
            let no_cookies = cookies.is_empty();
            let import = CookieImportRequest {
                platform: platform.clone(),
                browser_id: request.browser_id.clone(),
                profile_id: Some(extracted.profile_id.clone()),
                consent: request.consent.clone(),
                allow_elevation: false,
                owner_account: None,
            };
            validate_critical_cookies(platform, &cookies)
                .map_err(|error| {
                    if extracted.has_decryption_issue
                        || (no_cookies && !extracted.issue_text.is_empty())
                    {
                        cookie_extraction_error(&request.browser_id, &extracted.issue_text)
                    } else {
                        error
                    }
                })
                .and_then(|()| persist(&import, cookies))
                .unwrap_or_else(|message| CookieImportResult {
                    platform: platform.clone(),
                    browser_id: request.browser_id.clone(),
                    profile_id: Some(extracted.profile_id.clone()),
                    status: "failed".to_string(),
                    imported_count: 0,
                    requires_elevation: needs_elevation(&message),
                    message,
                })
        })
        .collect()
}

/// 登录态失效后，用设置中保存的浏览器来源静默重取 Cookie（自动续期）。
pub fn refresh_browser_cookies(
    platform: &str,
    browser_id: &str,
    profile_id: Option<String>,
) -> Result<CookieImportResult, String> {
    import_browser_cookies(&CookieImportRequest {
        platform: platform.to_string(),
        browser_id: browser_id.to_string(),
        profile_id,
        consent: "always".to_string(),
        allow_elevation: true,
        owner_account: None,
    })
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

/// 应用内登录窗口的 Cookie 导入：Cookie 来自应用自己的 WebView 数据目录，
/// 不读取任何浏览器数据，无需 TCC/钥匙串/完全磁盘访问授权。
pub fn save_webview_cookies(
    platform: &str,
    cookies: Vec<Cookie>,
) -> Result<CookieImportResult, String> {
    let platform = validate_platform(platform)?;
    validate_critical_cookies(platform, &cookies)?;
    write_cookie_file(platform, &cookies, None)?;
    Ok(CookieImportResult {
        platform: platform.to_string(),
        browser_id: "webview".to_string(),
        profile_id: None,
        status: "active".to_string(),
        imported_count: cookies.len(),
        requires_elevation: false,
        message: "应用内登录态已保存。".to_string(),
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

/// 调用方负责串行化认证操作；仅在 action 返回 Err 时恢复 Cookie，不回滚设置。
pub fn with_cookie_rollback<T>(
    platforms: &[String],
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    // 全部验证通过后才读取任何认证文件。
    for platform in platforms {
        validate_platform(platform)?;
    }
    let paths: Vec<PathBuf> = platforms
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|platform| cookie_file_path(platform))
        .collect();
    with_cookie_rollback_paths(&paths, action)
}

fn with_cookie_rollback_paths<T>(
    paths: &[PathBuf],
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let snapshots = paths
        .iter()
        .map(|path| match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("备份 Cookie 失败，操作未执行：{error}")),
        })
        .collect::<Result<Vec<_>, String>>()?;
    match action() {
        Ok(value) => Ok(value),
        Err(error) => {
            let mut failures = Vec::new();
            for (index, (path, original)) in paths.iter().zip(&snapshots).enumerate() {
                let unchanged = match fs::read(path) {
                    Ok(current) => original.as_deref() == Some(current.as_slice()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        original.is_none()
                    }
                    Err(_) => false,
                };
                if unchanged {
                    continue;
                }
                if let Err(restore_error) = restore_cookie_bytes(path, original.as_deref()) {
                    failures.push(format!("第 {} 个 Cookie 文件：{restore_error}", index + 1));
                }
            }
            if failures.is_empty() {
                Err(error)
            } else {
                Err(format!(
                    "{error}；Cookie 回滚未完成：{}",
                    failures.join("；")
                ))
            }
        }
    }
}

fn restore_cookie_bytes(target: &Path, original: Option<&[u8]>) -> Result<(), String> {
    let Some(bytes) = original else {
        return match fs::remove_file(target) {
            Ok(()) => sync_cookie_parent(target),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("移除新增 Cookie 失败：{error}")),
        };
    };
    let parent = target
        .parent()
        .ok_or_else(|| "Cookie 路径缺少父目录。".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建 Cookie 恢复目录失败：{error}"))?;
    let (temporary, mut file) = create_private_cookie_temp(parent)?;
    let restored = (|| {
        // Windows 在写入任何敏感字节前收紧 ACL；Unix 创建时即为 0600。
        restrict_to_current_user(&temporary, None)?;
        file.write_all(bytes)
            .map_err(|error| format!("恢复 Cookie 写入失败：{error}"))?;
        file.sync_all()
            .map_err(|error| format!("同步恢复 Cookie 失败：{error}"))?;
        drop(file);
        atomic_replace(&temporary, target)?;
        // atomic_replace 的 Unix 目录同步为 best-effort，此处必须报告失败。
        sync_cookie_parent(target)
    })();
    if let Err(error) = restored {
        return match fs::remove_file(&temporary) {
            Ok(()) => Err(error),
            Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => Err(error),
            Err(cleanup) => Err(format!("{error}；清理恢复临时文件失败：{cleanup}")),
        };
    }
    Ok(())
}

fn create_private_cookie_temp(parent: &Path) -> Result<(PathBuf, File), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    for _ in 0..128 {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(
            ".cookie-rollback-{}-{nonce}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("创建 Cookie 恢复临时文件失败：{error}")),
        }
    }
    Err("无法分配唯一的 Cookie 恢复临时文件。".to_string())
}

fn sync_cookie_parent(target: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let parent = target
            .parent()
            .ok_or_else(|| "Cookie 路径缺少父目录。".to_string())?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("同步 Cookie 恢复目录失败：{error}"))?;
    }
    #[cfg(not(unix))]
    let _ = target; // Windows 原子替换使用 MOVEFILE_WRITE_THROUGH。
    Ok(())
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
            extract_browser_cookies(&request).and_then(|(profile_id, cookies)| {
                let mut request = request;
                request.profile_id = Some(profile_id);
                persist_import(&request, cookies)
            })
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

pub(crate) fn validate_request(request: &CookieImportRequest) -> Result<(), String> {
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
    if request
        .profile_id
        .as_deref()
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(
            "请选择真实的浏览器 Profile 后再导入；若无法枚举，请先完成浏览器来源授权并刷新列表。"
                .to_string(),
        );
    }
    Ok(())
}

fn extract_browser_cookies(request: &CookieImportRequest) -> Result<(String, Vec<Cookie>), String> {
    let _access = crate::browser_access::restore(&request.browser_id);
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
    let mut extracted =
        unpack_browser_report(&request.browser_id, request.profile_id.as_deref(), report)?;
    let now = now_seconds();
    extracted.cookies.retain(|cookie| {
        domain_allowed(&request.platform, &cookie.domain) && cookie_is_current(cookie, now)
    });
    if let Err(error) = validate_critical_cookies(&request.platform, &extracted.cookies) {
        if extracted.has_decryption_issue || extracted.cookies.is_empty() {
            return Err(cookie_extraction_error(
                &request.browser_id,
                &extracted.issue_text,
            ));
        }
        return Err(format!(
            "{error} 当前读取的是“{}”；如登录账号位于其他 Profile，请在下拉框中切换后重试。",
            extracted.profile_name
        ));
    }
    Ok((extracted.profile_id, extracted.cookies))
}

struct ExtractedProfile {
    profile_id: String,
    profile_name: String,
    cookies: Vec<Cookie>,
    issue_text: String,
    has_decryption_issue: bool,
}

fn require_single_profile<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    expected: Option<&str>,
) -> Result<String, String> {
    let mut ids = ids.into_iter();
    let id = ids
        .next()
        .ok_or_else(|| "未读取到所选浏览器 Profile。".to_string())?;
    if id.trim().is_empty() || ids.next().is_some() || expected.is_some_and(|value| value != id) {
        return Err("浏览器来源未匹配唯一的所选 Profile，已停止导入以避免混用账号。".to_string());
    }
    Ok(id.to_string())
}

fn unpack_browser_report(
    browser_id: &str,
    expected: Option<&str>,
    report: rookie::report::ExtractionReport,
) -> Result<ExtractedProfile, String> {
    let mut cookies = Vec::new();
    let mut issue_text = String::new();
    let mut has_decryption_issue = false;
    for issue in report.issues {
        record_extraction_issue(&issue, &mut issue_text, &mut has_decryption_issue);
    }
    if report.profiles.is_empty() {
        return Err(cookie_extraction_error(browser_id, &issue_text));
    }
    let profile_id = require_single_profile(
        report
            .profiles
            .iter()
            .map(|profile| profile.profile.profile_id.as_str()),
        expected,
    )?;
    let mut profile_name = String::new();
    for profile in report.profiles {
        for issue in profile.issues {
            record_extraction_issue(&issue, &mut issue_text, &mut has_decryption_issue);
        }
        profile_name = profile.profile.display_name;
        for source in profile.sources {
            for issue in source.issues {
                record_extraction_issue(&issue, &mut issue_text, &mut has_decryption_issue);
            }
            if source.selected {
                cookies.extend(source.cookies);
            }
        }
    }
    Ok(ExtractedProfile {
        profile_id,
        profile_name,
        cookies,
        issue_text,
        has_decryption_issue,
    })
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
    let now = now_seconds();
    for cookie in cookies {
        if !domain_allowed(platform, &cookie.domain) || !cookie_is_current(cookie, now) {
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
    drop(file);
    if let Err(error) = restrict_to_current_user(&temporary, owner_account)
        .and_then(|()| atomic_replace(&temporary, &target))
    {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
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

pub(crate) fn validate_critical_cookies(platform: &str, cookies: &[Cookie]) -> Result<(), String> {
    let now = now_seconds();
    let has = |name: &str| {
        cookies.iter().any(|cookie| {
            cookie.name == name
                && domain_allowed(platform, &cookie.domain)
                && cookie_is_current(cookie, now)
        })
    };
    let valid = match platform {
        "douyin" => has("sessionid") || has("sessionid_ss"),
        "bilibili" => has("SESSDATA"),
        "youtube" => {
            // 与 settings::validate_cookie_file_for_platform 保持一致：
            // LOGIN_INFO 或 SAPISID 家族任一存在即可构成 YouTube 登录态
            has("LOGIN_INFO")
                || has("SAPISID")
                || has("__Secure-1PAPISID")
                || has("__Secure-3PAPISID")
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "登录态缺少 {} 的有效关键 Cookie（可能已过期），请确认所选 profile 已登录。",
            platform_label(platform)
        ))
    }
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cookie_is_current(cookie: &Cookie, now: u64) -> bool {
    // Netscape 的 0 和 rookie 的 None 均表示会话 Cookie。
    !cookie.value.trim().is_empty()
        && cookie
            .expires
            .is_none_or(|expires| expires == 0 || expires > now)
}

pub(crate) fn validate_platform(platform: &str) -> Result<&str, String> {
    if crate::settings::AUTH_PLATFORM_IDS.contains(&platform) {
        Ok(platform)
    } else {
        Err("不支持的平台。".to_string())
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

pub(crate) fn platform_label(platform: &str) -> &'static str {
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
pub(crate) fn atomic_replace(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // MOVEFILE_WRITE_THROUGH 让移动在返回前写穿到磁盘，rename 本身的持久性
    // 已由该标志保证，无需像 Unix 分支那样再 fsync 父目录。
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
pub(crate) fn atomic_replace(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|error| format!("原子替换 Cookie 文件失败：{error}"))?;
    // rename 只改目录项，需 fsync 父目录才能保证替换本身落盘（否则断电可能丢整个文件）。
    // fsync 失败时替换已完成，仅降低断电持久性，故尽力而为、不改变调用方语义。
    if let Some(parent) = target.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
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
    cfg!(target_os = "windows")
        && (lower.contains("app-bound")
            || lower.contains("appbound")
            || lower.contains("elevation")
            || lower.contains("decrypt")
            || lower.contains("provider_failed")
            || lower.contains("provider_unavailable")
            || lower.contains("access denied"))
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
    if cfg!(target_os = "windows") && (lower.contains("app-bound") || lower.contains("appbound")) {
        return format!("{browser} 使用了 App-Bound Encryption，普通权限无法解密 Cookie。");
    }
    // 只有明确的 EPERM 才提示局部授权，枚举失败本身不证明权限原因。
    let explicit_eperm = lower.contains("operation not permitted")
        || lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| word == "eperm")
        || lower.split("os error ").skip(1).any(|tail| {
            tail.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                == "1"
        });
    if cfg!(target_os = "macos") && explicit_eperm {
        return format!(
            "macOS 拒绝了 StreamVerse 读取 {browser} 的数据（EPERM）。请在浏览器来源旁点击局部授权，选择对应的浏览器数据目录后重试。"
        );
    }
    if lower.contains("installation_enumeration_failed")
        || lower.contains("failed profile enumeration")
        || lower.contains("not installed")
        || lower.contains("no_sources")
    {
        let detail: String = issue_text.trim().chars().take(160).collect();
        return format!(
            "未能枚举 {browser} 的 Profile。请确认浏览器已安装、数据目录存在，并重新选择浏览器来源。诊断信息：{detail}"
        );
    }
    // 兜底/已变更的 profile id 无法解析（rookie 选择器靠重新枚举工作）
    if lower.contains("unknown") && lower.contains("profile id") {
        return "无法识别所选的浏览器 Profile（浏览器数据不可读或 Profile 已变更）。请重新选择浏览器来源后重试。".to_string();
    }
    // 钥匙串拒绝/无法弹窗（security CLI exit 128）。必须排在 decrypt/provider_failed
    // 之前：钥匙串失败的诊断文本里也含 "provider_failed"/"access denied"，
    // 但 macOS 上的正确指引是钥匙串授权而非 elevation
    if cfg!(target_os = "macos")
        && (lower.contains("keychain")
            || lower.contains("user interaction is not allowed")
            || lower.contains("interaction canceled"))
    {
        return format!(
            "macOS 钥匙串拒绝了读取 {browser} Cookie 解密密钥（Safe Storage）的请求。请在弹出的授权框中点「允许」或「始终允许」；若之前点了「拒绝」，完全退出 StreamVerse 后重试会重新弹出。"
        );
    }
    if lower.contains("decrypt")
        || lower.contains("provider_failed")
        || lower.contains("provider_unavailable")
        || lower.contains("access denied")
        || lower.contains("elevation")
    {
        if cfg!(target_os = "windows") {
            return format!("{browser} 的 Cookie 解密被系统拒绝，需要 elevation 权限。");
        }
        return format!("{browser} 的 Cookie 解密失败。请检查浏览器登录状态及系统密钥访问，或改用 cookies.txt 导入。");
    }

    // 兜底：附上截断的原始诊断，真实原因不被泛化文案吞掉
    let detail = issue_text.trim();
    if detail.is_empty() {
        return format!(
            "未读取到 {browser} 中当前平台的登录 Cookie。请确认所选 Profile 已登录，或改用 cookies.txt 导入。"
        );
    }
    let truncated: String = detail.chars().take(160).collect();
    format!(
        "未读取到 {browser} 中当前平台的登录 Cookie。请确认所选 Profile 已登录，或改用 cookies.txt 导入。诊断信息：{truncated}"
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

    fn rollback_test_directory() -> std::path::PathBuf {
        let (path, file) = super::create_private_cookie_temp(&std::env::temp_dir()).unwrap();
        drop(file);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn rollback_restores_original_bytes_and_removes_new_files() {
        let root = rollback_test_directory();
        let existing = root.join("existing.txt");
        let added = root.join("added.txt");
        let original = b"synthetic-cookie\0\xff\r\n";
        std::fs::write(&existing, original).unwrap();
        let result: Result<(), String> =
            super::with_cookie_rollback_paths(&[existing.clone(), added.clone()], || {
                std::fs::write(&existing, b"replacement").unwrap();
                std::fs::write(&added, b"new").unwrap();
                Err("settings failed".to_string())
            });
        assert_eq!(result.unwrap_err(), "settings failed");
        assert_eq!(std::fs::read(&existing).unwrap(), original);
        assert!(!added.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_success_keeps_changes_and_returns_value() {
        let root = rollback_test_directory();
        let path = root.join("cookie.txt");
        std::fs::write(&path, b"old").unwrap();
        let result = super::with_cookie_rollback_paths(&[path.clone()], || {
            std::fs::write(&path, b"new").unwrap();
            Ok(42)
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_does_not_replace_unchanged_file_or_create_missing_file() {
        let root = rollback_test_directory();
        let path = root.join("unchanged.txt");
        let missing = root.join("missing.txt");
        std::fs::write(&path, b"unchanged").unwrap();
        let before = std::fs::metadata(&path).unwrap();
        let result: Result<(), String> =
            super::with_cookie_rollback_paths(&[path.clone(), missing.clone()], || {
                Err("settings failed".to_string())
            });
        assert_eq!(result.unwrap_err(), "settings failed");
        let after = std::fs::metadata(&path).unwrap();
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(before.ino(), after.ino());
        }
        assert!(!missing.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_backup_failure_prevents_action() {
        let root = rollback_test_directory();
        let mut called = false;
        let result = super::with_cookie_rollback_paths(&[root.clone()], || {
            called = true;
            Ok(())
        });
        assert!(!called);
        assert!(result.unwrap_err().contains("操作未执行"));
        std::fs::remove_dir_all(root).unwrap();
        // 非法平台在解析实际认证路径/读取 Cookie 之前拒绝。
        assert!(super::with_cookie_rollback(&["invalid".to_string()], || {
            panic!("invalid platform must not execute action");
            #[allow(unreachable_code)]
            Ok(())
        })
        .is_err());
    }

    #[test]
    fn rollback_reports_failure_and_still_restores_other_files() {
        let root = rollback_test_directory();
        let blocked = root.join("blocked.txt");
        let intact = root.join("intact.txt");
        std::fs::write(&blocked, b"synthetic-secret").unwrap();
        std::fs::write(&intact, b"original").unwrap();
        let result: Result<(), String> =
            super::with_cookie_rollback_paths(&[blocked.clone(), intact.clone()], || {
                std::fs::remove_file(&blocked).unwrap();
                std::fs::create_dir(&blocked).unwrap();
                std::fs::write(&intact, b"changed").unwrap();
                Err("settings failed".to_string())
            });
        let error = result.unwrap_err();
        assert!(error.contains("settings failed"));
        assert!(error.contains("Cookie 回滚未完成"));
        assert!(!error.contains("synthetic-secret"));
        assert_eq!(std::fs::read(&intact).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_reports_failure_to_remove_new_file() {
        let root = rollback_test_directory();
        let path = root.join("new.txt");
        let result: Result<(), String> = super::with_cookie_rollback_paths(&[path.clone()], || {
            std::fs::create_dir(&path).unwrap();
            Err("action failed".to_string())
        });
        assert!(result.unwrap_err().contains("移除新增 Cookie 失败"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_temp_names_are_unique_and_private() {
        let root = rollback_test_directory();
        let (first, first_file) = super::create_private_cookie_temp(&root).unwrap();
        let (second, second_file) = super::create_private_cookie_temp(&root).unwrap();
        assert_ne!(first, second);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                first_file.metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(first_file);
        drop(second_file);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fallback_profile_only_for_installed_chromium_browsers() {
        let temp = std::env::temp_dir().join(format!("sv-auth-test-{}", std::process::id()));
        let home = temp.join("home");
        // 未安装（数据目录不存在）→ 不兜底，浏览器不下拉展示
        assert!(super::fallback_profile("chrome", &home).is_none());
        // 已安装但枚举失败（如 TCC 拒绝）→ 保留 Default 入口供重选重试
        std::fs::create_dir_all(home.join("Library/Application Support/Google/Chrome")).unwrap();
        let profile = super::fallback_profile("chrome", &home).expect("installed chrome");
        assert_eq!(profile.id, "Default");
        assert!(profile.is_default);
        // 非 Chromium 系（firefox/safari）无 Default 目录约定，不兜底
        assert!(super::fallback_profile("firefox", &home).is_none());
        std::fs::remove_dir_all(&temp).ok();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn eperm_maps_to_macos_privacy_guidance() {
        let message = cookie_extraction_error(
            "chrome",
            "failed to read installation Local State: Operation not permitted (os error 1)",
        );
        assert!(message.contains("局部授权"));
        assert!(!message.contains("完全磁盘访问"));
        assert!(cookie_extraction_error("chrome", "EPERM").contains("局部授权"));
        assert!(!cookie_extraction_error("chrome", "os error 13").contains("局部授权"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_denial_maps_to_keychain_guidance() {
        // rookie 对 security CLI exit 128 的诊断文本，含 provider_failed/access denied，
        // 必须优先命中钥匙串指引而非 elevation 文案
        let message = cookie_extraction_error(
            "chrome",
            "provider_failed: Chromium v10 key provider failed: macOS Keychain lookup failed: macOS Keychain access denied or interaction canceled (exit code 128; sanitized stderr: User interaction is not allowed.)",
        );
        assert!(message.contains("钥匙串"));
        assert!(!message.contains("elevation"));
    }

    #[test]
    fn unmatched_issues_keep_raw_diagnostic_in_fallback_message() {
        let message = cookie_extraction_error("chrome", " source_open_failed: some novel cause");
        assert!(message.contains("诊断信息"));
        assert!(message.contains("some novel cause"));
        // 无任何诊断时保持原文案
        assert_eq!(
            cookie_extraction_error("chrome", ""),
            "未读取到 Google Chrome 中当前平台的登录 Cookie。请确认所选 Profile 已登录，或改用 cookies.txt 导入。"
        );
    }

    #[test]
    fn enumeration_failure_does_not_assume_privacy_denial() {
        let message = cookie_extraction_error(
            "chrome",
            " installation_enumeration_failed: /Users/x/Library/Application Support/Google/Chrome: read directory /Users/x/Library/Application Support/Google/Chrome",
        );
        assert!(message.contains("未能枚举"));
        assert!(!message.contains("隐私与安全性"));
        assert!(!message.contains("局部授权"));
        let message = cookie_extraction_error(
            "edge",
            "every detected edge installation failed profile enumeration: /path: read directory /path",
        );
        assert!(message.contains("未能枚举"));
        assert!(!message.contains("完全磁盘访问"));
        assert!(!cookie_extraction_error("chrome", "not installed").contains("局部授权"));
    }

    #[test]
    fn unknown_profile_id_guides_reselect() {
        let message = cookie_extraction_error("chrome", "unknown chrome profile id \"Default\"");
        assert!(message.contains("重新选择浏览器来源"));
    }

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

        // SAPISID 家族单独存在即构成登录态（SAPISIDHASH 鉴权），不再强制要求 LOGIN_INFO
        let sapisid_only = parse_manual_cookies("youtube", "SAPISID=account; SID=legacy").unwrap();
        assert!(validate_critical_cookies("youtube", &sapisid_only).is_ok());

        // 两族 Cookie 都不在场才判定缺少关键 Cookie
        let missing_critical = parse_manual_cookies("youtube", "SID=legacy; HSID=hint").unwrap();
        assert!(validate_critical_cookies("youtube", &missing_critical).is_err());
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
        assert_eq!(super::needs_elevation(&error), cfg!(target_os = "windows"));
        assert!(super::is_decryption_issue_code("decrypt_failed"));
        assert!(super::is_decryption_issue_code("provider_failed"));
        assert!(!super::is_decryption_issue_code("decode_failed"));
        assert_eq!(
            super::needs_elevation(&cookie_extraction_error(
                "chrome",
                "provider_failed: protected key unavailable"
            )),
            cfg!(target_os = "windows")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn decryption_errors_never_request_windows_elevation() {
        for issue in [
            "decrypt_failed",
            "provider_unavailable",
            "access denied",
            "elevation",
        ] {
            let message = cookie_extraction_error("chrome", issue);
            assert!(!message.contains("elevation"));
            assert!(!super::needs_elevation(&message));
        }
    }

    fn sync_request() -> super::BrowserSyncRequest {
        serde_json::from_value(serde_json::json!({
            "browserId": "chrome", "profileId": "test-profile", "consent": "always",
            "platforms": ["douyin", "bilibili", "youtube"]
        }))
        .unwrap()
    }

    #[test]
    fn single_import_requires_explicit_profile_before_browser_access() {
        for profile_id in [None, Some(String::new()), Some("  ".to_string())] {
            let request = super::CookieImportRequest {
                platform: "bilibili".to_string(),
                browser_id: "chrome".to_string(),
                profile_id,
                consent: "once".to_string(),
                allow_elevation: false,
                owner_account: None,
            };
            assert!(super::validate_request(&request)
                .unwrap_err()
                .contains("真实"));
        }
    }

    #[test]
    fn critical_cookies_must_be_current_and_match_platform_domain() {
        let mut cookies = parse_manual_cookies("bilibili", "SESSDATA=test").unwrap();
        for expires in [None, Some(0), Some(u64::MAX)] {
            cookies[0].expires = expires;
            assert!(validate_critical_cookies("bilibili", &cookies).is_ok());
        }
        cookies[0].expires = Some(1);
        assert!(validate_critical_cookies("bilibili", &cookies).is_err());
        cookies[0].expires = Some(100);
        assert!(!super::cookie_is_current(&cookies[0], 100));
        cookies[0].expires = None;
        cookies[0].domain = ".douyin.com".to_string();
        assert!(validate_critical_cookies("bilibili", &cookies).is_err());
    }

    #[test]
    fn sync_validates_input_and_deduplicates_platforms() {
        let mut request = sync_request();
        request.platforms.push("douyin".to_string());
        assert_eq!(
            super::validate_sync_request(&request).unwrap(),
            ["douyin", "bilibili", "youtube"]
        );
        for profile in [None, Some("".to_string()), Some("  ".to_string())] {
            let mut invalid = request.clone();
            invalid.profile_id = profile;
            assert!(super::validate_sync_request(&invalid).is_err());
        }
        for platforms in [vec![], vec!["douyin".to_string(), "invalid".to_string()]] {
            let mut invalid = request.clone();
            invalid.platforms = platforms;
            assert!(super::validate_sync_request(&invalid).is_err());
        }
        request.consent = "once".to_string();
        assert!(super::validate_sync_request(&request).is_ok());
        request.consent = "invalid".to_string();
        assert!(super::validate_sync_request(&request).is_err());
        request.consent = "always".to_string();
        request.browser_id = "invalid".to_string();
        assert!(super::validate_sync_request(&request).is_err());
    }

    #[test]
    fn profile_selection_rejects_missing_multiple_and_mismatched_profiles() {
        assert!(super::require_single_profile([], Some("a")).is_err());
        assert!(super::require_single_profile(["a", "b"], None).is_err());
        assert!(super::require_single_profile(["a", "b"], Some("a")).is_err());
        assert!(super::require_single_profile(["a"], Some("b")).is_err());
        assert!(super::require_single_profile([""], None).is_err());
        assert_eq!(
            super::require_single_profile(["a"], Some("a")).unwrap(),
            "a"
        );
    }

    #[test]
    fn batch_isolates_domains_and_preserves_failed_platform() {
        let request = sync_request();
        let platforms = super::validate_sync_request(&request).unwrap();
        let mut cookies = parse_manual_cookies("douyin", "sessionid=test-douyin").unwrap();
        cookies.extend(parse_manual_cookies("youtube", "LOGIN_INFO=test-youtube").unwrap());
        // 关键名称存在于错误平台/伪造后缀域名时，不能使 B 站校验通过。
        let mut foreign = parse_manual_cookies("bilibili", "SESSDATA=foreign").unwrap();
        foreign[0].domain = ".bilibili.com.evil.test".to_string();
        cookies.extend(foreign);
        let mut expired = parse_manual_cookies("bilibili", "SESSDATA=expired").unwrap();
        expired[0].expires = Some(1);
        cookies.extend(expired);
        cookies.extend(parse_manual_cookies("douyin", "SESSDATA=wrong-domain").unwrap());
        let extracted = super::ExtractedProfile {
            profile_id: "test-profile".to_string(),
            profile_name: "Test".to_string(),
            cookies,
            issue_text: String::new(),
            has_decryption_issue: false,
        };
        // 注入内存存储，绝不访问实际认证目录或浏览器。
        let mut stored = std::collections::BTreeMap::from([
            ("douyin".to_string(), "old-douyin".to_string()),
            ("bilibili".to_string(), "valid-bilibili".to_string()),
            ("youtube".to_string(), "old-youtube".to_string()),
        ]);
        let results =
            super::persist_platform_cookies(&request, &platforms, extracted, |import, cookies| {
                assert_ne!(import.platform, "bilibili", "失败平台不得调用持久化");
                assert_eq!(import.profile_id.as_deref(), Some("test-profile"));
                assert!(cookies
                    .iter()
                    .all(|cookie| domain_allowed(&import.platform, &cookie.domain)));
                // 一个写入失败不影响后续平台，失败结果也必须保留实际来源。
                if import.platform == "douyin" {
                    return Err("模拟写入失败".to_string());
                }
                stored.insert(import.platform.clone(), "new-youtube".to_string());
                Ok(super::CookieImportResult {
                    platform: import.platform.clone(),
                    browser_id: import.browser_id.clone(),
                    profile_id: import.profile_id.clone(),
                    status: "active".to_string(),
                    imported_count: cookies.len(),
                    requires_elevation: false,
                    message: String::new(),
                })
            });
        assert_eq!(
            results
                .iter()
                .map(|result| result.status.as_str())
                .collect::<Vec<_>>(),
            ["failed", "failed", "active"]
        );
        assert!(results
            .iter()
            .all(|result| result.profile_id.as_deref() == Some("test-profile")));
        assert_eq!(results[1].imported_count, 0);
        assert_eq!(stored["bilibili"], "valid-bilibili");
        assert_eq!(stored["douyin"], "old-douyin");
        assert_eq!(stored["youtube"], "new-youtube");
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
