---
name: grounded_research
description: "基于 Gemini Grounding 的深度研究技能 (Tier 1)。当环境中存在 gemini-cli 时，通过 CLI 调用 Google Search Grounding 实现 AI 原生搜索+推理一体化；否则降级为 web_search + web_fetch 手动管道。提供带来源引用的综合分析报告。"
user-invocable: true
version: 2.0.0
tags: [Intelligence]
related_skills: [web_search]
---

# Grounded Research — AI 原生深度研究 (Tier 1)

当你需要对复杂话题进行**深度研究、综合分析、多源对比**时，使用此技能。

> **与 `web_search` 的区别**：`web_search` 是 Tier 2 底层数据提供者（返回链接和摘要），需要你自己去读、去分析。`grounded_research` 是 Tier 1 高级研究员——它把搜索、阅读、分析、引用的全流程一步完成，直接交给你带引用的成品报告。

---

## ⚡ 环境感知路由

本技能的执行方式取决于宿主机的环境能力（由 `env_sniffer` 在启动时探测）：

### 模式 A: 内置工具直调（首选 — 当 `gemini_grounding` 工具可用时）

**前提**：System Prompt 中的 `<environment_capabilities>` 显示 `gemini(可用)`。

**执行方式**：直接调用内置的 `gemini_grounding` 工具，传入研究问题即可：

```
工具调用: gemini_grounding
参数: {"query": "你的研究问题", "language": "zh"}
```

> ✅ **无需使用 bash**！`gemini_grounding` 是一个独立的内置工具，内部安全地调用 gemini-cli，
> 不存在引号转义、路径解析等问题。直接传 query 字符串即可。

**优势**：
- 搜索+推理+引用一步完成，主 Agent 无需消耗 Token 去读网页
- 利用 Gemini 的原生 Grounding 能力，引用质量远高于手动爬取
- 每日 1500 次免费 Grounding 配额（Google AI Studio 免费层）
- 无 shell 转义风险（底层使用 subprocess.run(List) 直传参数）

### 模式 B: 手动管道（降级 — 当 CLI 不可用时）

**前提**：System Prompt 中的 `<environment_capabilities>` 显示 `gemini(不可用)`。

**执行方式**：退回到传统的 `web_search` → `web_fetch` → 人工分析管道：
1. 使用 `web_search` 收集相关链接
2. 使用 `web_fetch` 逐个抓取网页全文
3. 自行阅读、交叉验证、生成报告
4. 为每个事实附上 `[来源: URL]`

> ⚠️ 此模式消耗大量主 Agent Token，仅在模式 A 不可用时使用。

### 模式 C: 蓝图驱动流水线 (Blueprint-Driven Pipeline)

如果你的任务描述中包含了来自上游的 `Data_Blueprint.json` 或明确要求你进行**原始数据抓取 (Data Crawling)**：
1. **绝不总结**: 你只是一个无情的爬虫。下游有专门的 `semantic-distiller` 会处理你的数据。
2. **存盘优先**: 每次调用 `web_fetch` 获取到长文本后，**必须立即**使用 `write_to_file` 将其完整内容保存到 sandbox！
   - 路径规范：`workspace/sessions/{session_id}/sandbox/raw_data/source_01_domain.md`
   - 文件头部必须包含：`# [网页标题]` 和 `Source: [URL]`
3. **HTTP 403 兜底**: 如果 `web_fetch` 对某个 URL 返回 403/404，不要放弃！
   - 从 `web_search` 的搜索结果摘要中提取关键事实
   - 将摘要内容也存盘，标注 `[来源: 搜索摘要, 原始URL不可访问: {url}]`
   - 尝试使用替代 URL（如 web.archive.org 的存档版本）
4. **查缺补漏**: 对照蓝图中的 `data_gaps` 和 `stop_condition`，循环搜索-抓取-存盘，直至满足停止条件。
5. **移交报告**: 你的 `relay_summary` 只需报告"我已经抓取了 N 篇文章并存入 sandbox/raw_data 目录"，**严禁**在 summary 里默写文章内容！

---

## 📌 注意事项

- **配额管理**: Gemini Grounding 免费配额有限（每日 ~1500 次），请确保搜索词足够精准，避免无效搜索。
- **隐私保护**: 不要将用户的敏感信息或私有代码发送到联网搜索。
- **合规性**: 严禁通过 Cookie 劫持访问 Google 服务。所有 CLI 调用均使用官方 API Key。
- **模型回退**: 如果联网搜索失败，请告知用户并尝试基于现有知识库（RAG）回答。

---

## 调用示例

> "帮我搜一下 DeepSeek R1 最近在 GitHub 上的更新，并整理一份简报。"
> "调查一下 2026 年 3 月全球半导体市场的最新动态。"
> "对比分析 Tauri 2 和 Electron 在 2026 年的生态成熟度。"
