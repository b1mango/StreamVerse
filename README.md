# StreamVerse

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="StreamVerse logo" width="128" height="128" />
</p>

<p align="center">
  抖音、Bilibili、YouTube 桌面视频下载工具
</p>

<p align="center">
  <a href="https://github.com/b1mango/StreamVerse/releases"><img src="https://img.shields.io/badge/release-0.1.5-2ea043" alt="release 0.1.5" /></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-1f6feb" alt="platforms" />
  <img src="https://img.shields.io/badge/license-MIT-f5c542" alt="MIT license" />
</p>

<p align="center">
  中文 · <a href="README.en.md">English</a>
</p>

## 功能

**支持平台**

| 平台 | 单视频 | 主页批量 |
| --- | :---: | :---: |
| 抖音 | ✅ | ✅ |
| Bilibili | ✅ | ✅ |
| YouTube | ✅ | — |

**下载内容**

- 视频（多清晰度可选，支持无水印、高码率）
- 封面图
- 文案 / 简介
- 元数据 JSON

**任务管理**

- 实时下载进度、速度和剩余时间
- 暂停 / 继续 / 取消 / 重试
- 下载历史保留，重启后可重试失败任务
- 完成后一键定位文件

**登录与认证**

- 支持从 Chrome / Edge / Firefox 浏览器自动读取 Cookie
- 支持手动粘贴 Cookie 文本或导入 `cookies.txt`
- 抖音、Bilibili、YouTube 各自独立配置，互不干扰

**界面**

- 暗色 / 浅色双主题
- 批量列表支持搜索、拖选、清晰度单独设置
- 全局悬浮滚动按钮，长列表快速跳转
- 中英文界面切换

## 界面预览

| 首页 | 抖音工作区 | Bilibili 工作区 |
| :---: | :---: | :---: |
| ![Home](docs/screenshots/home.png) | ![Douyin workspace](docs/screenshots/douyin-workspace.png) | ![Bilibili workspace](docs/screenshots/bilibili-workspace.png) |

## 安装

从 [GitHub Releases](https://github.com/b1mango/StreamVerse/releases) 下载最新版本：

- **Windows**：`.exe` 安装包
- **macOS**：`.dmg` 安装包

## 使用

1. 打开设置，选择下载目录和并发数
2. 配置登录方式（浏览器读取或导入 Cookie）
3. 粘贴视频或主页链接，点击解析
4. 选择清晰度和下载内容，入队下载

## License

[MIT](LICENSE)
