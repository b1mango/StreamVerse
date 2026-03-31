# StreamVerse

`StreamVerse` 是一个桌面端多平台视频下载器，聚焦三件事：解析快、批量流程清晰、下载过程可控。

当前版本已经稳定覆盖：

- `抖音`：单视频下载、主页批量读取与批量下载
- `Bilibili`：单视频下载、UP 主投稿批量下载
- `YouTube`：单视频下载

应用基于 `Tauri 2 + Rust + Svelte 5`，采用“主应用壳 + 平台 pack + 共享运行时”的结构。主应用负责界面、队列、设置和任务调度；平台能力按 pack 安装与更新；`FFmpeg`、浏览器批量读取等共享能力按需补齐。

## 界面预览

![StreamVerse 平台首页](docs/screenshots/home.png)

![StreamVerse 抖音工作区](docs/screenshots/douyin-workspace.png)

![StreamVerse Bilibili 工作区](docs/screenshots/bilibili-workspace.png)

## 核心能力

- 单视频下载：解析链接后选择清晰度、内容类型，再进入任务队列
- 主页批量下载：先读取完整作品列表，再筛选、勾选、批量入队
- 格式去重：界面只保留用户可感知差异的清晰度项
- 单条画质选择：批量列表里的每个视频都可以单独改清晰度
- 任务队列：支持进度、速度、ETA、暂停、继续、取消、重试、定位文件
- 历史持久化：应用重启后保留最近任务状态
- 内置媒体能力：打包版自带 `FFmpeg`，高质量 `DASH` 下载无需额外安装
- 浏览器登录态接入：支持基于浏览器 Cookie 的受限内容解析

## 当前支持的工作流

### 抖音

- 单视频下载
- 主页批量读取
- 批量列表筛选、全选、逐项画质选择

### Bilibili

- 单视频下载
- `UP` 主投稿列表批量读取
- 批量列表筛选、全选、逐项画质选择

### YouTube

- 单视频下载
- 高质量格式下载与音视频合流

## 架构概览

- 主应用：`Tauri` 桌面壳、前端页面、任务队列、设置、模块中心
- 平台 pack：`douyin-pack`、`bilibili-pack`、`youtube-pack`
- 共享 pack：
  - `browser-bridge`：主页批量读取与浏览器会话导出
  - `download-engine`：`yt-dlp`
  - `media-engine`：`FFmpeg`
- 注册表与安装链路：支持本地 build 资源、远程 release bundle、按需安装与更新

## 仓库结构

- `src/`：Svelte 前端
- `src-tauri/`：Rust 后端、下载执行、pack 调度
- `scripts/`：Python 桥接脚本与打包辅助脚本
- `vendor/douyin_api/`：抖音桥接依赖
- `registry/plugins.json`：本地开发与打包时使用的 pack 注册表
- `.github/workflows/`：`macOS / Windows` 桌面构建与 pack 发布工作流

## 下载内容

每个任务都可以按需选择：

- 视频
- 封面
- 文案 `.txt`
- 元数据 `.json`

只选一个内容时直接落盘；选择多个内容时自动创建以标题命名的文件夹。

## 本地开发

### 依赖

- `Node.js 22`
- `Rust`
- `Python 3.11+`
- 可用的 Chromium 浏览器（抖音 / Bilibili 主页批量读取）

打包版会内置当前平台所需的 `FFmpeg`，一般不需要用户额外安装。

### 启动

```bash
npm ci
npm run check
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri:dev
```

### 打包

```bash
npm run tauri:build
```

## 发布

仓库已经接通两条 GitHub Actions 流程：

- `Build Desktop App`：构建 `macOS` 与 `Windows` 桌面包
- `Release Packs`：发布平台 pack 与共享依赖 bundle

推送 `v*` tag 后，会自动生成 GitHub Release，并附带：

- `macOS`：`DMG`
- `Windows`：`NSIS` 安装包

## 当前边界

- `YouTube` 目前只开放单视频下载
- 主页批量下载只面向已发布作品，不包含喜欢、收藏、直播等内容
- 部分抖音链接仍然依赖新鲜浏览器 Cookie
- `Windows` 构建通过 GitHub Actions 产出，仍建议继续做实机回归

## 文档

- [变更记录](CHANGELOG.md)
- [贡献说明](CONTRIBUTING.md)
- [项目路线图](docs/roadmap.md)
- [项目规则](docs/project-rules.md)
- [维护上下文](docs/maintainer-context.md)
