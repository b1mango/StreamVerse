# StreamVerse 维护上下文

## 当前基线

- 产品版本：`1.0.0`
- 桌面壳：Tauri 2
- 前端：Svelte 5 + TypeScript
- 后端：Rust
- 构建时 helper：Python 3.11 + PyInstaller；用户机器不需要 Python 或 pip
- 支持范围：抖音、Bilibili、YouTube 单视频；抖音和 Bilibili 主页批量；YouTube 频道与合集批量

1.0 是首个稳定版本。设置、任务、历史与认证使用当前稳定数据结构。

## 运行架构

三个平台 provider 内置在主程序中。项目没有动态 pack、远程 registry、模块安装器或首次启动依赖下载。

发布包包含四个 Tauri sidecar：

- `yt-dlp`：Bilibili / YouTube 解析与通用下载回退
- `deno`：yt-dlp 的 YouTube JavaScript 挑战求解
- `ffmpeg`：DASH 合流与音频提取
- `streamverse-helper`：抖音、Bilibili 主页和浏览器批量桥接

`sidecars.lock.json` 固定版本和 SHA-256。Deno 同时校验下载压缩包与解压后的可执行文件；`scripts/prepare-sidecars.mjs` 按目标 triple 校验并复制到 `src-tauri/binaries/`。

## 关键模块

- 前端壳层与工作流：[src/App.svelte](../src/App.svelte)
- 设置与 Cookie 授权：[src/lib/components/SettingsSheet.svelte](../src/lib/components/SettingsSheet.svelte)
- 虚拟化批量列表：[src/lib/components/BatchList.svelte](../src/lib/components/BatchList.svelte)
- 任务事件同步：[src/lib/backend.ts](../src/lib/backend.ts)
- Tauri 启动入口：[src-tauri/src/main.rs](../src-tauri/src/main.rs)
- IPC 与应用状态：[src-tauri/src/app.rs](../src-tauri/src/app.rs)
- 浏览器 Cookie：[src-tauri/src/auth.rs](../src-tauri/src/auth.rs)
- 内置 provider：[src-tauri/src/providers.rs](../src-tauri/src/providers.rs)
- sidecar 执行：[src-tauri/src/provider_runtime.rs](../src-tauri/src/provider_runtime.rs)
- 下载 facade：[src-tauri/src/ytdlp/mod.rs](../src-tauri/src/ytdlp/mod.rs)
- 任务控制：[src-tauri/src/ytdlp/controller.rs](../src-tauri/src/ytdlp/controller.rs)
- 传输与进程执行：[src-tauri/src/ytdlp/engine.rs](../src-tauri/src/ytdlp/engine.rs)
- 产物保存：[src-tauri/src/ytdlp/artifact.rs](../src-tauri/src/ytdlp/artifact.rs)

## Cookie 安全边界

- 首次浏览器读取前显示浏览器、Profile、平台域名和保存位置。
- `rookie-cookies` 只查询当前平台域名白名单。
- Chromium App-Bound Encryption 错误只允许二次确认后启动一次独立 UAC helper，主应用不提权。
- 不关闭用户浏览器，不调用 `cookies-from-browser`，不导出全站 Cookie。
- 手动 Header 和 `cookies.txt` 都由后端解析、过滤并原子写入应用认证目录。
- 前端不接收 Cookie 值或内部认证文件路径；日志不得记录 Cookie 值。

## 构建门禁

```powershell
npm ci
python -m pip install -r requirements-douyin-helper.txt pyinstaller==6.22.0
npm run build:helper
npm run prepare:sidecars
npm run check
npm test
npm run test:helper
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

GitHub Actions 在 Windows 生成 NSIS，在 macOS 生成 app/DMG。流水线只上传构建产物，不创建 Release。

## 待实机验证

- Windows Chrome / Edge / Firefox 的真实登录受限链接解析与下载
- Chromium App-Bound Encryption 的 UAC 取消、成功和失败路径
- macOS Chrome / Safari Cookie 读取与 DMG 启动 smoke test
- 三个平台对真实链接的长期稳定性与平台规则变动
