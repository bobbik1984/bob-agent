#!/usr/bin/env python3
"""
AKP 历史剪报满血复活器
读取 AKP/links 下的旧版剪报，提取其总结、标签和 URL，使用新版 MarkItDown 获取完整原文，
生成满血版存入 AKP/raw/{type}，然后将旧文件清理。
"""
import os
import re
import shutil
import time
from fetch_content import fetch_text
from save_to_workspace import detect_type_from_url

def parse_old_clipping(filepath):
    with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
        content = f.read()

    # 提取 frontmatter
    yaml_match = re.search(r'^---\s*\n(.*?)\n---\s*\n', content, re.DOTALL)
    if not yaml_match:
        return None
    
    yaml_content = yaml_match.group(1)
    
    url = None
    tags = ""
    date_str = ""
    content_type = "article"
    
    for line in yaml_content.split('\n'):
        if line.startswith('source:'):
            url = line.replace('source:', '').strip()
        elif line.startswith('tags:'):
            # 提取 [...] 里的内容
            tag_match = re.search(r'\[(.*?)\]', line)
            if tag_match:
                tags = tag_match.group(1)
        elif line.startswith('date:'):
            date_str = line.replace('date:', '').strip()
        elif line.startswith('type:'):
            content_type = line.replace('type:', '').strip()

    if not url or "http" not in url:
        return None

    # 提取摘要 (旧文件通常在 ## 核心摘要 & 论点 下面)
    summary_match = re.search(r'## .*?核心摘要.*?\n(.*?)(?:\n---|$)', content, re.DOTALL)
    summary = summary_match.group(1).strip() if summary_match else "无摘要"

    return {
        "url": url,
        "tags": tags,
        "date_str": date_str,
        "content_type": content_type,
        "summary": summary
    }

def process_links():
    # 本地开发环境检测
    knowledge_dir = os.path.dirname(os.path.dirname(os.path.dirname(
        os.path.dirname(os.path.abspath(__file__))
    )))
    local_path = os.path.join(knowledge_dir, "AKP")
    
    links_dir = os.path.join(local_path, "links")
    raw_dir = os.path.join(local_path, "raw")
    archive_dir = os.path.join(local_path, "archive_links_old")
    
    if not os.path.exists(links_dir):
        print(f"Directory {links_dir} not found.")
        return

    os.makedirs(archive_dir, exist_ok=True)
    
    files = [f for f in os.listdir(links_dir) if f.endswith('.md')]
    print(f"Found {len(files)} old clippings to upgrade.")
    
    success_count = 0
    for filename in files:
        filepath = os.path.join(links_dir, filename)
        print(f"\nProcessing {filename}...")
        
        parsed = parse_old_clipping(filepath)
        if not parsed:
            print("  [!] Failed to parse frontmatter or URL, skipping.")
            continue
            
        url = parsed["url"]
        content_type = parsed["content_type"]
        if content_type == 'article' or not content_type:
            content_type = detect_type_from_url(url)
            
        print(f"  Fetching: {url}")
        
        try:
            raw_text = fetch_text(url)
            
            # 构建新的 markdown
            md_content = f"""---
date: {parsed['date_str']}
tags: [{parsed['tags']}]
source: {url}
type: {content_type}
---

# 知识剪报

## 💡 核心摘要 & 论点
{parsed['summary']}

---

## 📥 原始内容 (Raw Data)
{raw_text}

---
*Upgraded by AKP_Link_Harvester v2.0*
"""
            # 存入 raw
            target_dir = os.path.join(raw_dir, content_type)
            os.makedirs(target_dir, exist_ok=True)
            
            target_path = os.path.join(target_dir, filename)
            with open(target_path, 'w', encoding='utf-8') as f:
                f.write(md_content)
                
            # 将旧文件移入 archive（或者直接删除）
            shutil.move(filepath, os.path.join(archive_dir, filename))
            
            success_count += 1
            print(f"  [+] Upgraded and moved to raw/{content_type}/{filename}")
            
            # 暂停 1 秒防止触发反爬
            time.sleep(1)
            
        except Exception as e:
            print(f"  [!] Error processing {url}: {e}")
            
    print(f"\nUpgrade complete! Successfully upgraded {success_count} clippings.")

if __name__ == "__main__":
    process_links()
