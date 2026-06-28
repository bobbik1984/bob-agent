"""note_graphify — 个人笔记图谱化 Skill

将 OneNote/Markdown 笔记批量提取为知识图谱节点和边，
支持增量更新，可持续化运行。
"""

import json
import re
import sys
import argparse
from pathlib import Path
from datetime import datetime

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

# ── 主题关键词规则表 ──
TOPIC_RULES = {
    "量化交易": ["stock", "quant", "trading", "策略", "交易", "finance", "金融", "复现", "指标", "cpa"],
    "建筑设计": ["design", "设计", "建筑", "项目", "施工", "工程", "vauxhall", "万象", "华润", "公寓", "物业", "办公"],
    "AI与自动化": ["ai", "agent", "llm", "skill", "openclaw", "deepseek", "gemini", "mcp", "自动", "智能", "助手"],
    "旅行与地理": ["travel", "japan", "mexico", "map", "地图", "咖啡", "旅", "高铁"],
    "创意灵感": ["游戏", "方块", "拼图", "lego", "培养皿", "大航海", "科幻", "podcast"],
    "个人成长": ["dream job", "做更好", "考核", "经手", "知识", "进化", "xiaomi", "books"],
    "情报系统": ["新闻", "intelligence", "news", "速报", "信息", "搜索"],
    "技术工具": ["python", "website", "web", "app", "rendering", "pano", "viewer", "装机"],
}

# ── 桥接关键词（用于检测笔记引用了现有图谱中的哪些项目/设备）──
BRIDGE_KEYWORDS = {
    "openclaw": "proj_openclaw",
    "code_runner": "proj_code_runner",
    "coderunner": "proj_code_runner",
    "quant lab": "proj_quant_lab",
    "quant_lab": "proj_quant_lab",
    "vps1": "vps1",
    "vps2": "vps2",
    "vps3": "vps3",
    "时空华夏": "proj_spacetime_china",
    "i know": "proj_iknow",
    "iknow": "proj_iknow",
}


def parse_frontmatter(text: str):
    """解析 YAML frontmatter"""
    m = re.match(r"^---\s*\n(.*?)\n---\s*\n", text, re.DOTALL)
    if not m:
        return {}, text
    
    fm = {}
    for line in m.group(1).splitlines():
        if ":" in line:
            k, v = line.split(":", 1)
            fm[k.strip()] = v.strip().strip('"').strip("'")
    
    body = text[m.end():]
    return fm, body


def extract_topics(title: str, body: str) -> list:
    """基于规则匹配主题"""
    combined = (title + " " + body).lower()
    topics = []
    for topic, keywords in TOPIC_RULES.items():
        for kw in keywords:
            if kw.lower() in combined:
                topics.append(topic)
                break
    return topics if topics else ["未分类"]


def extract_bridges(body: str) -> list:
    """检测笔记是否引用了现有图谱中的实体"""
    lower = body.lower()
    bridges = []
    for keyword, node_id in BRIDGE_KEYWORDS.items():
        if keyword in lower:
            bridges.append(node_id)
    return list(set(bridges))


def sanitize_id(title: str) -> str:
    """生成安全的节点 ID"""
    s = re.sub(r"[^\w\u4e00-\u9fff]", "_", title)
    s = re.sub(r"_+", "_", s).strip("_")
    return f"note_{s[:60]}"


def graphify_notes(notes_dir: str, graph_path: str, mode: str = "rule"):
    notes_path = Path(notes_dir)
    graph_file = Path(graph_path)
    
    # 加载现有图谱
    if graph_file.exists():
        data = json.loads(graph_file.read_text(encoding="utf-8"))
    else:
        data = {"nodes": [], "edges": [], "stats": {}}
    
    existing_ids = {n["id"] for n in data["nodes"]}
    existing_edges = {(e["source"], e["target"], e["relation"]) for e in data["edges"]}
    
    # 收集所有要处理的 md 文件
    md_files = sorted(notes_path.glob("*.md"))
    print(f"📂 Found {len(md_files)} markdown files in {notes_path}")
    
    new_nodes = []
    new_edges = []
    topic_nodes = set()
    topic_members = {}  # topic -> [(created_date, note_id), ...]
    
    for md_file in md_files:
        text = md_file.read_text(encoding="utf-8", errors="replace")
        fm, body = parse_frontmatter(text)
        
        title = fm.get("title", md_file.stem)
        created = fm.get("created", "")
        modified = fm.get("modified", "")
        
        note_id = sanitize_id(title)
        
        # 跳过已存在的
        if note_id in existing_ids:
            continue
        
        # 提取摘要（前 200 字）
        clean_body = re.sub(r"\s+", " ", body).strip()
        summary = clean_body[:200] + "..." if len(clean_body) > 200 else clean_body
        
        # 提取主题
        topics = extract_topics(title, body)
        
        # 创建笔记节点
        new_nodes.append({
            "id": note_id,
            "label": title,
            "type": "note",
            "created_at": created,
            "modified_at": modified,
            "metadata": {
                "description": summary,
                "source": "onenote",
                "file": md_file.name,
                "topics": topics,
            }
        })
        existing_ids.add(note_id)
        
        # 收集主题关联
        for topic in topics:
            topic_id = f"topic_{re.sub(r'[^\w\u4e00-\u9fff]', '_', topic)}"
            topic_nodes.add((topic_id, topic))
            
            edge_key = (note_id, topic_id, "discusses")
            if edge_key not in existing_edges:
                new_edges.append({
                    "source": note_id,
                    "target": topic_id,
                    "relation": "discusses",
                    "confidence": "EXTRACTED"
                })
                existing_edges.add(edge_key)
            
            if topic_id not in topic_members:
                topic_members[topic_id] = []
            topic_members[topic_id].append((created, note_id))
        
        # 桥接到现有图谱
        bridges = extract_bridges(body)
        for bridge_id in bridges:
            if bridge_id in existing_ids:
                edge_key = (note_id, bridge_id, "inspired")
                if edge_key not in existing_edges:
                    new_edges.append({
                        "source": note_id,
                        "target": bridge_id,
                        "relation": "inspired",
                        "confidence": "INFERRED"
                    })
                    existing_edges.add(edge_key)
    
    # 创建主题节点
    for topic_id, topic_label in topic_nodes:
        if topic_id not in existing_ids:
            new_nodes.append({
                "id": topic_id,
                "label": topic_label,
                "type": "topic",
                "metadata": {
                    "description": f"个人认知主题: {topic_label}",
                    "note_count": len(topic_members.get(topic_id, [])),
                }
            })
            existing_ids.add(topic_id)
    
    # 构建演化链 (evolved_into)
    evolution_count = 0
    for topic_id, members in topic_members.items():
        sorted_members = sorted(members, key=lambda x: x[0] or "")
        for i in range(len(sorted_members) - 1):
            src = sorted_members[i][1]
            tgt = sorted_members[i + 1][1]
            edge_key = (src, tgt, "evolved_into")
            if edge_key not in existing_edges:
                new_edges.append({
                    "source": src,
                    "target": tgt,
                    "relation": "evolved_into",
                    "confidence": "INFERRED"
                })
                existing_edges.add(edge_key)
                evolution_count += 1
    
    # 合并到图谱
    data["nodes"].extend(new_nodes)
    data["edges"].extend(new_edges)
    data["stats"]["node_count"] = len(data["nodes"])
    data["stats"]["edge_count"] = len(data["edges"])
    
    # 写出
    graph_file.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    
    print(f"\n✅ Note Graphify 完成!")
    print(f"   📝 新增笔记节点: {len([n for n in new_nodes if n['type'] == 'note'])}")
    print(f"   🏷️  新增主题节点: {len([n for n in new_nodes if n['type'] == 'topic'])}")
    print(f"   🔗 新增关联边:   {len(new_edges)}")
    print(f"   🧬 演化链边:     {evolution_count}")
    print(f"   📊 图谱总计:     {data['stats']['node_count']} nodes, {data['stats']['edge_count']} edges")
    print(f"   💾 输出: {graph_file}")


def main():
    parser = argparse.ArgumentParser(description="Note Graphify — 个人笔记图谱化")
    parser.add_argument("--notes", default="data/onenote_export/pages/", help="笔记目录")
    parser.add_argument("--graph", default="ecosystem.graph.json", help="目标图谱")
    parser.add_argument("--mode", default="rule", choices=["rule", "llm"], help="提取模式")
    args = parser.parse_args()
    
    graphify_notes(args.notes, args.graph, args.mode)


if __name__ == "__main__":
    main()
