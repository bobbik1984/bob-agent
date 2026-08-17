---
name: web-style-analyzer
description: >
  Analyze a website's visual design, UI style, color palette, layout composition, typography, and component patterns... Trigger when the user asks to "analyze style", "capture design", "extract UI", "clone aesthetic" etc. (e.g. "分析网站风格", "提取配色", "做成模板参考"). ⚠️ BOUNDARY NOTE: If the user simply asks to "collect", "save", or "clip" a URL for its text knowledge/news content without mentioning UI/Design/Style, **DO NOT** use this skill. Use the `AKP_Link_Harvester` skill instead. This skill is STRICTLY for front-end design aesthetics and CSS extraction.
version: 1.0.0
tags: [Development]
related_skills: []
---

# Web Style Analyzer Skill

Analyze a website's visual design from a URL or screenshot and produce a structured, reusable style reference document.

This skill has **two companion files** that accumulate over time:
- `examples.md` — your personal style library (past analyses you've saved)
- `tokens.json` — your preferred CSS design tokens / baseline variables

**Always read both files at the start of every session** (use the `view` tool). They inform how you frame the analysis and what the output should align with.

---

## Step 0 — Load Context

Before doing anything else:

1. Read `examples.md` — understand what styles the user has saved before. Note recurring themes, preferred aesthetics, patterns they keep returning to.
2. Read `tokens.json` — understand the user's current CSS baseline (colors, spacing, radii, etc.).
3. If both files are empty/blank, proceed without personalization. If they have content, use them to:
   - Compare the new site against past examples ("this is similar to X you saved before, but differs in Y")
   - Flag where the new site's tokens conflict with or complement the user's existing token system

---

## Step 1 — Retrieve & Observe

**Mode A — URL provided:**
Use `web_fetch` to retrieve the page. Extract:
- CSS variables (`:root` block)
- Inline styles and `<style>` blocks
- Class names (infer framework: Tailwind, Bootstrap, custom)
- HTML structure to understand layout hierarchy

**Mode B — Screenshot provided:**
Analyze the image directly. Infer all design properties visually. No fetching needed.

Both modes produce the same output.

---

## Step 2 — Extract Design Properties

Systematically extract all of the following:

### 🎨 Color Palette
- Background colors (page, sections, cards)
- Primary / accent colors (buttons, links, highlights)
- Text colors (headings, body, muted/secondary)
- Border and divider colors
- Gradient definitions (include exact angle + stops)
- All values as HEX. Group by semantic role.

### 🔤 Typography
- Font families (heading vs body; Google Fonts / system / custom)
- Font size scale (xs / sm / base / lg / xl / 2xl approximation)
- Font weights in use
- Line height and letter spacing feel (tight / normal / loose)
- Text transform patterns (uppercase labels, etc.)

### 📐 Layout & Composition
- Overall model: centered column / full-width / sidebar / grid
- Max content width
- Section structure: how the page divides vertically
- Whitespace style: compact / airy / spacious
- Grid or flex patterns for component layout

### 🧩 UI Components
For each visible component (navbar, hero, cards, buttons, forms, footer, tags, etc.):
- Visual style: flat / outlined / elevated / glassmorphism / neumorphic
- Border radius: none / subtle / rounded / pill
- Shadow: none / soft / strong / colored
- Hover/interactive state (if inferable)
- Internal spacing pattern

### ✨ Visual Style & Mood
- Overall aesthetic: minimal / corporate / editorial / playful / dark / brutalist / elegant / techy / organic
- Design era: flat / material / neo-brutalist / glassmorphism / skeuomorphic
- Illustration / icon style
- Animation hints (transitions, parallax, scroll effects)
- Signature details that make this site distinctive

---

## Step 3 — Produce the Reference Document

Output a **single self-contained HTML file**. Structure:

### Section 1 — Visual Summary
3–5 sentences: design personality, target audience feel, what makes it distinctive. If `examples.md` has entries, note similarity/difference to past saved styles.

### Section 2 — Color Palette
Rendered color swatches with HEX labels, grouped by role:
`Background | Text | Accent/Brand | Neutral/Border`

### Section 3 — Typography Specimen
Each font family rendered with a heading + body sample. Include `font-family` CSS stack.

### Section 4 — CSS Tokens
A ready-to-paste `:root {}` block. If `tokens.json` exists, produce **two versions side by side**:
- "This site's tokens" — extracted from the analyzed site
- "Merged suggestion" — how to integrate useful values into the user's existing token system, flagging conflicts

```css
:root {
  /* Colors */
  --color-bg: #0f0f0f;
  --color-primary: #6366f1;
  --color-text: #e5e7eb;
  --color-muted: #6b7280;
  --color-border: #1f2937;

  /* Typography */
  --font-heading: 'Inter', sans-serif;
  --font-body: 'Inter', sans-serif;
  --text-base: 16px;
  --leading-body: 1.6;

  /* Layout */
  --max-width: 1200px;
  --section-gap: 80px;

  /* Components */
  --radius-sm: 4px;
  --radius-base: 8px;
  --radius-lg: 16px;
  --radius-pill: 999px;
  --shadow-card: 0 4px 24px rgba(0,0,0,0.12);
  --shadow-hover: 0 8px 32px rgba(0,0,0,0.2);
}
```

### Section 5 — Component Patterns
For each major component:
- **Rendered live example** styled to match the source site
- **Raw CSS snippet** for copy-paste
- Brief design note (why it looks this way, what technique is used)

Minimum: Button (primary + secondary), Card, Navbar, Section heading, Tag/badge. Add others if present.

### Section 6 — Layout Skeleton
Minimal HTML + CSS skeleton reproducing the page's structural layout. Every section commented. Ready to use as a starter template.

### Section 7 — Design Notes & Reuse Tips
- What this design does particularly well
- Specific replicable techniques ("subtle noise texture via SVG filter", "card lift uses cubic-bezier(0.34, 1.56, 0.64, 1)")
- Gotchas ("hero effect requires backdrop-filter; check browser support")
- Font sources and fallbacks

---

## Step 4 — Generate Archive Entry

After producing the reference document, also output a compact **archive entry** formatted exactly like this, ready for the user to paste into `examples.md`:

```markdown
## [Site Name] — [Date]
**URL**: https://...
**Aesthetic tags**: minimal, dark, techy  ← 2–4 tags
**Color mood**: dark background, indigo accent, high contrast
**Typography**: Inter / system-ui, clean, tight headings
**Layout**: centered column, 1200px max, airy spacing
**Signature details**: subtle grain overlay, card hover lift, pill badges
**Tokens highlight**: --color-primary: #6366f1; --radius-base: 8px; --shadow-card: 0 4px 24px rgba(0,0,0,0.12)
**Reuse rating**: ⭐⭐⭐⭐ — highly reusable structure
**Notes**: [1–2 sentences on what's worth borrowing]
```

Tell the user: *"将以上内容粘贴到 examples.md 即可存档。"*

---

## Step 5 — Output Files

Save the HTML reference to the shared templates library:
```
common/templates/web/style-ref-[site-name].html
```

Call `write_to_file` to deliver it.

Also provide a 4–5 bullet inline summary of the site's design fingerprint in the chat — the user should get the gist immediately without opening the file.

---

## Quality Standards

- **Color accuracy**: Extract from CSS or match visually. Never approximate without flagging with `/* approx */`.
- **Component fidelity**: Rendered examples must look recognizably like the source. Not generic defaults.
- **Token usability**: Every CSS token must be a value that can be dropped into a real project unchanged.
- **Reusability**: Every snippet must be copy-pasteable and work standalone.
- **Context awareness**: If `examples.md` has past entries, always reference them. The user is building a cumulative library — make the connections explicit.
- **Dark/light fidelity**: Reference document background should match the analyzed site's dominant theme.

---

## Edge Cases

| Situation | Action |
|---|---|
| URL is JS-heavy / returns blank HTML | Note limitation; analyze any partial CSS retrieved; ask for screenshot |
| Paywalled or auth-required page | Tell user; ask for screenshot |
| Screenshot is low resolution | Best-effort; flag uncertain values with `/* approx */` |
| CSS-in-JS (no extractable styles) | Infer visually; note "styles inferred visually" throughout |
| User wants one component only | Focus Section 5; keep all other sections brief (1–2 lines each) |
| User wants to compare two sites | Full workflow for both; add a Comparison section after Section 1 |
| User says "build a page like this" | Run full analysis, then immediately use the output tokens + skeleton as the basis for the new page |
