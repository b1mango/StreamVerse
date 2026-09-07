# Changelog

All notable changes to `StreamVerse` will be documented in this file.

## [1.0.1] - 2026-08-24

### 新增

- 设置新增「关于」页：展示真实版本号与 GitHub 仓库地址，支持一键检查更新（读取最新 Release 并比较版本号）。
- Windows 构建改为无边框窗口，新增自绘标题栏（拖拽区、最小化/最大化/关闭），修复 Windows 10 顶部白边问题。
- macOS 版正式适配：原生红绿灯 + 隐藏式标题栏，顶栏/侧栏/下载队列覆毛玻璃，空白区域可拖拽移动窗口，窗口阴影恢复系统原生，圆角与字体栈向 macOS 靠拢。
- 主页/合集批量列表新增标题筛选搜索框（胶囊样式，与下载内容选项同行），支持按标题/作者过滤。
- YouTube 批量清晰度补全支持单独重读失败项：存在失败条目时操作栏出现「重读失败清晰度 · N」，只补读失败条目，不再整批重来。

### 修复

- 设置「检查更新」按钮的加载文案过长溢出按钮宽度，缩短为不溢出。
- 并行下载数可输入超界值（如 10）并保存成功，改为输入大于 8 时给出内联提示。
- YouTube 登录态频繁失效：关键 Cookie 校验放宽为 LOGIN_INFO 或 SAPISID 家族任一在场即可（SAPISIDHASH 鉴权）；本地 Cookie 文件校验未过时公开内容自动降级为来宾模式继续解析，不再整块阻断；「缺少关键 Cookie」类报错纳入登录态自愈路径（已授权浏览器静默重取并重试一次）。
- YouTube 主页/合集真实清晰度读取几秒即报「N 个读取失败」，改为动态持续读取、失败条目自动退避重试，工人错开启动降低限流概率；进度即时刷新，进度条始终反映真实完成数（已完成 X/Y），起始反馈不再长时间卡在（0/N）。
- YouTube 合集解析进度提示误写「正在读取主页视频」，改为「正在读取合集视频」。
- 窗口缩窄至 920px 以下、下载队列变为浮层时，背景回退为黑色，恢复为黑紫玻璃态。
- 左侧导航品牌位图标与应用图标不一致，统一为新版应用图标。
- 切换平台或解析模块后已解析结果丢失：解析记录按「平台 × 模式」快照记忆，YouTube 主页与合集可并行独立解析、互不中断，切回即可看到对应进度。
- YouTube 清晰度下拉选项较多时弹层滚动条无法滑动（全局滚动监听误关弹层），弹层内部滚动不再触发关闭，滚动到底也不穿透到外层列表。
- 抖音单视频解析慢（约 5 秒降至约 1.2 秒）：完整链接本地提取作品 ID 省去一次重定向请求，短链只解析一次，403 风控重试轮换真实 msToken 并收紧退避。
- 抖音 helper 的真实错误（登录态失效、403 等）不再被 yt-dlp 兜底流程吞掉，直接提示真实原因。
- 下载重试复用原目录断点续传：此前每次重试换新目录，已下载分片作废只能从头开始。
- 下载的 caption.txt 文案文件补充正文内容（此前仅含元信息）。
- yt-dlp 报错提取优先取 ERROR 行，不再被末尾 WARNING 行误导。
- YouTube 下载进度卡在 1% 后瞬间跳变：进度按实际下载字节动态刷新。
- 代理地址自动规范化，修复「http:/127.0.0.1:1082」类配置导致的 YouTube 解析/下载失败。
- B 站 1080P 下载结果仅 8.8MB 且无法播放：格式选择器锁定 avc1 编码流，无登录 Cookie 时明确报错提示。
- B 站批量列表文件大小缺失、估算偏差过大：改用经验中位码率估算，有真实大小时优先显示真实值。
- 下载队列完成提示文字靠左对齐，控制台折叠按钮移至最右侧。

### 变更

- 界面整体重构：移除 SignalField 信号波背景动画，改为静态微光背景；工作台内容居中限宽，输入控制台重新分层排布（模式切换、保存路径入口、平台/登录态 chip、Ctrl+Enter 提示与解析主按钮）。
- 操作反馈改为顶部 toast 浮层（成功/失败，自动消失），解析相关错误保留在控制台下方内联展示。
- 下载队列按「进行中 / 已结束」分组，任务卡片带封面缩略图（本地代理懒加载并缓存），操作按钮改为悬停显现；`DownloadTask` 新增 `coverUrl` 字段，旧持久化数据兼容。
- 主页/频道批量列表行加入缩略图，批量列表改为填充剩余窗口高度；下载内容选项改为胶囊开关。
- 设置由右侧窄 sheet 改为左导航宽面板（登录态 / 下载 / 外观与语言），主题与语言提为一级分组。
- 下载历史补齐缩略图、可读时间和「定位文件」入口；`DownloadHistoryEntry` 新增 `coverUrl` 与 `outputPath` 字段，旧数据兼容。
- 设计 token 收紧：正文 13px、弱文本对比度提升、统一圆角/阴影/缓动曲线，浅色主题同步更新；文档截图更新。
- 批量列表的清晰度下拉改为「清晰度 · 编码/容器 · 文件大小」（如 `1080P · H.265 · 100.28 MB`），不再显示分辨率宽高；B 站条目的「自适应」（DASH 流）改显示封装容器（如 mp4）；估算大小不再带 `≈` 前缀。
- 切换 单视频/主页/合集 模式时保留各模式已解析的结果与选择状态；YouTube 批量清晰度补全改为后台持续进行，切换模式不再中断，切回即可看到最新进度。
- 圆角语言收敛：`--radius-l` 收敛为 12px；无边框窗口增加内描边以界定窗口边缘（Win10 直角 / Win11 圆角均适用）；下载队列保持贴边方正形态。
- 设置：登录态区分「自动获取 / 手动获取」分组并移除认证目录说明文字。
- 英文模式改用内嵌的 IBM Plex Sans 可变字体（与用户站点字体一致），中文模式不变。
- YouTube 批量清晰度补全的进度条在后台补全期间持续显示。
- 移除工作区「解析 · 下载」眉题；左侧导航品牌位由 Sparkles 图标换为应用图标。
- 抖音输入规则收敛：单视频模块仅接受分享文案/短链，主页批量支持网页地址或分享文案。
- 所有原生下拉框替换为自定义下拉组件：选项配色与深色主题统一（悬浮高亮、选中项品牌色加对勾），弹层支持向上展开与键盘操作。
- 设置窗口改为透黑玻璃态（半透明背景加背景模糊），浅色主题同步适配。
- YouTube 主页/合集清晰度补全并发从 4 提升到 6（两个批次并行时减半为 3），失败条目按限流/普通错误分级退避重试。
- 标题栏版本号改为读取应用真实版本（如 1.0.1），移除无边框标题栏中的应用名文字；toast 文案统一去掉结尾句号；历史页平台标识不再强制大写。
- 深色主题背景与个人站（b1mango.cc）一致：`#2e2838 → #2a222d → #08070a` 黑紫径向渐变叠加双层 feTurbulence 噪点纹理（overlay + soft-light），暗色阶各层改为紫调近黑；浅色主题不带噪点。
- 应用图标全套替换为无背景透明版「星环星球」：Windows exe 图标、NSIS 安装包图标、macOS icns、窗口左上品牌位与设置「关于」页 logo 同源更新。
- 主题强调色恢复与个人站一致的薄荷绿（深色 `#6ee7b7` / 浅色 `#1dad85`），焦点环保留图标星环的暖金。
- 窗口外框回归方形铺满：放弃圆角透明窗口方案，应用外壳铺满整个窗口，内部卡片圆角同步收敛，与 Win10 直角窗口观感一致。
- YouTube「频道主页」表述统一为「主页频道」。
- README 界面截图更新为主页、下载历史、设置三张实机截图。
- YouTube/B 站下载在 aria2c sidecar 可用时启用多连接分段加速（B 站每流 8 连接、YouTube 16 连接），sidecar 缺失时自动回退单连接下载，重试支持分段续传。
- 下载任务调度改为有界 worker 线程池：排队任务不再各占一个系统线程，批量入队等待间隔仅在需要重新解析时生效。
- 抖音图册多图下载改为 4 路并发。
- 批量列表清晰度列加宽，「1080p60 · H.264 · 247.5 MB」等长标签完整显示。
- 内部重构：下载引擎按职责拆分为 progress/errors/files/dash/direct 模块，YouTube 批量清晰度补全抽为独立前端模块，删除失效的重复实现与死代码。

## [1.0.0] - 2026-08-23

### 新增

- “Signal Cinema”全窗口下载工作区、可折叠实时队列和设置侧边 sheet。
- 基于 `rookie-cookies` 的浏览器/Profile 枚举与按平台域名白名单导入。
- Cookie 首次授权、仅本次/始终允许、撤销删除、关键 Cookie 校验和 Windows 独立 UAC helper。
- 手动 Cookie Header 与 `cookies.txt` 导入，两者都重新过滤后原子写入应用认证目录。
- 固定版本且校验 SHA-256 的 yt-dlp、FFmpeg 和单文件 `streamverse-helper` sidecar。
- YouTube 频道与合集批量解析、虚拟化选择和批量入队。
- 固定 Deno sidecar，为 yt-dlp 提供不依赖用户环境的 YouTube JavaScript 挑战求解。

### 变更

- 三个平台改为进程内 provider；删除动态 pack、registry、安装/卸载与 pack Release 流程。
- 下载 IPC 收敛为 `DownloadRequest`；重试复用同一请求结构。
- 任务同步改为初始加载一次后接收增量 `TaskEvent`，删除前端轮询。
- 下载后端拆为启动、IPC、provider runtime、任务控制、传输执行和产物保存模块。
- 前端迁移到 Svelte 5 runes、callback props、Lucide 图标和 TanStack Virtual。
- 设置、任务、历史和认证使用新的稳定版数据结构。

### 修复

- Chrome Profile 默认改为最近使用项，修复多 Profile 机器读取到未登录目录的问题。
- 修复 Chrome App-Bound Encryption 部分解密失败时误报“缺少关键 Cookie”，现在会正确进入一次性 UAC helper。
- YouTube 登录态校验与 yt-dlp 对齐，支持 `__Secure-1PAPISID` 并要求完整的账号登录组合。
- 修复批量列表更新 virtualizer options 时订阅自身 store，导致所有主页批量模块触发 `effect_update_depth_exceeded` 的问题。
- 移除浏览器 UI 预览中的虚构解析结果和任务；真实解析只允许通过 Tauri 桌面运行时调用。
- 修复 Cookie 授权失败后弹窗无反馈且可重复提交的问题，并为浏览器数据库占用提供可执行提示。
- 修复 Windows UAC Cookie helper 经 PowerShell 转发时丢失参数和结果的问题，改用原生进程启动与退出状态检查。
- 修复 UAC helper 使用未限定用户名设置 Cookie 文件 ACL，导致主程序导入成功后仍被拒绝访问的问题。
- 修复解析结果缩略图未经过本地代理而被平台防盗链拦截的问题。
- 视频格式按清晰度、编码和容器保留最高码率唯一项，并改为单列列表展示。
- 修复竖版封面在固定预览框内被裁剪，改为完整比例显示。
- 修复 YouTube 下载误用失效产物路径导致视频黑屏、MP3 提取找不到输入文件的问题。
- YouTube 默认使用可并发的 HLS 格式与 32 路分片下载，并在完成后验证最终文件存在。
- YouTube 封面下载增加 JPEG 多级回退，避免 `vi_webp` 直链不可达时保存失败。
- 抖音单视频优先使用主页 `cover_original_scale` 原比例封面，缺失时再回退动态封面与视频帧。
- YouTube 频道主页与播放列表拆分为独立解析模式；频道根链接和 `/featured` 自动归一化到 `/videos`。
- YouTube 公开频道与播放列表在跳过网页解析时同步跳过认证预检，修复已导入 Cookie 的其他 Windows 设备上批量解析被错误中止的问题。
- YouTube 批量下载逐条重新解析真实格式，删除会制造失效地址的 `best` 占位格式。
- YouTube 下载固定 IPv4，低于 512 KiB/s 时自动重取播放地址并提示在设置中填写或切换代理。
- YouTube 下载前在 2 秒内检测本地代理端口；代理客户端未启动或端口错误时立即给出通用提示，不再长时间停留在“准备中”。
- Windows 取消 YouTube 下载时终止 yt-dlp、Deno 与 FFmpeg 整个进程树，并正确释放下载并发槽。

### 移除

- 运行时 Python、venv、pip 安装及首次启动依赖下载。
- `cookies-from-browser` 全站导出路径和会强制关闭 Chrome 的解锁插件。
- 动态模块中心、pack 二进制、pack registry 和旧界面组件。

## [0.1.5] — 2026-05-26

### 新增
- **YouTube 封面回退**：预览支持封面回退显示，减少代理链路差异导致的封面缺失。
- **长简介悬停查看**：简介默认截断，悬停可查看完整内容，标题保持完整显示。
- **MIT License 与英文 README**：补充正式开源协议，并新增英文项目介绍。
- **毛玻璃圆形进度环**：抖音 / B 站主页批量解析时，显示居中毛玻璃环形进度指示器，固定尺寸不跳变，替代页面底部内联进度条。
- **封面图本地缓存**：缩略图以 SHA-256 URL 哈希缓存至 `~/.streamverse/thumbnails/`，重复解析同一视频不再重复下载封面。
- **SHA-256 强制校验**：远程下载的 pack 必须提供 SHA-256 校验和（`file://` 本地源除外），缺少则拒绝安装，防止供应链篡改。
- **ZIP 炸弹防护**：pack 解压增加 500MB 上限，超限拒绝解压并报错。

### 修复
- **Windows 标题栏按钮重复**：恢复 Windows 原生窗口控制样式，避免右上角按钮显示两组。
- **B 站下载进度异常**：修复进度事件识别错误，恢复实时进度更新。
- **YouTube 缩略图代理**：缩略图请求读取代理设置，修复封面缺失。
- **解析与下载稳定性**：修复限速回填、抖音超时重试与 YouTube 格式解析问题。
- **安装包图标未刷新**：重新生成应用与 NSIS 安装包图标，确保安装包嵌入新版图标。
- **Cookie 域名泄露**：粘贴 Cookie Header 文本时，仅写入当前平台对应域名（抖音 → `.douyin.com` / `.iesdouyin.com`；B 站 → `.bilibili.com` / `.b23.tv`；YouTube → `.youtube.com` / `.google.com`），不再全量写入所有平台。
- **批量下载瞬间并发**：主页批量入队每个任务间隔 800ms，避免同时发起大量请求触发平台反爬。
- **下载完成提示路径过长**：消息从完整文件路径（含视频标题子目录）改为仅显示用户设置的保存根目录。
- **重复标签删除**：移除各处冗余 section-label（`single.preview`、`Profile Batch`、`batch.items`、`Profile Result`）及任务面板重复的「最近任务」标题。

### 变更
- **UI 色彩体系重构**：保留原始 StreamVerse 青绿色 `#30ccb0`；平台卡片悬停色统一，不再按抖音 / B 站 / YouTube 区分三色。
- **圆角收紧**：全局圆角从 `24px / 18px / 14px` 改为 `8px / 6px / 4px`。
- **字体切换**：Apple 系统字体栈（SF Pro / Segoe UI Variable）→ `Inter, system-ui`，消除 AI 生成模板感。
- **背景与卡片**：body 去径向渐变改为纯深色 `#0d1117`；卡片从半透明 `rgba(..., 0.88)` 改为不透明 `#161b22`。
- **加载动画**：脉冲点 `pulse-card` → 简洁旋转 CSS spinner。
- **分析进度弹窗**：全屏模态弹窗 + SVG 打勾动画 → 居中毛玻璃圆环，精简 ~180 行 CSS。
- **平滑滚动加速**：自定义 easeInOutQuart 动画从 720ms 缩短至 300ms。
- **移除 Safari 浏览器支持**：`rookie` 不支持 Safari 且 yt-dlp 回退需 Full Disk Access 权限，体验不可靠，全面移除。
- **yt-dlp 路径缓存**：首次解析后通过 `OnceLock` 缓存，后续下载不再重复探测候选路径和 `--version` 检查。
- **任务进度事件节流**：`tasks-changed` 事件增加 300ms 全局节流，减少前端高频渲染。
- **前端轮询降频**：任务列表 fallback 轮询从 2s 延长至 10s（事件驱动已覆盖正常路径）。
- **全局 HTTP 客户端复用**：pack 下载与注册表拉取共用 `OnceLock<Client>` 连接池，避免每次新建 TCP/TLS 连接。
- 重新生成 macOS 安装包与主程序构建产物。
- README 改为更标准的开源项目介绍结构，并补充 Python 迁移方向说明。
- Edge on Windows 与 Chrome 同步加入 Cookie 加密拦截提示。
- 下载完成提示改为只显示保存根目录，不再带视频标题子目录。

---

## [0.1.4] — 2026-04-04

### 新增
- **全局悬浮滚动按钮**：页面右下角固定悬浮「回到顶部 / 跳到底部」按钮，批量列表 700+ 条目时快速定位，毛玻璃风格匹配整体 UI
- **下载限速全路径覆盖**：速度限制现在对直链下载和 DASH 合流下载均生效（此前仅对 yt-dlp 子进程生效）
- **macOS 原生窗口外观**：切换到 `titleBarStyle: Overlay` + `decorations: true`，使用系统原生交通灯按钮、圆角和窗口阴影，移除所有 CSS hack
- **macOS 窗口拖动**：通过 Tauri `window.startDragging()` 实现标题栏拖动，双击标题栏最大化，拖动时禁止文本选中
- **设置面板毛玻璃效果**：Settings 面板使用 `backdrop-filter: blur(40px)` 半透明背景，滑入动画
- **URL 跨平台校验**：解析前自动检测 URL 所属平台，防止抖音链接粘贴到 B站模块等误操作

### 修复
- **深色模式下拉菜单白底白字**：修复 `<select>` 组件的 `<option>` 在深色模式下背景为白色导致文字不可见
- **4K 视频进度卡在 1%**：当 CDN 不返回 Content-Length 时，进度条改用渐近曲线 $1 - e^{-x/50}$ 代替固定 0/1
- **已完成任务缺少「定位文件」按钮**：yt-dlp 回退路径的 ArtifactSummary 现在正确设置 `output_path`
- **全局滚动按钮失效与卡顿**：统一改为滚动实际页面容器，修复"跳到底部"误回顶部，并降低长队列滚动时的抖动
- **浅色模式多处组件样式缺失**：补全 `.text-button.danger`、`.chip.accent`、`.settings-guide-card`、`.cookie-method-summary`、`.cookie-method-details` 的浅色样式
- **CSS 变量引用错误**：`.cookie-method-summary:hover` 引用了未定义的 `var(--foreground)`，修正为 `var(--text)`
- **macOS 窗口白边**：移除 `decorations: false` + `transparent: true` 导致的 WebView 白边，改用原生窗口装饰
- **macOS 解析弹窗卡死**：`withAnalysisProgress` 的 `onResult` 回调异常时未关闭弹窗，增加 try-catch 确保弹窗清理
- **YouTube Cookie 回归**：恢复 0.1.2 逻辑，始终直接传递 Cookie 避免 bot 检测
- **抖音「读取主页视频」按钮灰色**：修复 Cookie 来源判断，同时支持 `cookieBrowser` 和 `cookieFile`
- **抖音主页解析准确度**：修复分页逻辑提前中断（移除 `count=min()` 和内层 `break`），确保遍历至 API 报告 `has_more=false`
- **抖音主页解析速度**：共享 `httpx.AsyncClient` 复用 TCP/TLS 连接，批量大小从 18 提升到 20，避免每次请求重建 HTTP 客户端

### 变更
- 滚动按钮从任务队列面板级别移至全局页面级别
- 移除任务列表的 `max-height` 限制，恢复自然流动布局
- macOS 移除 objc2 系列依赖与自定义圆角函数，完全依赖系统窗口管理
- 抖音 `totalAvailable` 现在反映真实总数（视频 + 跳过的图文笔记）

---

## [0.1.3] — 2026-04-02

### 新增
- **分平台认证管理**：抖音 / B站 / YouTube 各自独立配置浏览器来源和 cookies.txt 文件，互不污染
- **Cookie 预检**：导入 cookies.txt 时自动检查是否包含当前平台关键登录 Cookie（抖音: `sessionid` / `sessionid_ss`；B站: `SESSDATA`），缺失时提前提示
- **Windows Chrome 引导**：在 Windows 上仅选 Chrome 浏览器来源、未提供 cookies.txt 时，提前提示用户导出 Cookie 而非等到下游报错
- **Windows 原生文件定位**：「定位文件」改用 Shell API（`SHOpenFolderAndSelectItems` / `ShellExecuteW`），不再依赖 `explorer.exe` 退出码判断
- **YouTube 多线程分片下载**：增加 `--concurrent-fragments 4`，提升 DASH 流下载吞吐量
- **抖音临时目录自动清理**：手动浏览器读取流程产生的 `manual-{browser}-{port}` 临时用户配置目录，在启动和完成时自动回收超过 6 小时的陈旧目录

### 修复
- **B站 / 抖音下载极慢**：`reqwest::Client` 和 yt-dlp 子进程默认继承系统代理，导致国内平台 CDN 流量绕境；现在非 YouTube 平台显式禁用代理（`.no_proxy()` + `--proxy ""`）
- **批量解析弹窗「秒关后卡顿」**：进度弹窗在后端返回后立即关闭，但 200+ 条数据的 Svelte 响应式渲染阻塞主线程；现在数据赋值在弹窗仍然可见时完成，弹窗等渲染结束后才关闭
- **缩略图并发风暴**：批量解析 200+ 条结果时一次性发出所有缩略图请求导致 UI 卡顿；改为每批 6 个串行执行
- **「定位文件」误报失败**：Windows 上 `explorer.exe` 已成功打开目录但返回非零退出码，UI 仍显示「文件管理器没有成功打开目标路径」
- **`build.rs` 编译警告**：移除未使用的 `std::io::Read` 导入和不可达代码分支
- **`ProfileBatchWorkspace` 编译警告**：移除未使用的 `pasteLabel` / `pasteLoadingLabel` 导出，将直接 `onmousedown` 替换为 DOM action

### 变更
- 代理策略收敛：仅 YouTube 使用代理；抖音 / B站 无论设置面板是否填写代理均走国内直连
- 设置面板重构为分平台认证卡片，每个平台独立显示浏览器来源、Cookie 文件和认证状态
- 设置存储从全局 `cookie_browser` / `cookie_file` 迁移为 `platform_auth` 映射，兼容旧版配置自动迁移
- 每平台导入的 Cookie 文本保存为独立文件（`saved-{platform}-cookies.txt`）

---

## [0.1.2] — 2026-04-01

### 新增
- 解析进度模态弹窗：批量解析时显示实时进度条，完成后播放打勾动画（1.2 秒后自动关闭）
- 新增 `download_history.rs` 下载历史模块
- 新增 `i18n.ts` 前端国际化模块
- 透明标题栏（`titleBarStyle: Overlay` + `hiddenTitle`），窗口顶部拖动区域

### 修复
- **UI 阻塞**：`analyze_profile_input` / `collect_profile_browser` 从 `std::thread::spawn` + `join` 改为 `tauri::async_runtime::spawn_blocking`，彻底解决批量解析和模块切换卡死问题
- **B站"访问权限不足"**：主页批量解析改为先请求 nav 获取 WBI 密钥，再对 profile info 请求做 WBI 签名
- **抖音 JSON 解析失败**：vendor 脚本 `print()` 污染 stdout，改为从 stdout 提取最后一行 JSON
- **进度条不满**：CSS transition + 组件提前卸载导致进度条永远到不了 100%，增加 320ms 延迟
- **设置保存冻结**：`save_settings` 改为异步
- **格式标签不响应**：Svelte 5 模板函数调用对闭包变量不自动响应，修复响应式
- **B站缩略图跨域**：通过 Rust `fetch_thumbnail` 命令代理请求并返回 base64
- **浅色模式样式**：修复 light 模式下多处 CSS 异常

### 变更
- B站并发数从 10 提升到 20（`FETCH_CONCURRENCY = 20`）
- 解析进度从内嵌面板移至全屏模态弹窗
- 默认主题 `dark`、默认语言 `zh-CN`（前后端统一）
- 移除输入框旁的粘贴按钮

---

## [0.1.1] — 2026-03-31

### Added

- Platform home screen for choosing `抖音` / `Bilibili` / `YouTube`
- New workspace structure with separate platform pages and platform-specific modes
- Separate workspaces for `单视频下载` and `主页批量下载`
- Optional download artifacts: video, cover, caption text, metadata JSON
- Profile batch preview flow with selectable items before enqueue
- Queue controls for pause, resume, cancel, and reveal in Finder / Explorer
- Save-directory settings, browser-cookie source settings, and download strategy settings
- Format deduplication to collapse visually identical quality entries
- Bilibili single-video download beta with FFmpeg-aware format handling
- Bilibili `UP 主批量下载` with preview-first selection flow
- Per-item quality selection for batch downloads
- YouTube single-video download
- Bundled `FFmpeg` for packaged `macOS / Windows` builds, with automatic `yt-dlp --ffmpeg-location` wiring
- Task history persistence across app restarts
- One-click retry for failed and cancelled tasks
- GitHub Actions workflow for `macOS + Windows` desktop builds
- Real UI screenshots under `docs/screenshots/`
- Maintainer docs for roadmap, contributor guidance, project rules, and context handoff
- Browser-window profile scanning for `抖音 / Bilibili` batch pages
- Searchable batch lists with no fixed 24 / 100 item ceiling
- Module-center runtime with on-demand pack install / uninstall / update
- Shared dependency packs:
  - `browser-bridge` for profile batch flows and Python helper runtime
  - `media-engine` for on-demand `FFmpeg` installation and DASH merging
- Pack registry, installed manifest tracking, and release bundle generation workflow

### Changed

- Refined README for open-source presentation and clearer workflow explanation
- Split the large `App.svelte` into reusable UI components
- Updated default save directory to `~/Movies/StreamVerse`
- Interrupted in-progress tasks are now restored as retryable failed tasks on next launch
- Batch pages now allow per-item format overrides before enqueue
- Batch pages now open a browser reader window, collect lightweight lists first, and resolve selected items later
- Refined download output rules:
  - single selected artifact saves directly
  - multiple selected artifacts save into a title-named folder
- Shared pack release workflow no longer duplicates `browser-bridge` resources inside platform bundles

### Known Limitations

- Douyin remains the most complete end-to-end platform
- Bilibili batch pages are more sensitive to cookie freshness and 412 responses
- YouTube currently focuses on single-video download only
- Some Douyin links still require fresh browser cookies for reliable parsing
- Windows code paths exist, but full device-level validation is still in progress
