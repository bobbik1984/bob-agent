---
name: note_graphify
description: 批量扫描 OneNote/Markdown 笔记，提取实体、主题和时序演化链，增量合并到 I Know 知识图谱中。支持持续更新——每次有新笔记加入，只需重新运行此 Skill。
version: 1.0.0
target_node: X1C (Dev Machine) / VPS3
tags: [Knowledge]
related_skills: []
---

# Note Graphify — 个人笔记图谱化

## Background

I Know 系统已经拥有了系统基建层（VPS、Projects、Skills）的完整图谱。但用户长期积累的个人思考笔记（OneNote 导出的 Markdown）尚未被纳入。

这些笔记横跨 2014–2026 年，记录了从建筑设计、量化交易到 AI 自主系统的认知演化。将它们图谱化后，可以：
1. 在 Live Dashboard 上看到"思想演化链"（早期的想法如何在后期被实现）。
2. 让 Dream Engine 识别"未竟的灵感碎片"，主动建议重新审视。
3. 将个人知识与系统架构形成关联桥接（笔记 `inspired` 项目）。

## Trigger

当用户说类似以下内容时触发：
- "帮我把笔记整理进知识图谱"
- "更新个人知识图谱"
- "扫描我的 OneNote 笔记"
- "graphify my notes"

## Input

| 参数 | 必须 | 默认值 | 说明 |
|------|------|--------|------|
| `notes_dir` | 否 | `data/onenote_export/pages/` | 笔记 Markdown 文件所在目录 |
| `graph_path` | 否 | `ecosystem.graph.json` | 目标图谱文件 |
| `mode` | 否 | `rule` | 提取模式：`rule`(纯规则) 或 `llm`(Gemini 辅助) |

## Execution

运行入口脚本：
```bash
cd /path/to/iknow
python -m note_graphify --notes data/onenote_export/pages/ --graph ecosystem.graph.json --mode rule
```

### 处理流程
1. **扫描**: 读取目录下所有 `*.md` 文件，解析 YAML frontmatter。
2. **去重**: 对比现有图谱，跳过已存在的笔记节点（基于 `note_` 前缀 ID）。
3. **实体提取**: 
   - `rule` 模式：基于标题关键词 + 正文模式匹配。
   - `llm` 模式：调用 Gemini 对每篇笔记做摘要+主题分类+实体识别。
4. **演化链检测**: 按主题对笔记排序，相同主题的笔记按时间顺序串联 `evolved_into` 边。
5. **桥接**: 检测笔记正文中是否提及现有图谱实体（如 VPS、OpenClaw），自动建立 `inspired` / `references` 边。
6. **合并输出**: 增量合并到目标 graph JSON 文件。

## Output
- 更新后的 `ecosystem.graph.json`（含个人笔记节点、主题节点、演化链边）。
- 控制台输出处理摘要（新增节点/边数量）。
