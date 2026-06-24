# Bob Agent — 产品特性全景 / Feature Reference

> **文档定位**：本文件是 Bob Agent 所有产品特性的**唯一真相源 (Single Source of Truth)**。
> 未来推广网站、README、应用商店描述等所有面向用户的文案，均应从本文件提取内容，而非反向维护。
>
> 最后更新：2026-06-24

---

## 目录 / Table of Contents

1. [极简交互与原生体感](#minimal-interaction--native-ux)
2. [多模型自动发现与协作](#model-hub--auto-discovery)
3. [MCP 可扩展工具生态](#mcp-extensible-tool-ecosystem)
4. [自进化认知记忆系统](#self-evolving-cognitive-memory)
5. [可视化知识图谱](#interactive-knowledge-graph)
6. [Goal Mode 闭环执行器](#goal-mode-execution-loop)
7. [微信穿透网关](#wechat-gateway)
8. [Web Drop 端到端加密极传](#web-drop-e2ee-file-transfer)
9. [Doctor 自检与自愈](#self-diagnosis--auto-repair)
10. [其他能力速览](#additional-capabilities)
11. [路线图 / Roadmap](#roadmap)

---

## 极简交互与原生体感 {#minimal-interaction--native-ux}

### 用户视角 / User Perspective

Bob 被设计为"桌面上最安静的幽灵副手"——它栖居于系统托盘中，只需按下全局快捷键 `Ctrl+Shift+B`，或轻点屏幕边缘的 Quick Capture Bubble，即可在不打断当前心流的前提下秒速记录闪念。你还可以直接拖拽任意文件、文件夹或截屏发送给 Bob，让它"看到"你正在处理的内容并给出即时反馈。

### 技术亮点 / Technical Highlights

- **灵感闪存 (Quick Capture Bubble)**：屏幕边缘常驻微型悬浮按钮，回车即存入 Quick Note 库
- **全局快捷键 `Ctrl+Shift+B`**：任何窗口环境下一键唤起，`lib.rs` 注册 Tauri Global Shortcut
- **文件/图片拖放 (Drag & Drop)**：拖入对话框后后端瞬间完成解析，支持文本与二进制文件
- **桌面截屏 (`system_take_screenshot`)**：一键截取当前桌面，注入 Vision 对话流程
- **系统托盘 (System Tray)**：Tauri 原生 Tray 图标 + 右键菜单，0 干扰常驻
- **单实例锁 (Single Instance Lock)**：`tauri-plugin-single-instance` 确保全局只运行一个 Bob 实例
- **克制的 UI 美学**：全 CSS Variables 明暗模式、对称 Padding、顺滑微动效，遵循前端铁律

### 状态 / Status

✅ Shipped

---

## 多模型自动发现与协作 {#model-hub--auto-discovery}

### 用户视角 / User Perspective

不再需要手动抄模型 ID——Bob 在你填入 API Key 后，会自动探测供应商最新上线的全部模型，智能过滤掉非聊天类型（Embedding、TTS 等），让你一键勾选即可使用。通过 Main/Clerk 双模型分工，核心推理交给强力模型，后台脏活（晨报、压缩、记忆提取）交给免费模型，极致省钱。

### 技术亮点 / Technical Highlights

- **`model_providers.json` 注册表**：统一声明所有 Provider 的 `base_url`、API Key、模型列表
- **`/v1/models` 自动发现**：启动或手动刷新时，向 `{base_url}/models` 发送 GET 请求，自动注册最新模型
- **智能类型过滤**：自动排除 Embedding / TTS / 语音 / 图像生成 / 安全审核等非聊天模型，新模型默认 `visible: false`
- **Main / Clerk 双模型角色指派**：
  - **Main Model**：核心推理 + 工具调用决策（如 `deepseek-chat`、`glm-4-plus`）
  - **Clerk Model**：极低成本后台任务——晨报、Session 压缩、记忆提取、Goal 评估（如 ModelScope 免费层）
- **离线 Sidecar 推理**：内置 `llama-server` 进程管理（`sidecar.rs`），一键加载本地 GGUF 模型，100% 离线推理
- **8+ Provider 路由**：DeepSeek / 火山引擎 (豆包) / 通义千问 / MiniMax / 智谱 GLM / ModelScope / Ollama / 自定义 OpenAI 兼容端点

### 状态 / Status

✅ Shipped

---

## MCP 可扩展工具生态 {#mcp-extensible-tool-ecosystem}

### 用户视角 / User Perspective

Bob 实现了完整的 MCP (Model Context Protocol) 客户端——你可以通过简单的 JSON 配置，接入来自社区或自建的任何 MCP Server（无论是 `npx`、`python` 还是原生二进制），所有外部工具会自动与 Bob 的 12 个内置工具合流，供大模型自由调度。

### 技术亮点 / Technical Highlights

- **JSON-RPC 2.0 stdio 客户端**：Rust 原生实现，通过 `stdin/stdout` 管道启动和管理 MCP Server 子进程
- **Windows `.cmd` 自动适配**：在 Windows 平台上自动将 `npx` / `npm` 等命令适配为 `.cmd` 调用，确保跨平台兼容
- **工具命名空间前缀 (Namespace Prefixing)**：所有 MCP 工具自动加前缀 `mcp_{server}_{tool}`，防止多 Server 之间的工具名冲突
- **60 秒超时预留**：容忍 `npx` 首次拉取包的延迟，避免冷启动误判超时
- **生命周期管理**：应用退出时自动安全 Kill 所有 MCP 子进程，防止孤儿进程泄露系统资源
- **`tools/list` 动态工具映射**：启动时自动获取 MCP Server 暴露的工具列表，解析为 OpenAI Function Calling JSON Schema

### 状态 / Status

✅ Shipped

---

## 自进化认知记忆系统 {#self-evolving-cognitive-memory}

### 用户视角 / User Perspective

Bob 不只是一个无状态的聊天框——它会"记住你"。每次对话后，Bob 在后台静默提取你的习惯偏好、项目决策、反馈纠偏，沉淀为长期知识。更神奇的是，Bob 每隔 24 小时会"做梦"：自动整理、合并冗余记忆，并将学到的反馈写回自己的人设定义，实现无监督自我进化。

### 技术亮点 / Technical Highlights

- **三层记忆漏斗模型**：
  - **Soul 层 (`SOUL.md`)**：人设定义 + 核心约束，直接注入 System Prompt
  - **Session 层 (短期热记忆)**：当前对话及最近几轮上下文，7 天后自动冷迁移
  - **Wiki 层 (长期冷知识)**：本地 Wiki 沉淀，全文检索 + 图谱关联召回
- **`<|mem|>` 暗号标记**：对话中出现该标记时，强制触发事实提取流程
- **静默事实提取 (Factual Extraction)**：Clerk 模型后台分析近 10 轮对话，提炼持久事实并归类为：
  - `user`：用户习惯与偏好
  - `project`：项目架构、技术选型与核心决策
  - `feedback`：对 AI 错误的纠偏记录（最重要——确保同一错误不再犯第二次）
  - `reference`：高频代码块、命令或常用 URL
- **SQLite FTS 即时同步**：提取的事实保存为 `wiki/learned/*.md`（含 YAML frontmatter），同时实时写入 SQLite FTS 虚拟表，`brain_search` 下一轮即可检索
- **做梦引擎 (Dream Engine)**：每 24 小时唤醒时自动后台 Compaction：
  - 清理过期 / 临时事实 (Stale Cleanup)
  - 合并内容相似的冗余事实 (Merge Similar)
  - **SOUL 自谐调 (Auto-Refinement)**：将 `feedback` 和 `user` 习惯通过 Clerk 总结后重新写回 `SOUL.md`，实现人设的无监督本地自我修正与进化

### 状态 / Status

✅ Shipped

---

## 可视化知识图谱 {#interactive-knowledge-graph}

### 用户视角 / User Perspective

你可以在 Bob 中看到一张动态的力导向知识图谱——所有你提到过的人物、技术、项目在画布上以节点和关系链呈现。点击某个节点，就能"顺藤摸瓜"式地展开周围的关联知识，像游历一张私人的脑图。

### 技术亮点 / Technical Highlights

- **SQLite 原生图存储**：在本地 `bob.db` 中维护 `kg_nodes`（实体/标签）+ `kg_edges`（关系）两张表，微秒级 CRUD
- **BFS 子图查询**：以关键词为 Seed，支持 `max_hops`（2-3 跳）宽度优先搜索，局部子图毫秒内渲染
- **别名合并与消歧 (Alias Resolution & Merging)**：`kg_merge_nodes` 将同一实体的不同别名（如 "DS" 与 "DeepSeek"）合并至主节点，自动重定向关联边，别名关系记入 Metadata
- **Vis.js 力导向可视化**：前端交互式画布，节点可拖拽、物理连接实时更新、支持缩放与平移
- **`wiki_fts` 回填 (Backfill)**：知识图谱节点与 Wiki FTS 索引双向同步，检索结果自动关联图谱实体

### 状态 / Status

✅ Shipped

---

## Goal Mode 闭环执行器 {#goal-mode-execution-loop}

### 用户视角 / User Perspective

开启 Goal Mode 后，Bob 不再是简单的一问一答。它会像一个执着的工程师那样死磕任务——反复调用工具、读文件、搜网页、写代码，直到一个独立的评估器（而非执行者自己）严格判定任务完全达标为止。最多自动重试 3 轮，无需你手动催促。

### 技术亮点 / Technical Highlights

- **Maker-Checker 双角色架构**：
  - **Maker (执行端)**：使用 Main Model，工具调用预算从默认 5 轮飙升至 **50 轮上限**，允许极其复杂的链路探索
  - **Checker (评估端)**：调用 Clerk Model 作为严格判决器，默认以 `FAIL` 为立场，逐条检查 Maker 产出
- **反馈注入与自动重试 (Feedback Injection & Auto-Retry)**：Checker 判定未通过时生成缺失清单，系统自动封装为新任务重新塞回 Maker 队列，最大 3 次外层循环
- **Progress 事件推送**：通过 `app.emit()` 实时向前端推送 Goal 执行进度（当前轮次、工具调用状态、评估结果）

### 状态 / Status

✅ Shipped

---

## 微信穿透网关 {#wechat-gateway}

### 用户视角 / User Perspective

把 Bob 变成你的微信私人助手——用手机微信发消息给 Bob，它就能在家里的电脑上帮你查日历、传文档、总结链接内容。出门在外也能远程操控桌面 AI，真正做到"口袋里装着一台桌面大模型"。

### 技术亮点 / Technical Highlights

- **Native Rust 微信 API 客户端**：基于 `ilink_bot_token` 安全认证，内置微信通信协议适配
- **QR Code 快速配对**：扫码即绑定，家中电脑秒变个人微信代理
- **会话管理指令集 (Session Manager)**：
  - `/sessions`：列出最近 5 个会话（含"刚刚"、"15分钟前"等人类友好时间戳），回复序号跨端切换上下文
  - `/new`：开启新对话
  - `/status`：查看当前绑定的会话 ID
  - `/help`：获取指令帮助
- **URL 预抓取 (T-WX01)**：检测到用户发送链接或 type=49 微信卡片 XML 时，后台自动爬取网页前 2000 字符，封装为 context 喂给大模型
- **CDN 加密文件直发 (Encrypted Upload)**：读取文件 → MD5 哈希 → **AES-128-ECB PKCS7** 本地加密 → 上传微信 CDN → 获取 filekey 发送给目标用户
- **`from_user` wxid 上下文路由**：当工具调用源自微信会话时，携带发送者加密 wxid，确保 `send_wechat_file` 的目标 ID 准确无误

### 状态 / Status

✅ Shipped

---

## Web Drop 端到端加密极传 {#web-drop-e2ee-file-transfer}

### 用户视角 / User Perspective

需要在手机和电脑之间传大文件？Bob 内置了零配置的跨端极速投送——打开分享链接，文件直接点对点飞过去，全程端到端加密，即使中继服务器也无法偷窥文件内容。

### 技术亮点 / Technical Highlights

- **三级渐进式传输架构 (3-Tier Progressive)**：
  - **Tier 1 — 本地环回 (Loopback)**：同设备间极速共享
  - **Tier 2 — WebRTC P2P**：通过 WebSocket 信令交换 SDP 和 ICE 候选者，建立点对点直连通道，跑满网卡带宽
  - **Tier 3 — WebSocket 加密中继 (Relay)**：P2P 握手失败时自动降级，经由 `bob.bobbik.org` 中继分发
- **AES-128-GCM 端到端加密**：本地内存生成 Room ID 与 AES-128 密钥
- **URL Hash Fragment 密钥投递**：分享链接以 `#room_id.key` 格式拼接，`#` 后的 Hash 永远不会随 HTTP 请求发送给服务器 → 服务器只做密文透传 → **绝对零知识 E2EE**
- **Progress 事件**：传输进度实时推送至前端 UI
- **TURN Server 支持**：严苛 NAT 环境下的 WebRTC 穿透兜底

### 状态 / Status

✅ Shipped

---

## Doctor 自检与自愈 {#self-diagnosis--auto-repair}

### 用户视角 / User Perspective

Bob 能给自己"看病"。当你感觉 Bob 状态异常时，只需让它运行 Doctor 自检，它会自动诊断 API 连接、数据库完整性、文件权限等核心指标，发现问题后还能一键自愈修复——回滚配置、重建数据库，全程不破坏你的核心数据。

### 技术亮点 / Technical Highlights

- **`system_health_check` 全面体检**：
  - API Key 连通性验证（逐个 Provider 探测）
  - SQLite 数据库读写完整性检查 (`PRAGMA integrity_check`)
  - Tauri 沙箱路径白名单权限验证
  - 本地 Sidecar (`llama-server`) 进程状态探测
- **`system_auto_fix` 一键自愈**：
  - 配置文件回滚 (Config Rollback)：从自动备份恢复损坏的 `config.json`
  - 数据库重新初始化 (DB Re-init)：检测到挂锁或损坏时，安全重建表结构
  - 在不破坏用户 Wiki / 对话历史等核心数据的前提下修复

### 状态 / Status

✅ Shipped

---

## 其他能力速览 {#additional-capabilities}

### 12 个 Rust 原生工具

| 工具 | 描述 |
|------|------|
| `read_file` | 读取文本文件 (≤500KB) |
| `list_dir` | 浏览目录结构 |
| `write_file` | 安全写入（路径白名单：wiki / workspace / tracked_folders） |
| `append_file` | 追加内容至文件 |
| `fetch_url` | 网页抓取 (≤2MB, 10s 超时, reqwest + scraper) |
| `web_search` | 搜索引擎查询（Tavily 主 + TinyFish 降级） |
| `list_skills` | 列出全部可用技能 |
| `read_skill` | 读取 SKILL.md 全文 |
| `system_time` | 当前时间 + 时区 + 星期 |
| `get_weather` | wttr.in 天气查询 |
| `brain_search` | 知识库全文检索 (wiki/) |
| `add_calendar_event` | 写入 SQLite 日程事件 |
| `send_wechat_file` | 发送文件给微信用户 (CDN + AES 加密) |

### Outbox 声明式配置 (AI Self-Configuration)

AI 只需输出 `bob-config` 代码块 → 写入 `bob_outbox.json` → Rust Reconciler 2 秒轮询 → 6 层校验墙（op 白名单 + provider 合法性 + key 长度 + config key 白名单 + 备份 + 审计日志）→ `config.json` 安全生效。AI 自己配置自己，用户无需手动编辑 JSON。

### 日历与日程管理 (Calendar & Scheduling)

自然语言输入 → 结构化事件解析 → SQLite `events` 表持久化。前端提供可拖拽的 WeekTimeline 周视图，支持 `event` / `todo` 类型、`pending` / `done` / `cancelled` 状态流转。

### 知识库索引 (Knowledge Base Indexing)

- **文件提取 (`kb_extractor.rs`)**：PDF / DOCX / XLSX → 纯文本
- **索引构建 (`kb_indexer.rs`)**：提取结果写入 SQLite FTS，`brain_search` 即时可检索

### HTTP API Server

基于 axum 构建，绑定 `127.0.0.1:3721`，供本地其他服务或脚本调用 Bob 能力。

### 连接器 (Connectors)

| 连接器 | 状态 |
|--------|------|
| 微信 (WeChat Gateway) | ✅ Shipped |
| Google Calendar | 📋 Planned |
| Gmail | 📋 Planned |
| 飞书 (Lark/Feishu) | 📋 Planned |
| Discord Bot | 📋 Planned |
| Telegram Bot | 📋 Planned |

### 浏览器增强 (Browser Enhancement)

CDP (Chrome DevTools Protocol) 集成，支持自动化网页操作。📋 Planned

### 主题系统 (Theme System)

全 CSS Variables 驱动的 Dark / Light 双主题 + 自定义 Accent Color，遵循"严禁色彩硬编码"铁律。

### 国际化 (i18n)

基于 `vue-i18n` 的双语支持：`zh-CN` (简体中文) / `en-US` (English)。

### 技能/插件系统 (Skill/Plugin System)

双目录设计：内置 `skills/` + 用户可配外部技能目录。每个技能为一个含 `SKILL.md`（YAML frontmatter + Markdown 指令）的目录，Bob 通过 `list_skills` / `read_skill` 按需加载。

---

## 路线图 / Roadmap {#roadmap}

| 里程碑 | 描述 | 状态 |
|--------|------|------|
| **v0.5** — Cognitive Engine v2 | 记忆系统升级：更精细的事实生命周期管理、跨 Session 关联推理、主动提醒 | 📋 Planned |
| **v0.6** — Document Export Engine | 专业文档输出引擎：PDF (HTML 模板) → XLSX (公式+样式) → DOCX (目录结构) → PPTX (模板注入) | 📋 Planned |
| Shell 执行沙箱 | 安全沙箱化的本地命令执行能力，受控环境下运行脚本 | 📋 Planned |
| 跨平台通讯矩阵 | Telegram Bot / Discord Bot 深度接入，任意 IM 平台远程唤醒本地 Bob | 📋 Planned |
| 加密凭据存储 | 迁移至 `tauri-plugin-stronghold`，API Key 不再明文存储 | 📋 Planned |

---

> **Bob Agent** — 隐于桌面，使命必达。
