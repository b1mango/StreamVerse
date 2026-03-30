# StreamVerse

`StreamVerse` 是一个面向桌面的多平台视频下载器，当前优先完成抖音下载体验，后续会扩展到 `Bilibili`、`YouTube` 等站点。

## 当前能力

- 支持粘贴抖音分享文案、短链和作品链接解析
- 支持可选清晰度、无水印优先、浏览器 Cookie 登录态
- 支持默认下载目录和单次任务目录覆盖
- 支持手动模式 / 智能模式、默认清晰度策略
- 支持任务进度、速度、ETA、完成后定位文件
- 已完成 macOS 桌面打包，代码结构按 macOS + Windows 双平台准备

## 技术栈

- 桌面壳：Tauri 2
- 后端：Rust
- 前端：Svelte 5 + TypeScript
- 解析链路：Douyin bridge + yt-dlp fallback

## 本地开发

```bash
npm install
npm run check
cd src-tauri && cargo test
npm run tauri:dev
```

## 路线图

- 当前：抖音单条下载打磨
- 下一步：Windows 实机验证、批量下载、失败重试、任务历史持久化
- 后续：Bilibili / YouTube 适配层
