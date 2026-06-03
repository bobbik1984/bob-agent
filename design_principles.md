# Bob Agent - UI 设计与排版规范 (UI Design Principles)

> 核心主旨：**暗色优先 · 亮暗双栖 · 高级质感 · 绝对对齐 · 语义化色彩**

本文件提取并固化了 bob-agent 桌面端在开发过程中总结的核心前端设计与排版规范。未来的任何功能开发、新页面添加，**必须**严格遵循以下标准，以确保桌面级应用的像素级精准和高级视觉体验。

---

## 1. 布局约束与"弹性铺满" (Layout & Stretching)

所有主视图（如 `ChatView`, `InboxView`, `SettingsView`）的容器结构必须遵循 **外层自适应铺满 + 内层绝对宽度限制** 的范式。

### 核心规则：
- **路由视图（Router-View 根节点）必须弹性铺满**：
  任何在主布局右侧区域加载的 `.xxx-view` 根容器，**必须**包含以下属性，否则会导致容器宽度坍缩或被内容强行撑开：
  ```css
  .xxx-view {
    flex: 1;
    min-width: 0;
    height: 100%;
    /* 根据是否需要滚动，选择 overflow: hidden 或 overflow-y: auto */
  }
  ```

- **内容约束 (Max-Width)**：
  内容展示区必须包裹在一个居中容器内，最大宽度严格限制为 **`1000px`**：
  ```css
  .content-wrapper {
    max-width: 1000px;
    width: 100%;
    margin: 0 auto;
  }
  ```

---

## 2. 内边距 (Padding) 的外置原则

为了确保 `1000px` 的宽度在所有页面中是**视觉等宽**的，绝不允许在带有 `max-width: 1000px` 和 `box-sizing: border-box` 的容器上添加左右 Padding。

### 核心规则：
- **Padding 必须加在滚动容器上**，而不是受限内容容器上。
- **正确示范**：
  ```css
  .scroll-area {
    /* 滚动条会贴着屏幕边缘，而 Padding 把内容往里推 */
    padding: var(--space-6) var(--space-8);
    overflow-y: auto;
  }
  .content-wrapper {
    max-width: 1000px;
    width: 100%;
    margin: 0 auto;
    padding: 0; /* 绝对不能有左右 Padding */
  }
  ```
通过这种方式，`ChatView`、`InboxView`、`SettingsView` 的文本起止边缘才会形成一条完美的直线。

---

## 3. 悬浮组件的"锚定对齐" (Absolute Alignment)

侧边栏的删除按钮、消息操作悬浮条等交互元素，定位时必须遵循父级 Padding 的空间感，而不是凭感觉设定偏移量。

### 核心规则：
- **垂直居中**：使用 `top: 50%; transform: translateY(-50%);` 而不是写死具体的 px。这样即使父级（如对话按钮）多了一行文本导致高度变化，悬浮元素依然完美居中。
- **右侧对齐**：绝对定位的 `right` 值必须与父容器的 `padding-right` 值相等（例如 `right: var(--space-3);`），这样悬浮元素的右边缘能与父容器的右内边框完美重叠，间距一致。
- **不要给包裹容器加 Padding**：如果悬浮元素是一个图标按钮，通过设定固定的宽高（如 `16x16px`）结合 Flex 居中图标来实现点击区，而不是用 `padding` 把图标挤小，这会导致图标的中心发生视觉偏移。

---

## 4. 色彩与主题 (Theme & Color)

Bob Agent 采用 **暗色优先 + 亮色完整适配** 的双模式设计。核心品牌色为 Bob Blue `#2776bb`。

### 核心规则：
- **语义化 CSS 变量**：严禁在组件内部硬编码颜色值（如 `#1c1c1c`, `rgba(0,0,0,0.5)`）。所有颜色必须使用 `index.css` 中定义的语义化变量（如 `var(--text-primary)`, `var(--bg-primary)`, `var(--border-subtle)`）。
- **`var()`优先策略**：任何新增组件的 `background`、`color`、`border-color`、`box-shadow` 属性必须首先尝试使用现有的 CSS 变量。只有当确实没有合适变量时，才可以新增语义化变量到 `index.css` 的 `:root` 和 `[data-theme="light"]` 中。
- **SVG Logo 双模式**：品牌 Logo 在暗色模式下为白色（通过 `--logo-filter: brightness(0) invert(1)`），在亮色模式下直接显示为品牌蓝 `#2776bb`（通过 `--logo-color` 变量 + `color: currentColor`）。
- **原生控件同步**：Electron 窗口的 `titleBarOverlay` 颜色必须通过 IPC (`system:update-theme`) 与应用主题保持实时同步。切换主题时，既要更新 CSS 变量，也要通知 Main 进程刷新原生标题栏。
- **弹窗/下拉菜单**：所有弹出菜单（模型切换、模式切换、CustomSelect 下拉框等）必须使用 `var(--bg-primary)` 作为背景色，`var(--shadow-lg)` 作为阴影，不允许硬编码暗色背景。
- **品牌色统一**：日程事件色块、链接高亮等需要使用品牌色时，统一使用 `#2776bb`。避免使用 `var(--accent-primary)` 以外的高饱和颜色（红/橙/黄仅限语义化用途：错误/警告/成功）。

---

## 5. 组件缩放与精致感 (Scaling for Refinement)

### 核心规则：
- **微缩尺寸**：次要交互元素（如侧栏的关闭叉号、头像 SVG）的尺寸应适当缩小。例如，默认的 lucide 图标一般为 24px，在侧栏中可以缩小至 `16px` 或 `12px`；内部放置的 SVG 可以采用外框尺寸的 `60%`，留出呼吸感。
- **UI 紧凑模式**：整个系统支持 `data-ui-scale` 缩放，开发时默认以 `compact` 紧凑模式作为基准进行像素级校对。文字不宜过大，多利用 `var(--text-sm)` (13px) 和 `var(--text-xs)` (12px) 展现高级的"仪表盘"专业感。

---

## 6. 侧边栏交互 (Sidebar Interaction)

侧边栏不使用 CSS 原生 `resize: horizontal`（因为只在右下角生效），而是用自定义的拖拽把手 + 折叠按钮组合。

### 核心规则：
- **拖拽把手**：一个 6px 宽的透明区域覆盖在侧边栏右边框的完整高度上。悬浮时通过 `::after` 伪元素显示一条 **1px 细线**（而非粗色块），颜色为 `var(--accent-primary)`。
- **折叠按钮**：使用一个 `position: absolute` 的小胶囊按钮，`top: 50%` 定位在侧边栏边缘的垂直中点。展开时显示 `<`（ChevronLeft），折叠后显示 `>`（ChevronRight）。
- **宽度持久化**：侧边栏宽度在拖拽结束时通过 `setConfig` 保存，启动时恢复。最小宽度 `200px`，最大 `600px`。
- **列表项高度一致性**：所有对话列表项（包括无消息记录的新对话）必须保持一致高度。对第二行文本使用 `\u00A0`（不可断空格）占位，避免因内容缺失导致行高坍缩。

---

## 7. 空状态设计 (Empty State)

空状态不使用文字引导或快捷按钮，而是展示品牌印记。

### 核心规则：
- **品牌水印**：新对话的空白页面使用一个 **5% 透明度** 的 Bob Logo SVG 作为水印，宽度等于内容区最大宽度 (`max-width: 1000px`)，位置 **紧贴输入框上方**（`justify-content: flex-end`）。
- **不干扰**：不显示问候语、提示文本或快捷操作按钮。用户熟练后不需要每次被引导。
- **Logo 响应主题**：水印的 `filter` 同样使用 `var(--logo-filter)`，确保在亮色和暗色模式下都有正确的可见度。

---

## 8. 信息密度与窄屏适配 (Information Density & Responsiveness)

### 核心规则：
- **日程时间轴**：全天 0–24 小时展示，只在 0、6、12、18、24 五个关键点显示数字，其余为淡色网格线。不使用 `:00` 后缀，不附加刻度短线。
- **以"今天"为中心的周视图**：日程不按固定的周一到周日排列，而是以今天为中心向前后各延伸 3 天，形成滚动的 7 天窗口。"今天"的日期标签使用品牌蓝高亮。
- **窄屏降级**：当窗口宽度 < 700px 时，横向时间轴自动切换为纵向卡片列表（与 TodoList 组件同构），确保信息不被截断或挤压。
- **事件色块**：在横向时间轴中，事件色块的高度必须与轨道高度完全一致（`top: 0; height: 100%`），不留上下间隙。使用品牌蓝，不加阴影。
- **可交互性**：事件色块可点击打开详情弹窗，提供查看/删除功能。拖拽移动和点击操作通过 `moved` 标志位区分，避免误触。

---

## 9. 页面标题与导航标签 (Page Headers & Navigation)

### 核心规则：
- **对话页标题**：使用 Bob Logo SVG 而非文字（如"AI 助手"），尺寸 24px，响应主题切换。
- **日程页标题**：使用图标 + "日程" 两字，不添加副标题或描述行（如"日程管理 + 待办清单"）。
- **导航标签精简**：侧边栏底部导航使用最简短的名称。"智能收件箱"简化为"日程"。
- **标题行高度统一**：所有页面的头部标题区域高度一致（36px `.view-title`），确保切换页面时视觉锚点不跳动。

---

## 10. 静态资源加载铁律 (Vite Assets vs Public)

在 Vue 模板和 JS 中加载图片时，必须严格区分存放目录和加载方式，否则打包为 Tauri 桌面端时必定报 404：

- **`src/assets/` 目录**：用于静态写死的小图标、背景图。
  - **加载方式**：可以使用相对路径 `../assets/icon.png` 或 Vite 专用的 `new URL('../../assets/icon.png', import.meta.url).href`。
- **`public/` 目录**：用于**动态拼接路径**（如模型 Logo）或不需要被打包重命名的大文件。
  - **加载方式**：**绝对禁止**使用 `new URL(..., import.meta.url)`。必须直接使用以 `/` 开头的绝对路径纯字符串。
  - **正确示范**：`return '/logos/deepseek.png';`
  - **错误示范**：`return new URL('/logos/deepseek.png', import.meta.url).href;` （这会在 Tauri 打包后引发路径解析崩溃）

---

## 11. Logo 配色铁律

Bob Logo 在不同主题下的颜色由 CSS 变量 `--logo-color` 统一控制（定义在 `src/index.css`）：

| 模式 | `--logo-color` 取值 | 视觉效果 |
|:---|:---|:---|
| **Dark** | `var(--text-primary)` (白色) | 白色 Logo，与文字融为一体 |
| **Light** | `var(--user-accent)` (强调色) | 品牌色 Logo，形成视觉焦点 |

**适用范围**：标题栏 Logo (App.vue)、对话头像 (ChatView.vue)、启动画面 (index.html)、引导页 (SetupWizard.vue)。

**代码规则**：
- SVG 内联 Logo：使用 `color: var(--logo-color)` + `fill="currentColor"`
- CSS Mask Logo：使用 `background-color: var(--logo-color)`
- **绝对禁止**在任何 Logo 元素上硬编码颜色值或直接使用 `var(--user-accent)`

---

## 12. 图标-文字垂直对齐铁律

Lucide SVG 图标与相邻文字在视觉上必须严格垂直居中对齐。**所有包含图标的容器**都必须使用 Flex 对齐，禁止依赖 `vertical-align`。

**标准写法**：
```css
/* ✅ 正确 — 图标容器用 flex 对齐 */
.section-title {
  display: flex;
  align-items: center;
  gap: 8px;
}
.section-icon {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

/* ❌ 错误 — vertical-align 在 flex 容器中无效 */
.section-icon {
  vertical-align: middle;
}
```

**适用范围**：设置页标题栏 (.section-title + .section-icon)、按钮内图标 (.btn)、弹窗标题 (.briefing-header + .briefing-icon)、所有行内图标+文字组合。

---

## 13. 强调色使用铁律

`--user-accent` 是用户选择的品牌色（如 `#2776BB`），但**不应直接用于图标/文字颜色**，因为它在深色背景上可能对比度不足。

| 用途 | 正确变量 | 说明 |
|:---|:---|:---|
| **Logo** | `var(--logo-color)` | 深色=白色，浅色=品牌色 |
| **文字/图标** | `var(--text-primary)` 或 `var(--text-secondary)` | 语义化，自动适配主题 |
| **交互高亮** | `var(--accent-primary)` | 按钮悬停、链接、选中状态 |
| **背景/填充** | `var(--user-accent)` | 开关滑块、进度条等大面积填充 |

**绝对禁止**在模板中写 `style="color: var(--user-accent);"` 来给图标上色。

---

## 14. 国际化 (i18n) 铁律

所有用户可见的文字**必须**通过 `vue-i18n` 的 `$t()` 函数调用，**同步**在两个 locale 文件中添加对应 key：
- `src/locales/zh-CN.json` — 简体中文
- `src/locales/en-US.json` — English

**禁止行为**：
```vue
<!-- ❌ 硬编码中文 -->
<span>扫码绑定微信</span>

<!-- ❌ fallback 兜底写法（说明 key 缺失） -->
{{ $t('settings.wechat_bot') || '微信助理' }}

<!-- ✅ 正确写法 -->
{{ $t('settings.wechat_scan') }}
```

**检查清单**（每次新增/修改 UI 文字时）：
1. 在 Vue 模板中使用 `$t('namespace.key_name')`
2. 在 `zh-CN.json` 中添加中文值
3. 在 `en-US.json` 中添加对应英文值
4. 切换语言测试两种语言都正确显示
