use super::engine::{
    build_http_client, infer_extension, silent_command, unique_output_dir, unique_output_path,
};
use crate::{platforms, provider_runtime, DownloadContentSelection};
use reqwest::header::{REFERER, USER_AGENT};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Clone)]
pub(super) struct DownloadArtifacts {
    pub(super) platform: String,
    pub(super) source_url: String,
    pub(super) asset_id: String,
    pub(super) title: String,
    pub(super) author: String,
    pub(super) publish_date: String,
    pub(super) cover_url: Option<String>,
    pub(super) referer: Option<String>,
    pub(super) user_agent: Option<String>,
}

#[derive(Default)]
pub(super) struct ArtifactSummary {
    pub(super) video_written: bool,
    pub(super) audio_written: bool,
    pub(super) text_written: bool,
    pub(super) cover_written: bool,
    pub(super) image_count: usize,
    pub(super) output_path: Option<String>,
    pub(super) destination_path: String,
    pub(super) base_dir: String,
    pub(super) warnings: Vec<String>,
}

#[derive(Clone)]
pub(super) struct OutputLayout {
    pub(super) base_dir: PathBuf,
    pub(super) bundle_dir: Option<PathBuf>,
    pub(super) single_stem: String,
}

impl OutputLayout {
    pub(super) fn asset_root(&self) -> &Path {
        self.bundle_dir.as_deref().unwrap_or(&self.base_dir)
    }

    pub(super) fn destination_path(&self) -> String {
        self.asset_root().to_string_lossy().to_string()
    }

    pub(super) fn bundle_entry_path(&self) -> Option<String> {
        self.bundle_dir
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
    }

    pub(super) fn video_path(&self, extension: &str) -> PathBuf {
        let file_name = if self.bundle_dir.is_some() {
            format!("video.{extension}")
        } else {
            format!("{}.{extension}", self.single_stem)
        };
        unique_output_path(self.asset_root().join(file_name))
    }

    fn caption_path(&self) -> PathBuf {
        if self.bundle_dir.is_some() {
            self.asset_root().join("caption.txt")
        } else {
            unique_output_path(self.asset_root().join(format!("{}.txt", self.single_stem)))
        }
    }

    fn cover_path(&self, extension: &str) -> PathBuf {
        let file_name = if self.bundle_dir.is_some() {
            format!("cover.{extension}")
        } else {
            format!("{} cover.{extension}", self.single_stem)
        };
        unique_output_path(self.asset_root().join(file_name))
    }

    fn audio_path(&self) -> PathBuf {
        let file_name = if self.bundle_dir.is_some() {
            "audio.mp3".to_string()
        } else {
            format!("{}.mp3", self.single_stem)
        };
        unique_output_path(self.asset_root().join(file_name))
    }

    pub(super) fn image_path(&self, index: usize, extension: &str) -> PathBuf {
        self.asset_root()
            .join(format!("image-{index:03}.{extension}"))
    }
}

pub(super) fn prepare_output_layout(
    base_dir: &Path,
    safe_title: &str,
    asset_id: &str,
    download_options: &DownloadContentSelection,
    force_bundle: bool,
) -> Result<OutputLayout, String> {
    if force_bundle || download_options.needs_bundle_directory() {
        let bundle_dir = unique_output_dir(base_dir.join(safe_title));
        fs::create_dir_all(&bundle_dir).map_err(|error| format!("创建作品文件夹失败：{error}"))?;
        return Ok(OutputLayout {
            base_dir: base_dir.to_path_buf(),
            bundle_dir: Some(bundle_dir),
            single_stem: format!("{safe_title} [{asset_id}]"),
        });
    }
    Ok(OutputLayout {
        base_dir: base_dir.to_path_buf(),
        bundle_dir: None,
        single_stem: format!("{safe_title} [{asset_id}]"),
    })
}

pub(super) fn persist_download_artifacts(
    output_layout: &OutputLayout,
    video_path: Option<&Path>,
    artifacts: &DownloadArtifacts,
    format_label: Option<&str>,
    download_options: &DownloadContentSelection,
    ffmpeg_path: Option<&str>,
) -> ArtifactSummary {
    let valid_video_path = video_path.filter(|path| path.is_file());
    let mut summary = ArtifactSummary {
        video_written: valid_video_path.is_some(),
        output_path: if output_layout.bundle_dir.is_some() {
            output_layout.bundle_entry_path()
        } else {
            valid_video_path.map(|path| path.to_string_lossy().to_string())
        },
        destination_path: output_layout.destination_path(),
        base_dir: output_layout.base_dir.to_string_lossy().to_string(),
        ..ArtifactSummary::default()
    };

    if download_options.download_audio {
        if let Some(source_video) = valid_video_path {
            let audio_output = output_layout.audio_path();
            let ffmpeg_binary = ffmpeg_path
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("ffmpeg");
            match silent_command(ffmpeg_binary)
                .arg("-y")
                .arg("-i")
                .arg(source_video)
                .arg("-vn")
                .arg("-acodec")
                .arg("libmp3lame")
                .arg("-q:a")
                .arg("2")
                .arg(&audio_output)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
            {
                Ok(output) if output.status.success() => {
                    summary.audio_written = true;
                    summary
                        .output_path
                        .get_or_insert_with(|| audio_output.to_string_lossy().to_string());
                }
                Ok(output) => summary.warnings.push(format!(
                    "MP3 提取失败：{}",
                    String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .last()
                        .unwrap_or("未知错误")
                )),
                Err(error) => summary
                    .warnings
                    .push(format!("启动 FFmpeg 提取音频失败：{error}")),
            }
        } else {
            summary
                .warnings
                .push("没有可用的视频文件，无法提取音频。".to_string());
        }
    }

    if download_options.download_caption {
        let path = output_layout.caption_path();
        match fs::write(&path, build_text_sidecar(artifacts, format_label)) {
            Ok(()) => {
                summary.text_written = true;
                summary
                    .output_path
                    .get_or_insert_with(|| path.to_string_lossy().to_string());
            }
            Err(error) => summary.warnings.push(format!("文案保存失败：{error}")),
        }
    }

    if download_options.download_cover {
        if let Some(cover_url) = artifacts.cover_url.as_deref() {
            match download_cover_image_to_path(
                &artifacts.platform,
                output_layout,
                cover_url,
                artifacts.referer.as_deref(),
                artifacts.user_agent.as_deref(),
            ) {
                Ok(path) => {
                    summary.cover_written = true;
                    summary
                        .output_path
                        .get_or_insert_with(|| path.to_string_lossy().to_string());
                }
                Err(error) => summary.warnings.push(format!("封面下载失败：{error}")),
            }
        } else {
            summary
                .warnings
                .push("当前作品没有可用封面地址。".to_string());
        }
    }
    summary
}

fn build_text_sidecar(artifacts: &DownloadArtifacts, format_label: Option<&str>) -> String {
    let mut sections = vec![
        format!(
            "平台：{}",
            platforms::human_platform_name(&artifacts.platform)
        ),
        format!("标题：{}", artifacts.title),
        format!("作者：{}", artifacts.author),
        format!("发布日期：{}", artifacts.publish_date),
        format!("资源 ID：{}", artifacts.asset_id),
        format!("来源链接：{}", artifacts.source_url),
    ];
    if let Some(label) = format_label {
        sections.push(format!("下载格式：{label}"));
    }
    if let Some(url) = artifacts.cover_url.as_deref() {
        sections.push(format!("封面链接：{url}"));
    }
    sections.join("\n")
}

fn download_cover_image_to_path(
    platform: &str,
    output_layout: &OutputLayout,
    cover_url: &str,
    referer: Option<&str>,
    user_agent: Option<&str>,
) -> Result<PathBuf, String> {
    let client = build_http_client(platform)?;
    let mut last_error = "没有可用封面地址".to_string();
    for candidate in provider_runtime::thumbnail_candidates(cover_url) {
        let mut request = client.get(&candidate).header(
            USER_AGENT,
            user_agent.unwrap_or(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124.0.0.0 Safari/537.36",
            ),
        );
        if let Some(value) = referer.or_else(|| provider_runtime::thumbnail_referer(&candidate)) {
            request = request.header(REFERER, value);
        }
        let response = match request.send() {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("请求 {candidate} 失败：{error}");
                continue;
            }
        };
        if !response.status().is_success() {
            last_error = format!("请求 {candidate} 返回 {}", response.status());
            continue;
        }
        let extension = infer_extension(&response, &candidate);
        let bytes = match response.bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                last_error = format!("读取 {candidate} 失败：{error}");
                continue;
            }
        };
        let path = output_layout.cover_path(&extension);
        fs::write(&path, bytes).map_err(|error| format!("写入封面文件失败：{error}"))?;
        return Ok(path);
    }
    Err(last_error)
}

pub(super) fn build_completion_message(output_dir: &str, summary: &ArtifactSummary) -> String {
    let mut items = Vec::new();
    if summary.video_written {
        items.push("视频");
    }
    if summary.audio_written {
        items.push("音频");
    }
    if summary.text_written {
        items.push("文案");
    }
    if summary.cover_written {
        items.push("封面");
    }
    if summary.image_count > 0 {
        items.push("图册");
    }
    let label = if items.is_empty() {
        "内容".to_string()
    } else {
        items.join("、")
    };
    let directory = if summary.base_dir.is_empty() {
        output_dir
    } else {
        &summary.base_dir
    };
    let image_detail = if summary.image_count > 0 {
        format!("（{} 张）", summary.image_count)
    } else {
        String::new()
    };
    let mut message = format!("下载完成，{label}{image_detail}已保存到 {directory}。");
    if !summary.warnings.is_empty() {
        message.push(' ');
        message.push_str(&summary.warnings.join("；"));
    }
    message
}
