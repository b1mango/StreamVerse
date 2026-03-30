# StreamVerse 项目维护规则

这份规则基于 `Antigravity-settings` 中“阶段沉淀、规则外置、跨会话恢复”的思路做了项目化适配，目标是让 `StreamVerse` 在持续迭代和开源协作中保持可恢复、可展示、可维护。

## 1. 阶段性沉淀

每完成一个有用户可感知变化的小阶段，至少同步一份长期文档，而不是只留在聊天记录里。

优先更新这些文件之一：

- [README.md](../README.md)
- [CHANGELOG.md](../CHANGELOG.md)
- [docs/roadmap.md](roadmap.md)
- [docs/maintainer-context.md](maintainer-context.md)

## 2. 维护上下文外置

跨会话恢复时，优先依赖仓库内文档，而不是只依赖临时对话上下文。

- 长期方向放在 [docs/roadmap.md](roadmap.md)
- 当前真实状态、架构和待办放在 [docs/maintainer-context.md](maintainer-context.md)
- 面向外部的项目说明放在 [README.md](../README.md)

## 3. 规则变更要联动文档

如果修改了工作流、下载行为、支持范围或贡献方式，不要只改一处说明。

至少联动检查：

- `README` 是否仍然准确
- `CHANGELOG` 是否需要补一条
- `roadmap` 与 `maintainer-context` 是否需要更新

## 4. 项目内不保存敏感运行态

以下内容只允许保留在本机运行环境，不写入仓库：

- 浏览器 Cookie
- 登录态文件
- 真实下载产物
- 带敏感信息的日志
- 本地缓存和临时目录

## 5. 规则适配边界

`Antigravity-settings` 的原始规则包含“把记忆同步到外部记忆仓库”的流程；对 `StreamVerse` 的适配版本不默认这么做。

在本项目中：

- 默认把维护信息沉淀在仓库自己的 `docs/` 文档里
- 只有当用户明确要求“保存记忆”或“同步到 Antigravity-settings”时，才执行外部记忆仓库流程

## 6. 开源展示优先级

对外展示时，优先保证这三件事：

1. 仓库首页能快速说明项目能做什么、不能做什么
2. 新贡献者能在 5 分钟内找到启动方式和改动入口
3. 维护者能在新会话里快速恢复当前状态与下一步重点
