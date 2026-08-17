# Mermaid Diagram 生成规范

> 仅当 Storyboard slide 包含 `type: "diagram"` 时加载此文件。

---

## 使用方式

在 HTML `<head>` 中引入 Mermaid CDN：

```html
<script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
```

---

## 1. 四品牌 Mermaid 主题

在每段 Mermaid 代码的开头注入 `%%{init}%%` Frontmatter 来设定品牌色。

### McKinsey

```
%%{init: {'theme':'base','themeVariables':{'primaryColor':'#052243','primaryTextColor':'#fff','primaryBorderColor':'#005EB8','lineColor':'#005EB8','secondaryColor':'#E8F4FD','tertiaryColor':'#F8F9FA','fontFamily':'Inter, Microsoft YaHei, sans-serif','fontSize':'14px'}}}%%
```

### BCG

```
%%{init: {'theme':'base','themeVariables':{'primaryColor':'#006B3F','primaryTextColor':'#fff','primaryBorderColor':'#00A86B','lineColor':'#00A86B','secondaryColor':'#E6F7EF','tertiaryColor':'#F8F9FA','fontFamily':'Inter, Microsoft YaHei, sans-serif','fontSize':'14px'}}}%%
```

### Bain

```
%%{init: {'theme':'base','themeVariables':{'primaryColor':'#1A1A1A','primaryTextColor':'#fff','primaryBorderColor':'#CB2026','lineColor':'#CB2026','secondaryColor':'#FDECEC','tertiaryColor':'#F8F9FA','fontFamily':'Inter, Microsoft YaHei, sans-serif','fontSize':'14px'}}}%%
```

### Roland Berger

```
%%{init: {'theme':'base','themeVariables':{'primaryColor':'#1E5FA6','primaryTextColor':'#fff','primaryBorderColor':'#1E5FA6','lineColor':'#1E5FA6','secondaryColor':'#E9F0F9','tertiaryColor':'#F8F9FA','fontFamily':'Inter, Microsoft YaHei, sans-serif','fontSize':'14px'}}}%%
```

---

## 2. 支持的图表类型

| diagram_type | Mermaid 语法 | 典型场景 |
|:-------------|:-------------|:---------|
| `flowchart` | `graph TD` / `graph LR` | 业务流程、决策树、系统通信 |
| `sequence` | `sequenceDiagram` | API 调用链、用户交互时序 |
| `class` | `classDiagram` | 系统架构、数据模型 |
| `state` | `stateDiagram-v2` | 状态机、生命周期 |
| `gantt` | `gantt` | 项目路线图、排期 |
| `mindmap` | `mindmap` | 主题拆解、头脑风暴 |

---

## 3. Reveal.js 嵌入模板

```html
<!-- 在 slide section 内部 -->
<div class="slide-header">
  <h2 class="action-title">{slide.title}</h2>
</div>
<div class="mermaid" style="flex:1; display:flex; align-items:center; justify-content:center;">
{品牌 init 注入}
{slide.mermaid 代码}
</div>
<div class="insight-box">{slide.insight}</div>
```

初始化脚本（放在 Reveal.initialize 之后）：

```javascript
mermaid.initialize({
  startOnLoad: false,
  theme: 'base',
  securityLevel: 'loose'
});

// 在 Reveal ready 后渲染所有 mermaid 块
Reveal.on('ready', async () => {
  const els = document.querySelectorAll('.mermaid');
  for (const el of els) {
    try {
      const { svg } = await mermaid.render('m-' + Math.random().toString(36).slice(2), el.textContent);
      el.innerHTML = svg;
    } catch (e) {
      console.error('Mermaid render error:', e);
      el.innerHTML = '<pre style="color:red;font-size:12px;">Diagram render failed: ' + e.message + '</pre>';
    }
  }
});
```

---

## 4. Self-Correction 规范

当生成 Mermaid 代码时，Designer Agent 应遵循以下流程：

### 生成阶段
1. 强制 LLM 只输出合法的 Mermaid 语法代码
2. 禁止输出任何引导性对话废话
3. 所有节点名称使用引号包裹（防止特殊字符导致语法错误）
4. 连接箭头使用标准类型：`-->`, `-.->`, `==>`, `-->`

### 验证阶段
1. 将生成的 Mermaid 代码传入验证脚本 `validate_mermaid.py`
2. 脚本返回 `PASS` 或 `FAIL + 错误信息`
3. 如果 FAIL：将错误信息附加到 Prompt 中，要求 LLM 修复
4. 最多重试 3 轮
5. 3 轮后仍失败：降级为文本列表展示

### 验证脚本位置
```
common/knowledge/skills/mckinsey-designer/references/validate_mermaid.py
```

---

## 5. 常见语法陷阱

Agent 在生成 Mermaid 代码时**必须**避免的错误：

| 陷阱 | 错误示例 | 正确示例 |
|:-----|:---------|:---------|
| 节点名含特殊字符 | `A[User (Admin)]` | `A["User (Admin)"]` |
| 箭头方向不一致 | 混用 `-->` 和 `->` | 统一使用 `-->` |
| subgraph 未闭合 | `subgraph X` 无 `end` | 每个 `subgraph` 必须有 `end` |
| 类定义冲突 | 同名节点不同形状 | 每个 ID 只定义一次 |
| 中文节点未加引号 | `A[数据采集]` | `A["数据采集"]` |
