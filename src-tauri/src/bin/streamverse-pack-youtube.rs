#[path = "../pack_common.rs"]
mod pack_common;

use pack_common::analyze_generic_url;

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
                "youtube",
                &source_url,
                cookie_browser.as_deref(),
                cookie_file.as_deref(),
            )?;
            println!(
                "{}",
                serde_json::to_string(&asset)
                    .map_err(|error| format!("序列化 YouTube 结果失败：{error}"))?
            );
            Ok(())
        }
        _ => Err(format!("不支持的 YouTube pack 动作：{action}")),
    }
}
