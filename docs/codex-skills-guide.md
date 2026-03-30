# Codex Skills 清单与使用教程

这份文档整理了当前机器上已安装到 Codex 的技能清单，以及日常使用这些技能的推荐方式。

## 当前安装情况

- 技能目录：[~/.codex/skills](/Users/b1mango/.codex/skills)
- 可直接使用的技能数量：29 个
- 说明：`.system` 属于 Codex 的系统技能目录，通常不需要手动调用，这里不计入清单
- 来源拆分：
  - Codex 内置技能：4 个
  - 从 `b1mango/Agent-Skills` 仓库安装的技能：25 个

## 怎么在 Codex 里使用技能

### 1. 直接点名技能

最稳的方式是在提问时直接写技能名。

```text
$playwright 帮我打开本地页面并截图
$frontend-design 把下载器首页做得更高级一些
$fix 帮我排查这个 Rust/Tauri 报错
$update-docs 根据刚才的改动更新 README
```

### 2. 用自然语言触发

很多技能也支持自然语言自动触发，比如：

- “帮我 review 这次改动”
- “帮我写一篇公众号文章”
- “帮我生成架构图”
- “帮我测试一下本地 Web 页面”

如果你希望命中率更高，建议在问题里把技能名也写上。

### 3. 多技能组合使用

一个任务可以显式提到多个技能。

```text
$playwright $webapp-testing 帮我检查本地页面的主要交互并截图
$frontend-design $update-docs 先改首页视觉，再补一段使用说明
$code-reviewer $pr-creator 先 review 当前改动，再帮我整理 PR 描述
```

### 4. 新装技能后需要重启

如果是刚安装或刚删除技能，重启 Codex 后识别会更稳定。

## 关键词触发建议

我已经把常用 skill 的 `description` 改成了更容易自动触发的版本，下面这些说法现在更适合作为你的日常口令：

- “做前端”“改页面”“做个界面”“美化一下”“优化 UI” -> `frontend-design`
- “查资料”“搜一下”“去网上找资料”“帮我看看官网”“联网查” -> `web-access`
- “修 bug”“排查一下”“报错了”“不能用”“为什么没反应” -> `fix`
- “review 一下”“帮我看看这次改动”“代码审查”“检查有没有问题” -> `code-reviewer`
- “补文档”“更新 README”“写教程”“同步说明” -> `update-docs`
- “测一下页面”“检查交互”“跑一下前端”“看看按钮能不能点” -> `webapp-testing`
- “打开页面看看”“帮我截图”“模拟点击” -> `playwright`
- “提 PR”“帮我写 PR 文案”“整理提交说明” -> `pr-creator`
- “画个图”“流程图”“架构图”“时序图” -> `codegen-diagram`
- “根据代码生成文档”“写接口文档”“补注释” -> `codegen-doc`
- “有没有这个 skill”“帮我找个技能”“什么 skill 能做这个” -> `find-skills`

如果你愿意更稳一点，直接把 skill 名写出来依然是命中率最高的方式。

## 技能清单

下面按用途分组列出当前可用技能。

### 内置基础技能

- `doc`：处理 `.docx` 文档，适合生成、编辑、排版检查 Word 文件
- `pdf`：处理 PDF 的读取、生成、抽取和版式检查
- `playwright`：用真实浏览器做自动化操作、截图、表单填写和网页调试
- `spreadsheet`：处理 `.xlsx`、`.csv`、`.tsv` 等表格文件

### 代码审查与修复

- `code-review-skill`：结构化代码审查，关注正确性、性能、安全性和可维护性
- `code-reviewer`：更偏正式 PR/代码变更审查，适合 review 本地改动或远端 PR
- `fix`：系统化排查和修复 bug、报错、异常行为
- `frontend-code-review`：专门审查前端文件，包括 `.tsx`、`.ts`、`.js`、`.vue`、`.css`、`.scss`
- `skill-vetter`：审计 skill 是否存在恶意逻辑、越权访问或可疑行为

### 文档、论文与内容写作

- `codegen-doc`：从代码自动生成文档、API 说明或 Markdown 文档
- `paper-write`：辅助学术论文、技术论文、综述等结构化写作
- `update-docs`：根据代码变更同步更新项目文档
- `wechat-article-writer`：写微信公众号文章，支持标题、正文和排版优化
- `geopolitical-deep-analysis`：中文时政与地缘政治深度分析长文写作
- `skill-prompt-convert`：在不同 AI 助手格式之间转换 prompt、skill 或说明文案

### 图表、流程图与演示文稿

- `codegen-diagram`：从代码生成 Mermaid、PlantUML 等架构图
- `drawio-diagram`：创建和编辑 draw.io / diagrams.net XML 图表
- `pptgen-drawio`：基于 draw.io 图和内容生成 PowerPoint 演示文稿

### 前端设计与体验

- `frontend-design`：生成更有设计感、可直接落地的前端页面或组件
- `web-design-guidelines`：按 Web 设计与可用性规范审查 UI/UX
- `vercel-react-best-practices`：基于 Vercel 工程实践优化 React / Next.js 性能与模式
- `webapp-testing`：用 Playwright 测试本地 Web 应用交互、截图、抓日志

### 联网、抓取与网页操作

- `web-access`：统一处理联网任务，包括搜索、网页访问、登录后操作和页面交互
- `scrapling`：高级网页抓取，适合反爬场景、动态页面和批量数据提取

### 技能管理与工作流

- `find-skills`：帮你发现“某类任务有没有现成 skill 可用”
- `skill-create`：创建、修改、优化和评估自定义 skill
- `memory-workflow`：把对话总结保存到指定记忆仓库
- `optimizing-tokens`：尽量减少上下文和 token 消耗，提升执行效率
- `pr-creator`：生成更规范的 PR 内容和提交流程说明

## 推荐的高频用法

### 做前端页面

```text
$frontend-design 为下载器首页做一个更克制但高级的桌面端界面
```

### 检查本地页面功能

```text
$playwright $webapp-testing 打开本地开发页面，检查输入链接、解析按钮和下载流程
```

### 审查改动

```text
$code-reviewer review 当前工作区改动，优先找 bug、回归和缺失测试
```

### 修 bug

```text
$fix 这个 Tauri 命令在点击下载后没有返回结果，帮我排查
```

### 更新文档

```text
$update-docs 根据最近代码改动补全 README 和 roadmap
```

### 生成图表

```text
$codegen-diagram 根据当前项目结构生成一个下载流程图
```

## 适合这个项目的技能组合

当前仓库是一个 `Tauri + Rust + Svelte + TypeScript` 的桌面下载器，最值得优先使用的是下面这些：

- `frontend-design`：做界面和交互升级
- `webapp-testing`：验证前端行为是否符合预期
- `playwright`：自动打开页面、截图、采集调试信息
- `fix`：定位下载、解析、登录相关 bug
- `code-reviewer`：在提交前做一轮审查
- `update-docs`：保持 README 和设计文档同步
- `pr-creator`：整理最终提交说明

## 一个实用工作流

你可以把一个需求拆成下面的连续步骤：

1. 用 `frontend-design` 改页面或交互
2. 用 `webapp-testing` 或 `playwright` 验证功能
3. 用 `fix` 修掉实际发现的问题
4. 用 `code-reviewer` 再做一轮检查
5. 用 `update-docs` 更新说明文档
6. 用 `pr-creator` 生成提交或 PR 描述

## 备注

- 如果你想查看某个 skill 的详细说明，可以直接打开对应目录下的 `SKILL.md`
- 自定义技能默认放在 [~/.codex/skills](/Users/b1mango/.codex/skills)
- 如果后续你还想继续从别的仓库安装技能，建议保留“技能名 + 用途”这种命名方式，后面更容易检索和触发
