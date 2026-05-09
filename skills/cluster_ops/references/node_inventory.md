# 🖥️ Node Inventory (设备清单)

**版本**: 2.0.0
**更新日期**: 2026-04-13
**维护者**: Bobbik

> 本文件是所有集群设备信息的**单一真相源 (Single Source of Truth)**。
> 其他文档中涉及 IP 和角色的描述，均以本文件为准。

---

## 快速查询表

| Node | 公网 IP | Tailscale IP | 服务商 | 角色 | 参与 OpenClaw |
|------|---------|-------------|--------|------|:---:|
| **RY** | `82.157.186.181` | — | 腾讯云 | OpenClaw + ChatUI | ✅ |
| **VPS1 (Huoshan)** | `115.190.248.194` | `100.64.0.1` | 火山引擎 (北京) | Brain / Gateway | ✅ |
| **VPS2 (Hostinger)** | `187.124.144.87` | `100.64.0.6` | Hostinger (US) | Intel Center | ✅ |
| **VPS3 (Contabo)** | `89.117.22.194` | `100.64.0.7` | Contabo (US) | Exec Center | ✅ |
| **VPS4 (Google)** | `34.42.162.75` | `100.64.0.8` | Google Cloud | CodeRunner Server | ✅ |
| **Oracle** | `129.153.218.179` | — | Oracle Cloud | VPN 中转 (v2ray) | ❌ |
| **X1C** | — | `100.64.0.4` | 本地 | Dev Hub | ❌ |
| **X1 Tablet** | — | `100.64.0.5` | 本地 | Remote IDE | ❌ |
| **Tencent_101** | `101.42.176.17` | `100.64.0.2` | 腾讯云 | MallOS (海南) | ❌ |
| **Tencent_43** | `43.135.159.146` | `100.64.0.3` | 腾讯云 | MallOS (深圳) | ❌ |

---

## RY — OpenClaw + ChatUI 节点

- **公网 IP**: `82.157.186.181`
- **服务商**: 腾讯云
- **角色**: 独立 OpenClaw 节点 + ChatUI 前端
- **Caddy 域名**:
  - `ry.bobbik.org` → OpenClaw Gateway `:18789`
  - `ryclaw.bobbik.org` → ChatUI 静态 + 文件上传 `:18080`
- **Caddy 配置**: `common/Deployment/RY/caddy/Caddyfile`

---

## VPS1 (Huoshan) — Brain / Gateway (中枢网关)

- **Tailscale IP**: `100.64.0.1`
- **公网 IP**: `115.190.248.194`
- **Hostname**: `iv-yeeq1jbmyo4c5qykpryt`
- **服务商**: 火山引擎 (北京，国内)
- **角色**: 中央调度、Headscale Hub、Beszel Hub、Dashboard Server
- **子 Agent**:
  - `main` (总指挥): 接收指令、理解意图、调用 `assign_task` 派发任务
  - `scheduler` (巡检员): 后台 cron 15min 心跳巡检、晨报生成、超时告警
- **模型策略**: 轻量免费优先 (ModelScope → Qwen → DeepSeek)
  - ⚠️ **国内节点，无法访问 Google API**
- **关键端口/服务**:
  | 端口 | 服务 |
  |------|------|
  | `:18789` | OpenClaw Gateway |
  | `:3080` | Mission Control Dashboard |
  | `:8088` | WeChat Bridge |
  | `:8090` | Beszel 监控 |
  | `:41641` | Headscale 控制台 |
  | `:8000` | Bedtime Story Backend (FastAPI) |
  | `:3000` | Bedtime Story Frontend (Next.js) |
- **项目部署**:
  - bedtime_story: `/opt/bedtime` → `bedtime.bobbik.org` (PM2 管理)
  - home-panel: `/opt/home-panel` → `panel.bobbik.org` (静态)
  - history: `/opt/history` → `history.bobbik.org` (静态)

---

## VPS2 / srv1471410 — Intel Center (情报中心)

- **Tailscale IP**: `100.64.0.6`
- **公网 IP**: `187.124.144.87`
- **Hostname**: `srv1471410`
- **服务商**: Hostinger (US，海外)
- **硬件**: 2C / 8G
- **角色**: 海外信息检索、RSS 监控、长文本分析
- **子 Agent**:
  - `researcher` (侦察员): 专职运行爬虫、web_search、RSS 监控
  - `analyst` (分析师): 长文本处理，PDF 财报分析，结构化 JSON 提取
- **模型策略**: 海外直连优先 (Google Gemini 🆓 → DeepSeek → ModelScope)
  - ✅ 可直连 Google API（Gemini 免费额度）
- **网络特点**: 海外节点，访问 ModelScope 国内服务有延迟

---

## VPS3 / vmi3145876 — Exec Center (执行中心)

- **Tailscale IP**: `100.64.0.7`
- **公网 IP**: `89.117.22.194`
- **Hostname**: `vmi3145876`
- **服务商**: Contabo (US，海外)
- **硬件**: 4C / 8G
- **角色**: 代码编写、复杂推理、GitHub 操作、quant_lab 运行
- **子 Agent**:
  - `coder` (开发专家): 代码编写、GitHub 操作、复杂逻辑推理
  - `commander` (算力指挥): SSH 推送脚本到 X1 Tablet，监控算力任务返回
- **模型策略**: 海外直连优先 (DeepSeek → Google Gemini 🆓 → DeepSeek R1)
  - ✅ 可直连 Google API（Gemini 免费额度）
- **关键部署**:
  - quant_lab: `/opt/quant_lab` (systemd 服务, Flask :8502)

---

## VPS4 (Google) — CodeRunner 线上节点

- **Tailscale IP**: `100.64.0.8`
- **公网 IP**: `34.42.162.75`
- **Hostname**: `instance-20260415-135432`
- **服务商**: Google Cloud Platform
- **硬件**: 2C / 4G / 100G Disk (ARM64 `aarch64`)
- **状态**: 已开机并部署 CodeRunner 线上版
- **角色设计**: 线上版 CodeRunner 独立宿主机，提供稳定的云端 AI 开发环境。

---

## X1 Carbon — Dev Hub (开发中枢)

- **Tailscale IP**: `100.64.0.4`
- **Hostname**: `bobbik-x1c`
- **角色**: 本地开发机、Syncthing 源
- **职责**: 代码编辑、技能开发。`common/` 目录修改自动同步到所有节点。
- **不运行 OpenClaw Agent**

---

## X1 Tablet — 远端 Ubuntu PC

- **Tailscale IP**: `100.64.0.5`
- **Hostname**: `bobbik-x1-tablet`
- **角色**: 远程桌面 Antigravity 代码编写
- **连接方式**: RDP / RustDesk 原始宽带连接
- **上网方式**: Antigravity 走 OpenVPN3 通过 VPN 上网

---

## Oracle — VPN 中转节点

- **公网 IP**: `129.153.218.179`
- **服务商**: Oracle Cloud
- **角色**: VPN 中转节点 (v2ray)
- **用途**: 作为网络代理中转，不运行 OpenClaw Agent

---

## SSH 远程访问

从 X1C 开发机可通过 PuTTY (plink/pscp) 直接操控所有 VPS：

| PuTTY Session 名 | 目标 | 公网 IP | 用户 |
|------------------|------|---------|------|
| `RY_Openclaw` | RY | `82.157.186.181` | — (SSH 密钥已配) |
| `Huoshan` | VPS1 | `115.190.248.194` | — (SSH 密钥已配) |
| `Hostinger` | VPS2 | `187.124.144.87` | — (SSH 密钥已配) |
| `Contabo_VPS` | VPS3 | `89.117.22.194` | `ubuntu` (SSH 密钥已配) |
| `Google_VPS` | VPS4 | `34.42.162.75` | `ubuntu` (SSH 密钥已配) |

```powershell
# 命令行访问示例 (PowerShell)
& "C:\Program Files\PuTTY\plink.exe" -batch -load "Contabo_VPS" "命令"
& "C:\Program Files\PuTTY\pscp.exe" -batch -load "Contabo_VPS" 本地文件 :远程路径
```

---

## 网络拓扑 ASCII 速查

```
                        ┌─────────────────────┐
                        │   Cloudflare DNS     │
                        │  *.bobbik.org        │
                        └──────────┬──────────┘
                                   │
    ┌──────────┬───────────────────┼───────────────────┬──────────┬──────────┐
    │          │                   │                   │          │          │
┌───┴───┐ ┌───┴─────┐      ┌──────┴──────┐     ┌─────┴─────┐ ┌──┴──────┐ ┌──┴──────┐
│  RY   │ │  VPS1   │      │    VPS2     │     │   VPS3    │ │  VPS4   │ │ Oracle  │
│ .181  │ │ .194    │      │   .87       │     │  .194     │ │  .75    │ │ .179    │
│OpenClaw││ Brain   │◄─TS─►│   Intel     │◄TS─►│  Exec     │ │CodeRuner│ │ VPN中转 │
└───────┘ └────┬────┘      └─────────────┘     └───────────┘ └─────────┘ └─────────┘
               │ Tailscale
          ┌────┴────┐
          │         │
      ┌───┴───┐ ┌──┴────┐
      │  X1C  │ │Tablet │
      │ .0.4  │ │ .0.5  │
      │  Dev  │ │Remote │
      └───────┘ └───────┘
```
