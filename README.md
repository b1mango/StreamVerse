# StreamVerse

`StreamVerse` 是一个面向桌面的多平台视频下载工具，专注于更快的解析体验、更清晰的批量流程，以及更可控的下载队列。

项目基于 `Tauri 2 + Rust + Svelte 5`，当前已经完成 `抖音`、`Bilibili`、`YouTube` 三个平台的核心下载能力，并通过模块化 pack 架构把平台能力、浏览器桥接和媒体运行时拆开管理。

## 当前支持的模块

### 抖音

- 单视频下载
- 主页视频批量读取与下载

### Bilibili

- 单视频下载
- 主页视频批量读取与下载

### YouTube

- 单视频下载

## 项目特点

- 单视频下载与主页视频批量读取与下载分离，操作路径更直接
- 批量列表支持筛选、勾选、逐项选择清晰度
- 任务队列支持进度、速度、ETA、暂停、继续、取消、重试、定位文件
- 格式列表已做去重，优先保留用户真正可感知的清晰度差异
- 打包版内置 `FFmpeg`，高质量音视频合流无需额外安装
- 浏览器登录态可接入受限内容解析与主页视频提取流程
- 平台能力通过 pack 分发，便于更新、替换和扩展

## 界面预览

### 主页展示

![StreamVerse 主页展示](docs/screenshots/home.png)

### 抖音工作区

![StreamVerse 抖音工作区](docs/screenshots/douyin-workspace.png)

### Bilibili 工作区

![StreamVerse Bilibili 工作区](docs/screenshots/bilibili-workspace.png)

## 架构设计

- 主应用：负责界面、设置、任务队列、模块中心和调度
- 平台 pack：`douyin-pack`、`bilibili-pack`、`youtube-pack`
- 共享能力：
  - `browser-bridge`：主页视频提取与浏览器会话导出
  - `download-engine`：`yt-dlp`
  - `media-engine`：`FFmpeg`
- 注册表与安装链路：支持本地资源、远程 release bundle、按需安装与更新

## 仓库结构

- `src/`：Svelte 前端界面
- `src-tauri/`：Rust 后端、任务执行、pack 调度
- `scripts/`：Python 桥接脚本与打包辅助脚本
- `vendor/douyin_api/`：抖音桥接依赖
- `registry/plugins.json`：本地开发与打包时使用的 pack 注册表
- `.github/workflows/`：桌面构建与 pack 发布工作流

## 下载内容

每个任务都可以按需选择：

- 视频
- 封面
- 文案 `.txt`
- 元数据 `.json`

选择多个内容时会自动创建以标题命名的文件夹，便于整理归档。

## 发布形态

仓库当前通过 GitHub Actions 产出桌面安装包与 pack 资源：

- `Build Desktop App`：构建 `macOS` 与 `Windows` 桌面包
- `Release Packs`：发布平台 pack 与共享依赖 bundle

推送 `v*` tag 后会生成 GitHub Release，并附带：

- `macOS`：`DMG`
- `Windows`：`NSIS` 安装包

## 本地开发

### 依赖

- `Node.js 22`
- `Rust`
- `Python 3.11+`
- 可用的 Chromium 浏览器，用于抖音 / Bilibili 主页视频提取

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

## 当前边界

- `YouTube` 当前仅开放单视频下载
- 主页视频批量读取与下载只面向已发布作品，不包含喜欢、收藏、直播等内容
- 部分抖音链接仍然依赖浏览器 Cookie
- `Windows` 构建目前通过 GitHub Actions 产出，仍建议继续做实机回归

## 文档

- [变更记录](CHANGELOG.md)
- [贡献说明](CONTRIBUTING.md)
- [项目路线图](docs/roadmap.md)
- [项目规则](docs/project-rules.md)
- [维护上下文](docs/maintainer-context.md)
