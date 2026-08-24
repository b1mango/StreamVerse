# StreamVerse 项目规则

## 构建

- Windows 构建：`RUSTUP_TOOLCHAIN=1.95 npm run tauri:build`（Rust 工具链固定 1.95）。
- 构建前先确认没有运行中的 `streamverse.exe` 实例，否则 NSIS 打包会因文件占用失败。
- 构建产物：
  - 安装包 `src-tauri/target/release/bundle/nsis/StreamVerse_<版本>_x64-setup.exe`
  - 免安装版 `src-tauri/target/release/streamverse.exe`
- 交付时给出 exe 的绝对路径，不要额外复制到别的文件夹。

## 版本与发布

- 版本号需同步四处：`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`（及 `Cargo.lock`）、`package.json`、README 徽章。
- **Release 变更说明必须按「新增 / 修复 / 变更」三个模块组织**（无内容的模块可省略），CHANGELOG.md 与 GitHub Release 说明保持一致。
- CHANGELOG.md 为混合行尾（部分 CRLF），编辑时注意精确匹配。

## Git

- 改动默认不提交；用户明确要求提交或推送时才执行。
- 推送走本机代理：`git -c http.proxy=http://127.0.0.1:7897 push origin HEAD:main`。

## 界面约定

- 窗口为方形外框（Win10/Win11 均不做圆角透明窗口），应用外壳铺满整个窗口。
- 深色主题为黑紫噪点渐变（与 b1mango.cc 一致），强调色为薄荷绿，焦点环为暖金。
- 应用图标为无背景透明版「星环星球」，所有图标位置（exe、安装包、窗口品牌位、关于页、README）保持同源。
