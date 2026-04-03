# Changelog

All notable changes to `StreamVerse` will be documented in this file.

## [0.1.4] — 2026-04-04

### 新增
- **全局悬浮滚动按钮**：页面右下角固定悬浮「回到顶部 / 跳到底部」按钮，批量列表 700+ 条目时快速定位，毛玻璃风格匹配整体 UI
- **下载限速全路径覆盖**：速度限制现在对直链下载和 DASH 合流下载均生效（此前仅对 yt-dlp 子进程生效）

### 修复
- **深色模式下拉菜单白底白字**：修复 `<select>` 组件的 `<option>` 在深色模式下背景为白色导致文字不可见
- **4K 视频进度卡在 1%**：当 CDN 不返回 Content-Length 时，进度条改用渐近曲线 $1 - e^{-x/50}$ 代替固定 0/1
- **已完成任务缺少「定位文件」按钮**：yt-dlp 回退路径的 ArtifactSummary 现在正确设置 `output_path`
- **全局滚动按钮失效与卡顿**：统一改为滚动实际页面容器，修复“跳到底部”误回顶部，并降低长队列滚动时的抖动
- **浅色模式多处组件样式缺失**：补全 `.text-button.danger`、`.chip.accent`、`.settings-guide-card`、`.cookie-method-summary`、`.cookie-method-details` 的浅色样式
- **CSS 变量引用错误**：`.cookie-method-summary:hover` 引用了未定义的 `var(--foreground)`，修正为 `var(--text)`

### 变更
- 滚动按钮从任务队列面板级别移至全局页面级别
- 移除任务列表的 `max-height` 限制，恢复自然流动布局

---

## [0.1.3] — 2026-04-02

### 新增
- **分平台认证管理**：抖音 / B站 / YouTube 各自独立配置浏览器来源和 cookies.txt 文件，互不污染
- **Cookie 预检**：导入 cookies.txt 时自动检查是否包含当前平台关键登录 Cookie（抖音: `sessionid` / `sessionid_ss`；B站: `SESSDATA`），缺失时提前提示
- **Windows Chrome 引导**：在 Windows 上仅选 Chrome 浏览器来源、未提供 cookies.txt 时，提前提示用户导出 Cookie 而非等到下游报错
- **Windows 原生文件定位**：「定位文件」改用 Shell API（`SHOpenFolderAndSelectItems` / `ShellExecuteW`），不再依赖 `explorer.exe` 退出码判断
- **YouTube 多线程分片下载**：增加 `--concurrent-fragments 4`，提升 DASH 流下载吞吐量
- **抖音临时目录自动清理**：手动浏览器读取流程产生的 `manual-{browser}-{port}` 临时用户配置目录，在启动和完成时自动回收超过 6 小时的陈旧目录

### 修复
- **B站 / 抖音下载极慢**：`reqwest::Client` 和 yt-dlp 子进程默认继承系统代理，导致国内平台 CDN 流量绕境；现在非 YouTube 平台显式禁用代理（`.no_proxy()` + `--proxy ""`）
- **批量解析弹窗「秒关后卡顿」**：进度弹窗在后端返回后立即关闭，但 200+ 条数据的 Svelte 响应式渲染阻塞主线程；现在数据赋值在弹窗仍然可见时完成，弹窗等渲染结束后才关闭
- **缩略图并发风暴**：批量解析 200+ 条结果时一次性发出所有缩略图请求导致 UI 卡顿；改为每批 6 个串行执行
- **「定位文件」误报失败**：Windows 上 `explorer.exe` 已成功打开目录但返回非零退出码，UI 仍显示「文件管理器没有成功打开目标路径」
- **`build.rs` 编译警告**：移除未使用的 `std::io::Read` 导入和不可达代码分支
- **`ProfileBatchWorkspace` 编译警告**：移除未使用的 `pasteLabel` / `pasteLoadingLabel` 导出，将直接 `onmousedown` 替换为 DOM action

### 变更
- 代理策略收敛：仅 YouTube 使用代理；抖音 / B站 无论设置面板是否填写代理均走国内直连
- 设置面板重构为分平台认证卡片，每个平台独立显示浏览器来源、Cookie 文件和认证状态
- 设置存储从全局 `cookie_browser` / `cookie_file` 迁移为 `platform_auth` 映射，兼容旧版配置自动迁移
- 每平台导入的 Cookie 文本保存为独立文件（`saved-{platform}-cookies.txt`）

---

## [0.1.2] — 2026-04-01

### 新增
- 解析进度模态弹窗：批量解析时显示实时进度条，完成后播放打勾动画（1.2 秒后自动关闭）
- 新增 `download_history.rs` 下载历史模块
- 新增 `i18n.ts` 前端国际化模块
- 透明标题栏（`titleBarStyle: Overlay` + `hiddenTitle`），窗口顶部拖动区域

### 修复
- **UI 阻塞**：`analyze_profile_input` / `collect_profile_browser` 从 `std::thread::spawn` + `join` 改为 `tauri::async_runtime::spawn_blocking`，彻底解决批量解析和模块切换卡死问题
- **B站"访问权限不足"**：主页批量解析改为先请求 nav 获取 WBI 密钥，再对 profile info 请求做 WBI 签名
- **抖音 JSON 解析失败**：vendor 脚本 `print()` 污染 stdout，改为从 stdout 提取最后一行 JSON
- **进度条不满**：CSS transition + 组件提前卸载导致进度条永远到不了 100%，增加 320ms 延迟
- **设置保存冻结**：`save_settings` 改为异步
- **格式标签不响应**：Svelte 5 模板函数调用对闭包变量不自动响应，修复响应式
- **B站缩略图跨域**：通过 Rust `fetch_thumbnail` 命令代理请求并返回 base64
- **浅色模式样式**：修复 light 模式下多处 CSS 异常

### 变更
- B站并发数从 10 提升到 20（`FETCH_CONCURRENCY = 20`）
- 解析进度从内嵌面板移至全屏模态弹窗
- 默认主题 `dark`、默认语言 `zh-CN`（前后端统一）
- 移除输入框旁的粘贴按钮

---

## [0.1.1] — 2026-03-31

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
