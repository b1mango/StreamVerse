use super::controller::current_proxy_for_platform;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub(super) const YOUTUBE_LOW_SPEED_BPS: u64 = 1024 * 1024;

pub(super) fn compute_percent(downloaded_bytes: u64, total_bytes: u64) -> u32 {
    if total_bytes == 0 {
        // Unknown total: show indeterminate-style progress capped at 90%
        if downloaded_bytes == 0 {
            return 0;
        }
        // Asymptotic curve: quickly rises then flattens near 90%
        let mb = downloaded_bytes as f64 / (1024.0 * 1024.0);
        return (90.0 * (1.0 - (-mb / 50.0).exp())).round().clamp(1.0, 90.0) as u32;
    }

    ((downloaded_bytes as f64 / total_bytes as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u32
}

pub(super) fn dash_combined_progress(downloaded_bytes: u64, total_bytes: u64) -> u32 {
    if total_bytes == 0 {
        if downloaded_bytes == 0 {
            return 0;
        }
        let mb = downloaded_bytes as f64 / (1024.0 * 1024.0);
        return (90.0 * (1.0 - (-mb / 100.0).exp()))
            .round()
            .clamp(1.0, 90.0) as u32;
    }

    ((downloaded_bytes as f64 / total_bytes as f64) * 96.0)
        .round()
        .clamp(0.0, 96.0) as u32
}

pub(super) fn dash_phase_progress(downloaded_bytes: u64, total_bytes: u64, start: u32, end: u32) -> u32 {
    if end <= start {
        return start;
    }

    if total_bytes == 0 {
        if downloaded_bytes == 0 {
            return start.max(1);
        }
        // Unknown total: asymptotic progress within [start, end)
        let span = (end - start) as f64;
        let mb = downloaded_bytes as f64 / (1024.0 * 1024.0);
        let offset = (span * (1.0 - (-mb / 50.0).exp()))
            .round()
            .clamp(0.0, span - 1.0) as u32;
        return (start + offset).clamp(start, end - 1);
    }

    let span = (end - start) as f64;
    let offset = ((downloaded_bytes as f64 / total_bytes as f64) * span)
        .round()
        .clamp(0.0, span) as u32;
    (start + offset).clamp(start, end)
}
/// 3-second sliding window speed tracker for direct downloads.
pub(super) struct SpeedTracker {
    samples: VecDeque<(Instant, u64)>,
    window: Duration,
}

impl SpeedTracker {
    pub(super) fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            window: Duration::from_secs(3),
        }
    }

    pub(super) fn record(&mut self, now: Instant, bytes: u64) {
        self.samples.push_back((now, bytes));
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        while self.samples.front().is_some_and(|(t, _)| *t < cutoff) {
            self.samples.pop_front();
        }
    }

    fn speed_bps(&self) -> Option<f64> {
        if self.samples.len() < 2 {
            return None;
        }
        let (first_t, first_b) = self.samples.front().unwrap();
        let (last_t, last_b) = self.samples.back().unwrap();
        let dt = last_t.duration_since(*first_t).as_secs_f64();
        if dt <= 0.0 {
            return None;
        }
        Some((*last_b - *first_b) as f64 / dt)
    }

    pub(super) fn human_speed_text(&self) -> String {
        match self.speed_bps() {
            Some(bps) if bps > 0.0 => human_bytes(bps, "/s"),
            _ => "-".to_string(),
        }
    }

    pub(super) fn human_eta_text(&self, downloaded_bytes: u64, total_bytes: u64) -> String {
        if total_bytes == 0 || downloaded_bytes >= total_bytes {
            return "—".to_string();
        }
        match self.speed_bps() {
            Some(bps) if bps > 0.0 => {
                let remaining = ((total_bytes - downloaded_bytes) as f64 / bps).round() as u64;
                let minutes = remaining / 60;
                let seconds = remaining % 60;
                format!("{minutes:02}:{seconds:02}")
            }
            _ => "—".to_string(),
        }
    }
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

fn parse_transfer_speed_bps(speed_text: &str) -> Option<u64> {
    let normalized = speed_text.trim().replace(' ', "").to_ascii_lowercase();
    let value = normalized.strip_suffix("/s")?;
    let (number, multiplier) = [
        ("gib", 1024_u64 * 1024 * 1024),
        ("mib", 1024_u64 * 1024),
        ("kib", 1024_u64),
        ("gb", 1_000_000_000_u64),
        ("mb", 1_000_000_u64),
        ("kb", 1_000_u64),
        ("b", 1_u64),
    ]
    .into_iter()
    .find_map(|(suffix, multiplier)| {
        value
            .strip_suffix(suffix)
            .map(|number| (number, multiplier))
    })?;
    let number = number.parse::<f64>().ok()?;
    (number.is_finite() && number >= 0.0).then(|| (number * multiplier as f64).round() as u64)
}

pub(super) fn progress_task_message(platform: &str, speed_text: &str) -> String {
    if platform == "youtube"
        && parse_transfer_speed_bps(speed_text)
            .is_some_and(|speed| speed > 0 && speed < YOUTUBE_LOW_SPEED_BPS)
    {
        return youtube_low_speed_message(current_proxy_for_platform(platform).is_some());
    }
    "正在下载…".to_string()
}

fn youtube_low_speed_message(proxy_configured: bool) -> String {
    if proxy_configured {
        "YouTube 并发下载已生效，但当前速度低于 1 MiB/s。若持续如此，请在代理客户端中切换到更快节点，再重新解析并下载。"
            .to_string()
    } else {
        "YouTube 下载速度明显偏低，请在设置中填写可用代理地址和端口后重新解析并下载。".to_string()
    }
}
pub(super) struct ProgressLine {
    pub(super) percent: u32,
    pub(super) speed_text: String,
    pub(super) eta_text: String,
}

pub(super) fn parse_progress_line(line: &str) -> Option<ProgressLine> {
    let cleaned = strip_ansi_sequences(line);
    let line = cleaned.trim();
    let Some(payload) = line
        .strip_prefix("download:progress:")
        .or_else(|| line.strip_prefix("progress:"))
    else {
        return parse_standard_ytdlp_progress_line(line)
            .or_else(|| parse_aria2_progress_line(line));
    };
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

fn parse_aria2_progress_line(line: &str) -> Option<ProgressLine> {
    let marker = line.find("[#")?;
    let payload = line[marker..].split(']').next()?;
    let progress = payload.split_whitespace().nth(1)?;
    let percent = progress
        .split_once('(')
        .and_then(|(_, tail)| tail.strip_suffix("%)"))?
        .parse::<u32>()
        .ok()?;
    let speed_text = payload
        .split(" DL:")
        .nth(1)?
        .split_whitespace()
        .next()?
        .to_string();
    let eta_text = payload
        .split(" ETA:")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .unwrap_or("—")
        .to_string();
    Some(ProgressLine {
        percent: percent.min(100),
        speed_text: format!("{speed_text}/s"),
        eta_text,
    })
}

fn strip_ansi_sequences(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }
        if ch != '\r' {
            output.push(ch);
        }
    }

    output
}

fn parse_standard_ytdlp_progress_line(line: &str) -> Option<ProgressLine> {
    if !line.contains("[download]") {
        return None;
    }

    let percent_end = line.find('%')?;
    let percent_start = line[..percent_end]
        .rfind(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .map(|index| index + 1)
        .unwrap_or(0);
    let percent_text = line[percent_start..percent_end].trim();
    let percent = percent_text.parse::<f32>().ok()?.round() as u32;

    let speed_text = line
        .split(" at ")
        .nth(1)
        .map(|tail| tail.split(" ETA ").next().unwrap_or(tail).trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("-");
    let eta_text = line
        .split(" ETA ")
        .nth(1)
        .map(|tail| tail.split_whitespace().next().unwrap_or(tail).trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("—");

    Some(ProgressLine {
        percent: percent.clamp(0, 100),
        speed_text: speed_text.to_string(),
        eta_text: eta_text.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        compute_percent, parse_progress_line, parse_transfer_speed_bps, progress_task_message,
        youtube_low_speed_message,
    };
    use super::super::errors::normalize_download_failure;

    #[test]
    fn computes_percent_safely() {
        assert_eq!(compute_percent(50, 100), 50);
        assert_eq!(compute_percent(0, 0), 0);
        assert_eq!(compute_percent(101, 100), 100);
    }

    #[test]
    fn parses_ytdlp_transfer_speeds_and_warns_for_slow_youtube_downloads() {
        assert_eq!(parse_transfer_speed_bps("77.07KiB/s"), Some(78_920));
        assert_eq!(parse_transfer_speed_bps("3.25 MiB/s"), Some(3_407_872));
        assert!(progress_task_message("youtube", "77.07KiB/s").contains("代理"));
        assert!(youtube_low_speed_message(true).contains("代理客户端"));
        assert!(youtube_low_speed_message(true).contains("并发下载已生效"));
        assert!(youtube_low_speed_message(false).contains("设置中填写"));
        assert_eq!(progress_task_message("youtube", "3.25MiB/s"), "正在下载…");
        assert_eq!(progress_task_message("douyin", "77.07KiB/s"), "正在下载…");
        assert!(normalize_download_failure(
            "youtube",
            "ERROR: HTTP Error 403: Forbidden".to_string()
        )
        .contains("切换代理"));
    }

    #[test]
    fn parses_aria2_multi_connection_progress() {
        let progress =
            parse_progress_line("[#901af4 73MiB/738MiB(9%) CN:16 DL:11MiB ETA:59s]").unwrap();
        assert_eq!(progress.percent, 9);
        assert_eq!(progress.speed_text, "11MiB/s");
        assert_eq!(progress.eta_text, "59s");
    }

    #[test]
    fn parses_download_prefixed_progress_lines() {
        let line = parse_progress_line("download:progress:42.5%|3.1MiB/s|00:19").unwrap();
        assert_eq!(line.percent, 43);
        assert_eq!(line.speed_text, "3.1MiB/s");
        assert_eq!(line.eta_text, "00:19");
    }

    #[test]
    fn parses_standard_ytdlp_progress_lines() {
        let line =
            parse_progress_line("[download]  42.5% of   45.00MiB at  3.1MiB/s ETA 00:19").unwrap();
        assert_eq!(line.percent, 43);
        assert_eq!(line.speed_text, "3.1MiB/s");
        assert_eq!(line.eta_text, "00:19");
    }

    #[test]
    fn parses_ansi_colored_progress_lines() {
        let line =
            parse_progress_line("\u{1b}[0;32mdownload:progress:7.4%|280.5KiB/s|01:12\u{1b}[0m")
                .unwrap();
        assert_eq!(line.percent, 7);
        assert_eq!(line.speed_text, "280.5KiB/s");
        assert_eq!(line.eta_text, "01:12");
    }
}
