#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod auth;
mod download_history;
mod formats;
mod media_contract;
mod parser;
mod platforms;
mod provider_runtime;
mod providers;
mod settings;
mod task_store;
mod ytdlp;

pub(crate) use app::{DownloadRequest, DownloadTask};
pub(crate) use media_contract::{DownloadContentSelection, ProfileBatch, VideoAsset, VideoFormat};

fn main() {
    app::run();
}
