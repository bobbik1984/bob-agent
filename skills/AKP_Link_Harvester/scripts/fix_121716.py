import os
import re
import sys
from fetch_content import fetch_text

def fix_file():
    file_path = r'D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\AKP\raw\article\Clipping_20260412_121716.md'
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()

    m = re.search(r'^source:\s*(https?://.+)$', content, re.MULTILINE)
    if not m:
        print("URL not found")
        return
        
    url = m.group(1).strip()
    raw_text = fetch_text(url)
    
    # Split content precisely at the Raw Data header
    parts = content.split('## 📥 原始内容 (Raw Data)')
    if len(parts) != 2:
        print("Header not found")
        return
        
    new_content = parts[0] + '## 📥 原始内容 (Raw Data)\n' + raw_text + '\n\n---\n*Upgraded by AKP_Link_Harvester v2.0*\n'
    
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(new_content)
        
    print(f"Fixed {file_path}! Appended {len(raw_text)} chars.")

if __name__ == '__main__':
    fix_file()
