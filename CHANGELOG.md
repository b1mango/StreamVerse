# Changelog

All notable changes to `StreamVerse` will be documented in this file.

## [Unreleased]

### Added

- Platform home screen for choosing `抖音` / `Bilibili` / `YouTube`
- New workspace structure with separate platform pages and platform-specific modes
- Separate workspaces for `单视频下载` and `主页批量下载`
- Optional download artifacts: video, cover, caption text, metadata JSON
- Profile batch preview flow with selectable items before enqueue
- Queue controls for pause, resume, cancel, and reveal in Finder / Explorer
- Save-directory settings, browser-cookie source settings, and download strategy settings
- Format deduplication to collapse visually identical quality entries
- Bilibili single-video download beta with FFmpeg-aware format handling
- Bilibili `UP 主批量下载` with preview-first selection flow
- Per-item quality selection for batch downloads
- YouTube single-video download
- Bundled `FFmpeg` for packaged `macOS / Windows` builds, with automatic `yt-dlp --ffmpeg-location` wiring
- Task history persistence across app restarts
- One-click retry for failed and cancelled tasks
- GitHub Actions workflow for `macOS + Windows` desktop builds
- Real UI screenshots under `docs/screenshots/`
- Maintainer docs for roadmap, contributor guidance, project rules, and context handoff
- Browser-window profile scanning for `抖音 / Bilibili` batch pages
- Searchable batch lists with no fixed 24 / 100 item ceiling
- Module-center runtime with on-demand pack install / uninstall / update
- Shared dependency packs:
  - `browser-bridge` for profile batch flows and Python helper runtime
  - `media-engine` for on-demand `FFmpeg` installation and DASH merging
- Pack registry, installed manifest tracking, and release bundle generation workflow

### Changed

- Refined README for open-source presentation and clearer workflow explanation
- Split the large `App.svelte` into reusable UI components
- Updated default save directory to `~/Movies/StreamVerse`
- Interrupted in-progress tasks are now restored as retryable failed tasks on next launch
- Batch pages now allow per-item format overrides before enqueue
- Batch pages now open a browser reader window, collect lightweight lists first, and resolve selected items later
- Refined download output rules:
  - single selected artifact saves directly
  - multiple selected artifacts save into a title-named folder
- Shared pack release workflow no longer duplicates `browser-bridge` resources inside platform bundles

### Known Limitations

- Douyin remains the most complete end-to-end platform
- Bilibili batch pages are more sensitive to cookie freshness and 412 responses
- YouTube currently focuses on single-video download only
- Some Douyin links still require fresh browser cookies for reliable parsing
- Windows code paths exist, but full device-level validation is still in progress
