#!/usr/bin/env python3
"""
build_prompt.py — Build a ready-to-use LLM translation prompt from extraction JSON.

Usage:
    python3 build_prompt.py extracted.json
    python3 build_prompt.py extracted.json --target-lang fr
    python3 build_prompt.py extracted.json --include-notes

Outputs a prompt to stdout that can be sent directly to an LLM.
The LLM should return a JSON array of {id, original, translation} objects.
"""

import argparse
import json
import sys
from pathlib import Path

LANG_NAMES = {
    "zh": "Simplified Chinese",
    "en": "English",
    "ja": "Japanese",
    "ko": "Korean",
    "fr": "French",
    "de": "German",
    "es": "Spanish",
    "pt": "Portuguese",
    "ru": "Russian",
    "ar": "Arabic",
}


def main():
    parser = argparse.ArgumentParser(description="Build LLM translation prompt from extracted JSON")
    parser.add_argument("extracted", help="Extracted JSON file from extract_text.py")
    parser.add_argument("--target-lang", help="Override target language code")
    parser.add_argument("--include-notes", action="store_true", help="Include speaker notes")
    args = parser.parse_args()

    data = json.loads(Path(args.extracted).read_text(encoding="utf-8"))

    source_lang = data.get("detected_language", "unknown")
    target_lang = args.target_lang or data.get("target_language", "en")

    source_name = LANG_NAMES.get(source_lang, source_lang)
    target_name = LANG_NAMES.get(target_lang, target_lang)

    # Collect all translatable elements (now paragraph-level)
    items = []
    for slide in data.get("slides", []):
        for el in slide.get("elements", []):
            if not el.get("skip", False):
                items.append({
                    "id": el["element_id"],
                    "original": el["text"],
                    "context": f"Slide {slide['slide_number']}, type: {el.get('type', 'text')}",
                })

        if args.include_notes and "notes" in slide:
            notes_id = f"notes_s{slide['slide_index']}"
            items.append({
                "id": notes_id,
                "original": slide["notes"],
                "context": f"Slide {slide['slide_number']}, type: speaker_notes",
            })

    items_json = json.dumps(items, ensure_ascii=False, indent=2)

    prompt = f"""You are a professional translator specializing in business presentations.

Source language: {source_name} ({source_lang})
Target language: {target_name} ({target_lang})
Total elements to translate: {len(items)}

## CONTEXT
You are translating a complete presentation with {data.get('slide_count', '?')} slides.
Each element is a full paragraph (may contain multiple sentences). Read ALL source texts below before translating any of them — this ensures consistent terminology throughout the presentation.

## TRANSLATION RULES
1. Preserve formatting: keep \\n (newlines), bullet symbols (•, -, –, *, ◎), numbered lists, and spacing patterns
2. Do NOT translate: URLs, email addresses, brand names, proper nouns that should remain in original
3. Match approximate length when possible (slide space is fixed and cannot be expanded)
4. Register: use professional business language appropriate for presentations
5. {"Chinese→English: write clear, concise English. Avoid literal word-for-word translation." if source_lang == "zh" else "English→Chinese: use Simplified Chinese (简体中文), formal register (正式语体)."}
6. Consistency: if a term appears multiple times, always translate it the same way

## OUTPUT FORMAT
Return ONLY a valid JSON array. No preamble, no explanation, no markdown code fences.
Each object must have exactly these keys: "id", "original", "translation"

Example output:
[
  {{"id": "s0_sp0_p0", "original": "季度报告", "translation": "Quarterly Report"}},
  {{"id": "s0_sp1_p0", "original": "2024年第三季度", "translation": "Q3 2024"}}
]

## SOURCE TEXTS TO TRANSLATE
{items_json}

Remember: output ONLY the JSON array. Start with [ and end with ].
"""

    print(prompt)
    print(f"\n# INFO: {len(items)} elements ready for translation ({source_name} → {target_name})", file=sys.stderr)


if __name__ == "__main__":
    main()
