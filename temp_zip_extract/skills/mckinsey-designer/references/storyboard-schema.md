# Storyboard JSON Schema

> 此文件定义 mckinsey-consultant 输出、mckinsey-designer 消费的 **Storyboard JSON 接口规范**。

---

## 顶层结构

```jsonc
{
  "meta": {
    "title": "项目名称",
    "date": "2026-04-22",
    "author": "分析师姓名",
    "brand": "mckinsey",          // mckinsey | bcg | bain | roland_berger
    "confidential": true
  },
  "slides": [
    { /* Slide Object */ }
  ]
}
```

---

## Slide Object 通用字段

每个 slide 必须包含 `type` 和 `title`，其他字段按类型可选。

```jsonc
{
  "type": "cover",                // 必填: slide 类型
  "title": "完整句子的 Action Title", // 必填: 主标题
  "subtitle": "",                 // 可选: 副标题
  "insight": "",                  // 可选: "So What?" 洞察框文字
  "notes": ""                     // 可选: 演讲者备注（不渲染到 slide）
}
```

---

## Slide Types 详解

### 1. `cover` — 封面

```jsonc
{
  "type": "cover",
  "title": "AI赋能量化交易研究",
  "subtitle": "2026年4月 · 内部研讨",
  "image": {                      // 可选: 背景图
    "source": "unsplash",         // unsplash | icon | generate | url
    "query": "artificial intelligence trading"  // unsplash 搜索词
  }
}
```

### 2. `divider` — 章节过渡页

```jsonc
{
  "type": "divider",
  "title": "第二章：多智能体架构",
  "subtitle": "从单体模型到协同系统的范式转型",
  "icon": "network"               // 可选: Lucide icon 名称
}
```

### 3. `scqa` — SCQA 叙事框架

```jsonc
{
  "type": "scqa",
  "title": "量化交易正从静态代码演进为自主协同系统",
  "situation": "2024年生成式AI金融投资达339亿美元，LLM已成为推理工具。",
  "complication": "单体模型面对复杂量化任务容易注意力稀释，成本失控。",
  "question": "如何在保持分析质量的同时控制推理成本？",  // 可选
  "answer": "全面转向多智能体架构(MAS)，引入 MCP 协议。",
  "bullets": [                    // 可选: 右侧论据列表
    "研发流程：多角色智能体网络自动辩论",
    "因子挖掘：大模型驱动的经济学逻辑自进化",
    "基础设施：MCP 打破数据孤岛"
  ],
  "insight": "不拥抱 Agentic 工作流的团队将在迭代速度上落后数个数量级。"
}
```

### 4. `data` — 数据图表页

```jsonc
{
  "type": "data",
  "title": "LLM 框架在抗衰减能力上显著优于传统方法",
  "layout": "left-chart-right-text",  // left-chart-right-text | full-chart | top-chart-bottom-text
  "chart": {
    "type": "bar",                // bar | line | waterfall | pie | donut | radar | hbar
    "title": "框架对比",           // 图表内标题
    "data": [
      { "label": "传统 GP", "value": 35 },
      { "label": "LLM 框架", "value": 92, "highlight": true }
    ]
  },
  "bullets": [
    "统计 + 经济学双目标优化",
    "智能正则化实现抗衰减"
  ],
  "source": "数据来源: XYZ Research, 2025",
  "insight": "任何新因子必须通过 LLM 可解释性审查。"
}
```

**chart.data 高级格式（多系列）**：

```jsonc
{
  "chart": {
    "type": "line",
    "title": "收益趋势",
    "categories": ["Q1", "Q2", "Q3", "Q4"],
    "series": [
      { "name": "策略 A", "data": [12, 18, 25, 30] },
      { "name": "策略 B", "data": [8, 15, 22, 35], "highlight": true }
    ]
  }
}
```

### 5. `matrix` — 矩阵 (2×2)

```jsonc
{
  "type": "matrix",
  "title": "企业应聚焦明星业务投入",
  "axis_x": "相对市场份额 (High → Low)",
  "axis_y": "市场增长率 (High → Low)",
  "quadrants": [
    {
      "position": "top-left",
      "title": "⭐ Stars",
      "content": "市场前景广阔，持续投入。",
      "kpi": "+95% 增长",
      "accent": true
    },
    {
      "position": "top-right",
      "title": "❓ Question Marks",
      "content": "高增长低份额，评估投资。",
      "kpi": "待观察"
    },
    {
      "position": "bottom-left",
      "title": "🐄 Cash Cows",
      "content": "稳定现金流，维持运营。",
      "kpi": "100% 份额"
    },
    {
      "position": "bottom-right",
      "title": "🐕 Dogs",
      "content": "消耗资源，逐步退出。",
      "kpi": "30% ↓"
    }
  ],
  "insight": "Q3 前完成瘦狗业务剥离，释放资源给明星象限。"
}
```

### 6. `comparison` — 对比表

```jsonc
{
  "type": "comparison",
  "title": "LLM 因子发现框架显著优于传统方法",
  "columns": ["维度", "传统量化", "LLM 框架"],
  "rows": [
    ["优化目标", "纯统计指标", { "text": "统计+经济学逻辑", "highlight": true }],
    ["抗衰减", "弱，模式拥挤", { "text": "强，智能正则化", "highlight": true }],
    ["审计治理", "黑盒特征", { "text": "全链路文本追踪", "highlight": true }]
  ],
  "insight": "关键发现：拒绝仅凭回测跑出来的因子。"
}
```

### 7. `columns` — N 栏等分

```jsonc
{
  "type": "columns",
  "title": "MAS 通过专业分工抑制确认偏误",
  "items": [
    {
      "title": "1. 分析师集群",
      "icon": "search",
      "content": "基本面、情绪、技术分析师并发拉取数据。"
    },
    {
      "title": "2. 结构化辩论",
      "icon": "swords",
      "content": "看多/看空 Debater 通过对抗消除偏差。",
      "accent": true
    },
    {
      "title": "3. 执行与风控",
      "icon": "shield-check",
      "content": "交易员下单，风控行使否决权。"
    }
  ],
  "insight": "架构设计需从 Prompt 调优转向组织动力学。"
}
```

### 8. `summary` — 执行摘要

```jsonc
{
  "type": "summary",
  "title": "执行摘要",
  "peak": "AI 多智能体架构是量化交易的下一代基础设施",
  "arguments": [
    "MAS 实现专业分工，抑制单模型认知偏差",
    "LLM 因子发现在抗衰减和可解释性上显著优于 GP",
    "MCP 协议打破数据孤岛，实现实时数据融合"
  ],
  "insight": "2026年底前完成核心系统向 Agentic 架构的迁移。"
}
```

### 9. `diagram` — 流程图/架构图

```jsonc
{
  "type": "diagram",
  "title": "三节点集群实现大脑、情报与执行的专业分工",
  "diagram_type": "flowchart",    // flowchart | sequence | class | state | gantt | mindmap
  "direction": "TD",              // TD | LR | RL | BT (仅 flowchart)
  "mermaid": "graph TD\n  A[\"用户请求\"] --> B{\"任务类型\"}\n  B -->|\"情报\"| C[\"VPS2\"]\n  B -->|\"代码\"| D[\"VPS3\"]",
  "insight": "分布式架构使任务路由延迟降低至 <200ms"
}
```

**注意**：`mermaid` 字段的值是纯 Mermaid DSL 代码，**不包含** `%%{init}%%` 主题头——Designer 会根据 `meta.brand` 自动注入品牌主题。

---

## 图片字段 (`image`) 规范

```jsonc
{
  "image": {
    "source": "unsplash",         // unsplash | icon | generate | url
    "query": "business meeting",  // source=unsplash 时的搜索词
    "name": "bar-chart",          // source=icon 时的 Lucide 图标名
    "prompt": "flat vector...",   // source=generate 时的生成提示
    "url": "https://...",         // source=url 时的直接地址
    "alt": "商务会议场景"          // 无障碍描述
  }
}
```

---

## 验证规则

Designer 在解析 Storyboard 时应验证：

1. ✅ `meta.brand` 是四个合法值之一
2. ✅ 每个 slide 有 `type` 和 `title`
3. ✅ `type: "data"` 的 slide 必须包含 `chart` 字段
4. ✅ `chart.data` 数组不为空
5. ✅ `type: "matrix"` 恰好有 4 个 quadrants
6. ✅ `type: "diagram"` 必须包含 `mermaid` 字段且非空
7. ✅ `type: "diagram"` 的 `mermaid` 代码通过 `validate_mermaid.py` 验证
8. ⚠️ 缺少 `insight` 时自动添加占位: "[ 待补充洞察 ]"
