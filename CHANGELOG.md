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
- Real UI screenshots under `docs/screenshots/`
- Maintainer docs for roadmap, contributor guidance, project rules, and context handoff

### Changed

- Refined README for open-source presentation and clearer workflow explanation
- Split the large `App.svelte` into reusable UI components
- Updated default save directory to `~/Movies/StreamVerse`
- Refined download output rules:
  - single selected artifact saves directly
  - multiple selected artifacts save into a title-named folder

### Known Limitations

- Douyin remains the most complete end-to-end platform
- Bilibili currently focuses on single-video download only
- Some Douyin links still require fresh browser cookies for reliable parsing
- Windows code paths exist, but full device-level validation is still in progress
