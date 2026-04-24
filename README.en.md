# StreamVerse

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="StreamVerse logo" width="128" height="128" />
</p>

<p align="center">
  A desktop video parsing and download workspace for Douyin, Bilibili, and YouTube, built with Tauri 2, Rust, and Svelte 5.
</p>

<p align="center">
  <a href="https://github.com/b1mango/StreamVerse/releases"><img src="https://img.shields.io/badge/release-0.1.5-2ea043" alt="release 0.1.5" /></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-1f6feb" alt="platforms" />
  <img src="https://img.shields.io/badge/license-MIT-f5c542" alt="MIT license" />
  <img src="https://img.shields.io/badge/Tauri-2.x-8b5cf6" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/Svelte-5-ff3e00" alt="Svelte 5" />
</p>

<p align="center">
  <a href="README.md">中文</a> · English
</p>

## Overview

StreamVerse provides a unified desktop workflow for parsing links, selecting formats, and managing downloads across Douyin, Bilibili, and YouTube. The project focuses on desktop usability, queue management, and network/auth isolation that works well in mainland China network environments.

Current scope:

- Douyin single-video downloads and profile batch downloads
- Bilibili single-video downloads and creator upload batch downloads
- YouTube single-video downloads
- Multiple output artifacts, including video, cover, caption text, and metadata
- Persistent task history with retryable failed or interrupted tasks

## Features

### Download workflow

- Separate single-item and batch workspaces for each platform
- Batch preview, search, selection, and per-item quality choice before enqueue
- Selective outputs for video, cover, caption text, and metadata
- High-quality muxed downloads with bundled `FFmpeg` in the full build
- Concurrent fragment downloads for YouTube DASH streams
- Unified speed limiting for both direct downloads and `yt-dlp`-driven downloads

### Auth and network strategy

- Per-platform auth configuration for Douyin, Bilibili, and YouTube
- Cookie preflight checks when importing `cookies.txt`
- Proxy isolation: proxy is used for YouTube only; Douyin and Bilibili stay direct by default
- Clear Windows Chrome guidance when manual cookie export is required

### Desktop UX

- Native window controls on Windows and native window styling on macOS
- Real-time progress, speed, and ETA in the task queue
- One-click reveal in file manager for completed tasks
- Persistent history across restarts with retry support
- Centralized settings for save path, concurrency, proxy, speed limit, notifications, and auth

### Runtime and architecture

- Svelte 5 frontend with Rust-based task orchestration and local file operations
- `yt-dlp` as the main download engine, with `FFmpeg` for muxing and post-processing
- Pack-based runtime organization for platform-specific capabilities
- Python helper scripts are still used for current browser-assisted batch collection flows, but the roadmap is to move more of this logic into Rust and keep scripts as fallback only

## Platform support

| Platform | Single video | Batch profile | Auth |
| --- | :---: | :---: | --- |
| Douyin | Supported | Supported | Browser session / `cookies.txt` |
| Bilibili | Supported | Supported | Browser session / `cookies.txt` |
| YouTube | Supported | Not yet | Browser session / `cookies.txt` / proxy |

## Screenshots

| Home | Douyin workspace | Bilibili workspace |
| :---: | :---: | :---: |
| ![Home](docs/screenshots/home.png) | ![Douyin workspace](docs/screenshots/douyin-workspace.png) | ![Bilibili workspace](docs/screenshots/bilibili-workspace.png) |

## Installation

Prebuilt packages are available from [GitHub Releases](https://github.com/b1mango/StreamVerse/releases).

- Windows: NSIS installer (`.exe`)
- macOS: DMG

## Usage

1. Configure the default download path, concurrency, speed limit, and proxy strategy in settings.
2. Set up auth per platform. Fresh cookies are recommended for Douyin and Bilibili; a proxy is recommended for YouTube when needed.
3. Open the target workspace, paste a link, and analyze it.
4. Choose the desired format and output artifacts, then enqueue the task.

## Development

### Requirements

- Node.js 22+
- Rust stable
- Python 3.10+ for development and the current browser-bridge batch helpers
- Windows or macOS
- A Chromium-based browser

### Local development

```bash
npm ci
npm run tauri:dev
```

### Checks

```bash
npm run check
cargo test --manifest-path src-tauri/Cargo.toml
```

### Build packages

```bash
npm run tauri:build
```

Notes:

- `npm run tauri:build` produces the full package with bundled `FFmpeg`

## Project layout

```text
src/                    Svelte frontend
src-tauri/              Rust backend, task orchestration, Tauri configuration
scripts/                Python helper scripts and browser bridge logic
vendor/douyin_api/      Douyin-related helper dependency
registry/plugins.json   Pack registry
docs/                   Screenshots and project documentation
```

## Current limitations

- YouTube currently supports single-video workflows only
- Some Douyin and Bilibili links still require valid cookies
- On newer Windows Chrome builds with App-Bound Encryption, automatic cookie extraction may fail and manual `cookies.txt` export may be required
- Batch collection is focused on published works, not likes, favorites, live content, or other content types

## Docs

- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
- [Roadmap](docs/roadmap.md)
- [Project rules](docs/project-rules.md)
- [Maintainer context](docs/maintainer-context.md)

## Contributing

Issues and pull requests are welcome. Before submitting changes, run at least:

```bash
npm run check
cargo test --manifest-path src-tauri/Cargo.toml
```

## License

[MIT](LICENSE)
