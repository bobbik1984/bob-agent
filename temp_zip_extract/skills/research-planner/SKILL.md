---
name: research-planner
description: "Research architecture specialist. Make sure to use this skill whenever a user requests a deep research report, competitive analysis, industry briefing, presentation deck, or any structured deliverable requiring multi-source data collection. This skill generates a Data Blueprint (structured JSON) that defines exactly what data gaps need to be filled BEFORE any web search or data collection begins, preventing aimless research loops and token explosion."
version: 1.0.0
tags: [Workflow]
related_skills: [semantic-distiller, mckinsey-consultant, grounded_research, brainstorming]
metadata:
  last_updated: "2026-04-25"
  architecture: "Blueprint-Driven Research Pipeline"
  downstream: "semantic-distiller → mckinsey-consultant (or any report generator)"
---

# Research Planner

**定位**: DAG 管线中的**前置架构师**。在任何搜索动作开始之前，先产出一份结构化的"数据蓝图（Data Blueprint）"，为下游的 Researcher 和 Distiller 设定明确的搜索边界和验收标准。

**核心理念**: 麦肯锡的"Ghost Deck"法 —— 先画空架子，再定向找数据填空。绝不盲目撒网。

**输入**: 用户的原始命题（自然语言）
**输出**: `Data_Blueprint.json`（结构化 JSON）

---

## ⚠️ 行为规则

| # | 规则 | ✅ 正确 | ❌ 禁止 |
|:--|:-----|:--------|:--------|
| 1 | 不搜索 | 纯逻辑推演，零工具调用 | 调用 web_search 或 web_fetch |
| 2 | 框架驱动 | 使用 MECE / PEST / SWOT 等分析框架拆解命题 | 凭感觉罗列零散问题 |
| 3 | 可验证 | 每个数据筐必须定义具体的验收标准 | 模糊要求如"尽量详细" |
| 4 | 精简 | 3-5 个数据筐，不超过 7 个 | 拆得太碎导致搜索效率低下 |
| 5 | 页面预估 | 为最终报告预估合理页数和内容密度 | 不考虑下游交付物的容量 |

---

## 🎯 执行流程

### STEP 1: 命题解构

接收用户的原始请求，识别：
- **核心议题（Core Question）**: 用户想要回答的终极问题
- **利益相关方（Stakeholders）**: 这份报告的受众是谁
- **交付格式（Deliverable Format）**: PPT / 报告 / 简报 / 仪表盘

### STEP 2: 框架选择

根据命题性质选择最合适的分析框架：
- **行业分析** → PEST + Porter's Five Forces
- **战略决策** → SWOT + MECE
- **竞品对比** → Feature Matrix + Value Chain
- **趋势研判** → Timeline + Scenario Planning
- **投资研究** → Financial Metrics + Risk Assessment

### STEP 3: 数据筐定义（Data Gaps）

将分析框架拆解为 3-5 个"数据筐"。每个筐必须包含：

```json
{
  "data_blueprint": {
    "core_question": "用户的核心议题",
    "target_pages": 15,
    "analysis_framework": "MECE + PEST",
    "data_gaps": [
      {
        "gap_id": "G1",
        "intent": "证明什么结论 / 回答什么子问题",
        "required_metrics": ["具体需要的数据类型，如年份、金额、案例名"],
        "suggested_sources": ["推荐的搜索方向，如官方网站、世界银行报告"],
        "stop_condition": "当获取到 X 条具体数据点时即可停止",
        "priority": "high / medium / low"
      }
    ],
    "report_skeleton": [
      {"page": 1, "type": "cover", "title": "报告标题"},
      {"page": 2, "type": "scqa", "title": "问题定义", "data_source": "G1"},
      {"page": 3, "type": "data", "title": "数据页标题", "data_source": "G2"}
    ]
  }
}
```

### STEP 4: 输出蓝图

将 `Data_Blueprint.json` 写入 workspace，供下游节点读取。

**关键约束**：
- `stop_condition` 必须是可量化的（如"至少 3 个案例"、"至少 5 年的时间序列数据"）
- `report_skeleton` 必须与 `target_pages` 匹配
- 每个 data page 必须关联至少一个 `gap_id`

---

## 📊 预估法则（Rule of Thumb）

基于经验数据的信息密度漏斗：

| 阶段 | 文字量 | 说明 |
|:-----|:------|:-----|
| 终点（Slides 文字） | ~1500 字 | 15 页幻灯片上的核心文字 |
| 中转（研究简报） | ~3000 字 | 文员蒸馏后的高密度事实 |
| 源头（原始抓取） | ~50000-80000 字 | 研究员需要抓取的网页原文总量 |

**换算**：每个数据筐预估 1-2 万字原始抓取量 → 蒸馏后约 600-800 字 → 最终映射到 2-3 页幻灯片。

---

## 💡 使用示例

**Input**: "帮我分析一带一路倡议十年的成果与挑战，做15页报告"

**Output** (Data_Blueprint.json):
```json
{
  "data_blueprint": {
    "core_question": "一带一路倡议十年：成果几何，挑战何在？",
    "target_pages": 15,
    "analysis_framework": "PEST + Timeline",
    "data_gaps": [
      {
        "gap_id": "G1",
        "intent": "量化十年总体投资规模与覆盖范围",
        "required_metrics": ["累计投资总额（美元）", "覆盖国家数量", "重点基建项目清单（至少5个）"],
        "suggested_sources": ["一带一路官网", "世界银行报告", "商务部统计"],
        "stop_condition": "获取到总投资额 + 至少5个具体项目案例即可停止",
        "priority": "high"
      },
      {
        "gap_id": "G2",
        "intent": "评估对东盟/中亚经济体的实际拉动效应",
        "required_metrics": ["GDP增长对比数据", "就业创造数据", "贸易额变化"],
        "suggested_sources": ["IMF报告", "ADB报告", "各国统计局"],
        "stop_condition": "获取到至少3个国家的前后对比数据",
        "priority": "high"
      },
      {
        "gap_id": "G3",
        "intent": "识别主要风险与批评声音",
        "required_metrics": ["债务陷阱案例", "环境争议", "地缘政治摩擦"],
        "suggested_sources": ["CSIS", "Brookings", "Reuters"],
        "stop_condition": "获取到至少2个负面案例和1个中立评估",
        "priority": "medium"
      }
    ]
  }
}
```
