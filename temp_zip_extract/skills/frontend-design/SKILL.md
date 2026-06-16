---
name: frontend-design
description: Create distinctive, production-grade frontend interfaces with high design quality. Use this skill when the user asks to build web components, pages, artifacts, posters, or applications (examples include websites, landing pages, dashboards, React components, HTML/CSS layouts, or when styling/beautifying any web UI). Generates creative, polished code and UI design that avoids generic AI aesthetics.
license: Complete terms in LICENSE.txt
version: 1.2.0
tags: [Development]
related_skills: []
---

This skill guides creation of distinctive, production-grade frontend interfaces that avoid generic "AI slop" aesthetics. Implement real working code with exceptional attention to aesthetic details and creative choices.

The user provides frontend requirements: a component, page, application, or interface to build. They may include context about the purpose, audience, or technical constraints.

## Design Thinking & Template-Driven Execution (核心工作流)

Before coding, you MUST anchor your design into a concrete set of CSS Design Tokens. Do not rely on your own abstract interpretation of "good design" adjectives.

**Step 1: Locate the Aesthetic Source of Truth**
Always look into the directory `common/templates/web/` for structural reference templates. Currently available templates include (but are not limited to):
- `style-ref-minrims.html` (Tech Minimalist)
- `style-ref-ndc.html` (Editorial Clean)
- `style-ref-bontempi.html`
- `style-ref-pantone2025.html`

If the user specifies a template, use it. If not, pick the one that fits the purpose best.

**Step 2: Token Extraction (非常关键)**
You MUST use `view_file` to read the chosen HTML template.
Extract the exact CSS variables `:root { ... }`, the exact padding algorithms, border structures (or lack thereof), typography stack, and spacing rhythms. 

**Step 3: Intentional Application**
Apply these extracted tokens rigorously. 
- Do not add arbitrary drop shadows if the template does not use them.
- Do not inject "AI-feel purple neon gradients" if the template relies on high-contrast solids.
- Let the Template's exact spacing and font weighting drive the layout. E.g., if the template uses `margin-bottom: 80px` for separation instead of borders, you MUST do the same.

Then implement working code (HTML/CSS/JS, React, Vue, etc.) that is:
- Production-grade and functional
- Visually striking and memorable
- Cohesive with a clear aesthetic point-of-view
- Meticulously refined in every detail

## Frontend Aesthetics Guidelines

Your aesthetics are strictly bound by the Template you extracted in Step 2. However, follow these universal anti-slop guidelines:

- **Typography & Structure**: Use the font-stack defined in the template. Structure the page through massive scale differences (e.g., `48px` vs `12px`) rather than throwing everything inside slightly different shaped cards.
- **Color & Theme**: **STRICTLY PROHIBIT** the cliché "AI-feel" palette unless the user EXPLICITLY asks for Synthwave/Neon. Never inject unsolicited purple/blue glowing gradients, or heavy frosted glass (`backdrop-filter` abuse). Your entire palette must exclusively use the `--color-*` variables defined in the reference `.html` template.
- **Motion**: Use the `transition` variables from the template. Avoid bouncy effects.
- **Spatial Composition**: Elements shouldn't float arbitrarily—they should align to an invisible grid. Use the container widths and `padding` structures extracted from the template.
- **Iconography & Media**: **MINIMIZE EMOJI USAGE**. Over-reliance on emojis is a hallmark of generic "AI slop". For a premium, mature aesthetic, strictly prefer professional vector icons (e.g., Lucide) or high-quality photography (e.g., Unsplash).
- **Details**: Solid borders and negative space are preferred over fuzzy drop shadows. Buttons should mimic the exact styling (pill-shaped vs sharp rectangles) dictated by the chosen template.

**IMPORTANT**: Elegance comes from high contrast, precise alignment, and eliminating the unnecessary. When in doubt, trust the Template's CSS tokens over your own generative tendencies.

## Responsive & Layering Engineering (防坑指南)

Beautiful design that breaks on mobile or has overlapping layers is FAILED design. Apply these hard-won engineering rules:

### Z-Index Hierarchy (严格分层制度)
Define a clear, documented z-index scale. NEVER use arbitrary numbers like `99999`. Use this tiered system:
```
Level 0  (auto)    — Normal document flow
Level 1  (10-50)   — Sticky headers, in-flow panels (dev-log, drag handles)
Level 2  (100)     — Sidebar navigation  
Level 3  (200)     — Dropdowns, tooltips, hover popovers
Level 4  (500)     — Mobile slide-out menus
Level 5  (999)     — Backdrop overlays (semi-transparent click-away layers)
Level 6  (1000)    — Modal panels, mobile settings, floating action sheets
Level 7  (9999)    — System-level overlays (context menus, toasts)
Level 8  (99999)   — Critical: thought overlays, full-screen takeovers
```
**Rule**: Every `z-index > 100` MUST have a backdrop overlay at `z-index - 1` behind it, or use `position: fixed` with an opaque background.

### Mobile Overlay Pattern (移动端弹出面板标准)
When a panel opens on mobile (settings, menus, dropdowns), ALWAYS implement:
1. **Opaque Background**: Use `var(--bg-secondary)` (solid), NEVER `var(--bg-glass)` (semi-transparent). Glass effects bleed through on mobile.
2. **Backdrop Overlay**: Add a `position: fixed; inset: 0` backdrop at `z-index: N-1` with `background: rgba(0,0,0,0.4)` to:
   - Visually separate the panel from the content behind it
   - Provide a click/tap target to dismiss the panel
3. **Fixed Positioning**: Use `position: fixed` (not `absolute`) for mobile panels — absolute positions break when the page scrolls.
4. **Max-Height Guard**: Always add `max-height: calc(100vh - Npx); overflow-y: auto;` to prevent panels from overflowing the viewport.

### CSS Variable Completeness (变量完整性)
When referencing a CSS variable, ALWAYS ensure it is defined in BOTH `:root` (light) AND `[data-theme="dark"]`. Common offenders:
- `--bg-heavy` — Needed for opaque panel backgrounds
- `--bg-tooltip` — Needed for hover popover backgrounds
- `--shadow-color` — Needed for theme-aware box-shadows
**Rule**: After adding any new CSS variable reference, grep both theme blocks to verify it exists. Missing variables silently fail to `transparent`, causing the exact "blending into background" bug.

### Desktop ↔ Mobile Reconciliation (响应式变形非缩放)
- **Do not just shrink fonts**: If a desktop layout has 5 items in a row, DO NOT shrink the font size to 8px on mobile to force them to fit. This is unreadable.
- **Transform the layout**: A horizontal top bar on PC should transform into a Hamburger Menu, a bottom tab bar, or a horizontally scrollable container (`overflow-x: auto`) on mobile.
- **Base Font Size**: Never go below `14px` for body text on mobile. Ideally `16px` for `<input>` fields to prevent iOS Safari from auto-zooming when the user focuses on an input.

### Mobile Viewport & Input Constraints (移动端视口与键盘防挡)
Mobile browsers have dynamic address bars and virtual keyboards that break traditional CSS:
1. **Never use `100vh` for full-screen containers**: The `100vh` unit calculates the screen height *including* the hidden address bar, causing the bottom of your UI to be cut off on iOS/Android. ALWAYS use `100dvh` (Dynamic Viewport Height) instead.
2. **Safe Areas**: For bottom-fixed elements (like nav bars), always add padding for modern phones with physical notches/home bars: `padding-bottom: calc(env(safe-area-inset-bottom) + 16px)`.
3. **Virtual Keyboard Overflow**: When a user taps an `<input>`, the virtual keyboard pops up and shrinks the viewport. 
   - NEVER use `position: fixed; bottom: 0` for text inputs (it will detach or hide behind the keyboard).
   - Ensure the container uses `dvh` and is scrollable. 
   - Use the `scrollIntoView()` JS API on input focus if necessary, or rely on CSS `interactive-widget: overlays-content`.

Remember: Claude is capable of extraordinary creative work. Don't hold back, show what can truly be created when thinking outside the box and committing fully to a distinctive vision.

## Slides & Presentation Generation (幻灯片生成规范)

当用户要求生成 slides / 幻灯片 / presentation 时，**必须** 使用 Reveal.js CDN，禁止自己编写 slide 引擎。

### 标准 Reveal.js CDN 引用
```html
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/reveal.js@5.1.0/dist/reveal.css">
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/reveal.js@5.1.0/dist/theme/black.css">
<script src="https://cdn.jsdelivr.net/npm/reveal.js@5.1.0/dist/reveal.js"></script>
<script>Reveal.initialize({ hash: true, slideNumber: true });</script>
```

### 高级设计要求 (禁止原始/默认样式)
1. **禁止使用默认原始样式**：虽然引用了 Reveal.js，但你必须在 `<style>` 标签中覆盖核心变量，禁止使用原始的白底黑字或简单的黑底白字。
2. **注入现代 CSS 变量**：
   - 使用现代渐变背景（例如深空灰到暗紫色的微渐变，或者玻璃态质感）。
   - 覆盖 `--r-main-font` 和 `--r-heading-font` 为现代字体（如 Inter, Roboto, 或系统默认无衬线体）。
   - 覆盖 `--r-background-color` 和 `--r-main-color`。
3. **版式与动画**：
   - 利用 CSS Grid/Flexbox 在 slide 内部分页。
   - 对重点元素使用微动画（micro-animations）、高对比度卡片（带毛玻璃特效 `backdrop-filter: blur`）。
   - 使用 Reveal.js 的 `<section data-auto-animate>` 实现平滑过渡。
4. **代码结构**：使用单文件自包含 HTML，内联所有自定义的高级 CSS。保持响应式布局。
5. **控制栏**：启用 `slideNumber: true` 和 `hash: true`。
