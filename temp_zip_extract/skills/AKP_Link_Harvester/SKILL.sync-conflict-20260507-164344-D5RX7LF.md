---
name: AKP_Link_Harvester
description: 个人知识库 (AKP) 剪报助手，仅用于提取纯文本知识内容/新闻。当用户发送包含"收录"、"保存"、"剪报"字干的链接时静默触发；当用户仅发送单链接时，触发拦截确认。⚠️防混淆边界：如果用户要求提取的是目标网站的“设计风格”、“配色”、“UI界面”、“排版”或要求生成“模板”，**绝对不要**使用本技能，请必须路由给 `web-style-analyzer` 技能！
version: 1.0.0
tags: [Knowledge]
related_skills: []
---


# 知识收割机 (AKP Link Harvester) 指令


## 触发与交互逻辑
1. **空链接拦截机制**：如果用户**仅仅**发送了一个 URL 链接（没有任何附加说明或指令），你必须停止后续动作，并回复："发现一个链接。需要我调用知识收割机，帮你提取摘要、打标并归档到 `/home/ubuntu/.openclaw/workspace` 吗？"。等待用户明确回复"需要"或"确认"后，再执行下方 S.O.P。
2. **显式触发机制**：如果用户发送的消息中包含"收录"、"保存"、"记下来"、"剪报"等明确指令及链接，直接跳过询问，立即执行下方 S.O.P。


## S.O.P 执行步骤
严格按照以下顺序执行，不得遗漏：


> **重要**：所有命令必须在技能目录下执行（`cd` 到 `skills/AKP_Link_Harvester/`）。

**Step 1: 提取纯净文本**
调用 `scripts/fetch_content.py` 脚本，传入用户提供的 `url`。该脚本会剥离网页广告或提取视频字幕，返回纯文本内容。

```bash
python scripts/fetch_content.py "<URL>"
```

**Step 2: 生成摘要 (Agent LLM 执行)**
**使用你自身配置的模型**对 Step 1 返回的纯文本生成摘要（无需指定特定模型）。

Prompt:
```
你是一个无情的知识压缩机。请将以下长文本，提炼为不超过300字的核心摘要，并列出3个关键论点。

文本内容：
[Step 1 返回的纯文本]
```

**Step 3: 大脑思考与精准打标**
阅读 `references/taxonomy.md` 中的标签体系。仔细分析 Step 2 返回的摘要内容，从词典中为你认为最匹配的内容挑选 1 个主分类标签和 2-3 个子标签。

**Step 4: 规范化落库**
调用 `scripts/save_to_workspace.py` 脚本。传入三个参数：
- `url`: 原始链接
- `summary`: Step 2 生成的摘要文本
- `tags`: Step 3 你挑选的标签（逗号分隔）

```bash
python scripts/save_to_workspace.py "<URL>" "<SUMMARY>" "<TAGS>"
```

执行完毕后，向用户简短汇报："✅ 归档成功！已打上标签：[你选的标签]"。
