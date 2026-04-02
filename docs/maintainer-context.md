# StreamVerse 维护上下文

这份文档用于跨会话恢复项目状态，避免重要信息只存在于临时对话中。

## 项目定位

`StreamVerse` 是一个桌面端多平台视频下载器，当前已经接入 `抖音 / Bilibili / YouTube` 单视频下载，并提供 `抖音 / Bilibili` 的主页批量读取与批量下载流程。

## 当前技术栈

- 桌面壳：`Tauri 2`
- 后端：`Rust`
- 前端：`Svelte 5 + TypeScript`
- 平台模块：`douyin-pack`、`bilibili-pack`、`youtube-pack`
- 共享依赖包：`browser-bridge`、`media-engine`
- 抖音解析：Python bridge + 浏览器 Cookie + 直链下载 + `yt-dlp fallback`
- 批量读取：Python Playwright 浏览器窗口读取 + 会话 Cookie 导出
- Bilibili / YouTube 解析：`yt-dlp + 浏览器 Cookie + FFmpeg 合并高质量 DASH`

## 当前工作流

### 平台首页

- 首屏先选择 `抖音`、`Bilibili` 或 `YouTube`
- 平台切换后再进入对应工作区，避免不同站点规则混杂

### 单视频下载

- 输入分享文案、短链或作品链接
- 解析作品信息与可选格式
- 勾选视频 / 封面 / 文案 / 元数据
- 创建单任务并显示实时进度

### 主页批量下载

- 输入抖音主页链接或 Bilibili UP 主空间页链接
- 打开浏览器窗口读取完整作品列表
- 勾选要下载的作品
- 统一入队下载
- 批量页支持筛选、全选、逐项勾选
- 如果所选作品已经带格式信息，可继续逐条修改清晰度

## 关键模块

- 前端入口：[src/App.svelte](../src/App.svelte)
- 组件目录：[src/lib/components/](../src/lib/components/)
- 前端选项常量：[src/lib/options.ts](../src/lib/options.ts)
- 前端格式工具：[src/lib/media.ts](../src/lib/media.ts)
- 前端类型：[src/lib/types.ts](../src/lib/types.ts)
- Tauri 命令入口：[src-tauri/src/main.rs](../src-tauri/src/main.rs)
- 共享契约：[src-tauri/src/media_contract.rs](../src-tauri/src/media_contract.rs)
- 平台入口：[src-tauri/src/providers.rs](../src-tauri/src/providers.rs)
- pack 调度：[src-tauri/src/pack_host.rs](../src-tauri/src/pack_host.rs)
- pack 安装器：[src-tauri/src/pack_manager.rs](../src-tauri/src/pack_manager.rs)
- pack 映射表：[src-tauri/src/pack_registry.rs](../src-tauri/src/pack_registry.rs)
- pack 公共运行时：[src-tauri/src/pack_common.rs](../src-tauri/src/pack_common.rs)
- 平台 pack 二进制：[src-tauri/src/bin/](../src-tauri/src/bin/)
- 浏览器批量脚本：[scripts/profile_browser_scan.py](../scripts/profile_browser_scan.py)
- 抖音桥接脚本：[scripts/douyin_bridge.py](../scripts/douyin_bridge.py)
- Bilibili 主页桥接脚本：[scripts/bilibili_profile_bridge.py](../scripts/bilibili_profile_bridge.py)
- 下载执行与任务控制：[src-tauri/src/ytdlp.rs](../src-tauri/src/ytdlp.rs)
- 格式去重与默认格式策略：[src-tauri/src/formats.rs](../src-tauri/src/formats.rs)
- 设置持久化：[src-tauri/src/settings.rs](../src-tauri/src/settings.rs)

## 当前已完成

- 单视频下载与主页批量下载已拆成独立页面
- 平台首页已经接入，进入应用先选平台
- `App.svelte` 已拆分出多个可复用组件
- 可选下载内容已经接入：视频、封面、文案、元数据 JSON
- 多内容下载时会创建标题文件夹，单内容下载时直接落盘
- 队列支持暂停、继续、取消、定位文件
- 格式列表已做前后端双重去重
- 最近任务会持久化到本地，应用重启后仍可见
- 失败和取消任务支持一键重试
- YouTube 单视频下载已接入
- 仓库已有 `macOS + Windows` 双平台 GitHub Actions 构建工作流
- README 已补真实界面截图
- Bilibili 单视频下载与 `UP 主批量下载` 已接入，未检测到 FFmpeg 时会对高质量格式给出提示
- README 已更新为面向开源展示的版本
- 抖音 / Bilibili 批量页已切到浏览器窗口读取完整列表
- 批量入队时会为所选作品补解析，而不是在列表阶段逐条慢速解析
- 首页已切换为模块中心，支持安装、卸载、停用、更新
- 平台主路径已经通过本地 pack 调度
- pack 注册表、已安装 manifest、远程 zip bundle 安装链路已接通
- `browser-bridge` 会在主页批量模块安装时自动补齐
- `media-engine` 会在高质量 `Bilibili / YouTube` 下载时按需安装

## 当前限制

- 当前最稳定的是抖音
- 部分抖音链接在游客态下不稳定，依赖浏览器 Cookie
- 主页批量下载当前不包含喜欢、收藏、合集和直播
- 浏览器批量读取依赖本机可用的 Chromium 浏览器
- Bilibili 高质量 DASH 依赖 FFmpeg 合并
- YouTube 当前仍只支持单视频下载
- Windows 仍需补实机打包与运行验证
- pack 远程发布流程已接好，但还没跑过真实 GitHub Release 首次发版

## 当前建议优先级

1. 做 Windows 实机验证
2. 跑一次真实 GitHub Release 发版，验证 pack 远程安装与更新
3. 做 `YouTube` 频道批量下载
4. 继续设计站点适配层，为更多平台预留统一接口
5. 继续优化浏览器批量读取的恢复与异常提示
