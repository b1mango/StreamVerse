# StreamVerse

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="StreamVerse logo" width="128" height="128" />
</p>

<p align="center">
  Desktop video downloader for Douyin, Bilibili, and YouTube
</p>

<p align="center">
  <a href="https://github.com/b1mango/StreamVerse/releases"><img src="https://img.shields.io/badge/release-2.0.0-2ea043" alt="release 2.0.0" /></a>
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
| YouTube | ✅ | ✅ (channel / playlist) |

**Download Options**

- Video (multiple resolutions, watermark-free, high bitrate)
- MP3 audio
- Cover image
- Caption / description

**Task Queue**

- Real-time progress, speed, and ETA
- Pause / resume / cancel / retry
- Persistent history with retry across restarts
- One-click reveal in file manager

**Authentication**

- Scoped cookie import from Chrome / Edge / Firefox on Windows and Chrome / Safari on macOS
- Explicit first-use consent showing browser, profile, domain scope, and local storage location
- Manual paste of cookie text or import `cookies.txt`
- Per-platform independent configuration

**Interface**

- Dark / light theme
- Virtualized batch lists above 50 items with checkbox, Shift range, select-all, and invert
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

## Local Development

Node.js 22, Python 3.11 (build time only), and Rust stable are required. Packaged apps do not require system Python, pip, or first-launch downloads.

```powershell
npm ci
python -m pip install -r requirements-douyin-helper.txt pyinstaller==6.22.0
npm run build:helper
npm run prepare:sidecars
npm run check
npm test
npm run tauri:dev
```

`sidecars.lock.json` pins yt-dlp, Deno, FFmpeg, and the Windows aria2 build by version and SHA-256. Tauri packages `yt-dlp`, `deno`, `ffmpeg`, and `streamverse-helper` for each target triple. Windows additionally bundles aria2 to download YouTube HTTPS media over 16 Range connections. Deno is used only for yt-dlp's YouTube JavaScript challenge solving. aria2 is distributed under GPL-2.0-or-later; its license and corresponding complete source archive are included with the Windows application.

## 2.0 Upgrade

Version 2.0 does not migrate 0.1 settings, tasks, or cookies. Reconfigure the download directory and platform authentication after upgrading.

## License

[MIT](LICENSE)
