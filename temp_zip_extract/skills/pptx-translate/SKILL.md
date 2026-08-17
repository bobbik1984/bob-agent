---
name: pptx-translate
description: |
  Translate PowerPoint presentations (.pptx) between languages while preserving layout, formatting, and visual structure. Trigger this skill whenever the user wants to translate a presentation, slides, or deck — or mentions terms like "translate pptx", "翻译PPT", "translate slides", "中英互译", "PowerPoint translation", or asks to convert slide content to another language. Also trigger when the user uploads a .pptx file and mentions any target language. Default behavior is Chinese ↔ English auto-detection. This skill uses a context-first strategy: extract ALL text at paragraph level first, translate holistically for consistency, then write translations back.
metadata: {"openclaw": {"emoji": "🌐", "requires": {"bins": ["python3"]}, "install": [{"id": "pip-deps", "kind": "node", "label": "Install Python deps via pip"}]}}
version: 1.0.0
tags: [Utilities]
related_skills: []
---

# PPTX Translate Skill

Translate PowerPoint presentations between languages while preserving all formatting, layouts, and visual structure.

## Strategy: Context-First Translation

**Never translate slide-by-slide in isolation.** The correct workflow is:

1. **Extract** — pull all text from the entire presentation at once
2. **Translate** — send full context to LLM for consistent terminology
3. **Write back** — apply translations to the exact XML positions

This ensures consistent terminology, proper context for ambiguous phrases, and awareness of the full document scope before translating any single element.

---

## Quick Start

```bash
# Step 1: Extract all text
python3 {baseDir}/scripts/extract_text.py input.pptx > /tmp/pptx_text.json

# Step 2: Translate (LLM does this — see Translation section below)

# Step 3: Write translations back
python3 {baseDir}/scripts/apply_translations.py input.pptx /tmp/translated.json output.pptx

# Step 4: Verify
python3 {baseDir}/scripts/verify.py input.pptx output.pptx
```

---

## Detailed Workflow

### Step 1: Extract Text

```bash
python3 {baseDir}/scripts/extract_text.py <input.pptx> [--output /tmp/extracted.json]
```

Output format (paragraph-level — each element is a full paragraph with all runs merged):
```json
{
  "source_file": "presentation.pptx",
  "slide_count": 12,
  "detected_language": "zh",
  "slides": [
    {
      "slide_index": 0,
      "slide_number": 1,
      "elements": [
        {
          "element_id": "s0_sp0_p0",
          "type": "title",
          "text": "原始文字",
          "run_count": 1,
          "font_size": 36,
          "is_bold": true
        },
        {
          "element_id": "s0_sp3_g0_p0",
          "type": "text_box",
          "text": "组合内文字",
          "run_count": 1,
          "font_size": 12
        }
      ]
    }
  ],
  "all_texts": ["原始文字", "组合内文字", "..."]
}
```

### Step 2: Translate (LLM Task)

After extraction, you (the LLM agent) perform the translation. Follow these rules:

**Read the resources guide first:**
→ See `{baseDir}/resources/translation_guide.md` for domain-specific terminology handling, tone guidelines, and common pitfalls.

**Translation prompt template** (use this when calling yourself or another LLM):

```
You are a professional translator specializing in business presentations.

Source language: {detected_language}
Target language: {target_language}

CONTEXT: You are translating a complete presentation. Maintain consistent terminology throughout. The full original text is provided so you understand the complete context before translating.

RULES:
- Preserve all formatting markers: \n (newlines), bullet symbols (•, -, *), numbers
- Do NOT translate: proper nouns, brand names, technical terms marked with [KEEP], URLs, email addresses
- Keep the same approximate length when possible (slide space is fixed)
- For Chinese→English: use professional business register, not literal translation
- For English→Chinese: use Simplified Chinese, formal register

Return ONLY a JSON array in this exact format:
[
  {"id": "s0_sp0_p0", "original": "原始文字", "translation": "Original Text"},
  ...
]

SOURCE TEXTS (translate all of these):
{json_array_of_all_texts_with_ids}
```

**Important:** Send ALL texts in ONE request for context-aware translation. Do not split by slide.

### Step 3: Apply Translations

```bash
python3 {baseDir}/scripts/apply_translations.py \
  <input.pptx> \
  <translations.json> \
  <output.pptx> \
  [--backup]
```

The script:
- Matches translations by `element_id` (paragraph-level IDs like `s0_sp0_p0`)
- Recursively processes group shapes (IDs like `s0_sp3_g0_p0`)
- Puts the full translation in the first run of the paragraph, clears remaining runs
- Preserves all XML attributes (font, size, color, position) from the first run
- Also supports legacy run-level IDs (`s0_sp0_p0_r0`) for backward compatibility
- Creates a backup if `--backup` is specified

### Step 4: Verify

```bash
python3 {baseDir}/scripts/verify.py <original.pptx> <translated.pptx>
```

Checks:
- Slide count matches
- All text elements have been translated (no untouched source text remaining)
- No XML corruption
- Reports any elements that were skipped or failed

---

## Language Detection & Defaults

- **Default**: Auto-detect source language, translate to the other (Chinese ↔ English)
- If source is Chinese (zh) → translate to English (en)
- If source is English (en) → translate to Chinese (zh)
- User can override: "translate to Japanese", "translate to French", etc.

To specify: pass `--target-lang <code>` to extract_text.py (writes into JSON metadata).

---

## Element ID Format

- **Top-level shapes**: `s{slide}_sp{shape}_p{para}` — e.g. `s0_sp2_p0`
- **Group shape children**: `s{slide}_sp{shape}_g{child}_p{para}` — e.g. `s0_sp3_g0_p0`
- **Nested groups**: additional `_g{idx}` segments — e.g. `s0_sp3_g0_g1_p0`

Both extract and apply scripts recursively traverse group shapes, so text inside grouped objects (common in award cards, icon+label combos, etc.) is fully supported.

---

## Handling Special Cases

**Group shapes**: Text inside grouped objects is automatically extracted and translated. No special handling needed — the scripts recurse into groups.

**Mixed-language slides**: Elements already in the target language are marked `"skip": true` in extraction output. Do not translate these.

**Text with variables/placeholders**: Text containing `{...}` or `[...]` patterns — preserve them exactly.

**Numbers-only paragraphs** and **single symbols**: extraction script auto-marks as `"skip": true`. Note: short Chinese text (1-2 chars) is NOT skipped since it may be part of a meaningful paragraph.

**Speaker notes**: Extracted separately under `"notes"` key in each slide object. Translate them too unless user says otherwise.

**Watermark/DRM strings**: Some PPTX files contain hidden hex strings at 1pt font size. These are extracted but should not be translated — exclude them from translation input.

---

## Dependencies

Install once:
```bash
pip3 install python-pptx langdetect --break-system-packages
```

All scripts use only `python-pptx` and standard library. No external APIs needed for extract/apply steps.
