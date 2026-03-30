# Contributing to StreamVerse

`StreamVerse` 当前是一个以 `Tauri 2 + Rust + Svelte 5` 为核心的桌面下载器项目，优先打磨抖音体验，后续扩展到更多站点。

## 本地开发

### 依赖

- `Node.js`
- `Rust`
- `Python 3`
- `yt-dlp`

### 常用命令

```bash
npm install
npm run check
cd src-tauri && cargo test
cd ..
npm run tauri:dev
```

## 提交前检查

按改动范围至少跑对应检查：

- 文档改动：确认 `README`、`CHANGELOG`、相关文档已同步
- 前端改动：`npm run check`
- Rust / Tauri 改动：`cd src-tauri && cargo test`
- 需要打包验证时：`npm run tauri:build`

## 贡献约定

- 保持改动聚焦，不把无关重构混进同一提交
- 不提交 `node_modules`、`dist`、`src-tauri/target`、浏览器 Cookie、下载产物或本地日志
- 如果改动影响用户可见行为，请同步更新：
  - [README.md](README.md)
  - [CHANGELOG.md](CHANGELOG.md)
  - [docs/roadmap.md](docs/roadmap.md) 或 [docs/maintainer-context.md](docs/maintainer-context.md)
- 涉及 UI 变化时，优先补截图或录屏说明，方便开源展示与回归检查

## 站点适配约定

- 将站点特定逻辑尽量收敛在独立模块中，不把平台细节散落到 UI 层
- 统一归一到共享数据模型，例如 `VideoAsset` 与 `VideoFormat`
- 新站点的格式列表进入 UI 前，必须经过去重和排序
- 优先保留“直链下载”能力，同时保留 `yt-dlp fallback` 作为兜底
- 不把浏览器 Cookie、账户信息或任何敏感数据写入仓库

## 当前优先级

1. 抖音下载链路稳定性
2. Windows 实机验证
3. 批量下载体验与失败重试
4. Bilibili / YouTube 适配层
