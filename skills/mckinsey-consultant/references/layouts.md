# McKinsey 页面布局库

> 所有布局均标注了对应的 **Storyboard JSON `type`**，供 STEP 5 生成 JSON 时直接使用。

---

## 1. 封面页

**Storyboard type**: `cover`
**视觉意图**: `image:unsplash`（推荐横版商务/科技场景）

**结构**:
- 全屏背景图（Unsplash）+ 半透明遮罩
- 居中: 主标题 + 副标题（日期/场合）
- 底部: 品牌 logo + 机密标记

**适用**: 报告首页

---

## 2. 章节过渡页

**Storyboard type**: `divider`
**视觉意图**: `icon:lucide`（章节主题图标）或 `none`（纯色背景）

**结构**:
- 品牌色纯色背景
- 居中: 章节编号 + 标题
- 可选: Lucide 图标装饰

**适用**: 章节之间的视觉分隔

---

## 3. SCQA 叙事页

**Storyboard type**: `scqa`
**视觉意图**: 通常 `none`（文字驱动），复杂场景可配 `icon:lucide`

**结构**:
- 顶部: 论点式 Action Title
- 左侧: Situation → Complication → Question → Answer 叙事弧
- 右侧: MECE 论据列表（Bullets）

**适用**: 问题定义、执行摘要、核心论点阐述

---

## 4. 数据图表页

**Storyboard type**: `data`
**视觉意图**: `chart:bar` / `chart:line` / `chart:waterfall` / `chart:pie` / `chart:radar` / `chart:hbar`

**结构**:
- 顶部: 论点式 Action Title
- 中部: ECharts 图表（占 60-70% 空间）
- 侧边/底部: 解读文字 + 数据来源
- 底部: Insight Box

**布局变体**:
| layout 值 | 说明 |
|:----------|:-----|
| `left-chart-right-text` | 左图右文（默认） |
| `full-chart` | 图表全宽，文字在下 |
| `top-chart-bottom-text` | 上图下文 |

**适用**: 趋势展示、数值对比、市场份额、增长拆解
**图表选择速查**:
```
趋势/时间序列 → line
数值对比/排名 → bar / hbar
增长拆解/变化归因 → waterfall
构成/比例 → pie / donut
多维评估 → radar
```

---

## 5. 2×2 矩阵页

**Storyboard type**: `matrix`
**视觉意图**: `none`（矩阵本身就是视觉元素）

**结构**:
- 顶部: 论点式 Action Title
- X/Y 轴标签
- 4 象限: 标题 + 内容 + 可选 KPI
- 底部: Insight Box

**适用**: 战略定位、BCG 矩阵、优先级排序
**示例**: 竞争强度×盈利潜力, 增长率×市场份额

---

## 6. 对比表格页

**Storyboard type**: `comparison`
**视觉意图**: `none`（表格本身就是信息载体）

**结构**:
- 顶部: 论点式 Action Title
- 多维度对比表格（深色表头 + 高亮优势项）
- 底部: Insight Box

**适用**: 多维度对比、竞品分析、方案评估

---

## 7. N 栏等分页

**Storyboard type**: `columns`
**视觉意图**: `icon:lucide`（每栏顶部图标）或 `none`

**结构**:
- 顶部: 论点式 Action Title
- 2-4 栏并列，每栏: 图标 + 小标题 + 内容
- 可选: 高亮某一栏（accent）

**适用**: 并列特性展示、步骤流程、团队分工

---

## 8. 流程/架构图页

**Storyboard type**: `diagram`
**视觉意图**: `diagram:flowchart` / `diagram:sequence` / `diagram:class` / `diagram:state` / `diagram:mindmap`

**结构**:
- 顶部: 论点式 Action Title
- 中部: Mermaid 流程图/架构图（品牌主题注入）
- 底部: Insight Box

**适用**: 系统架构、业务流程、状态转换、因果关系、决策树
**典型场景**:
```
系统通信/API 调用链 → flowchart / sequence
数据模型/类关系 → class
生命周期/状态机 → state
主题拆解/脑图 → mindmap
```

---

## 9. 执行摘要页

**Storyboard type**: `summary`
**视觉意图**: `none`（核心论点页，文字驱动）

**结构**:
- 顶部: Peak 核心结论（一句话）
- 中部: 3 个 MECE Arguments（支撑金字塔）
- 底部: Insight Box（行动号召）

**适用**: 报告倒数第 2 页（最后生成）

---

## 布局选择决策树

```
需要展示趋势/数据?      → 数据图表页 (data)
  └─ 趋势 → chart:line
  └─ 对比 → chart:bar
  └─ 拆解 → chart:waterfall
  └─ 构成 → chart:pie
  └─ 多维 → chart:radar

需要展示流程/架构?      → 流程架构图页 (diagram)
  └─ 业务流程 → diagram:flowchart
  └─ 调用时序 → diagram:sequence
  └─ 数据模型 → diagram:class

需要对比分析?           → 对比表格页 (comparison) 或 左右分栏 (scqa)
需要战略定位?           → 2×2矩阵页 (matrix)
需要并列展示?           → N栏等分页 (columns)
需要叙事论证?           → SCQA 页 (scqa)
需要总结洞察?           → 执行摘要页 (summary)
```

---

## Visual Intent 标注规范

在 Dummy Pages (STEP 5) 中，每页必须标注视觉意图：

| 标注 | 含义 | 渲染资产 |
|:-----|:-----|:---------|
| `chart:bar` | 柱状图 | ECharts |
| `chart:line` | 折线图 | ECharts |
| `chart:waterfall` | 瀑布图 | ECharts |
| `chart:pie` / `chart:donut` | 饼图/环形图 | ECharts |
| `chart:radar` | 雷达图 | ECharts |
| `chart:hbar` | 横向条形图 | ECharts |
| `diagram:flowchart` | 流程图 | Mermaid |
| `diagram:sequence` | 时序图 | Mermaid |
| `diagram:class` | 类图 | Mermaid |
| `diagram:state` | 状态图 | Mermaid |
| `diagram:mindmap` | 思维导图 | Mermaid |
| `image:unsplash` | Unsplash 背景图 | Unsplash API |
| `icon:lucide` | Lucide 图标装饰 | Lucide CDN |
| `none` | 纯文字/表格 | 无额外资产 |

### Visual Balance 审视（内容驱动，无硬性比例）

- 审视全文：是否有适合图表化的数据被遗漏为纯文字？
- 审视全文：是否有适合流程图化的架构/流程描述被遗漏？
- 封面 **推荐** 有 `image:unsplash`
- 章节过渡页 **推荐** 有 `icon:lucide`
- **无固定比例要求**：纯论证型报告可以全文无图；数据密集型报告可以多数页有图——由主题内容决定
