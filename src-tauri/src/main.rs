#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod auth;
mod browser_access;
mod download_history;
mod formats;
mod login_window;
mod media_contract;
mod parser;
mod persistence;
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
