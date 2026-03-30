# StreamVerse 维护上下文

这份文档用于跨会话恢复项目状态，避免重要信息只存在于临时对话中。

## 项目定位

`StreamVerse` 是一个桌面端多平台视频下载器，当前优先完成并打磨抖音体验，同时已经接入 `Bilibili` 单视频下载 Beta，后续会继续扩展到 `YouTube` 等站点。

## 当前技术栈

- 桌面壳：`Tauri 2`
- 后端：`Rust`
- 前端：`Svelte 5 + TypeScript`
- 抖音解析：Python bridge + 浏览器 Cookie + 直链下载 + `yt-dlp fallback`
- Bilibili 解析：`yt-dlp + 浏览器 Cookie + FFmpeg 合并高质量 DASH`

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

- 输入抖音主页链接
- 读取作品列表
- 勾选要下载的作品
- 统一按当前策略入队下载

## 关键模块

- 前端入口：[src/App.svelte](../src/App.svelte)
- 组件目录：[src/lib/components/](../src/lib/components/)
- 前端选项常量：[src/lib/options.ts](../src/lib/options.ts)
- 前端格式工具：[src/lib/media.ts](../src/lib/media.ts)
- 前端类型：[src/lib/types.ts](../src/lib/types.ts)
- Tauri 命令入口：[src-tauri/src/main.rs](../src-tauri/src/main.rs)
- 抖音桥接：[src-tauri/src/douyin.rs](../src-tauri/src/douyin.rs)
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
- README 已补真实界面截图
- Bilibili 单视频下载 Beta 已接入，未检测到 FFmpeg 时会对高质量格式给出提示
- README 已更新为面向开源展示的版本

## 当前限制

- 当前最稳定的是抖音
- 部分抖音链接在游客态下不稳定，依赖浏览器 Cookie
- 主页批量下载当前不包含喜欢、收藏、合集和直播
- 批量下载当前按全局策略选格式，尚未支持逐条手选清晰度
- Bilibili 目前只做了单视频下载，UP 主批量下载尚未实现
- Bilibili 高质量 DASH 依赖 FFmpeg 合并
- Windows 仍需补实机打包与运行验证

## 当前建议优先级

1. 继续把前端组件拆细，收敛跨页面共享逻辑
2. 增加 Bilibili 的 UP 主批量下载与更多附加内容
3. 补任务历史持久化与失败重试
4. 做 Windows 实机验证
5. 继续设计站点适配层，为 `YouTube` 预留统一接口
