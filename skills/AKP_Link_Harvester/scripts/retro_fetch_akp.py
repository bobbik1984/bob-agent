import os
import re
import sys
from pathlib import Path
from fetch_content import fetch_text
import yaml

def retro_fetch():
    script_dir = Path(__file__).resolve().parent
    akp_raw_dir = script_dir.parent.parent.parent / "AKP" / "raw"
    
    if not akp_raw_dir.exists():
        print(f"Directory not found: {akp_raw_dir}")
        return

    count = 0
    for root, dirs, files in os.walk(akp_raw_dir):
        for file in files:
            if not file.endswith('.md'): continue
            
            filepath = os.path.join(root, file)
            with open(filepath, 'r', encoding='utf-8') as f:
                content = f.read()
                
            if '> 内容通过外部工具提取，原始链接见 source 字段' not in content:
                continue
                
            # Parse URL from frontmatter using Regex to avoid YAML parse errors
            m = re.search(r'^source:\s*(https?://.+)$', content, re.MULTILINE)
            if not m:
                continue
            
            url = m.group(1).strip()
                
            if not url:
                continue
                
            print(f"Fetching full content for {file} (URL: {url})...")
            try:
                raw_text = fetch_text(url)
                if not raw_text or raw_text.startswith("获取内容失败"):
                    print(f"  [Failed] {raw_text}")
                    continue
                
                # Replace the placeholder
                new_content = content.replace(
                    '> 内容通过外部工具提取，原始链接见 source 字段',
                    raw_text
                )
                
                with open(filepath, 'w', encoding='utf-8') as f:
                    f.write(new_content)
                    
                print(f"  [Success] Saved {len(raw_text)} chars to {file}")
                count += 1
            except Exception as e:
                print(f"  [Error] {str(e)}")

    print(f"Retroactively fetched content for {count} files.")

if __name__ == "__main__":
    retro_fetch()
