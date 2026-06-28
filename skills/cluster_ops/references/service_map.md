# 🌐 Service Map (服务映射与运维手册)

**版本**: 2.1.0
**更新日期**: 2026-04-14

---

## Cloudflare Tunnel 域名映射

所有公网服务通过 Cloudflare Tunnel 暴露，**零端口暴露**。每台 VPS 运行独立的 `cloudflared` 进程。

### VPS1 域名 (完整 Caddyfile 映射)

> 源文件: `common/Deployment/VPS1/caddy/Caddyfile`

| # | 域名 | Caddy 反代目标 | 后端服务 | 类型 |
|---|------|---------------|---------|------|
| 1 | `openclaw.bobbik.org` | `127.0.0.1:18789` | OpenClaw Gateway | WebSocket API |
| 2 | `hs.bobbik.org` | API→`:41641` / UI→`:8001` | Headscale + headscale-ui | 网络控制台 |
| 3 | ~~`work.bobbik.org`~~ | ~~`:3000`~~ | ~~Dify~~ | ⛔ 已禁用 |
| 4 | `wechat.bobbik.org` | `127.0.0.1:8088` | WeChat Bridge | 微信互通接口 |
| 5 | `monitor.bobbik.org` | `127.0.0.1:8090` | Beszel | 全局监控面板 |
| 6 | `sync.bobbik.org` | `127.0.0.1:8384` | Syncthing WebUI | 文件同步管理 |
| 7 | `panel.bobbik.org` | 静态文件 `/opt/home-panel` | — | 家庭智能显示屏 |
| 8 | `test.bobbik.org` | 静态文件 `/opt/test` | — | 测试网站聚合 |
| 9 | `village.bobbik.org` | `127.0.0.1:3080` | Mission Control | Agent 村庄看板 |
| 10 | `history.bobbik.org` | 静态文件 `/opt/history` | — | 时空华夏 (根路径→`/frontend/index.html`) |
| 11 | `t.bobbik.org` | 静态文件 `/opt/t` | — | OpenClaw 测试站 |
| 12 | `bedtime.bobbik.org` | API→`:8000` / Frontend→`:3000` | Bedtime Story | AI 绘本动画工厂 |

**Headscale 反代特殊说明**: `hs.bobbik.org` 采用了路径分流架构：
- `/api/*`, `/machine/*`, `/ts2021*`, `/register/*` → Headscale 后端 `:41641`
- 其他路径 → headscale-ui 容器 `:8001`

**`115.190.248.194` (公网 IP) 也直接解析到 Headscale 反代**，等同于访问 `hs.bobbik.org`。

**`bedtime.bobbik.org` 路径分流**:
- `/api/*` → FastAPI 后端 `:8000`
- `/assets/*` → 后端静态资源 `:8000`
- 其他 → Next.js 前端 `:3000`

### VPS3 域名 (Contabo — `89.117.22.194`)

| 域名 | Caddy 反代目标 | 类型 |
|------|---------------|------|
| `contabo.bobbik.org` | caddy:80 | 静态页面 |
| `contabo-quant.bobbik.org` | caddy → Flask:8502 | quant_lab Dashboard |

### RY 域名 (腾讯云 — `82.157.186.181`)

> 源文件: `common/Deployment/RY/caddy/Caddyfile`

| 域名 | Caddy 反代目标 | 类型 |
|------|---------------|------|
| `ry.bobbik.org` | `localhost:18789` | OpenClaw Gateway |
| `ryclaw.bobbik.org` | ChatUI 静态 + `:18080` 上传 | 聊天界面 + 文件上传 |

**`ryclaw.bobbik.org` 路径分流**:
- `/upload*` → `:18080` (文件上传服务)
- `/files/*` → 静态文件 `/home/ubuntu/.openclaw/workspace/data` (已上传文件下载)
- 其他 → 静态文件 `/home/ubuntu/.openclaw/workspace/chatui` (ChatUI 前端)

---

## 部署服务详情

### CodeRunner (VPS4)

| 项 | 值 |
|----|-----|
| 宿主机 | VPS4 (Google Cloud) |
| 服务 | 线上版 CodeRunner (独立 AI 开发平台) |
| 说明 | 经部署为线上独立应用节点 |

### quant_lab (VPS3)

| 项 | 值 |
|----|-----|
| 部署位置 | `/opt/quant_lab` |
| 虚拟环境 | `/opt/quant_lab/.venv` |
| systemd 服务 | `quant_lab` (Flask :8502) |
| 情报目录 | `/home/ubuntu/.openclaw/workspace/common/intelligence` |
| 一键部署 | PC 端运行 `python deploy/push_to_vps.py` |
| 配置分层 | `system_config.yaml` (通用) + `local.yaml` (机器特有，不部署) |

### OpenClaw Agent (全节点)

| 项 | 值 |
|----|-----|
| 运行方式 | `tmux:ai` (各节点独立 tmux session) |
| 监听端口 | `:18789` (HTTP API) |
| 配置文件 | `~/.openclaw/agents.json` / `openclaw.json` |
| 共享目录 | `~/.openclaw/workspace/common/` (Syncthing 同步) |

### bedtime_story (VPS1)

| 项 | 值 |
|----|-----|
| 部署位置 | `/opt/bedtime` |
| 后端 | FastAPI (PM2: `bedtime-api`, 端口 `:8000`) |
| 前端 | Next.js (PM2: `bedtime-web`, 端口 `:3000`) |
| 域名 | `bedtime.bobbik.org` |
| 打包部署 | PC 端 `pack.bat` → `bedtime_deploy.zip` → VPS 端 `vps_deploy.sh` |
| 技术栈 | Python(FastAPI) + Node(Next.js/React) + DashScope AI |

---

## 网络基础设施

### Tailscale 私有网络

| 组件 | 状态 | 说明 |
|------|------|------|
| 控制面 | Headscale (自管) | 运行于 VPS1, 端口 `:41641` |
| 网段 | `100.64.0.0/24` | 全节点自动分配 |
| 安全性 | 全链路 WireGuard 加密 | 零信任模型 |

### Syncthing 文件同步

| 同步目录 | 参与节点 | 说明 |
|---------|---------|------|
| `common/` | VPS1, VPS2, VPS3, X1C | Skills、Knowledge、Config 实时同步 |

**关键特性**:
- 去中心化：任何节点故障不影响数据
- X1C 是主编辑源head，修改后自动扩散到所有 VPS
- 修改 `common/knowledge/skills/` 下的文件即可让所有节点获得最新技能

### Cloudflare

| 功能 | 说明 |
|------|------|
| DNS | `*.bobbik.org` 全域 |
| Tunnel | 每台 VPS 独立 Tunnel（互不干扰） |
| 证书 | Cloudflare 自动管理 SSL |

### Caddy (反向代理)

**架构链路**: `用户浏览器 → Cloudflare Edge (SSL) → cloudflared → Caddy:80 (Docker) → 后端服务`

Caddy **不负责 SSL 终结**（这是 Cloudflare 的活），它只监听 HTTP `:80`，根据域名（`Host` header）将流量分拣到对应后端。

| 部署节点 | 运行方式 | Caddyfile 位置 | docker-compose 位置 | 域名数量 |
|---------|---------|---------------|--------------------|---------| 
| VPS1 | **Docker** (`caddy-gateway`) | `/opt/caddy/Caddyfile` | `/opt/caddy/docker-compose.yml` | 14 个 |
| RY | Docker | `common/Deployment/RY/caddy/Caddyfile` | — | 3 个 |
| VPS3 | 裸装 | Contabo 本地 Caddyfile | — | 2 个 |

**⚠️ VPS1 Caddy 是 Docker 容器 (`network_mode: host`)**，关键影响：
1. **文件系统隔离**：容器内看不到宿主机目录，新增静态站点必须在 `docker-compose.yml` 的 `volumes:` 中增加挂载
2. **重载方式**：不能用 `caddy reload`，必须 `cd /opt/caddy && docker compose down && docker compose up -d`
3. **配置同步**：Syncthing 同步源是 `common/Deployment/VPS1/caddy/Caddyfile`，同步目标是 `/opt/caddy/Caddyfile`（通过 Syncthing 直接映射，无需手动 cp）

### 🔴 新增域名必做四步 (Checklist)

给 VPS1 新增一个 `xxx.bobbik.org` 站点时，**以下步骤缺一不可**：

| # | 做什么 | 在哪做 | 怎么做 |
|---|--------|--------|--------|
| 1 | 添加 DNS CNAME 记录 | Cloudflare DNS 面板 | `xxx` → CNAME → 已有 Tunnel UUID，开启 Proxied |
| 2 | 添加 Tunnel Public Hostname | Cloudflare Zero Trust → Networks → Tunnels → VPS1 Tunnel | Subdomain=`xxx`, Domain=`bobbik.org`, Service=`http://localhost:80` |
| 3 | 添加 Caddy 路由规则 | 本地编辑 `Deployment/VPS1/caddy/Caddyfile`，等 Syncthing 同步 | 静态站点用 `root * /opt/xxx` + `file_server`，API 用 `reverse_proxy` |
| 4 | **如果是静态站点**：更新 Docker 挂载 | SSH 到 VPS1 编辑 `/opt/caddy/docker-compose.yml` | `volumes:` 中新增 `- /opt/xxx:/opt/xxx:ro`，然后 `docker compose down && docker compose up -d` |

---

## 自动化管线 (Cron)

> **V7.0 整合原则**：情报采集统一归 VPS3 quant_lab 管线执行，VPS2 仅负责"读成品 + 推送"。
> 消除跨节点重复采集，利用 Syncthing 自动同步 `common/intelligence/` 成品数据。

### 📊 quant_lab 统一管线 (UTC 21:00 / 北京 05:00, VPS3 执行)

```
21:00  daily_pipeline   → 美股收盘后统一执行：
       ├── Step 1: 统一情报采集
       │   ├── global_intelligence  (全球 RSS + API)
       │   ├── china_intelligence   (国内 RSS)
       │   └── youtube_intelligence (YouTube 视频采集，VPS3 本地)
       │   → 输出: common/intelligence/processed/
       │
       ├── Step 2-8: 因子计算 → 叙事引擎 → 信号生成 → 对抗审查
       ├── Step 9: Paper Trading 虚拟执行
       └── Step 10: Dashboard 生成 + 审计日志

每月15日 weight_evolver  → 达尔文自我进化 (因子权重自适应 + 正则化)
```

### 📰 每日推送 (06:35-06:45 UTC+8, VPS2 执行)

```
# ❌ 已迁移至 VPS3 quant_lab 统一管线：
# 06:00  china_intel      → 移除（VPS3 统一采集）
# 06:15  global_intel     → 移除（VPS3 统一采集）
# 06:15  youtube_intel    → 移除（VPS3 本地执行）

# ✅ 保留：
06:35  summarize_news   → 读取 common/intelligence/ (Syncthing 同步来的成品数据)
06:45  delivery         → Telegram 推送给用户
```

### 🔧 系统维护 & 数据管线

```
00:00  akp_digest       → VPS1 消化 inbox 到 raw/
01:00  akp_graphify_sync→ VPS1 图谱导入与 AI 语义关联
*/15   heartbeat        → VPS1 集群心跳巡检
22:00  daily_review     → VPS1 每日总结
23:00  sync_cron_jobs   → 全节点 cron 同步
```

### ⚙️ Cron 变更操作流程

```
config_sync PULL   → 拉取 VPS2/3 的 jobs.json 到本地
本地编辑 jobs.json → 增删改 cron 条目
config_sync PUSH   → 推送回目标节点
sync_cron_jobs     → 远程 cron reload 生效
```

---

## 跨节点任务派发协议

**派发方式**: 通过 `assign_task` 技能写入 `tasks_ledger.json` 后，向目标节点发送 HTTP 唤醒请求：

```
POST http://<Tailscale_IP>:18789/v1/chat/completions
```

**跨节点 Agent 寻址**:
- 默认 Agent: `"model": "openclaw"` 或省略 agent header
- 指定 Agent: `"model": "openclaw:<agentId>"` 或 Header `x-openclaw-agent-id: <agentId>`

**模型路由策略**:
```
用户指令 → VPS1 main → assignment 路由
  │
  ├─ 纯 Shell 命令?  → executor (零成本)
  ├─ 情报采集?       → VPS2 / Gemini Flash 🆓
  ├─ 深度分析?       → VPS2 analyst / Gemini 🆓
  ├─ 代码任务?       → VPS3 main / DeepSeek
  ├─ 推理任务?       → VPS3 reasoner / R1
  └─ 图像任务?       → VPS3 vision / Gemini 🆓
```

---

## 技术决策备忘

1. **分布式调度**: 国内任务国内节点执行，海外任务海外节点执行
2. **免费模型优先**: Gemini 免费额度覆盖 90%+ 任务，日常运行成本趋近 ¥0
3. **Syncthing 同步**: 去中心化，任何节点故障不影响数据
4. **Tailscale 组网**: 全链路加密，零暴露面
5. **每 VPS 独立 Tunnel**: Cloudflare Tunnel per-VPS，互不干扰，零端口暴露
6. **多环境配置**: `local.yaml` 覆写机制，同一代码库 PC/VPS 零修改运行
