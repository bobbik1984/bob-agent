---
name: mckinsey-designer
description: "MBB+ 级别视觉渲染引擎。当用户要求生成 McKinsey 风格报告、商业演示、16:9 幻灯片、专业 HTML 报告或咨询级演示文稿时必须使用此技能。消费 Storyboard JSON，生成纯净纵向分页 HTML（严禁 Reveal.js 框架）。支持 ECharts 图表、四大品牌皮肤（深蓝科技/翠绿咨询/暗金尊享/极简黑白）。"
version: 1.1.0
tags: [Workflow, Design, Presentation]
related_skills: [mckinsey-consultant, api-registry, frontend-design]
metadata:
  last_updated: "2026-04-22"
  architecture: "JSON-Driven Rendering Pipeline"
  upstream: "mckinsey-consultant (produces Storyboard JSON)"
---

# McKinsey Designer V1.0

**定位**: 双智能体架构中的**视觉引擎**。与 `mckinsey-consultant`（策略大脑）解耦，只关注"如何把结构化内容渲染成专业幻灯片"。

**输入**: Storyboard JSON（由 `mckinsey-consultant` STEP 5 输出，或用户手动提供）
**输出**: 单文件纵向滚动分页 HTML 演示文稿（放弃庞大的 Reveal.js，使用纯净响应式 HTML 保证大模型输出不超载）

---

## ⚠️ 行为规则

| # | 规则 | ✅ 正确 | ❌ 禁止 |
|:--|:-----|:--------|:--------|
| 1 | 职责边界 | 只做视觉渲染，不修改商业逻辑 | 质疑或修改 Storyboard 中的数据/结论 |
| 2 | 品牌一致性 | 整份 deck 使用同一品牌皮肤 | 同一 deck 混用多个品牌色 |
| 3 | 图表数据 | 严格使用 Storyboard 提供的数据 | 自行编造或修改数据 |
| 4 | 配图 | 优先 Icon → Unsplash → AI 生成 | 使用外链图片（无法离线） |
| 5 | 视觉意图 | 遵循 Consultant 标注的 Visual Intent | 对 `none` 页自行添加图表/配图 |

---

## 🎯 渲染管线

```
输入: Storyboard JSON
  │
  ├─ STEP 1: 读取品牌皮肤 → 加载 CSS Variables
  ├─ STEP 2: 遍历 slides[] 数组
  │    ├─ 识别 slide.type (cover / scqa / data / diagram / matrix / comparison / divider)
  │    ├─ 选择对应 Layout 组件
  │    ├─ 如有 chart → 生成 ECharts 初始化代码
  │    ├─ 如有 mermaid → 注入品牌主题 + Mermaid 代码 + 验证
  │    ├─ 如有 image → Unsplash API 或 Icon 渲染
  │    └─ 注入文本内容 + 底部 Insight Box
  ├─ STEP 3: 组装为完整的纵向分页 HTML
  └─ STEP 4: 输出 .html 文件
```

---

## 📖 分步执行指南

### STEP 1: 解析输入

接收 Storyboard JSON 或 Markdown，提取：
- `brand`: 品牌皮肤 (mckinsey / bcg / bain / roland_berger)
- `slides[]`: 幻灯片数组

**加载**: `file_read(references/design-system.md)` — 获取品牌色板和 CSS tokens。

---

### STEP 2: 逐页渲染

对每一页 slide，按 `type` 字段选择布局：

| type | 布局 | 核心组件 |
|:-----|:-----|:---------|
| `cover` | 全屏封面 | Hero 标题 + 副标题 + 可选背景图 |
| `scqa` | 左右分栏 | 左: SCQA 叙事弧 / 右: MECE 论据列表 |
| `data` | 图表+文字 | ECharts 图表 + 解读文字 |
| `matrix` | 2×2 网格 | BCG 矩阵 / 对比框架 |
| `comparison` | 对比表格 | 多维度对比表 |
| `columns` | N 栏等分 | 2-4 栏平行内容 |
| `divider` | 章节过渡 | 大号标题 + 可选配图 |
| `diagram` | 流程/架构图 | Mermaid 渲染 + 品牌主题注入 |
| `summary` | 执行摘要 | SCQA + Insight Box |

**加载**: `file_read(references/chart-themes.md)` — 仅当 slide 含 chart 字段时加载。
**加载**: `file_read(references/diagram-spec.md)` — 仅当 slide 含 mermaid 字段时加载。

---

### STEP 3: 图表渲染

当 slide 包含 `chart` 字段时：

1. 引入 ECharts CDN: `https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js`
2. 从 `chart-themes.md` 加载当前品牌的 ECharts 主题
3. 支持的图表类型：

| chart.type | ECharts 类型 | 典型场景 |
|:-----------|:-------------|:---------|
| `bar` | 柱状图 | 收入对比、市场份额 |
| `line` | 折线图 | 趋势分析、增长曲线 |
| `waterfall` | 瀑布图 | 利润桥、成本分解 |
| `pie` / `donut` | 饼图/环形图 | 市场构成 |
| `radar` | 雷达图 | 多维能力评估 |
| `hbar` | 横向条形图 | 排名对比 |

4. 图表容器使用 `<div>` 嵌入，`width: 100%; height: 60vh;`

---

### STEP 3.5: Diagram 渲染

当 slide 包含 `type: "diagram"` 时：

1. 引入 Mermaid CDN: `https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js`
2. 从 `diagram-spec.md` 加载当前品牌的 Mermaid 主题 `%%{init}%%` 头
3. 将品牌主题头 + `slide.mermaid` 代码拼接，放入 `<div class="mermaid">`
4. 请务必仔细检查你的 Mermaid 语法，确保它能直接在浏览器中成功渲染。不要使用复杂的子图语法以降低出错率。

---

### STEP 4: 配图策略

按优先级执行（三层降级）：

**Layer 1 — Lucide Icons（推荐，即时）**
```html
<script src="https://unpkg.com/lucide@latest"></script>
<i data-lucide="trending-up" style="width:48px; color:var(--brand-accent);"></i>
```
适用：流程步骤、概念图标、装饰性符号。

**Layer 2 — Unsplash API（封面/过渡页）**
```
1. 从 api-registry 读取 Unsplash access_key
2. GET https://api.unsplash.com/photos/random?query={slide.image.query}&orientation=landscape
3. 提取 response.urls.regular 作为背景图
4. 添加 attribution 到页脚（Unsplash 使用条款要求）
```

**Layer 3 — AI 图像生成（备选）**
调用 generate_image 工具，使用标准化 Prompt：
> "Flat vector illustration, consulting style, {theme}, {brand_color} and white, minimal, no text, 16:9"

---

### STEP 5: 组装输出

将所有渲染结果组装为单文件标准 HTML（无 JS 动画框架，依赖原生 CSS 控制分页与滚动）。
完成后，**必须调用 `write_file` 工具**，将生成的完整 HTML 代码保存到当前设定的 **工作目录(Workspace)** 的 `exports/[项目名]_Presentation.html` 中。如果未配置工作目录，请先提示用户配置。

```html
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <script src="https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
  <script src="https://unpkg.com/lucide@latest"></script>
  <style>
    /* 品牌皮肤 CSS Variables 注入位置 */
    body { font-family: sans-serif; background: #f5f5f5; margin: 0; padding: 20px; }
    .slide-page { 
        background: white; 
        margin: 0 auto 30px; 
        padding: 40px; 
        max-width: 1000px; 
        min-height: 562px; /* 16:9 approx */
        box-shadow: 0 4px 12px rgba(0,0,0,0.1);
        border-radius: 8px;
        page-break-after: always;
    }
  </style>
</head>
<body>
  <!-- 渲染的 slide-page divs -->
  <div class="slide-page">
    <!-- Slide 1 内容 -->
  </div>
</body>
</html>
```

---

## 📚 Reference 索引

| 文件 | 用途 | 何时加载 |
|:-----|:-----|:---------|
| `design-system.md` | 四大品牌 CSS Variables + 布局规则速查 | STEP 1（每次必加载）|
| `chart-themes.md` | ECharts 四套品牌主题注册代码 | STEP 3（仅含图表时加载）|
| `diagram-spec.md` | Mermaid 四品牌主题 + 渲染模板 + Self-Correction 规范 | STEP 3.5（仅含 diagram 时加载）|
| `storyboard-schema.md` | Storyboard JSON 接口规范 + 示例 | 解析输入时参考 |

**⚠️ 只加载当前步骤需要的文件！**

---

## 🔄 与 mckinsey-consultant 的协作

```
用户请求 → mckinsey-consultant (STEP 1-5)
                │
                ├─ 输出: Storyboard JSON (项目名_Storyboard.json)
                │   包含: slides[], brand, metadata
                │
                └─→ mckinsey-designer (STEP 1-5)
                        │
                        └─ 输出: HTML 演示文稿 (项目名_Presentation.html)
```

**手动触发**: 用户也可以直接提供 Storyboard JSON 或 Markdown 给 Designer，无需经过 Consultant。

---

## ✅ 自检清单

- [ ] 所有 slide 使用同一品牌皮肤的 CSS Variables
- [ ] 每页 Action Title 是完整句子（非标签）
- [ ] 图表使用品牌主题色，非 ECharts 默认色
- [ ] 配图有 alt 文字和归属标注
- [ ] Insight Box 使用深色底 + 亮色左边条
- [ ] Mermaid 图表使用品牌主题注入，非默认灰色
- [ ] Mermaid 代码通过 validate_mermaid.py 验证
- [ ] HTML 文件可离线打开（CDN 资源有 fallback）
- [ ] 页脚包含品牌标识 + 页码
