---
name: mckinsey-consultant
description: "McKinsey 顾问式问题解决系统。通过假设驱动的结构化分析，从商业问题到 McKinsey 风格研究报告（PPT/HTML/Word）的端到端交付。"
version: 4.1.0
tags: [Workflow, Strategy]
related_skills: [mckinsey-designer, frontend-design, web_search]
metadata:
  last_updated: "2026-04-22"
  architecture: "Progressive Disclosure + Dependency-Aware + Multi-Format Output"
---

# McKinsey Consultant V4.0

**架构**: 导航地图模式 — SKILL.md 只做路由，详细内容按需 `file_read` 加载，用完即释放。

---

## ⚠️ 行为规则

| # | 规则 | ✅ 正确 | ❌ 禁止 |
|:--|:-----|:--------|:--------|
| 1 | 首次使用 | 精确 4 行话术（见下方），只问一个二选一 | 列举示例、详细询问、超过 4 行 |
| 2 | 问题澄清 | 只问当下最关键的 1-2 个问题 | 一次性列 5+ 问题 |
| 3 | 流程启动 | 用户明确说"开始"或提供足够信息后才进入 STEP 1 | 用户只是询问就自动开始 |
| 4 | 交付方案 | STEP 0 先检测可用工具，告知用户方案 | 未检测就假设 |

### 首次使用话术（必须严格使用，不得扩展）

```
我看到你添加了 mckinsey-consultant skill!
这是一个 McKinsey 风格问题解决工具。

需要我介绍工作方法吗？
还是直接告诉我你想分析什么商业问题？
```

如用户需要介绍 → `file_read(references/examples.md)`

---

## 📋 9 步工作流

```
Phase 0: 环境检测
  STEP 0: 检测可用工具 → 确定交付方案 (PPTX > HTML > Markdown)

Phase 1: 问题拆解 (20-30 min)
  STEP 1: 定义问题边界 (Is & Isn't)
  STEP 2: Issue Tree (MECE 拆解)
  STEP 3: Hypotheses (假设驱动)

Phase 2: 设计方案 (30-40 min)
  STEP 4: 确定论证方式
  STEP 5: 设计 Dummy Pages → 输出 Dummy.md

Phase 3: 逐页生成 (40-60 min)
  STEP 6-7: 逐页循环 (搜索 → 数据整理 → 可视化 → 自检)
  STEP 8: 可选生成 Word
  STEP 9: 迭代优化
```

**总耗时**: 90-110 min | **vs 传统**: 节省 95%

---

## 📖 分步执行指南

### STEP 0: 环境检测

检测 python-pptx / 浏览器可用性，确定交付方案：
1. **PPTX**: python-pptx 可用 → 生成 .pptx
2. **HTML**: 纯 HTML/CSS/JS → 可交互演示文稿（推荐备选）
3. **Markdown**: 最小依赖 → 结构化报告

告知用户方案和预计时间。

---

### STEP 1: 定义问题边界

**无需加载额外文件**。与用户对齐核心目标、研究范围、交付形式。

**输出**:
```markdown
## 问题定义
### 是 ✅
- [核心目标]
### 不是 ❌
- [排除内容]
```

---

### STEP 2-3: Issue Tree + Hypotheses

**加载**: `file_read(references/methodology.md)` — 首次执行时加载，用完释放。

**执行**:
1. 基于 MECE 原则拆解问题 (2-3 层)
2. 执行 5-10 次快速 web_search 获取框架信息
3. 形成 Hypothesis Tree + Storyline
4. **完成后执行 "So What?" Test** — 每个假设必须有方向性洞察

**输出**: Issue Tree + Hypothesis Tree（展示给用户确认）

---

### STEP 4-5: Dummy Pages 设计

**加载**: `file_read(references/layouts.md)` + `file_read(references/design-specs.md)` + `file_read(references/page-dependencies.md)` — 用完释放。

**执行**:
1. 为每个假设选择布局类型（从 `layouts.md` 中选取对应的 Storyboard type）
2. 应用设计规范（配色、字号、密度）
3. **★ 为每页标注 Visual Intent（视觉意图）**
   - 该页核心论据是数据型还是论证型？
   - 数据型 → 选择 `chart:` 类型（bar/line/waterfall/pie/radar）
   - 流程/架构描述 → 选择 `diagram:` 类型（flowchart/sequence/class）
   - 封面/过渡 → 标注 `image:unsplash` 或 `icon:lucide`
   - 纯论证/表格 → 标注 `none`
4. 标注每页依赖关系（✅ 独立 / ⏩ 依赖前页 / ⏪ 依赖后页）
5. **Pyramid Principle 检查** — 执行摘要是否有 Peak + 3 MECE Arguments
6. **Horizontal Logic 检查** — 所有 Action Title 串读是否连贯
7. **★ Visual Balance 审视**（根据主题内容判断，无硬性比例）
   - 审视全文：是否有适合图表化的数据被遗漏为纯文字？
   - 审视全文：是否有适合流程图化的架构描述被遗漏？
   - 注意：纯论证型报告可以全文无图表；数据密集型报告可以多数页有图表——由主题决定，不设比例

**输出**: `项目名_DummyPages_日期.md`

Dummy.md 结构:
```markdown
# [项目名] Dummy Pages
## 项目信息 (日期/总页数/章节/交付格式)
## 页面依赖关系总览 (三轮生成顺序)
## 视觉元素分布概览 (图表X页 / 图X页 / 流程图X页 / 纯文字X页)
## 第1页: 封面
  - 布局: cover
  - 视觉意图: image:unsplash (“AI technology business”)
## 第2页: 执行摘要 (⏪ 最后生成)
  - 布局: summary
  - 视觉意图: none
## 第3-N页: 正文页
  - 布局: data / scqa / diagram / ...
  - 视觉意图: chart:bar / diagram:flowchart / none
  - 依赖: ✅ 独立
```

---

### STEP 6-7: 逐页数据收集 + 生成

**加载**: `file_read(references/excel-data-spec.md)` — 首次执行时，用完释放。

**⚠️ 核心原则: 必须逐页循环，不能一次性处理多页！**

```
对于每一页:
  0. 依赖检查 → 如有缺失，告知用户并提供选项
  1. 查看 Dummy.md 该页设计要求
  2. 执行 2-5 次 web_search
  3. 数据整理 (原始数据 + 来源 URL → 最终数据)
  4. 生成可视化 (PPTX / HTML / Markdown)
  5. 自检: 布局匹配 ✓ 图表匹配 ✓ 真实数据 ✓ 来源记录 ✓
  6. 告知用户完成，等待确认
  7. 清空该页搜索结果上下文
  8. 继续下一页
```

**断点续写**: 用户上传 Dummy.md + 已完成文件 → Agent 从指定页继续。

---

### STEP 8: 可选生成 Word

**触发**: 用户明确要求。默认不提。

---

### STEP 9: 迭代优化

**加载**: `file_read(references/troubleshooting.md)` — 遇到问题时加载。

优化重点: 颜色对比度 / 信息密度 / 元素遮挡 / 文字溢出 / 响应式适配。

---

## 🎨 HTML 演示方案（推荐）

**双智能体模式（推荐）**：
1. STEP 5 输出 `项目名_Storyboard.json`（格式见 `mckinsey-designer/references/storyboard-schema.md`）
2. 告知用户：“Storyboard 已生成，请激活 `mckinsey-designer` 技能进行视觉渲染。”
3. Designer 消费 JSON → 输出纵向分页 HTML 演示文稿

**单 Agent 模式（兼容）**：
1. 前往 `common/templates/web/style-ref-mbb.html` 加载 MBB+ 设计系统
2. 提取 CSS Tokens（品牌色板、字阶、网格组件）
3. 直接生成 HTML（适用于简单场景或 Designer 不可用时）

---

## 📚 Reference 索引

Agent 根据当前 STEP 按需读取：

| 文件 | 用途 | 何时加载 |
|:-----|:-----|:---------|
| `methodology.md` | MECE/Issue Tree/Hypotheses 方法论 + Pyramid/SCQA/"So What?" 执行指令 + 工作流时间表 | STEP 2-3 |
| `theory.md` | 方法论理论深度参考 (Problem Structuring/Communication/Meta Thinking) | Agent 遇到方法论困惑或用户要求理论解释时 |
| `layouts.md` | 7 种 McKinsey 页面布局库 | STEP 4-5 |
| `design-specs.md` | 配色/字号/信息密度规范 | STEP 4-5 |
| `page-dependencies.md` | 页面依赖关系标注规范 | STEP 4-5 |
| `excel-data-spec.md` | 数据文件结构规范 | STEP 6 |
| `troubleshooting.md` | 常见问题解决方案 | STEP 9 遇到问题时 |
| `examples.md` | 完整案例参考 (有声市场/充电桩) | 用户要求案例时 |

**⚠️ 除当前步骤需要的文件外，不要加载其他文件！**

---

## 📦 交付物清单

| 标准交付 | 可选交付 |
|:---------|:---------|
| DummyPages.md (设计规范) | Word 报告 |
| 数据附录 (JSON/CSV) | Excel 数据表 |
| 演示文稿 (PPTX 或 HTML) | |

---

## ✅ Agent 核心检查

- [ ] 首次使用: 严格 4 行话术，不列举示例
- [ ] 只加载当前 STEP 需要的 reference，用完释放
- [ ] STEP 4-5: 每页必须标注 Visual Intent（视觉意图）
- [ ] STEP 4-5: 通过 Visual Balance 审视（内容驱动，无硬性比例）
- [ ] STEP 6-7: 严格逐页循环，每页完成后清空搜索结果
- [ ] 每个 Action Title 通过 "So What?" Test
- [ ] Executive Summary 使用 SCQA 结构 + Pyramid 金字塔
- [ ] 所有结论是 Synthesis（综合洞察），不是 Summary（事实罗列）
