# MBB+ Design System Reference

> 此文件是 mckinsey-designer 的核心视觉参考。每次渲染时 STEP 1 必须加载。

---

## 1. 品牌皮肤 CSS Variables

根据 Storyboard 中的 `brand` 字段，注入对应的 CSS Variables 到 `<style>` 标签中。

### McKinsey & Company (`brand: "mckinsey"`)

```css
:root {
  --brand-primary: #052243;
  --brand-accent: #005EB8;
  --brand-accent-tint: #E8F4FD;
  --brand-highlight: #FFDB00;
  --brand-text: #333333;
  --brand-text-muted: #666666;
  --brand-bg: #FFFFFF;
  --brand-bg-alt: #F8F9FA;
  --brand-border: #E5E7EB;
  --brand-negative: #D9534F;
  --brand-positive: #2E7D32;
  --brand-footer: "McKinsey & Company";
}
```

### Boston Consulting Group (`brand: "bcg"`)

```css
:root {
  --brand-primary: #006B3F;
  --brand-accent: #00A86B;
  --brand-accent-tint: #E6F7EF;
  --brand-highlight: #FFDB00;
  --brand-text: #333333;
  --brand-text-muted: #4A4A4A;
  --brand-bg: #FFFFFF;
  --brand-bg-alt: #F8F9FA;
  --brand-border: #E5E7EB;
  --brand-negative: #D9534F;
  --brand-positive: #00A86B;
  --brand-footer: "Boston Consulting Group";
}
```

### Bain & Company (`brand: "bain"`)

```css
:root {
  --brand-primary: #1A1A1A;
  --brand-accent: #CB2026;
  --brand-accent-tint: #FDECEC;
  --brand-highlight: #CB2026;
  --brand-text: #333333;
  --brand-text-muted: #6B6B6B;
  --brand-bg: #FFFFFF;
  --brand-bg-alt: #F8F9FA;
  --brand-border: #E5E7EB;
  --brand-negative: #CB2026;
  --brand-positive: #2E7D32;
  --brand-footer: "Bain & Company";
}
```

### Roland Berger (`brand: "roland_berger"`)

```css
:root {
  --brand-primary: #1E5FA6;
  --brand-accent: #1E5FA6;
  --brand-accent-tint: #E9F0F9;
  --brand-highlight: #FFB800;
  --brand-text: #333333;
  --brand-text-muted: #555555;
  --brand-bg: #FFFFFF;
  --brand-bg-alt: #F8F9FA;
  --brand-border: #E5E7EB;
  --brand-negative: #D9534F;
  --brand-positive: #2E7D32;
  --brand-footer: "Roland Berger";
}
```

---

## 2. 字阶规范

所有品牌共用同一字阶：

| 元素 | 大小 | 权重 | 颜色 |
|:-----|:-----|:-----|:-----|
| Hero Title (封面) | 48px | 700 | `--brand-primary` |
| Action Title (每页标题) | 28px | 700 | `--brand-primary` |
| Sub-title | 16px | 400 | `--brand-text-muted` |
| Body | 15px | 400 | `--brand-text` |
| Bullet / List | 14px | 400 | `--brand-text` |
| KPI 数据 | 36px | 700 | `--brand-accent` |
| Caption / Source | 11px | 400 | `--brand-text-muted` |
| Footer | 10px | 400 | `#999999` |

**字体栈**: `'Inter', 'Helvetica Neue', 'Microsoft YaHei', sans-serif`

引入: `<link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;600;700&display=swap" rel="stylesheet">`

---

## 3. 布局组件

### 3.1 Slide 容器

```css
.reveal .slides section {
  width: 100%;
  height: 100%;
  padding: 40px 50px 30px;
  display: flex;
  flex-direction: column;
  font-family: 'Inter', 'Microsoft YaHei', sans-serif;
}
```

### 3.2 Header (每页必有)

```css
.slide-header {
  border-bottom: 3px solid var(--brand-primary);
  padding-bottom: 12px;
  margin-bottom: 20px;
  flex-shrink: 0;
}
.action-title {
  font-size: 28px;
  font-weight: 700;
  color: var(--brand-primary);
  line-height: 1.2;
  margin: 0 0 6px;
}
```

### 3.3 Grid Utilities

```css
.grid-2  { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; flex: 1; }
.grid-3  { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 20px; flex: 1; }
.grid-2x2 { display: grid; grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; gap: 16px; flex: 1; }
```

### 3.4 Insight Box (底部洞察框，每页推荐)

```css
.insight-box {
  background: var(--brand-primary);
  color: #fff;
  padding: 12px 18px;
  border-left: 5px solid var(--brand-highlight);
  font-size: 14px;
  font-weight: 600;
  margin-top: auto;
  flex-shrink: 0;
}
```

### 3.5 SCQA 叙事块

```css
.scqa-s { background: var(--brand-accent-tint); border-left: 4px solid var(--brand-accent); padding: 10px 14px; margin-bottom: 8px; }
.scqa-c { background: #FFF3F3; border-left: 4px solid var(--brand-negative); padding: 10px 14px; margin-bottom: 8px; }
.scqa-q { background: var(--brand-bg-alt); border-left: 4px solid var(--brand-text-muted); padding: 10px 14px; margin-bottom: 8px; }
.scqa-a { background: var(--brand-primary); color: #fff; padding: 10px 14px; font-weight: 600; }
```

### 3.6 Card (象限/栏目)

```css
.card {
  background: var(--brand-bg-alt);
  border: 1px solid var(--brand-border);
  padding: 16px 20px;
  border-radius: 2px;
}
.card.accent {
  border-top: 4px solid var(--brand-accent);
  background: var(--brand-accent-tint);
}
```

### 3.7 Data Table

```css
.mbb-table { width: 100%; border-collapse: collapse; font-size: 14px; }
.mbb-table th { background: var(--brand-primary); color: #fff; padding: 10px 14px; text-align: left; font-weight: 600; }
.mbb-table td { padding: 10px 14px; border-bottom: 1px solid var(--brand-border); }
.mbb-table tr:nth-child(even) td { background: var(--brand-bg-alt); }
```

### 3.8 Footer

```css
.slide-footer {
  display: flex;
  justify-content: space-between;
  font-size: 10px;
  color: #999;
  padding-top: 8px;
  flex-shrink: 0;
}
```

---

## 4. 排版铁律速查

| 规则 | CSS 实现 | 原理 |
|:-----|:---------|:-----|
| 金字塔结构 | `flex-direction: column` | 结论先行 |
| MECE 分区 | `grid: 1fr 1fr / 1fr 1fr` | 互不重叠，完全穷尽 |
| 呼吸留白 | `padding: 16px 20px; gap: 20px` | 格式塔接近性 |
| 深色白字 | `bg: brand-primary; color: #fff` | 表头/洞察框强制 |
| 数据高亮 | `color: brand-accent; bold` | KPI 抢夺注意力 |
| 洞察框 | `border-left: 5px solid brand-highlight` | "So What?" 驱动 |
| 一页一论点 | 拆为独立 `<section>` | 信息不过载 |
| Action Title | 完整句子 | "市场份额增长5%" ≠ "市场表现" |

---

## 5. 视觉参考文件

完整的四品牌 HTML 参考实现位于：
`common/templates/web/style-ref-mbb.html`

Agent 可在需要时 `file_read` 该文件查看实际渲染效果和完整 HTML 结构。
