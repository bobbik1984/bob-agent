---
name: cluster_ops
description: 集群运维知识总控台。涵盖所有 VPS 节点的 IP、角色、模型策略、域名映射、Tunnel 配置、Syncthing 同步、SSH 接入、Cron 调度管线等信息。当用户询问"VPS"、"节点"、"部署"、"域名"、"IP"、"Tunnel"、"Cloudflare"、"Caddy"、"Syncthing"、"Tailscale"、"Headscale"、"模型路由"、"cron"、"心跳"、"quant_lab"、"OpenClaw"、"openclaw.json"等基础设施话题时触发此技能。也可在用户问"哪个 VPS 负责什么"、"某个服务部署在哪"、"怎么 SSH 到某台机器"时触发。⚠️防混淆：本技能仅负责基础设施知识查询，不负责执行派单（那是 assignment 技能的工作）、不负责模型价格查询（那是 model-registry 技能的工作）。
version: 2.0.0
tags: [SystemOps]
related_skills: [assignment, assign_task, heartbeat_monitor, sync_cron_jobs, system_info, model-registry]
---

# Cluster Ops — 集群运维总控台 (v2.0)

本技能是整个分布式 AI 集群的**运维百科全书**。任何 Agent 在回答基础设施相关问题之前，必须先查阅本技能的参考文件，禁止凭记忆回答。

## 核心规则

1. **禁止凭记忆回答 IP、端口、域名** — 必须现查 `references/` 目录下的文件，因为这些信息会频繁更新。
2. **禁止混淆职责** — 本技能只管"查"不管"派"。任务派发是 `assignment` 的事，模型价格是 `model-registry` 的事。
3. **如果本地参考文件信息不足**，可以查询 iKnow 知识图谱获取更深层关联（见下方 iKnow 集成）。

## 快速路由：你想查什么？

| 问题类型 | 应读取的参考文件 |
|---------|----------------|
| 节点 IP / 角色 / 硬件 / 服务商 | `references/node_inventory.md` |
| 域名 → 哪台 VPS → 什么服务 | `references/service_map.md` |
| SSH 怎么连、PuTTY Session 名 | `references/node_inventory.md` → SSH 部分 |
| Cron 定时任务 / 情报管线时间表 | `references/service_map.md` → 自动化管线 |
| 模型路由策略（哪台 VPS 用什么模型） | `references/node_inventory.md` → 各节点模型策略 |
| Syncthing / Tailscale / Cloudflare 配置 | `references/service_map.md` → 网络层 |
| 跨节点任务派发怎么工作 | `references/service_map.md` → 任务派发协议 |

## iKnow 知识图谱集成

当 `references/` 目录下的静态文件无法满足查询需求时（例如需要追溯某个服务的历史变更、查找项目间的依赖关系），可以利用 iKnow 知识图谱 API：

### 接入方式

iKnow 知识图谱服务运行于本地开发机（X1C）或已部署的服务器上：

```
Base URL: http://localhost:8000/api/graph
```

### 可用 API 端点

| 端点 | 方法 | 用途 |
|------|------|------|
| `/api/graph/stats` | GET | 图谱统计（节点数、边数、社区数） |
| `/api/graph/query?q=VPS1&hops=2` | GET | 以关键词查询局部子图（如查 VPS1 的所有关联） |
| `/api/graph/full` | GET | 获取完整知识图谱（注意数据量大） |
| `/api/graph/communities` | GET | 获取社区聚类列表 |
| `/api/graph/surprising_edges` | GET | 获取跨社区意外连接（发现隐藏关联） |
| `/api/graph/note/{node_id}/content` | GET | 读取知识节点的完整 Markdown 内容 |
| `/api/graph/note/{node_id}/relations` | GET | 获取节点的所有入边和出边关系 |

### 典型查询场景

```python
# 例1: 查询 VPS3 相关的所有知识关联
GET /api/graph/query?q=VPS3&hops=2

# 例2: 查询 quant_lab 的部署依赖链
GET /api/graph/query?q=quant_lab&hops=3

# 例3: 查询某个笔记的完整内容
GET /api/graph/note/note_deployment_guide/content
```

### 图谱中的基础设施节点类型

iKnow 知识图谱中存在 `infrastructure` 和 `device` 类型的节点，它们通过以下边关系连接：
- `deployed_on` — 项目/服务部署在哪台设备上
- `syncthing_sync` — 哪些节点之间通过 Syncthing 同步
- `tailscale_vpn` — 哪些节点在同一个 Tailscale 网络中
- `depends_on` — 服务间的依赖关系

## 参考文件

| 文件 | 用途 | 更新频率 |
|------|------|---------|
| `references/node_inventory.md` | 全部节点硬件/IP/角色/模型/SSH 的唯一真相源 | 每次节点变更时 |
| `references/service_map.md` | 域名映射、部署服务、网络拓扑、Cron 管线 | 每次服务变更时 |
