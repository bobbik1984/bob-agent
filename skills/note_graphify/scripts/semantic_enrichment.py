"""semantic_enrichment.py — 基于 LLM 深度语义分析的图谱增强

由 Antigravity (Gemini) 对全部 94 篇笔记进行语义分析后生成的增强关联。
这些关联是规则引擎无法发现的"隐性联系"。
"""

import json
from pathlib import Path

# ═══════════════════════════════════════════════════
# Gemini 语义分析结果：跨笔记的隐性关联
# ═══════════════════════════════════════════════════

SEMANTIC_EDGES = [
    # ── "记忆图像化" 就是 I Know 的原型想法 ──
    # 2025年你写下"记忆图像化 三维多维"，现在 I Know 的知识图谱可视化正是这个想法的实现
    {"source": "note_记忆图像化", "target": "proj_iknow", "relation": "inspired", "confidence": "SEMANTIC",
     "reason": "笔记中'记忆图像化 三维多维'的概念，正是 I Know 知识图谱可视化的原型想法"},

    # ── "宠物的电子生命" 是 AI 应用方向 ──
    # 宠物离世后的数字替身、在特定地点打卡、社交——这是典型的 AI + AR 应用
    {"source": "note_宠物的电子生命", "target": "topic_AI与自动化", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "数字宠物替身涉及 AI 生成、AR 空间定位、社交网络——是纯粹的 AI 应用产品"},

    # ── "Everything is Related" 是 I Know 的哲学根基 ──
    # 2021年你就写下"关键词间的联系，相关性，影响力，多方位联系"——这就是知识图谱的核心哲学
    {"source": "note_Everything_is_Related", "target": "proj_iknow", "relation": "inspired", "confidence": "SEMANTIC",
     "reason": "2021年写的'关键词间的联系、相关性、影响力'正是知识图谱的核心哲学，5年后在 I Know 中实现"},

    # ── "思考和实现的路径" 定义了你的 AI 方法论 ──
    # "怎么用AI减轻重复工作？怎么替代我？"——这是你构建整个 OpenClaw+CodeRunner 的根本动机
    {"source": "note_思考和实现的路径", "target": "proj_openclaw", "relation": "inspired", "confidence": "SEMANTIC",
     "reason": "'怎么使用AI来减轻重复工作？怎么替代我？'——这是构建 OpenClaw 自主系统的根本动机"},
    {"source": "note_思考和实现的路径", "target": "proj_code_runner", "relation": "inspired", "confidence": "SEMANTIC",
     "reason": "'寻找痛点，什么是花最多时间的事？'——CodeRunner 正是为了自动化编码痛点而生"},

    # ── "室内地图 Mall" (MallOS) 是你最大的独立项目 ──
    # 29KB 的巨型笔记，完整的商业软件设计，含授权体系、部署架构
    {"source": "note_室内地图_Mall", "target": "topic_建筑设计", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "MallOS 是建筑/商业地产领域的数字化管理平台"},
    {"source": "note_室内地图_Mall", "target": "topic_AI与自动化", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "MallOS 涉及 Web 3D 可视化、自动化协同管理、WeChat 鉴权等技术栈"},
    {"source": "note_室内地图_Mall", "target": "topic_技术工具", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "MallOS 使用 Leaflet、Flask、Docker、Nginx 等完整技术栈"},

    # ── "Pano Viewer" 和 "室内地图 Mall" 属于同一产品家族 ──
    {"source": "note_Pano_Viewer", "target": "note_室内地图_Mall", "relation": "evolved_into", "confidence": "SEMANTIC",
     "reason": "Pano Viewer (360°全景浏览) 是 MallOS 室内地图系统的子模块"},

    # ── "咖啡地图" 和 "室内地图 Mall" 共享空间可视化理念 ──
    {"source": "note_咖啡地图", "target": "note_室内地图_Mall", "relation": "related_to", "confidence": "SEMANTIC",
     "reason": "咖啡地图(POI 标注+搜索)与 MallOS(空间可视化+管理)共享地理信息可视化理念"},

    # ── "Dream Job" 勾画了你的建筑+科技交叉志向 ──
    {"source": "note_Dream_Job", "target": "topic_个人成长", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "职业理想横跨 Airport Design、Product Manager、Factory/Data Center、SpaceX/Tesla"},
    {"source": "note_Dream_Job", "target": "note_Xiaomi", "relation": "related_to", "confidence": "SEMANTIC",
     "reason": "Dream Job 列出了 Tesla/Xiaomi/Google，Xiaomi 笔记则是这个方向的实际求职记录"},

    # ── "000——尝试了什么？" 是你的项目总索引 ──
    # 列出了 7 个尝试：Wanderwise、语音克隆、3D Tetris、高铁在哪儿、攒机app、MallOS、大航海
    {"source": "note_000__尝试了什么_", "target": "note_室内地图_Mall", "relation": "references", "confidence": "SEMANTIC",
     "reason": "尝试清单第6项 'Indoor Map MallOS'"},
    {"source": "note_000__尝试了什么_", "target": "note_大航海_or_我的世界", "relation": "references", "confidence": "SEMANTIC",
     "reason": "尝试清单第7项 '大航海游戏 10%'"},
    {"source": "note_000__尝试了什么_", "target": "note_立体俄罗斯方块", "relation": "references", "confidence": "SEMANTIC",
     "reason": "尝试清单第3项 '3D环形Tetris'"},
    {"source": "note_000__尝试了什么_", "target": "note_高铁运行", "relation": "references", "confidence": "SEMANTIC",
     "reason": "尝试清单第4项 '高铁在哪儿'"},

    # ── "大航海 or 我的世界" 是游戏设计笔记 ──
    {"source": "note_大航海_or_我的世界", "target": "topic_创意灵感", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "详细的游戏系统设计：城市港口、船只、人物、发现物、存档系统"},

    # ── "培养皿里的争斗" 可能是 AI Agent 对抗模拟 ──
    {"source": "note_培养皿里的争斗", "target": "topic_AI与自动化", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "'培养皿里的争斗'暗示受限空间内的竞争模拟，类似 AI Agent 多智体对抗演化"},

    # ── "帮我做决定" 是 AI 辅助决策工具 ──
    {"source": "note_帮我做决定", "target": "topic_AI与自动化", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "'左右为难'时让 AI 辅助决策——是 AI Agent 产品化的典型场景"},

    # ── "个人知识的进化" 直接催生了 I Know 项目 ──
    {"source": "note_个人知识的进化", "target": "proj_iknow", "relation": "inspired", "confidence": "SEMANTIC",
     "reason": "笔记中写到'追踪认知的变化、怎么进入检索的数据库、怎么融入RAG、最后建立关联'——这就是 I Know 的产品需求文档"},

    # ── "小助手" 记录了 OpenClaw 的完整诞生过程 ──
    {"source": "note_小助手", "target": "proj_openclaw", "relation": "inspired", "confidence": "SEMANTIC",
     "reason": "笔记完整记录了从飞书集成、MCP服务、多模型路由到Web Portal的OpenClaw架构演化"},
    {"source": "note_小助手", "target": "vps1", "relation": "references", "confidence": "SEMANTIC",
     "reason": "笔记中讨论了 VPS2/VPS3 的 DeepSeek+Qwen 配置"},
    {"source": "note_小助手", "target": "vps2", "relation": "references", "confidence": "SEMANTIC",
     "reason": "笔记中讨论了 VPS2 的模型路由"},

    # ── 跨域创意关联 ──
    {"source": "note_Xiaomi", "target": "topic_建筑设计", "relation": "discusses", "confidence": "SEMANTIC",
     "reason": "Xiaomi笔记的核心是建筑设计背景下向科技工厂领域的职业转型"},
    {"source": "note_新闻速报", "target": "proj_openclaw", "relation": "inspired", "confidence": "SEMANTIC",
     "reason": "新闻速报的情报收集需求直接催生了 OpenClaw 上的 intelligence 技能体系"},
]


def apply_enrichment(graph_path: str):
    path = Path(graph_path)
    data = json.loads(path.read_text(encoding="utf-8"))

    existing_ids = {n["id"] for n in data["nodes"]}
    existing_edges = {(e["source"], e["target"], e["relation"]) for e in data["edges"]}

    added = 0
    skipped_missing = 0

    for edge in SEMANTIC_EDGES:
        key = (edge["source"], edge["target"], edge["relation"])

        # 跳过重复边
        if key in existing_edges:
            continue

        # 跳过源或目标不存在的
        if edge["source"] not in existing_ids or edge["target"] not in existing_ids:
            skipped_missing += 1
            print(f"  ⚠️ 跳过 (节点不存在): {edge['source']} → {edge['target']}")
            continue

        data["edges"].append({
            "source": edge["source"],
            "target": edge["target"],
            "relation": edge["relation"],
            "confidence": edge["confidence"],
            "metadata": {"reason": edge.get("reason", "")},
        })
        existing_edges.add(key)
        added += 1
        print(f"  ✅ {edge['source']} →[{edge['relation']}]→ {edge['target']}")

    data["stats"]["edge_count"] = len(data["edges"])
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")

    print(f"\n🧬 语义增强完成!")
    print(f"   新增语义边: {added}")
    print(f"   跳过 (节点缺失): {skipped_missing}")
    print(f"   图谱总计: {data['stats']['node_count']} nodes, {data['stats']['edge_count']} edges")


if __name__ == "__main__":
    import sys
    graph = sys.argv[1] if len(sys.argv) > 1 else "ecosystem.graph.json"
    apply_enrichment(graph)
