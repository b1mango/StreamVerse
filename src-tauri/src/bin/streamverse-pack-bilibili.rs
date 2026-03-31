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

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--url" => url = args.next(),
            "--cookie-browser" => cookie_browser = args.next(),
            "--cookie-file" => cookie_file = args.next(),
            _ => {}
        }
    }

    let source_url = url.ok_or_else(|| "缺少 --url 参数。".to_string())?;

    match action.as_str() {
        "analyze-single" => {
            let asset = analyze_generic_url(
                "bilibili",
                &source_url,
                cookie_browser.as_deref(),
                cookie_file.as_deref(),
            )?;
            println!(
                "{}",
                serde_json::to_string(&asset)
                    .map_err(|error| format!("序列化 Bilibili 结果失败：{error}"))?
            );
            Ok(())
        }
        "analyze-profile" => run_profile_bridge(
            &source_url,
            cookie_browser.as_deref(),
            cookie_file.as_deref(),
        ),
        _ => Err(format!("不支持的 Bilibili pack 动作：{action}")),
    }
}

fn run_profile_bridge(
    source_url: &str,
    cookie_browser: Option<&str>,
    cookie_file: Option<&str>,
) -> Result<(), String> {
    let python_bin = ensure_helper_runtime()?;
    let owned_cookie_file = match (cookie_file, cookie_browser) {
        (Some(path), _) => Some(PathBuf::from(path)),
        (None, Some(browser)) => Some(export_browser_cookies_for_url(
            browser,
            "https://space.bilibili.com/",
        )?),
        (None, None) => None,
    };

    let script = pack_common::resource_root()
        .join("scripts")
        .join("bilibili_profile_bridge.py");
    let mut command = Command::new(&python_bin);
    command.arg(&script).arg("--url").arg(source_url);
    if let Some(cookie_file) = owned_cookie_file.as_ref() {
        command.arg("--cookie-file").arg(cookie_file);
    }

    let output = command
        .output()
        .map_err(|error| format!("启动 Bilibili 主页 pack 失败：{error}"))?;

    if cookie_file.is_none() {
        cleanup_cookie_file(&owned_cookie_file);
    }

    if output.status.success() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
        Ok(())
    } else {
        Err(read_process_error(
            &output.stderr,
            "Bilibili 主页 pack 执行失败。",
        ))
    }
}
