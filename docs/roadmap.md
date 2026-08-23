# StreamVerse 路线图

## 2.0 当前目标

- 固定 sidecar、内置 provider 和无运行时 Python 的可复现桌面包
- 浏览器 Cookie 明确授权、最小域名读取和可撤销本地保存
- `DownloadRequest` 与增量 `TaskEvent` 作为稳定内部契约
- “Signal Cinema”工作区在常用桌面尺寸和 Windows 缩放下无重叠
- Windows NSIS 实机通过；macOS app/DMG 在 CI 通过并完成后续实机 smoke test

## 已完成

- 删除动态 pack、registry、安装/卸载和 pack 发布链路
- 抖音、Bilibili、YouTube provider 内置
- yt-dlp、FFmpeg、Python helper 按目标 triple 打包
- 下载任务事件驱动同步
- Cookie 浏览器/Profile 枚举、授权、撤销、手动粘贴和文件导入
- Svelte 5 runes、Lucide 图标、50 项以上批量虚拟化
- 后端拆分为 IPC、认证、provider runtime、任务控制、传输和产物模块

## 近期验证

1. Windows 三浏览器 Cookie 矩阵和真实受限链接下载。
2. Windows 125% / 150% 缩放与 1024x720 最小窗口回归。
3. macOS Chrome / Safari 读取和 DMG 启动测试。
4. 真实抖音/Bilibili 批量 500 项的选择、入队、取消与重试压力测试。

## 后续产品范围

- YouTube 频道与合集批量已纳入 2.0。
- 新平台只有在现有三平台门禁稳定后再评估。
- 不恢复动态 pack；平台扩展继续使用进程内 provider 和共享数据契约。
