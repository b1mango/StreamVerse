# StreamVerse

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="StreamVerse Logo" width="128" height="128" />
</p>

<p align="center">
  <strong>多平台视频聚合下载工具</strong><br/>
  Tauri 2 · Rust · Svelte 5 · macOS & Windows
</p>

<p align="center">
  <a href="CHANGELOG.md"><img src="https://img.shields.io/badge/version-0.1.4-brightgreen" alt="version" /></a>
  <img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Windows-blue" alt="platforms" />
  <img src="https://img.shields.io/badge/license-MIT-yellow" alt="license" />
  <img src="https://img.shields.io/badge/tauri-v2-orange" alt="tauri" />
</p>

---

## 简介

StreamVerse 是一个面向桌面的多平台视频下载工具。项目基于 **Tauri 2 + Rust + Svelte 5** 构建，支持 **抖音**、**Bilibili**、**YouTube** 三个平台的单视频下载与主页批量下载，通过模块化 pack 架构管理平台能力、浏览器桥接和媒体运行时。

## 平台支持

| 平台 | 单视频下载 | 主页批量下载 | 认证方式 |
| --- | :---: | :---: | --- |
| 抖音 | ✅ | ✅ | 浏览器 Cookie / cookies.txt |
| Bilibili | ✅ | ✅ | 浏览器 Cookie / cookies.txt |
| YouTube | ✅ | — | 浏览器 Cookie / cookies.txt |

## 功能特性

**下载能力**
- 单视频下载与主页批量下载分模块操作，路径清晰
- 每个任务可独立选择下载内容：视频 / 封面 / 文案 / 元数据
- 批量列表支持搜索筛选、全选 / 反选、逐项清晰度选择
- 多项下载自动创建以标题命名的文件夹归档
- YouTube 支持 DASH 多线程分片下载（`--concurrent-fragments`）
- 下载限速对所有下载路径生效（直链 / DASH 合流）
- URL 跨平台校验：解析前自动检测链接所属平台，防止误操作

**任务队列**
- 实时显示进度、速度、ETA
- 支持暂停 / 继续 / 取消 / 重试
- 任务历史跨重启持久化，中断任务恢复为可重试状态
- 一键定位已下载文件（Windows 使用原生 Shell API）
- 全局悬浮滚动按钮：批量列表过长时一键跳到顶部 / 底部

**认证与网络**
- 分平台独立管理认证：每个平台独立配置浏览器来源和 cookies.txt
- Cookie 导入预检：自动校验关键登录 Cookie 是否存在
- 代理策略隔离：仅 YouTube 走代理，国内平台始终直连
- 浏览器窗口登录态接入受限内容解析

**界面与体验**
- macOS 原生窗口外观：系统交通灯按钮、圆角、阴影，双击标题栏最大化
- 设置面板毛玻璃效果（`backdrop-filter: blur`），滑入动画
- 全局悬浮滚动按钮：批量列表过长时一键跳到顶部 / 底部
- 深色 / 浅色主题全局适配

**架构设计**
- 格式列表智能去重，保留用户可感知的清晰度差异
- 内置 FFmpeg，高质量音视频合流无需额外安装
- 平台能力通过 pack 分发，支持按需安装与更新
- 抖音解析连接复用：共享 HTTP 客户端，大幅提升批量解析速度

## 界面预览

| 主页 | 抖音工作区 | Bilibili 工作区 |
| :---: | :---: | :---: |
| ![主页](docs/screenshots/home.png) | ![抖音](docs/screenshots/douyin-workspace.png) | ![Bilibili](docs/screenshots/bilibili-workspace.png) |

## 架构

```
StreamVerse
├── 主应用 (Tauri + Svelte)
│   ├── 界面层 — Svelte 5 组件
│   ├── 调度层 — 任务队列、设置、模块中心
│   └── 后端层 — Rust 命令、yt-dlp 调用、HTTP 客户端
│
├── 平台 Pack
│   ├── douyin-pack    — 抖音解析与批量读取
│   ├── bilibili-pack  — B站解析与批量读取
│   └── youtube-pack   — YouTube 解析
│
└── 共享依赖
    ├── browser-bridge   — 浏览器会话、主页视频提取
    ├── download-engine  — yt-dlp
    └── media-engine     — FFmpeg
```

## 仓库结构

```
src/                    Svelte 前端界面
src-tauri/              Rust 后端、任务执行、pack 调度
  src/bin/              平台 pack 二进制入口
scripts/                Python 桥接脚本与打包辅助脚本
vendor/douyin_api/      抖音桥接依赖
registry/plugins.json   pack 注册表
docs/                   项目文档与截图
```

## 快速开始

### 环境要求

- Node.js 22+
- Rust (stable)
- Python 3.10+
- Chromium 内核浏览器（用于抖音 / Bilibili 主页批量读取）

### 开发

```bash
npm ci                  # 安装前端依赖
npm run tauri:dev       # 启动开发模式
```

### 构建

```bash
npm run tauri:build     # 构建安装包
```

构建产物：
- **Windows**：NSIS 安装包（`.exe`）+ MSI
- **macOS**：DMG

## 发布

推送 `v*` tag 触发 GitHub Actions 自动构建，生成 GitHub Release 并附带各平台安装包。

## 当前限制

- YouTube 暂仅支持单视频下载，频道批量下载在规划中
- 主页批量读取仅覆盖已发布作品，不含喜欢、收藏、直播等
- 部分抖音链接需要浏览器 Cookie 才能正常解析
- Windows Chrome 115+ 启用 App-Bound Encryption 后无法自动读取 Cookie，需手动导出 cookies.txt

## 文档

- [变更记录](CHANGELOG.md)
- [贡献说明](CONTRIBUTING.md)
- [项目路线图](docs/roadmap.md)
- [项目规则](docs/project-rules.md)
- [维护上下文](docs/maintainer-context.md)

## License

[MIT](LICENSE)
