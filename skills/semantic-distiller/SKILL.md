---
name: semantic-distiller
description: "Ultra-long-text semantic compression engine. Make sure to use this skill whenever the research pipeline has accumulated over 10,000 characters of raw web content, articles, or reports that need to be condensed before being passed to expensive downstream models. This skill extracts high-density factual summaries with source attribution, reducing 50,000+ words of raw data into ~3,000 words of actionable intelligence while preserving all citation URLs."
version: 1.0.0
tags: [Intelligence]
related_skills: [research-planner, grounded_research, mckinsey-consultant]
metadata:
  last_updated: "2026-04-25"
  architecture: "Cost-Optimized Distillation Pipeline"
  upstream: "research-planner (provides Data Blueprint), grounded_research (provides raw data)"
  downstream: "mckinsey-consultant, or any report/analysis generator"
---

# Semantic Distiller

**定位**: DAG 管线中的**语义脱水机**。读取研究员（Researcher）堆积的海量原始文本，根据上游蓝图（Data Blueprint）进行定向提炼，输出极高密度的事实简报。

**核心价值**: 将十几万字的"垃圾堆"浓缩为 3000 字的"黄金简报"，使下游昂贵的推理模型（如 Qwen-Max）永远只需处理精华内容，从根本上杜绝 Token 爆炸。

**输入**: 
1. `workspace/raw_data/*.md` — 研究员抓取的原始网页文件
2. `workspace/Data_Blueprint.json` — 上游 research-planner 输出的数据筐定义（可选，若无则自行判断重点）

**输出**: `workspace/research_brief.md` — 带来源引用的高密度事实简报

---

## ⚠️ 行为规则

| # | 规则 | ✅ 正确 | ❌ 禁止 |
|:--|:-----|:--------|:--------|
| 1 | 只读不搜 | 只读取本地文件，不调用任何搜索工具 | 调用 web_search / web_fetch |
| 2 | 保留出处 | 每条事实必须标注 `[来源: URL]` | 输出无来源的裸数据 |
| 3 | 框架对齐 | 严格按 Data Blueprint 的数据筐分类提取 | 偏离蓝图自行发散 |
| 4 | 长度控制 | 总输出严格控制在 3000 字以内 | 输出冗长的重复信息 |
| 5 | 事实优先 | 只提取可验证的硬核事实和数据 | 保留原文中的修辞、广告、导航 |
| 6 | 去重交叉 | 多源相同数据取最权威来源 | 同一事实重复出现多次 |

---

## 🎯 执行流程

### STEP 1: 加载输入 (注意 32K 上下文切片限制)

1. 读取 `workspace/Data_Blueprint.json`（如存在）。
   - 提取 `data_gaps[]` 作为提炼的分类维度。
2. 扫描 `workspace/raw_data/` 目录，列出所有 `.md` / `.txt` 文件。
3. **⚠️ 32K 窗口切片逻辑**: 由于你（Distiller 模型）的上下文窗口为 32,000 Token，你**绝对不能**一次性使用 `cat` 或 `read` 将所有原始文件全部读入内存！你必须：
   - 逐个读取文件（每次读取单个文件）。
   - 提取该文件中的事实，存入临时的本地结果文件中。
   - 清理上下文，再读取下一个文件。

### STEP 2: 定向提炼

对每篇原始文本执行以下操作：

1. **识别来源**: 从文件顶部提取 `Source: URL`（web_fetch 默认会写入）。
2. **筐匹配**: 对照 Data Blueprint 的每个 `gap_id`，判断这篇文章能填补哪个数据筐。
3. **事实抽取**: 仅提取以下类型的信息：
   - 具体数字（金额、百分比、年份）
   - 具名案例（项目名、公司名、国家名）
   - 权威观点（附带发言人身份）
   - 时间线节点（关键事件及日期）
4. **溯源标注**: 每条事实末尾追加 `[来源: URL]`。

### STEP 3: 去重与整合

1. 将所有筐的提取结果汇总。
2. 对重复事实进行交叉验证：
   - 如果多个来源报告相同数字 → 保留最权威的来源（如官方 > 媒体 > 博客）。
   - 如果数据存在矛盾 → 保留两方数据并标注差异。
3. 按 Data Blueprint 的 `gap_id` 顺序排列。

### STEP 4: 生成简报

输出 `research_brief.md`，格式如下：

```markdown
# 研究简报: [核心议题]

生成时间: YYYY-MM-DD
数据来源: X 篇原始文档
蓝图对齐: Data_Blueprint.json

---

## G1: [数据筐1标题]

- [事实1具体内容] [来源: https://...]
- [事实2具体内容] [来源: https://...]
- **关键数字**: [核心指标] [来源: https://...]

## G2: [数据筐2标题]

- [事实3具体内容] [来源: https://...]
- ...

## G3: [数据筐3标题]

- ...

---

## 附录: 全部数据来源

1. [标题/域名] — URL
2. [标题/域名] — URL
...
```

---

## 📏 输出质量标准

| 指标 | 目标值 |
|:-----|:------|
| 总字数 | 2000-3500 字 |
| 来源覆盖率 | ≥80% 的原始文件被引用 |
| 溯源标注率 | 100%（每条事实必须有 URL） |
| 数据筐填充率 | ≥90%（每个筐至少 2 条事实） |
| 重复率 | <5%（同一事实不重复出现） |

---

## 💡 使用示例

**场景**: 研究员已抓取了 5 篇关于一带一路的网页，存入 `workspace/raw_data/`

**Input files**:
```
workspace/
├── Data_Blueprint.json (from research-planner)
└── raw_data/
    ├── source_01_yidaiyilu_gov.md  (15000 chars)
    ├── source_02_worldbank.md       (12000 chars)
    ├── source_03_reuters.md         (8000 chars)
    ├── source_04_brookings.md       (11000 chars)
    └── source_05_imf_report.md      (9000 chars)
```

**Output** (`workspace/research_brief.md`):
```markdown
# 研究简报: 一带一路十年成果与挑战

## G1: 总体投资规模
- 截至2023年底，中国已与150+国家签署合作文件，累计投资超1万亿美元 [来源: https://www.yidaiyilu.gov.cn/...]
- 中老铁路全长1035公里，总投资59.5亿美元，2021年12月通车 [来源: https://www.worldbank.org/...]
...
```
