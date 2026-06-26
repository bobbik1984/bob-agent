# bob-agent LLM-Wiki — 全量功能与逻辑字典 (Feature & Logic Directory)

> **定位**：供 AI 编码代理（及人类开发者）快速检索、定位、阅读和修改 Bob-Agent 核心功能的“数字地图”。
> 当你需要修改、调试或理解某个特定功能的端到端逻辑时，请直接跳转到对应小节。

---

## 🧭 全景索引与快捷搜索标签

| 搜索关键词/功能 | 核心 Vue 组件 / 触发点 | Bridge 桥接层函数 | Rust Command 后端实现 | 核心业务文件 |
| :--- | :--- | :--- | :--- | :--- |
| **闪念速记 / 灵光一现** | [QuickNoteOverlay.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/components/QuickNoteOverlay.vue) | `appendQuickNote` | `system_append_quick_note` | [lib.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/lib.rs) |
| **做梦引擎 (记忆整理)** | `App.vue` / 后台守护 | `summarizeSession` | `system_summarize_session` | [dream.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/dream.rs) |
| **微信穿透网关** | `SettingsView.vue` (QR 扫码) | `wechatGetLoginQr` | `wechat_get_login_qr` | [mod.rs (wechat)](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/wechat/mod.rs) |
| **Web Drop 极传** | `ChatView.vue` (文件分享) | `startWebDrop` | `start_web_drop` | [web_drop.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/web_drop.rs) |
| **Goal Mode 闭环执行** | `ChatView.vue` (模式切换) | `llmChat` (携带 mode="goal") | `llm_chat` (调用 goal 循环) | [goal.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/goal.rs) |
| **多模型自动发现** | `SettingsView.vue` (刷新) | `llmRefreshModels` | `llm_refresh_models` | [llm.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/llm.rs) |
| **MCP 工具生态** | `SettingsView.vue` (配置) | `mcpGetConfig` | `mcp_get_config` | [mcp.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/mcp.rs) |
| **可视化知识图谱** | `GraphView.vue` (图谱画布) | `kgGetFullGraph` | `kg_get_full_graph` | [kg.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/kg.rs) |
| **Doctor 自检与自愈** | `SettingsView.vue` (诊断) | `healthCheck` | `system_health_check` | [doctor.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/doctor.rs) |
| **本地离线 Sidecar** | `SettingsView.vue` | `startOfflineEngine` | `start_offline_engine` | [sidecar.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/sidecar.rs) |
| **日程与时间轴** | [InboxView.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/views/InboxView.vue) | `listEvents` | `system_list_events` | [calendar.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/calendar.rs) |
| **Outbox 声明式配置** | 后台 Reconciler | `writeOutbox` | `system_write_outbox` | [outbox.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/outbox.rs) |

---

## ⚡ 1. 闪念速记 / 灵光一现 (Quick Notes)

### 1.1 功能概述
用户通过全局快捷键 `Ctrl+Shift+N` 唤起悬浮闪念框（Quick Capture Bubble），输入闪念回车后，系统通过 Rust 后端无缝写入本地速记 Markdown 文件。主打**极速、轻量、无干扰**。

### 1.2 核心逻辑流
```
[用户按下 Ctrl+Shift+N] 或 [点击 Bubble]
  │
  ▼
[Vue 渲染 QuickNoteOverlay.vue] ───── 输入内容, 回车确认
  │
  ▼  window.electronAPI.appendQuickNote(content)
[tauri-bridge.js] ─────────────────── invoke("system_append_quick_note", { content })
  │
  ▼
[lib.rs (system_append_quick_note)] ── 读取并追加至 {data_dir}/quick_notes.md
  │
  ▼  追加格式: \n- [YYYY-MM-DD HH:MM:SS] {content}\n
[返回 ok: true 与文件路径] ──────────── 前端展示 "已记录" (Lucide Check) 自动淡出
```

### 1.3 关键代码位置
- **前端 UI**: [QuickNoteOverlay.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/components/QuickNoteOverlay.vue)
  - 监听全局快捷键 `onGlobalKey()`，调用 `open()` 激活输入框并聚焦。
  - `submit()` 读取输入文本，执行 IPC 请求并淡入“已记录”提示，800ms 后自动关闭。
- **Bridge 垫片**: [tauri-bridge.js](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/tauri-bridge.js)
  - `appendQuickNote: async (content) => invoke('system_append_quick_note', { content })`
- **Rust 后端**: [lib.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/lib.rs) 中的 `system_append_quick_note()`:
  - 自动获取 `get_data_dir().join("quick_notes.md")`，以 `.append(true)` 追加时间戳与格式化行。

### 1.4 修改/扩展指导
- **需要修改文件存储路径**：修改 `lib.rs` 中 `system_append_quick_note` 的 `path` 变量。
- **需要扩展格式（例如支持 YAML Frontmatter）**：直接在 `lib.rs` 中修改 `entry` 的格式化 string。

---

## 🧠 2. 做梦引擎 (Dream Engine / Cognitive Compaction)

### 2.1 功能概述
防长线上下文超载的核心机制。每次对话结束，静默分析近 20 轮上下文提取事实偏好。每天夜间或后台空闲时唤起“梦境”，使用 Clerk 低成本模型压缩 Session 归档，并将用户反馈自动重写回 `SOUL.md` 进行人设进化。

### 2.2 核心逻辑流
```
[对话结束 / he() 切换会话] 
  │
  ▼
[lib.rs 触发 system_summarize_session] ── 提炼当前 Session 话题，保存为 json 日志
  │
  ▼
[后台 Dream Engine 轮询/触发] ────────── 异步拉取近 7 天 JSON，唤醒 Clerk 整理
  │
  ├─► 冗余事实合并与生命周期清理
  ├─► >7天 Session 冷迁移至 wiki/sessions/ (冷记忆归档)
  └─► 重塑 SOUL.md: 提取 feedback 写入 SOUL 的进化模块，实现无监督微调
```

### 2.3 关键代码位置
- **Rust 后端核心**: [dream.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/dream.rs)
  - `system_summarize_session()`: 从 SQLite 获取最后 20 条消息，进行 V1 基础摘要并持久化在 `memory/sessions/{conv_id}.json`。
  - `compress_session_async()`: 启动后台异步任务，由 Clerk 提炼为 JSON-LD 格式的实体事实。
  - `system_get_dream_report()`: 整合生成当日晨报，供前端渲染“小结通知”。
  - `system_dismiss_dream()`: 清空/标记已读状态。
- **前端配合**: `App.vue` 挂载 `onRemoteNewMessage` 触发本地会话摘要；`ChatView.vue` 展现晨报卡片。

### 2.4 修改/扩展指导
- **优化记忆召回率**：修改 [llm.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/llm.rs) 中的 `build_memory_summary()` 函数，调整 Wiki 事实与灵魂注入 System Prompt 的比例与层级。

---

## 💬 3. 微信穿透网关 (WeChat Gateway)

### 3.1 功能概述
让用户通过手机微信远程唤醒并控制 Bob-Agent 的网关。支持多会话上下文路由、微信文件 CDN 加密直发和网页链接预抓取，确保远程体验不打折。

### 3.2 核心逻辑流
```
[手机微信发送消息] ──► [ilink 微信云服务器] ──► [本地 Rust WeChat Client]
                                                      │
                                                      ├─► 解析 /commands
                                                      │   ├─ /sessions (列出历史)
                                                      │   ├─ /new (新建会话)
                                                      │   └─ /status (查看当前)
                                                      │
                                                      └─► 普通文本/网页
                                                          ├─ 自动抓取前 2000 字符 (T-WX01)
                                                          ├─ 携带 wxid 唤醒 LLM 引擎
                                                          └─ 发送回复 (或加密文件 CDN)
```

### 3.3 关键代码位置
- **微信模块根目录**: [src-tauri/src/wechat/](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/wechat)
- **指令解析器**: [session_mgr.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/wechat/session_mgr.rs)
  - 拦截微信端私聊文本，解析 `/sessions`、`/new` 等斜杠指令。
- **消息监听 & 路由**: [monitor.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/wechat/monitor.rs)
  - 处理 WebSocket 传入流量，触发大模型响应，并通过微信回复。
- **网页预抓取**: [api.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/wechat/api.rs) / [web.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/web.rs)
- **文件 CDN 加密上传**: [cdn.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/wechat/cdn.rs)
  - 使用 `AES-128-ECB` 本地加密文件并上传至微信服务器，生成多媒体消息。

### 3.4 修改/扩展指导
- **新增远程指令**：修改 `session_mgr.rs` 的 `handle_text_msg()` 分支，增加微信用户的交互协议。

---

## 🚀 4. Web Drop 极传 (E2EE File Transfer)

### 4.1 功能概述
本地零配置的跨端大文件闪传。本地内存生成随机 Room ID 和密钥，文件数据经过 WebRTC P2P (或 WebSocket 密文中继) 端到端加密流式发送。

### 4.2 核心逻辑流
```
[用户在 UI 拖入大文件并点击 Share] ──► [start_web_drop()]
                                              │
                                              ├─ 生成随机 Room ID + 密钥
                                              ├─ 构造 URL: https://bob.bobbik.org/#room_id.key
                                              └─ 开启异步 WebRTC 信令监听
                                                      │
[接收方点击链接，中继协调] ◄───────────────────────────┘
   ├── P2P 直连成功 ──► 通过 DataChannel 加密传输 (跑满网卡)
   └── P2P 穿透失败 ──► 降级为 AES-128-GCM 加密 WS 密文中继
```

### 4.3 关键代码位置
- **核心实现**: [web_drop.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/web_drop.rs)
  - `start_web_drop()`: 分配共享 URL（Hash 部分包含密钥，安全不泄漏给中继），生成 `tokio::spawn` 守护协程。
  - `run_drop_session()`: 处理 WebSocket 信令握手、WebRTC 候选人交换，以及向 `bob.bobbik.org` (中继端) 握手失败时的 AES 流式加密转发。

---

## 🎯 5. Goal Mode 闭环执行器 (Goal Loop)

### 5.1 功能概述
支持长时间、大预算、多步骤死磕任务的模式。内置 Maker-Checker 架构，将工具调用循环上限提升至 50 轮，并使用独立 Clerk 对结果进行严格质检与重试。

### 5.2 核心逻辑流
```
[用户开启 Goal 模式发送任务]
  │
  ▼◄───────────────────────────────────────────┐
[Maker 模式执行 (Main Model)]                  │
  ├── 智能调度 Tools 循环 (工具上限谱写至 50 轮)   │ 重新灌入上下文与清单
  └── 输出最终报告                             │
  │                                            │
  ▼                                            │
[Checker 模式质检 (Clerk Model)]               │
  ├── 严格评判: 默认倾向 FAIL                   │
  ├── 若 PASS: 闭环完成，输出结果              │
  └── 若 FAIL: 整理缺失清单 (Feedback) ────────┘
```

### 5.3 关键代码位置
- **核心主循环**: [goal.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/goal.rs)
  - `execute_goal_loop()`: 负责三轮外部大重试，维护 Checker 的反馈流。
- **LLM 协同**: [llm.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/llm.rs) 中的 `stream_internal()`
  - 检查 `agent_mode == "goal"`，将 `MAX_TOOL_CALL_ROUNDS` 设为 50，并在每轮工具结束后主动注入 `Restatement` 消息（重申任务目标防失焦）。

---

## 🔌 6. MCP 客户端与外部生态 (Model Context Protocol)

### 6.1 功能概述
支持大模型动态装载社区或自建的 MCP Server (stdio/Json-RPC 2.0)。本地 12 个原生工具和所有 MCP 外部工具自动合并，提供统一的执行接口。

### 6.2 核心代码位置
- **Rust 客户端**: [mcp.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/mcp.rs)
  - `McpClient` 结构体：管道读写、事件分发。
  - `mcp_get_config()` 和 `mcp_set_config()`：读写 `{data_dir}/mcp_config.json`。
  - 自动将 `npx` 等命令替换为 Windows `.cmd` 兼容的执行模式。
- **工具管理器**: [tools.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/tools.rs) 中的 `get_tool_schemas()`:
  - 读取并遍历活跃的 MCP 客户端，将工具名称统一加前缀 `mcp_{server}_{tool}` 防止命名冲突。

---

## 📊 7. 可视化知识图谱 (Knowledge Graph)

### 7.1 功能概述
将用户的 Wiki 内容、Session 记忆实体以可视化的力导向图呈现。支持点击节点探索关联网络、多别名消歧合并。

### 7.2 核心代码位置
- **Rust 后端**: [kg.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/kg.rs)
  - 数据库表定义：`kg_nodes`（id, label, category, metadata）和 `kg_edges`（source, target, relation, weight）。
  - `kg_get_full_graph()`：返回全量可视化数据。
  - `kg_merge_nodes()`：执行别名消歧，将重复节点融合并重定向所有连接边。
- **前端渲染**: `src/views/GraphView.vue` (使用 Vis.js 画布进行物理交互渲染)。

---

## 🩺 8. Doctor 自检与自愈 (Self-Repair)

### 8.1 功能概述
应用的自动化运维与防崩溃机制。当系统出现 API 连接异常、SQLite 数据库死锁/损坏或权限不足时，通过健康检测进行配置回滚或安全表重建。

### 8.2 核心代码位置
- **诊断与自愈**: [doctor.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/doctor.rs)
  - `system_health_check()`：逐项检查 `API Connectivity`、`SQLite Integrity`、`llama-server Process`。
  - `system_auto_fix()`：自动从配置历史备份副本恢复损坏的 `config.json`，或在保证 Wiki / 聊天历史不丢失的前提下重建数据表。

---

## 📅 9. 日程与时间轴 (Calendar & Scheduling)

### 9.1 功能概述
将日常事件管理 AI 化。用户输入自然语言日程（如“下周三下午三点开会”），LLM 自动调用 `add_calendar_event` 解析为结构化字段存入 SQLite，前端则通过拖拽式周视图时间轴进行动态同步。

### 9.2 核心代码位置
- **Rust 后端**: [calendar.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/calendar.rs)
  - SQLite `events` 表 CRUD。
  - `system_parse_event()`：自然语言文本的结构化参数转换。
- **前端组件**: `src/components/WeekTimeline.vue` (时间轴拖拽交互)。

---

## 🛡️ 10. Outbox 声明式配置 (AI Declarative Configuration)

### 10.1 功能概述
AI 自主修改配置（例如自动写入 API Key、修改默认模型等）的沙箱安全方案。LLM 通过写 Outbox 中间文件触发同步，而非直改全局配置文件。

### 10.2 核心代码位置
- **核心实现**: [outbox.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/outbox.rs)
  - `write_outbox()`：写入 `bob_outbox.json`。
  - Rust 后端内置 2 秒轮询 of Reconciler 守护进程，通过 6 层校验（操作白名单、Provider 校验、凭证哈希防劫持等）安全合并入最终的 `config.json`。
