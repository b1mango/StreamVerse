# StreamVerse

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="StreamVerse logo" width="128" height="128" />
</p>

<p align="center">
  Desktop video downloader for Douyin, Bilibili, and YouTube
</p>

<p align="center">
  <a href="https://github.com/b1mango/StreamVerse/releases"><img src="https://img.shields.io/badge/release-0.1.5-2ea043" alt="release 0.1.5" /></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-1f6feb" alt="platforms" />
  <img src="https://img.shields.io/badge/license-MIT-f5c542" alt="MIT license" />
</p>

<p align="center">
  <a href="README.md">中文</a> · English
</p>

## Features

**Supported Platforms**

| Platform | Single Video | Profile Batch |
| --- | :---: | :---: |
| Douyin | ✅ | ✅ |
| Bilibili | ✅ | ✅ |
| YouTube | ✅ | — |

**Download Options**

- Video (multiple resolutions, watermark-free, high bitrate)
- Cover image
- Caption / description
- Metadata JSON

**Task Queue**

- Real-time progress, speed, and ETA
- Pause / resume / cancel / retry
- Persistent history with retry across restarts
- One-click reveal in file manager

**Authentication**

- Auto-detect cookies from Chrome / Edge / Firefox
- Manual paste of cookie text or import `cookies.txt`
- Per-platform independent configuration

**Interface**

- Dark / light theme
- Batch list with search, drag-select, per-item quality picker
- Floating scroll buttons for long lists
- Chinese / English i18n

## Screenshots

| Home | Douyin workspace | Bilibili workspace |
| :---: | :---: | :---: |
| ![Home](docs/screenshots/home.png) | ![Douyin workspace](docs/screenshots/douyin-workspace.png) | ![Bilibili workspace](docs/screenshots/bilibili-workspace.png) |

## Installation

Download the latest version from [GitHub Releases](https://github.com/b1mango/StreamVerse/releases):

- **Windows**: `.exe` installer
- **macOS**: `.dmg` package

## Usage

1. Open Settings, choose download folder and concurrency
2. Set up authentication (browser session or cookie import)
3. Paste a video or profile link, click Analyze
4. Pick quality and content options, enqueue the download

## License

[MIT](LICENSE)
