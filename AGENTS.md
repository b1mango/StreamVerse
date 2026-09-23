# StreamVerse 项目规则

## 全局规则来源

- 全局开发与协作规则采用 `https://github.com/b1mango/Agent-settings` 远程 `main` 的 `rules/AGENTS.md` 与 `rules/编程准则.md`。
- `rules/提示词库.md` 仅选用模板时读取；`skills/` 为按需参考技能库，入口 `skills/README.md`，不强制逐项调用。
- 以下为项目专属约定；与用户最新授权冲突时，以用户要求为准。

## 构建

- Windows 构建：`RUSTUP_TOOLCHAIN=1.95 npm run tauri:build`（Rust 工具链固定 1.95）。
- 构建前先确认没有运行中的 `streamverse.exe` 实例，否则 NSIS 打包会因文件占用失败。
- 构建产物：
  - 安装包 `src-tauri/target/release/bundle/nsis/StreamVerse_<版本>_x64-setup.exe`
  - 免安装版 `src-tauri/target/release/streamverse.exe`
- 交付时给出 exe 的绝对路径，不要额外复制到别的文件夹。
- macOS 构建：`npm run tauri:build`（首次需 `pip install pyinstaller -r requirements-douyin-helper.txt` 并先跑 `npm run build:helper`；sidecar 下载走代理时加 `NODE_USE_ENV_PROXY=1 HTTPS_PROXY=http://127.0.0.1:7897`）。
  - 产物：`src-tauri/target/release/bundle/dmg/`（未签名，首次打开需右键打开）。
  - 平台差异配置走 Tauri 平台配置文件：`tauri.windows.conf.json`（Windows 无边框）、`tauri.macos.conf.json`（macOS 恢复原生窗口阴影）。

## 界面约定

- Windows：窗口为方形外框（Win10/Win11 均不做圆角透明窗口），应用外壳铺满整个窗口，无边框 + 自绘标题栏。
- macOS：原生红绿灯 + 隐藏标题栏（`hiddenTitle` + `Overlay`），顶栏/侧栏空白区充当拖拽区（`is-macos` 类 + `App.svelte` mousedown 处理），侧栏/顶栏/队列面板覆毛玻璃，圆角与系统字体栈向 macOS 靠拢。
- 深色主题为黑紫噪点渐变（与 b1mango.cc 一致），强调色为薄荷绿，焦点环为暖金。
- 应用图标为无背景透明版「星环星球」，所有图标位置（exe、安装包、窗口品牌位、关于页、README）保持同源。

## 版本与发布

- 版本号需同步四处：`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`（及 `Cargo.lock`）、`package.json`、README 徽章。
- **Release 变更说明必须按「新增 / 修复 / 变更」三个模块组织**（无内容的模块可省略），CHANGELOG.md 与 GitHub Release 说明保持一致。
- CHANGELOG.md 为混合行尾（部分 CRLF），编辑时注意精确匹配。

## Git

- 本地按完成的任务提交，保护其他工作中的改动；只有用户明确要求推送时才推送。
- 推送走本机代理：`git -c http.proxy=http://127.0.0.1:7897 push origin HEAD:main`。

