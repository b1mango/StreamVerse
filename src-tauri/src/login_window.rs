//! 应用内登录窗口：在应用私有 WebView 中登录平台，Cookie 存于应用自己的
//! 数据目录——不读取任何浏览器数据，无需 TCC/钥匙串/完全磁盘访问授权。
//! （实测：未签名构建读浏览器数据目录被 macOS 静默拒绝且不弹授权框。）
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rookie::enums::Cookie;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::auth;
use crate::settings;
use crate::app::AppState;

/// Google 禁止内嵌 WebView 登录（disallowed_useragent），YouTube 不支持应用内登录
fn login_url(platform: &str) -> Option<&'static str> {
    match platform {
        "douyin" => Some("https://www.douyin.com/"),
        "bilibili" => Some("https://passport.bilibili.com/login"),
        _ => None,
    }
}

pub fn open(app: &AppHandle, platform: &str) -> Result<(), String> {
    let platform = auth::validate_platform(platform)?;
    let Some(url) = login_url(platform) else {
        return Err("该平台暂不支持应用内登录（Google 限制内嵌网页登录），请改用浏览器导入或 cookies.txt。".to_string());
    };
    let label = format!("login-{platform}");
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }
    let url = url
        .parse()
        .map_err(|error| format!("登录地址无效：{error}"))?;
    WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(format!("登录 {}", auth::platform_label(platform)))
        .inner_size(1100.0, 820.0)
        .build()
        .map_err(|error| format!("创建登录窗口失败：{error}"))?;
    let app_handle = app.clone();
    let platform = platform.to_string();
    thread::spawn(move || poll_login_cookies(app_handle, label, platform));
    Ok(())
}

/// 登录窗口存活期间轮询应用私有 Cookie 库，关键 Cookie 出现即导入并关窗；
/// 最多 10 分钟，避免用户忘关窗口时线程空转。
fn poll_login_cookies(app: AppHandle, label: String, platform: String) {
    for _ in 0..400 {
        thread::sleep(Duration::from_millis(1500));
        let Some(window) = app.get_webview_window(&label) else {
            return;
        };
        let Ok(cookies) = window.cookies() else {
            continue;
        };
        let converted: Vec<Cookie> = cookies.iter().filter_map(convert_cookie).collect();
        if auth::validate_critical_cookies(&platform, &converted).is_err() {
            continue;
        }
        if let Err(error) = complete_import(&app, &platform, converted) {
            eprintln!("应用内登录导入失败：{error}");
            continue;
        }
        let _ = window.close();
        return;
    }
}

fn complete_import(app: &AppHandle, platform: &str, cookies: Vec<Cookie>) -> Result<(), String> {
    auth::save_webview_cookies(platform, cookies)?;
    {
        let state = app.state::<AppState>();
        let mut guard = state.settings.lock().unwrap();
        let entry = guard.platform_auth.entry(platform.to_string()).or_default();
        entry.mode = "webview".to_string();
        entry.browser_id = None;
        entry.profile_id = None;
        entry.consented_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        settings::save_settings(&guard)?;
    }
    let _ = app.emit("login-imported", platform.to_string());
    Ok(())
}

fn convert_cookie(cookie: &tauri::webview::Cookie) -> Option<Cookie> {
    Some(Cookie {
        // cookie crate 按 RFC 6265 抹掉 Domain 前导点；Netscape 格式用点前缀表示
        // 跨子域共享，平台关键 Cookie（SESSDATA/sessionid）都是域级，必须恢复
        domain: cookie.domain().map(|domain| {
            if domain.starts_with('.') {
                domain.to_string()
            } else {
                format!(".{domain}")
            }
        })?,
        path: cookie.path().unwrap_or("/").to_string(),
        secure: cookie.secure().unwrap_or(false),
        expires: cookie
            .expires_datetime()
            .and_then(|time| u64::try_from(time.unix_timestamp()).ok()),
        name: cookie.name().to_string(),
        value: cookie.value().to_string(),
        http_only: cookie.http_only().unwrap_or(false),
        same_site: -1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_url_covers_douyin_and_bilibili_but_not_youtube() {
        assert!(login_url("douyin").unwrap().contains("douyin.com"));
        assert!(login_url("bilibili").unwrap().contains("bilibili.com"));
        // Google 禁止内嵌 WebView 登录，YouTube 必须走浏览器导入或 cookies.txt
        assert!(login_url("youtube").is_none());
    }

    #[test]
    fn convert_cookie_maps_tauri_cookie_fields() {
        let raw = tauri::webview::Cookie::build(("SESSDATA", "abc123"))
            .domain(".bilibili.com")
            .path("/")
            .secure(true)
            .http_only(true)
            .build();
        let cookie = convert_cookie(&raw).unwrap();
        assert_eq!(cookie.name, "SESSDATA");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.domain, ".bilibili.com");
        assert_eq!(cookie.path, "/");
        assert!(cookie.secure);
        assert!(cookie.http_only);
        assert!(cookie.expires.is_none());
        // 转换结果能通过 bilibili 关键 Cookie 校验
        assert!(auth::validate_critical_cookies("bilibili", &[cookie]).is_ok());
    }
}
