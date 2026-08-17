#!/usr/bin/env python3
"""
AKP 快速捕获脚本 — 零 LLM 调用，< 2 秒完成
将 URL + 可选备注写入 inbox/ 目录，等待后续 akp_digest 批量处理
"""
import sys
import os
import json
import re
from datetime import datetime, timezone, timedelta


def detect_type(url: str) -> str:
    """根据 URL 模式自动识别内容类型"""
    url_lower = url.lower()

    # YouTube
    if any(domain in url_lower for domain in ['youtube.com', 'youtu.be']):
        return 'youtube'

    # 微信公众号
    if 'mp.weixin.qq.com' in url_lower:
        return 'article'

    # 论文/PDF
    if 'arxiv.org' in url_lower or url_lower.endswith('.pdf'):
        return 'paper'

    # Twitter/X
    if any(domain in url_lower for domain in ['twitter.com', 'x.com']):
        return 'article'

    # GitHub
    if 'github.com' in url_lower:
        return 'article'

    # 通用博客/文章
    if any(domain in url_lower for domain in [
        'medium.com', 'substack.com', 'zhihu.com',
        'juejin.cn', 'csdn.net', 'blog'
    ]):
        return 'article'

    return 'misc'


def quick_capture(url: str, note: str = "", source: str = "manual"):
    """快速捕获 URL 到 inbox/"""
    # 使用显式 WORKSPACE 常量构建路径
    HOME = os.environ.get("HOME", os.environ.get("USERPROFILE", "/home/ubuntu"))
    WORKSPACE = os.environ.get(
        "AKP_WORKSPACE",
        os.path.join(HOME, ".openclaw/workspace/common/knowledge/AKP")
    )

    # 本地开发环境检测
    # scripts/quick_capture.py → scripts → AKP_Link_Harvester → skills → knowledge
    knowledge_dir = os.path.dirname(os.path.dirname(os.path.dirname(
        os.path.dirname(os.path.abspath(__file__))
    )))
    local_path = os.path.join(knowledge_dir, "AKP")
    if os.path.exists(local_path):
        WORKSPACE = local_path

    inbox_dir = os.path.join(WORKSPACE, "inbox")
    os.makedirs(inbox_dir, exist_ok=True)

    # 生成唯一 ID
    now = datetime.now(timezone(timedelta(hours=8)))
    timestamp = now.strftime("%Y%m%d_%H%M%S")
    entry_id = f"akp_{timestamp}"

    # 检测 URL 类型
    content_type = detect_type(url)

    entry = {
        "id": entry_id,
        "url": url,
        "type": content_type,
        "source": source,
        "note": note,
        "status": "queued",
        "created_at": now.isoformat()
    }

    # 写入 inbox
    filepath = os.path.join(inbox_dir, f"{entry_id}.json")
    with open(filepath, 'w', encoding='utf-8') as f:
        json.dump(entry, f, ensure_ascii=False, indent=2)

    return entry_id, content_type, filepath


if __name__ == "__main__":
    # 解决 Windows GBK 编码问题
    import io
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

    if len(sys.argv) < 2:
        print("Usage: python quick_capture.py <URL> [备注] [来源]")
        sys.exit(1)

    url = sys.argv[1]
    note = sys.argv[2] if len(sys.argv) > 2 else ""
    source = sys.argv[3] if len(sys.argv) > 3 else "manual"

    entry_id, content_type, filepath = quick_capture(url, note, source)
    print(f"✅ 已捕获 [{content_type}] → inbox/{entry_id}.json")
