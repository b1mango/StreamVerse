#[path = "../pack_common.rs"]
mod pack_common;

use pack_common::{
    analyze_generic_url, cleanup_cookie_file, ensure_helper_runtime,
    export_browser_cookies_for_url, read_process_error,
};
use std::path::PathBuf;
use std::process::Command;

fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let action = args.next().ok_or_else(|| "缺少动作参数。".to_string())?;

    let mut url = None::<String>;
    let mut cookie_browser = None::<String>;
    let mut cookie_file = None::<String>;
    let mut port = None::<String>;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--url" => url = args.next(),
            "--cookie-browser" => cookie_browser = args.next(),
            "--cookie-file" => cookie_file = args.next(),
            "--port" => port = args.next(),
            _ => {}
        }
    }

    let source_url = url.ok_or_else(|| "缺少 --url 参数。".to_string())?;

    match action.as_str() {
        "analyze-single" | "analyze-profile" => run_analyze_action(
            &action,
            &source_url,
            cookie_browser.as_deref(),
            cookie_file.as_deref(),
        ),
        "open-profile-browser" | "collect-profile-browser" => run_profile_browser_action(
            &action,
            &source_url,
            cookie_browser.as_deref(),
            cookie_file.as_deref(),
            port.as_deref(),
        ),
        _ => Err(format!("不支持的 Douyin pack 动作：{action}")),
    }
}

fn helper_script_path() -> PathBuf {
    pack_common::resource_root()
        .join("scripts")
        .join("douyin_bridge.py")
}

fn profile_scan_script_path() -> PathBuf {
    pack_common::browser_bridge_resource_root()
        .join("scripts")
        .join("profile_browser_scan.py")
}

fn run_analyze_action(
    action: &str,
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
) -> Result<(), String> {
    let python_bin = ensure_helper_runtime()?;
    let owned_cookie_file = match (cookie_file, cookie_browser) {
        (Some(path), _) => Some(PathBuf::from(path)),
        (None, Some(browser)) => Some(export_browser_cookies_for_url(
            browser,
            "https://www.douyin.com/",
        )?),
        (None, None) => None,
    };

    let mut command = Command::new(&python_bin);
    command.arg(helper_script_path());

    match action {
        "analyze-single" => {
            command.arg("analyze");
        }
        "analyze-profile" => {
            command.arg("profile").arg("--limit").arg("2000");
        }
        _ => return Err(format!("不支持的 Douyin pack 动作：{action}")),
    }

    command.arg("--url").arg(source_url);
    if let Some(cookie_file) = owned_cookie_file.as_ref() {
        command.arg("--cookie-file").arg(cookie_file);
    }

    let output = command
        .output()
        .map_err(|error| format!("启动 Douyin pack 失败：{error}"))?;

    let result = if output.status.success() {
        let raw_stdout = String::from_utf8_lossy(&output.stdout);
        let json_line = raw_stdout
            .lines()
            .rev()
            .find(|line| line.starts_with('{'))
            .unwrap_or(&raw_stdout);
        print!("{json_line}");
        Ok(())
    } else if action == "analyze-single" {
        let fallback = analyze_generic_url(
            "douyin",
            source_url,
            None,
            owned_cookie_file.as_ref().and_then(|path| path.to_str()),
        );
        match fallback {
            Ok(asset) => {
                println!(
                    "{}",
                    serde_json::to_string(&asset)
                        .map_err(|error| format!("序列化 Douyin 兜底结果失败：{error}"))?
                );
                Ok(())
            }
            Err(_) => Err(read_process_error(&output.stderr, "Douyin pack 执行失败。")),
        }
    } else {
        Err(read_process_error(&output.stderr, "Douyin pack 执行失败。"))
    };

    if cookie_file.is_none() {
        cleanup_cookie_file(&owned_cookie_file);
    }

    result
}

fn run_profile_browser_action(
    action: &str,
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
    port: Option<&str>,
) -> Result<(), String> {
    let python_bin = ensure_helper_runtime()?;
    let owned_cookie_file = match (cookie_file, cookie_browser) {
        (Some(path), _) => Some(PathBuf::from(path)),
        (None, Some(browser)) if action == "collect-profile-browser" => Some(
            export_browser_cookies_for_url(browser, "https://www.douyin.com/")?,
        ),
        _ => None,
    };

    let mut command = Command::new(&python_bin);
    command
        .arg(profile_scan_script_path())
        .arg("--platform")
        .arg("douyin")
        .arg("--url")
        .arg(source_url);

    if let Some(browser) = cookie_browser.filter(|value| !value.trim().is_empty()) {
        command.arg("--browser").arg(browser);
    }
    command.env("STREAMVERSE_DOUYIN_BRIDGE_PATH", helper_script_path());
    if let Some(cookie_file) = owned_cookie_file.as_ref() {
        command.arg("--cookie-file").arg(cookie_file);
    }

    match action {
        "open-profile-browser" => {
            command.arg("--launch-manual-browser");
        }
        "collect-profile-browser" => {
            let port = port.ok_or_else(|| "缺少 --port 参数。".to_string())?;
            command.arg("--connect-port").arg(port);
        }
        _ => return Err(format!("不支持的 Douyin 浏览器动作：{action}")),
    }

    let output = command
        .output()
        .map_err(|error| format!("启动 Douyin 浏览器 pack 失败：{error}"))?;

    if cookie_file.is_none() {
        cleanup_cookie_file(&owned_cookie_file);
    }

    if output.status.success() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
        Ok(())
    } else {
        Err(read_process_error(
            &output.stderr,
            "Douyin 浏览器 pack 执行失败。",
        ))
    }
}
