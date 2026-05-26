---
name: optimizing-tokens
description: Token optimization: minimize context reads (grep>full-file), concise output (JSON>prose), batch operations, self-check before every action
---
# Optimizing Tokens

减少 token 使用量和成本的综合工具包。

## 核心原则

### 1. 最小化上下文读取
- 只读必要的文件区域，不要读取整个文件
- 使用 search_content 定位再精确查看，而非浏览整文件
- 避免重复读取已知内容

### 2. 精简输出
- **JSON 优于 prose**：结构化数据用 JSON 格式返回
- **表格优于段落**：比较性信息用表格展示
- **列表优于描述**：枚举项用列表而非段落
- 省略客套话和填充短语
- 不要重复用户已知的信息

### 3. 批量操作
- 合并可并行的工具调用到同一批次
- 合并相关的文件编辑到 multi_edit
- 一次性收集所有需要的上下文，不要分多次

### 4. 直接性优先
- 直接给出答案，不要铺垫
- 如果已知解决方案，直接实施
- 仅在必要时提问

## 自检清单

在每次操作前问自己:
- [ ] 这个文件真的需要完整读取吗？
- [ ] 能否通过搜索直接定位？
- [ ] 这些工具调用能否合并？
- [ ] 输出中是否有冗余信息？
- [ ] 是否在重复已知的上下文？
