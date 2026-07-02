# Bob-Agent 开发全局路线图 (Roadmap)

> 🎯 **当前版本**: `v0.4.4` — Ghost Partner (幽灵副手) 阶段正式版。
> ♻️ **已完成**: Tauri 迁移、主体模式、微信/TG/Discord 通道、文档输出引擎、Goal 闭环执行引擎、Web Drop P2P 极传、知识图谱融合、智能笔记模块。
> 📋 **下一目标**: 目标 20 — 内网穿墙隧道 | 目标 22 — Bob-Mobile 手机端 MVP | 目标 23 — 跨端同步引擎 (详见 `docs/MOBILE_BLUEPRINT.md`)。

---

## 📍 目标 1: Tauri 基础脚手架 ✅
- [x] T-101: 初始化 Tauri V2 环境 (`tauri init`)。
- [x] T-102: 配置 `tauri.conf.json`（无边框、透明、380x750）。
- [x] T-103: 跑通 `npm run dev:tauri` 前端热更新。
- [x] T-104: 配置国内 Cargo 镜像源（清华 TUNA）。
- [x] T-105: 注入 `tauri-bridge.js` 适配器层，拦截 `window.electronAPI`。
- [x] T-106: 安装 `@tauri-apps/plugin-dialog` 并授权 `dialog:default` 能力。

---

## 📍 目标 2: 前端完整性保障 (让所有页面能正常渲染) ✅
> ⚠️ 本阶段**不写任何 Rust 代码**，只在 `tauri-bridge.js` 中补全 Mock，以及在 `App.vue` 中添加窗口按钮。

### 2A: 窗口外壳修复 (Window Chrome)
- [x] T-201: 在 `App.vue` 的 `.titlebar-right` 中添加自定义的最小化/最大化/关闭按钮。
- [x] T-202: 使用 `@tauri-apps/api/window` 的 `getCurrentWindow()` 绑定 `minimize()`, `toggleMaximize()`, `close()` 方法。
  - *避坑*: Tauri V2 没有 Electron 的 `titleBarOverlay`，必须手动实现。

### 2B: Bridge Mock 完整性 (补全所有 53 个接口)
> 以下是当前 `tauri-bridge.js` 中**完全缺失**的接口，按组件分类列出。

#### ChatView.vue 依赖 (聊天核心)
- [x] T-211: `onStreamChunk(callback)` — 必须返回清理函数 `() => {}`，否则 `onUnmounted` 崩溃。
- [x] T-212: `getModelPool()` — 返回 `[]`（模型池为空）。
- [x] T-213: `getActiveModels()` — 返回 `{ main: '', clerk: '' }`。
- [x] T-214: `assignModelRole(id, role)` — 返回 `{ ok: true }`。
- [x] T-215: `getMessages(convId)` — 返回 `[]`（消息列表为空）。
- [x] T-216: `addMessage(convId, role, content, img)` — 返回 `true`。
- [x] T-217: `getConversation(id)` — 返回 `{ id, title: '新对话', cost: 0 }`。
- [x] T-218: `updateConversationCost(id, cost)` — 空操作。
- [x] T-219: `sendVision(msgs, img, access, mode)` — 返回 `{ error: null, content: '' }`。
- [x] T-220: `stopGeneration()` — 空操作。

#### InboxView.vue 依赖 (收件箱/日历)
- [x] T-221: `listEvents()` — 返回 `[]`（日历事件为空）。
- [x] T-222: `updateEventStatus(id, status)` — 空操作。
- [x] T-223: `parseEvent(text)` — 返回 `{ title: text, type: 'event' }`。
- [x] T-224: `confirmEvent(event)` — 返回 `{ ok: true }`。
- [x] T-225: `deleteEvent(id)` — 返回 `true`。
- [x] T-226: `updateEventTime(id, start, end)` — 空操作。

#### SettingsView.vue 依赖 (设置页面)
- [x] T-231: `getApiKeys()` — 返回 `{}`（无已保存密钥）。
- [x] T-232: `setApiKey(providerId, key)` — 空操作。
- [x] T-233: `getToolStatuses()` — 返回 `[]`。
- [x] T-234: `getTrackedFolders()` — 返回 `[]`。
- [x] T-235: `addTrackedFolder(path)` — 空操作。
- [x] T-236: `removeTrackedFolder(path)` — 空操作。
- [x] T-237: `selectFolderToTrack()` — 复用 `open({ directory: true })` 原生弹窗。
- [x] T-238: `selectDir()` — 复用 `open({ directory: true })` 原生弹窗。
- [x] T-239: `getMcpConfig()` — 返回 `{ mcpServers: {} }`。
- [x] T-240: `setMcpConfig(config)` — 空操作。
- [x] T-241: `rescanModels()` — 空操作。

#### ModelHub.vue 依赖 (模型中心)
- [x] T-251: `getModelPool()` — 与 T-212 共用同一个 Mock。
- [x] T-252: `assignModelRole(id, role)` — 与 T-214 共用。
- [x] T-253: `rescanModels()` — 与 T-241 共用。

#### MorningBriefing.vue 依赖 (晨间汇报/做梦引擎)
- [x] T-261: `getDreamReport()` — 返回 `null`（无汇报）。
- [x] T-262: `dismissDream()` — 空操作。
- [x] T-263: `onDreamCompleted(callback)` — 必须返回清理函数 `() => {}`。

#### PluginManager.vue 依赖 (插件管理)
- [x] T-271: `getPlugins()` — 返回 `[]`。
- [x] T-272: `installPlugin(id)` — 返回 `true`。
- [x] T-273: `onPluginProgress(callback)` — 必须返回清理函数 `() => {}`。
- [x] T-274: `onPluginUpdated(callback)` — 必须返回清理函数 `() => {}`。

#### 文件操作依赖
- [x] T-281: `readFile(path)` — 返回 `''`。
- [x] T-282: `selectFile()` — 复用 `open()` 弹窗。
- [x] T-283: `getFilePath(file)` — 返回 `file.name`。
- [x] T-284: `getFileMeta(path)` — 返回 `{ name: '', size: 0 }`。
- [x] T-285: `openFile(path)` — 空操作（后续用 `@tauri-apps/plugin-shell` 的 `open` 实现）。
- [x] T-286: `showInFolder(path)` — 空操作。
- [x] T-287: `scanFolder(path)` — 返回 `[]`。
- [x] T-288: `estimateKB(path)` — 返回 `{ totalFiles: 0, totalSize: 0 }`。
- [x] T-289: `getClipboardImage()` — 返回 `null`。
- [x] T-290: `showNotification(title, body)` — `console.log` 输出。

### 2C: 导航验证
- [x] T-291: 验证 Chat → Inbox → Settings 三个视图的切换无报错。
- [x] T-292: 验证侧栏对话列表的创建、重命名、删除操作不报错。

---

## 📍 目标 3: Rust 原生化 — 配置与凭证
> 将 Bridge 中的 Mock 逐步替换为真正的 Rust 实现。
- [x] T-301: Rust `config_get/set` 读写 `config.json`。
- [x] T-302: Rust `config_get_all` 返回完整配置对象。
- [x] T-303: ~~Stronghold~~ → 使用 `keyring` crate 实现 OS 级 Keychain 加密存储（Windows DPAPI / macOS Keychain）。
- [x] T-304: 注册全局快捷键 (`Ctrl+Shift+B`) 唤醒窗口。

## 📍 目标 4: Rust 原生化 — 数据库引擎 (SQLite)
- [x] T-401: 引入 `rusqlite`，初始化 `conversations` 和 `messages` 表。
- [x] T-402: 实现 `db_conversations`, `db_conversation_create/delete/rename/get`。
- [x] T-403: 实现 `db_messages`, `db_message_add`。

## 📍 目标 5: Rust 原生化 — 大模型通信 (LLM Engine)
- [x] T-501: 用 Rust `reqwest` 发送 Chat 请求。
- [x] T-502: 实现 SSE 流式解析，通过 `app.emit("llm:chunk")` 推送。
- [x] T-503: 图片视觉请求 (`sendVision`) 的 Base64 传输。
- [x] T-504: ModelHub 模型池扫描与角色指派的 Rust 实现。
- [x] T-505: **Panic 防御加固** — 消除 `lib.rs` 中 13 处裸 `.unwrap()` 和 `llm.rs` 中 2 处 UTF-8 字节切片越界，改用 `match` 优雅降级，防止中文/Emoji 内容导致整个 Tauri 进程崩溃退出。

## 📍 目标 6: Rust 原生化 — 工具链与知识库 ✅
- [x] T-601: 文件夹递归扫描 (`walkdir`)。
- [x] T-602: **网页抓取引擎** — 新建 `web.rs`，`reqwest` + `scraper` 实现 HTML 获取与 DOM 解析，智能提取 `<article>/<main>` 正文（2MB/10s 安全限制）。
- [x] T-603: **技能与插件扫描引擎** — 新建 `plugins.rs`，原生扫描 `externalSkillsDir`，解析 SKILL.md YAML frontmatter，注册内置系统能力。
- [x] T-604: **做梦引擎 V1** — 新建 `dream.rs`，对话结束时提取核心话题存入 `memory/sessions/`，聚合生成晨间简报。
- [x] T-605: **日程管理引擎** — 新建 `calendar.rs`，SQLite 持久化的事件/待办系统，支持 `listEvents`, `parseEvent`, `confirmEvent`, `deleteEvent`, `updateEventStatus`, `updateEventTime` 全部 6 个接口。
- [x] T-606: **文件读取引擎** — `system_read_file` 原生读取文本文件（500KB 安全上限 + UTF-8 校验）。
- [x] T-607: **文件夹跟踪** — `system_get/add/remove_tracked_folders` 持久化到 config.json。
- [x] T-608: **系统工具原生化** — `openFile`, `showInFolder`, `getVersion`, `getLogPath`, `openLogDir`, `openDataDir`, `getToolStatuses`, `factoryReset` 全部 Rust 原生。剩余 Mock: `updateTheme`, `getClipboardImage`, `showNotification`。

## 📍 目标 7: 收尾与发布
- [x] T-701: **彻底移除 `electron/` 目录** — 42 个遗留文件已清除，Bridge 中的 Electron 引用注释已更新。
- [x] T-702: ~~集成 `llama-cpp-rs` 离线推理~~ (已由 Sidecar + llama-server 方案完美替代)。
- [x] T-703: 执行 `npm run tauri build`，验证打包流程，生成 `.msi` 或 `.exe` 安装包。

---

## 📍 目标 8: 声明式配置 + 单向调谐 (Outbox/Reconciler 架构) ✅
> 🎯 **目标**: 让 Bob 在获得第一个 API Key（点火）后，具备自主配置系统的能力。
> 🛡️ **安全原则**: AI 只写 Outbox 文件（"办公桌"），Rust 内部守护者单向读取、校验、生效，AI 永远碰不到核心配置的"保险柜"。

### Phase 1: Rust 后端 — Outbox 引擎

- [x] T-801: **新建 `outbox.rs` 模块骨架** — 定义 `get_outbox_dir()`, `get_outbox_path()`, Outbox JSON 结构体，操作白名单常量 `ALLOWED_OPS` 和安全 config key 白名单 `SAFE_CONFIG_KEYS`。
- [x] T-802: **实现 `write_outbox()` 函数** — 接受 `Vec<Value>` 操作列表，写入 `data_dir/outbox/bob_outbox.json`，自动创建目录。
- [x] T-803: **实现 `validate_operation()` 校验防火墙** — 检查 op 类型白名单、provider 合法性、API Key 最小长度、config key 白名单，返回 `Result<(), String>`。
- [x] T-804: **实现 `reconcile()` 核心调谐逻辑** — 读取 Outbox → 逐条校验 → 合并到 config.json → 备份旧 config 为 `config.bak.json` → 删除/归档已消费的 Outbox。
- [x] T-805: **实现 `start_reconciler()` 后台轮询守护** — Tokio `interval(2s)` 轮询检测 outbox 文件变更，调用 `reconcile()`，成功后 `app.emit("config:reconciled", count)`。
- [x] T-806: **注册 IPC 命令 `system_write_outbox`** — 在 `lib.rs` 中添加 `mod outbox`，注册新 Tauri command，在 `setup()` 中 `tokio::spawn` 启动 Reconciler 后台任务。
- [x] T-807: **审计日志** — 每次 reconcile 执行写入 `data_dir/logs/reconciler.log`（时间戳 + 操作 + 结果），便于用户审计 AI 做了什么。

### Phase 2: 桥接层 + 前端

- [x] T-811: **`tauri-bridge.js` 新增接口** — `writeOutbox(operations)` 映射到 `system_write_outbox`；`onConfigReconciled(callback)` 监听 `config:reconciled` 事件。
- [x] T-812: **ChatView.vue 消息后处理器** — 在 AI 回复渲染逻辑中检测 ` ```bob-config ``` ` 代码块，提取 JSON 操作，调用 `writeOutbox()`，并从显示内容中移除该块（用户不需要看到原始 JSON）。
- [x] T-813: **App.vue 全局事件监听** — 监听 `config:reconciled`，触发 SettingsView 的 API Key 状态刷新 + ChatView 的模型指示器刷新 + 可选 Toast 通知。
- [x] T-814: **System Prompt 注入** — 在 `llm.rs` 的 `stream_internal()` 系统提示词中，追加 Outbox 使用说明（教 AI 如何输出 `bob-config` 代码块来声明配置变更）。

### Phase 3: 用户体验

- [x] T-821: **SettingsView 引导文案** — API 密钥管理面板顶部增加提示："💡 您也可以直接在对话中告诉 Bob 帮您配置密钥"。
- [ ] T-823: **端到端集成测试** (集成测试后期再检测) — 在对话中模拟"帮我配好这个 Key: sk-test123"，验证 Outbox 写入 → Reconciler 消费 → config 更新 → UI 自动刷新的完整链路。
- [ ] T-824: **防破坏测试** (集成测试后期再检测) — 手动写入格式错误/恶意字段的 `bob_outbox.json`，验证 Reconciler 不崩溃、程序正常运行、审计日志正确记录拒绝原因。

---

## 📍 目标 9: Tool Calling 引擎 (Agent 升级) ✅
> 🎯 **目标**: 让 Bob 从 ChatBot 升级为 Agent——能够主动调用工具（读文件、抓网页、查技能），而非仅靠用户拖拽喂数据。
> 🏗️ **方案**: Rust 侧实现（方案 A），全部在进程内完成，不依赖 Python/Node。

### Phase 1: Rust 工具引擎

- [x] T-901: **新建 `tools.rs` 工具注册表** — 定义 `get_tool_schemas()` 返回 OpenAI Function Calling 格式的工具描述；`execute_tool()` 异步执行调度器。
- [x] T-902: **暴露第一批工具** — `read_file`、`list_dir`、`fetch_url`、`list_skills`、`read_skill`，全部复用已有 Rust 原生能力。
- [x] T-903: **改造 `llm.rs` stream_internal** — 包裹 Tool Calling 循环：发送含 `tools[]` 的请求 → 检测 `tool_calls` → 执行工具 → 回注结果 → 重新请求 LLM → 直到纯文本回复（上限 5 轮）。
- [x] T-904: **SSE 流解析增强** — 在现有的 `delta.content` / `delta.reasoning_content` 解析基础上，新增 `delta.tool_calls[i]` 的增量累加解析。
- [x] T-905: **编译验证** — `cargo check` 通过。
- [x] T-906: **web_search 工具** — Tavily (主) + TinyFish (降级) 双引擎搜索，纯 Rust reqwest 实现。API Key 已注入 config.json。

### Phase 2: 集成与测试

- [x] T-911: **系统提示词增强** — 在 system prompt 中注入可用技能摘要 + 工具列表，引导 LLM 按需调用。
- [x] T-912: **端到端测试** — 用户已在日常使用中持续验证 Tool Calling + Outbox 完整链路。
- [x] T-913: **前端 Tool Calling UI** — ChatView 中渲染 `tool_start` / `tool_end` 事件，让用户看到 Bob 在调用什么工具。

---

## 🔍 IPC 数据契约审计 (防踩坑指南)
> 为了避免像 ModelHub 那样因前后端数据结构不匹配导致的 Bug，以下列出各 Vue 组件实际期望的 Rust 返回数据结构（基于 `tauri-bridge.js` 尚未实现的 Mock 接口梳理）：

### 1. 文件/工作区扫描 (`scanFolder` / `getFileMeta`)
- **来源**: `ChatView.vue` (处理拖拽上传文件夹)
- **期望格式 (`getFileMeta`)**: `{ name: string, size: number, isDir: boolean, isDirectory: boolean }`
- **期望格式 (`scanFolder`)**: 必须返回一个对象，包含 `{ error: boolean, message?: string, ... }`，否则会导致无法弹出“确认扫描”卡片。

### 2. 工具与凭证状态 (`getToolStatuses` / `getApiKeys`)
- **来源**: `SettingsView.vue` (设置页 API 密钥管理)
- **期望格式 (`getApiKeys`)**: `{ "deepseek": "sk-xxx...", "openai": "sk-yyy..." }`。前端通过 Key 匹配对应服务商。
- **期望格式 (`getToolStatuses`)**: 必须是一个对象数组 `[{ name: string, isActive: boolean, description: string, missingCredentials: string[] }]`。前端依靠 `missingCredentials.length > 0` 来决定是否标红并显示警告。

### 3. 日历与做梦引擎 (`listEvents` / `getDreamReport`)
- **来源**: 暂未深度排查，目前 Mock 强行返回空数组或 `null`。在实现 T-604 (做梦引擎) 时必须严格审查 Vue 端如何渲染报告对象。
- **注意**: `summarizeSession` 预期返回布尔值以触发前端通知。

### 4. 跨服务商模型切换 (API Key 鉴权陷阱)
- **现象**: 在对话框中如果从 `deepseek` 切换为 `doubao`，大模型引擎仍会错误地使用 `deepseek` 的 API Key 去请求，导致鉴权失败。
- **改进要求**: 
  1. Rust 引擎在发送 `stream_chat` 请求时，必须根据当前选中的 `modelId` 动态反查其所属的 `provider`，并读取对应 `provider` 的专属 API Key。
  2. 前端 UI (`ModelHub` 或模型下拉菜单) 应当过滤掉那些尚未配置 API Key 的模型，或者在切换到未配置密钥的模型时，主动拦截并弹窗提示用户前往设置页填写。

- [x] **UI规范化重构 (T-600前置补漏)**:
  - 移除了设置页面和工具状态中的表情符号(✅⚠️)，统一替换为 `var(--accent-primary)` 的高亮指示灯。
  - 将 `credentialProviders` 严格切分为『模型供应商密钥』与『插件/外部服务密钥』两大独立区域。
  - 强制牛马模型(Clerk)和主力模型(Main)的激活高亮样式完全遵守项目的主题强调色(`var(--accent-primary)`)，清除了野生绿色(#22c55e)及置灰反馈。
  - 补充修复了 `settings.open_log_dir` 等遗漏的翻译占位符解析问题。

### 5. 残留 Mock 清单 (技术债)
> 以下接口仍为 Mock，尚未绑定真正的 Rust invoke：
- **🧠 记忆与做梦引擎 (T-604)**: summarizeSession, getDreamReport, dismissDream, onDreamCompleted。
- **🛠️ 系统级交互 (T-608 残留)**: updateTheme, getClipboardImage, showNotification。
- **⚙️ MCP 配置 (T-609)**: getMcpConfig, setMcpConfig。
> ✅ 已消除: T-603(插件扫描), T-605(日程), T-606(文件读取), T-607(文件夹跟踪), T-608(大部分系统工具)
---

## 📅 开发日志

### 2026-06-30 (今天)

**主题**: UI 体验清债 + Slash Command 显性化重构准备

**完成**:
1. [Fix] **知识图谱布局重构** — 图例面板左下角单列排布，搜索框浮层化，清除无效的 `unknow` 文本。
2. [Fix] **笔记侧边栏 (Note Explorer) 一致性** — 时间轴文本单行截断，暗色模式下 `timeline-dot` 颜色对比度修复。
3. [Audit] **全局状态审计** — 确认 Tauri V2 后端的 `tokio-cron-scheduler` 引擎 (无人值守自动化) 已完全实装并在运行中，清理了旧版冗余的 todo 检查项。
4. [Audit] **Slash Commands 摸底** — 梳理了当前仅存的 `/memo`、`@note` 和 `@` 命令，发现极度隐蔽的 UX 问题。

**未完成**:
- [ ] T-2001: 实现 Slash/Mention Command 的智能悬浮补全菜单 (方案 A)

**已完成 (Phase 2.5 & Phase 3)**:
- [x] T-1731: Phase 2.5: Dream 标签语义去重引擎 (LLM 聚类 + 晨间简报合并提案)
- [x] T-1741: Phase 3-1: 笔记 Promote 提升为独立长笔记
- [x] T-1742: Phase 3-2: Dream 笔记扫描集成 (Notebook Digest)
- [x] T-1743: Phase 3-3: 气泡直接存为笔记与 @note 隐式上下文注入 (即 T-2002 对应的方案 B)


### 2026-05-15 (今天)

**主题**: Agent 化升级 - Tool Calling 引擎 + Web Search

**完成**:
1. 新建 `tools.rs` 工具注册表 - 6 个零外部依赖原生工具: read_file, list_dir, fetch_url, web_search, list_skills, read_skill
2. 重写 `llm.rs` stream_internal - 完整 Tool Calling 循环 (最多 5 轮), SSE 流式 delta.tool_calls 增量累加解析
3. web_search 双引擎 - Tavily (主, POST JSON body) + TinyFish (降级, GET header), 纯 Rust reqwest
4. Tavily + TinyFish API Key 注入 - 从 unified_api_registry.json 提取, 写入 config.json
5. Outbox 白名单修正 - KNOWN_PROVIDERS 中 TAVILY_API_KEY -> tavily, TINYFISH_API_KEY -> tinyfish
6. System Prompt 增强 - 工具列表 + 动态技能摘要注入
7. 编译通过 - cargo check 0 errors
8. [Fix] MiniMax/Qwen 的 OpenAI 兼容模式下的 `<think>` 标签流式切分状态机
9. [Fix] 流式缓冲区尾部残留导致文本截断、错觉"停在思考中"的严重 Bug（补全了收尾的 emit）
10. [Fix] 流式交互 UI 体验：完善等待动画 (弹跳圆点) 的显示时机，只在请求飞出至正文流入的真空期展现，彻底折叠 `<think>` 阶段内容。
11. [Feature] 确认 `system_time`, `get_weather`, `write_file`, `brain_search` 四个核心工具已在 `tools.rs` 中用 Rust 原生实现完毕，补全了底层基础感知能力。

**未完成**:
- [ ] T-912: npm run tauri dev 端到端测试
- [x] T-913: ChatView 前端 Tool Calling UI (tool_start/tool_end 事件渲染)

---

### 2026-05-16

**主题**: 日程系统工业化 + 双窗口修复 + 文档大扫除

**完成**:
1. [Feature] `add_calendar_event` 工具 - 在 tools.rs 中新增，让 LLM 能自主向 SQLite events 表写入日程
2. [Fix] `calendar.rs` 字段名不匹配 - 后端返回 `startTime`(驼峰) vs 前端读 `start_time`(下划线)，导致日程面板永远空白
3. [Fix] `tools.rs` start_time 格式拼接 - 大模型传 date+startTime 分离参数，需拼接为 `YYYY-MM-DD HH:MM:SS` ISO 格式
4. [Fix] 双窗口幽灵进程 - `prevent_close` 在 dev 模式下导致殆尸进程。dev 模式改为真关闭，release 模式保留托盘隐藏
5. [Feature] `tauri-plugin-single-instance` - 第二次启动自动唤醒已有窗口，不再弹出重复实例
6. [Fix] 导航更名 - “收件箱”→“日程” (zh-CN.json / en-US.json)
7. [Docs] 文档大扫除 - 重写 ARCHITECTURE.md, README.md, USER_GUIDE.md；更新 AGENTS.md, progress.yaml；归档 DEVELOPMENT_PLAN.md

**未完成**:
- [ ] T-912: 端到端测试
- [x] T-1001: 灵魂注入 (SOUL.md)

---

### 2026-05-17

**主题**: P0 灵魂/记忆完成 + P2 安全加固 + 全局快捷键 + UX 统一 + Restatement 引擎创新

**完成**:
1. [P0] T-1001 灵魂注入 — 重写 `data/memory/SOUL.md` 为完整人设定义，`llm.rs` 每次对话自动注入
2. [P0] T-1002 热记忆注入 — `build_memory_summary()` 读取最近 3 份 session 摘要注入上下文
3. [Security] `write_file` / `append_file` 路径白名单升级 — 新增 `resolve_write_path()` 统一鉴权：相对路径→安全目录，绝对路径→需在 workspaceDir/tracked_folders 内，globalFileAccess 开关可全开放
4. [Security] `read_file` 路径穿越防御 — `..` 检测前置拦截
5. [Security] Tool Calling 审计日志 — 新增 `audit_tool_call()`，每次工具调用写入 `AppData/bob-agent/logs/tools.log`
6. [Security] TinyFish URL 编码安全 — 从简单 `replace(' ', '+')` 升级为 RFC 3986 percent-encoding
7. [Feature] T-304: 全局快捷键 `Ctrl+Shift+B` — 添加 `tauri-plugin-global-shortcut`，任意界面一键唤起 Bob 窗口
8. [Feature] `SearchCard.vue` 搜索结果卡片 — 与 FileCard 统一设计语言 (inline pill + Lucide 图标)，web_search 结果自动解析为可点击卡片
9. [UI] 工具调用圆点样式 — 运行中=实心主题色闪烁，完成=实心主题色静止，统一 `var(--accent-primary)`
10. [Engine] **尾部注意力重申 (Restatement)** — 在 `llm.rs` Tool Calling 循环第 2 轮起，于 messages 尾部动态注入 system 消息，重申用户原始请求和 SOUL 规则，利用 U 型注意力防止多轮工具调用后失焦
11. [Research] 分析 `code_runner/references/20260516_别人的测试经验_agent开发和codex应用.md`，提取 4 个架构启发（Restatement/技能固化/排队纠偏/Cron自动化），确认 skill-creator 已就位
12. [Docs] 全量文档更新 — todo.md / progress.yaml / AGENTS.md / ARCHITECTURE.md / USER_GUIDE.md / README.md 同步刷新
13. [Engine] T-1003: **异步记忆压缩 (Dream V2)** — 重写 `dream.rs`，启动后 5 秒延迟触发 `compress_sessions_async()`，用 Clerk 模型将 V1 JSON 摘要升级为高质量 Markdown 总结
14. [Engine] T-1004: **冷热记忆迁移** — 新增 `migrate_stale_sessions()`，启动时同步执行，将 >7 天的 session 文件从 `memory/sessions/` 归档到 `wiki/sessions/`（跨盘复制+删除）
15. [Feature] T-304: **全局快捷键确认** — `Ctrl+Shift+B` 沿用，标记完成
16. [Build] cargo check 0 errors 编译验证通过，emoji 安全审计通过

**未完成**:
- [ ] T-912: 端到端测试

### 2026-06-11

**主题**: 本地文件服务 CSP 放行与生产环境发布打包

**完成**:
1. [Fix] **CSP 安全策略修复** — 修改 `index.html` 的 Content-Security-Policy 头部，添加 `http://127.0.0.1:*` 到 `img-src` 白名单，解决 WebView2 静默拦截本地图片请求导致图标/本地图片无法显示的问题。
2. [Cleanup] **移除前端冗余日志** — 移除 `useChat.js` 在渲染管道中为了调试路径正则替换及 DOMPurify 的调试语句。
3. [Build] **自动化发布构建** — 执行 `scripts/release.bat`，完成 Tauri 双重编译（主程序+引导安装器），打包产出 `dist-release/bob-installer.exe` 和 `dist-release/bob-agent-portable.zip`。
4. [Docs] 全面更新开发文档、Changelog 及 progress 记录。

**未完成**:
- [ ] 思考状态 (streamThinking) 的前端流式动态加载动画。

## 📍 目标 10: 认知与记忆引擎升级 (Phase 2)
> 🎯 **目标**: 让 Bob 拥有长期记忆能力，理解自己的“人设”，并能主动维护和检索知识库。

- [x] T-1001: **灵魂注入 (SOUL)** — 创建 `data/memory/SOUL.md` 并在 `llm.rs` 组装系统提示词时将其全文注入。
- [x] T-1002: **热记忆注入** — 在对话启动时，自动读取 `data/memory/sessions/` 下的近期对话总结并注入上下文。
- [x] T-1003: **异步记忆压缩 (Dream V2)** — 启动时后台用 Clerk 模型异步压缩未处理的 session，生成 Markdown 摘要替换原始 JSON。
- [x] T-1004: **冷热记忆迁移** — 启动时自动将 `mtime > 7` 天的热记忆从 `memory/sessions/` 归档到 `wiki/sessions/`（跨盘安全：复制+删除）。
- [x] T-1005: **记忆沙箱保护** — ~~严格锁定 data/wiki/~~ → 升级为 `resolve_write_path()` 白名单引擎：wiki/ + workspaceDir/ + tracked_folders + globalFileAccess 开关

---

## 🔧 可完善项 (Improvement Backlog)

### 工具层
- [x] **write_file 工具** - 需要路径安全白名单 (仅 data/ 和 workspaceDir/)
- [x] **URL 编码** - TinyFish 手动 URL 拼接不安全, 应引入 percent-encoding crate
- [x] **工具结果缓存** — 会话级 HashMap 缓存 (read_file/list_dir/list_skills/read_skill/system_time)，写操作自动清空
- [x] **工具执行超时** - 每个工具 30秒超时保护 (tokio::time::timeout)

### 安全层
- [x] **read_file 路径沙箱** - 限制在 workspaceDir + externalSkillsDir 内
- [x] **Tool Calling 审计日志** - 记录每次工具调用到 logs/tools.log

### 前端 UX
- [x] **Tool Calling 进度指示** - ChatView 显示"正在搜索..."可视化反馈
- [x] **工具结果折叠** - 太长的结果折叠显示
- [x] **搜索结果卡片** - web_search 结果渲染为带标题链接的卡片

### 引擎层
- [x] **🔥 尾部注意力重申 (Restatement)** — 在 `llm.rs` 多轮 Tool Calling 循环中，于第 2 轮起在 messages 尾部动态插入 system 消息重申当前任务约束和 SOUL 规则，利用大模型 U 型注意力防止长线任务失焦（灵感来源：`code_runner/references/20260516_别人的测试经验_agent开发和codex应用.md`）
- [x] **并行工具调用** - 多个 tool_calls 时用 futures_util::future::join_all 并行执行
- [x] **DeepSeek 兼容性** - 验证 thinking mode + tool_calls 同时工作 (已在 llm.rs 增加 reasoning_content 回传机制修复 400 错误)
- [ ] **Epic: 离线模型 Tool Calling (Offline Function Calling)**
  - [ ] 调研本地 `llama-server` (如 Llama-3/Qwen) 的 function calling 输出格式
  - [ ] 在 `llm.rs` 中针对 `provider == "offline"` 增加降级逻辑（如果模型不支持标准 JSON，则通过 Prompt 强制要求 XML 标签包裹）
  - [ ] 实现离线模式下的错误重试机制（解析 JSON 失败时，返回报错信息让小模型自行修正）
- [ ] **Epic: 排队纠偏 (Queue Correction)**
  - [ ] 在前端 `ChatView.vue` 添加“打断执行”按钮，点击后发送中止信号给 Tauri 后端
  - [ ] 在 `llm.rs` 中引入 `tokio::sync::mpsc` 监听前端中断信号
  - [ ] 修改 `stream_internal` 的 Tool Calling 循环：一旦收到中断信号，立刻跳出循环并丢弃挂起的工具
  - [ ] 将用户新输入的“纠正指令”作为新一轮上下文直接喂给 LLM 重新规划
- [x] **Epic: 无人值守自动化 (Cron Automations)**
  - [x] 引入 `tokio-cron-scheduler` 库，在 `lib.rs` 的后台守护线程中初始化
  - [x] 在应用启动时，从 SQLite 读取用户的自动化日程
  - [x] 编写后台无头 (Headless) 唤醒逻辑
- [ ] 引入 `tokio-cron-scheduler` 库，在 `lib.rs` 的后台守护线程中初始化
  - [ ] 在应用启动时，从 SQLite 读取用户的自动化日程（如每天 08:00 播报新闻）
  - [ ] 编写后台无头 (Headless) 唤醒逻辑：时间一到，自动后台组装 Prompt 并调用 `stream_internal`，将结果通过系统通知（Notification）或悬浮窗推给用户

### 体验层
- [x] **T-822: SetupWizard i18n 收尾** — 硬编码中文替换为 $t() 调用，zh-CN/en-US 双语同步完成
- [x] **T-304: 全局快捷键** — Ctrl+Shift+B 硬编码版已实现并稳定运行，后续可考虑可配置化
- [x] **搜索/文件/文件夹卡片统一设计** — 抽取了 `.bob-card-inline` 和 `.bob-card-block` 组件基类和样式 token，SearchCard/FileCard/FolderDropCard/ConfirmCard 共享 CSS，消除冗余
- [x] **技能固化 (Skill Solidification)** — `skill-creator` 已存在于 `skills/` 目录，Bob 可通过 `list_skills` + `read_skill` 调用，无需额外开发
- [x] **视觉感知：截屏分析** — 截图直接粘贴到对话框即可，无需独立按钮
- [x] **上下文感知：@唤起本地文件** — ChatView.vue 已实现 @ 触发文件选择器

---

## 📍 目标 10: 架构审计与安全加固 (Post-Migration Audit)
> 🎯 **目标**: 根据 Jules 提供的 Electron 到 Tauri 迁移审计报告，全面清理技术债并加固系统安全性。
> 🛡️ **安全原则**: 消除 Rust 后端 Panic 隐患，彻底清理弃用的 Node.js 依赖，封堵路径穿越漏洞。

### 第一阶段: 高危漏洞与冗余清理 (Phase 1)
- [x] T-1001: **Rust 异常处理加固 (`unwrap` 消除)** — 扫描 `src-tauri/src/tools.rs` (Weather API 解析) 和 `sidecar.rs` (Mutex locks) 中的 `.unwrap()` 调用。将其替换为安全的 `if let`、`match` 或 `unwrap_or_else`，避免因外部 API 异常或线程锁毒化导致应用崩溃。
- [x] T-1002: **清理 Electron 依赖残留** — 从 `package.json` 彻底移除 `better-sqlite3`, `electron`, `electron-builder` 及冗余的 NPM scripts，净化打包环境，减少 Node_modules 体积。
- [x] T-1003: **路径穿越防范增强** — 修改 `src-tauri/src/tools.rs` 中的 `resolve_write_path` 方法。在处理软链接时，对目标路径的父目录应用 `std::fs::canonicalize()`，并强制校验其前缀是否符合白名单 (Workspace/Data dir)，以防高级越权写入攻击。

### 第二阶段: 架构重构与性能优化 (Phase 2 - 延后执行)
- [x] T-1004: **数据库逻辑解耦** — 将 `src-tauri/src/lib.rs` 中的 `rusqlite` SQLite 相关逻辑抽离到独立的 `src-tauri/src/db.rs` 模块，规范化 Tauri State 在跨文件中的生命周期传递，给入口文件瘦身。
- [x] T-1005: **前端事件订阅内存泄露排查** — 在 Vue 组件（如 ChatView, SettingsView）中，审查所有 Tauri 事件的监听，确保在 `onUnmounted` 时正确调用 `unlisten()` 回调函数，避免重复渲染和内存消耗。

### 第三阶段: 生产级加固 (v0.2.0 Sprint — 已完成)
- [x] **T-1006: 僵尸进程根治** — Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) 绑定 llama-server 子进程，主进程退出时内核自动清理。
- [x] **T-1007: LLM IPC 流式降压** — 30ms / 4字符 Buffer & Debounce，削减 ~80% IPC 调用次数，消除高吞吐下的 UI 线程卡顿。
- [x] **T-1008: 路径穿越双层防御** — `urlencoding::decode` 先解 URL 编码再做 `..` 检测，加上 `canonicalize()` 白名单校验，堵住编码绕过攻击向量。
- [x] **T-1009: 晨报弹窗阴影修复** — 移除 `MorningBriefing.vue` 外层 `overflow: hidden`，解除对 `box-shadow` 的硬切裁剪。
- [x] **T-1010: 启动画面主题匹配** — 启动 Logo 改为 CSS Mask 方案，深色主题白色/浅色主题强调色，`localStorage` 持久化 accent，消除闪白。
- [x] **T-1011: Bob 头像资源纳入 Vite 管道** — `bob_logo.svg` 从硬编码绝对路径改为 `new URL('/bob_logo.svg', import.meta.url).href`，解决 WebView 缓存导致头像消失问题。
- [x] **T-1012: Apple Glass 主题移除**
- [x] **版本封版** — package.json / tauri.conf.json / Cargo.toml 统一升至 0.3.1-pre。

---

## 目标 11: v0.3 — 微信接入 + HTTP API (已完成)

微信接入模块已在 Rust 侧原生实现 (wechat/ 9个文件 + http_api.rs)，桌面端 UI 已适配。

---

## 📍 目标 12: v0.4 — Ghost Partner (幽灵副手)
> 🎯 **目标**: 从"被动响应的聊天机器人"进化为"主动辅助的桌面幽灵副手"。
> 📋 **来源**: `docs/20260606_AI 桌面助手竞品与差异化战略.docx` 竞品分析 + 差异化战略梳理。
> 🏗️ **核心定位**: 「中国泛白领办公桌上的幽灵副手」— 极度轻量、原生体验、纯本地化，拒绝全能 IDE 叙事。

### Phase 1: 低成本高感知

- [x] T-1201: **富文本剪贴板交接 (Rich-Text Clipboard Handoff)**
  - [x] Cargo.toml 引入 `tauri-plugin-clipboard-manager`
  - [x] lib.rs 注册插件，capabilities/default.json 增加 `clipboard-manager:default` 权限
  - [x] ChatView 消息气泡增加"复制为富文本"按钮（图标: `ClipboardCopy`）
  - [x] 实现 Markdown → HTML 序列化（复用 `marked` 渲染结果），写入 `text/html` 剪贴板格式
  - [x] 用户按 Ctrl+V 即可在 Outlook / 微信公众号编辑器 / WPS 中粘贴为带表格、高亮、加粗的富文本
  - *设计理念: 拒绝视觉模拟 (GUI 自动化)，用优雅的"剪贴板交接"保留人类点击"发送"的安全控制感*

### Phase 2: 核心调度基建

- [x] T-1211: **桌面微心跳引擎 (Micro-Heartbeat Scheduler)**
  - [x] Cargo.toml 引入 `tokio-cron-scheduler` 或类似轻量定时器
  - [x] 新建 `scheduler.rs` 模块
  - [x] SQLite 新增 `schedules` 表 (id, trigger_type, prompt_template, enabled, last_run)
  - [x] lib.rs setup() 中初始化调度器，从 DB 恢复已有任务
  - [x] IPC 命令: `system_list_schedules`, `system_add_schedule` 等
  - [x] 执行逻辑: **触发条件不依赖绝对时间（如早8点），而是基于“应用首次启动”、“闲置后再次激活”或“相对间隔”**。触发后 → 后台组装 Prompt → 调 `stream_internal` → 结果写入通知/Inbox
  - *对齐 todo.md 原有的自动化需求，更适合桌面单机环境*

### Phase 1: 文件操作工具集 (Shell-Lite, 5 个新工具)

- [x] T-1611: **create_directory 工具** — `tools.rs` L1550, `create_dir_all` + `resolve_write_path`
- [x] T-1612: **move_file 工具** — `tools.rs` L1561, `fs::rename` + 自动创建目标目录
- [x] T-1613: **copy_file 工具** — `tools.rs` L1579, `fs::copy` + 返回字节数
- [x] T-1614: **delete_file 工具 (回收站优先)** — `tools.rs` L1597, `trash::delete`
- [x] T-1615: **rename_file 工具** — `tools.rs` L1608, 同目录 `fs::rename`
- [x] T-1616: **System Prompt 更新** — 5 个工具 schema 已注册到 `get_tool_schemas()`

- [ ] T-1212: **文件目录监控 (Micro-Heartbeat File Watch)**
  - [ ] Cargo.toml 引入 `notify` crate（文件系统事件通知，比 walkdir 轮询更省电）
  - [ ] 监控 tracked_folders + 微信下载目录
  - [ ] 新文件出现时，通过系统托盘气泡或桌面通知轻提醒
  - [ ] 可选: 自动触发 LLM-Wiki ingest（需用户在提醒中确认）

### Phase 3: 交互升级

- [x] T-1221: **快速输入气泡窗 (Quick-Capture Bubble)**
  - [x] 实现为 QuickNoteOverlay 组件，点击 Bob Logo 唤醒
  - [x] 输入内容通过 provide/inject 发送给 ChatView 处理
  - [x] 后端语义路由: 结合现有 Tool Calling 和日历系统，判断输入类型
  - *实现方式与原始设计有差异（非独立窗口，而是主窗口内叠加层），但功能等价*

### Phase 4: 高阶自治（延后 / 按需）

- [ ] T-1231: **Outbox 预填提案 (Proactive Proposals)**
  - [ ] 微心跳检测到新事务时，后台模型分析并预填 `bob_outbox.json` 草案
  - [ ] 用户唤醒桌面时，弹出执行清单一键确认
  - [ ] 将交互逻辑从"被动等待指令"升级为"主动提供执行提案"

- [ ] T-1232: **过程记忆提取 (Procedural Memory / Skill Extraction)**
  - [ ] Dream Engine 复盘近期重复操作模式，将高频操作固化为新的 Skill 定义
  - [ ] 引入自学习循环，实现"越用越顺手"的正向演进
  - *注: 这是研究课题，暂不排期，需先观察 dream.rs V2 的实际产出质量*

---

## 目标 13: v0.4 — 体感/防御/主动性升级 (借鉴 Hermes Desktop)
> **目标**: 把 Bob 已有的强大后端能力"浮出水面"，让用户真正感知到 Bob 在背后做了什么。
> **来源**: Hermes Desktop 竞品分析 + `bob_optimization_plan.md` + `bob_v04_dev_guide.md`
> **设计红线**: 14px 最大圆角 / 纯灰度 / 无蓝色 / **严禁 Emoji** / 无技术术语 / 所有通知可关闭

### Phase 1: 体感升级 — 让用户"看得见" (1-2周)

- [x] T-1301: **对话历史全文搜索**
  - [x] `db.rs`: 新建 `messages_fts` FTS5 虚拟表，对 `messages.content` 建索引
  - [x] `db.rs`: 创建 INSERT/DELETE 触发器，自动同步 FTS 索引
  - [x] `db.rs`: 启动时回填存量消息到 FTS 索引 (INSERT OR IGNORE)
  - [x] `db.rs`: 新增 IPC 命令 `db_search_messages(query)` → 返回 `Vec<{id, conversation_id, conv_title, snippet, created_at}>`，LIMIT 30
  - [x] `lib.rs`: 注册 `db_search_messages` 命令
  - [x] `tauri-bridge.js`: 新增 `searchMessages(query)` 映射
  - [x] `ChatView.vue`: 侧边栏顶部新增搜索输入框 (32px高, bg-secondary, 300ms debounce)
  - [x] `ChatView.vue`: 搜索结果卡片列表 (单行标题 + 双行 snippet, `<mark>` 用 accent-primary 高亮)
  - [x] `ChatView.vue`: 点击搜索结果 → 加载对应对话并滚动到匹配消息
  - [x] `index.css`: 搜索结果样式 (`.search-result-item`, `.search-highlight`)
  - *技术详情见 bob_v04_dev_guide.md T-1301*

- [x] T-1302: **记忆透明化 — "Bob 的记忆"**
  - [x] 新建 `memory.rs` (或在 `dream.rs` 中扩展): `system_get_memory_entries()` 读取 sessions/ 和 wiki/ 目录
  - [x] `memory.rs`: `system_delete_memory_entry(type, id)` 安全删除记忆文件
  - [x] `lib.rs`: 注册 `system_get_memory_entries`, `system_delete_memory_entry`
  - [x] `tauri-bridge.js`: 新增 `getMemoryEntries()`, `deleteMemoryEntry(type, id)`
  - [x] `SettingsView.vue`: 在主题区块之后新增"Bob 的记忆"区块 (卡片列表，每条一行 + X 删除按钮)
  - [x] `zh-CN.json` / `en-US.json`: 新增 `settings.bob_memory`, `settings.bob_memory_hint` 翻译 (严禁 Emoji)
  - *技术详情见 bob_v04_dev_guide.md T-1302*

- [x] T-1303: **Cron 执行结果通知**
  - [x] `Cargo.toml`: 启用 `tauri` 的 `notification` feature
  - [x] `capabilities/default.json`: 添加 `notification:default` 权限
  - [x] `scheduler.rs`: `execute_cron_job()` Step 6 后追加 `app.notification().builder().title().body().show()`
  - [x] `InboxView.vue`: `scheduler:completed` 事件处理中为最新完成项添加 `.cron-result-new` 高亮类 (3s 后移除)
  - [x] `App.vue`: 全局监听 `scheduler:completed`，若当前不在 InboxView → 日程导航项显示红点 badge
  - [x] 通知文案: 直接使用任务 title，正文为结果前 100 字 (严禁 Emoji)
  - *技术详情见 bob_v04_dev_guide.md T-1303*

### Phase 2: 防御升级 — 让用户"不出错" (2-3周)

- [x] T-1304: **启动自检医生 (Bob Doctor)**
  - [x] 新建 `doctor.rs`: 定义 `CheckResult` 结构体 (code, severity, message, fixable)
  - [x] `doctor.rs`: `system_health_check()` 检查 config.json 可读性、bob.db 可读性、API Key 存在性、磁盘可写性
  - [x] `doctor.rs`: `system_auto_fix(code)` 自动修复逻辑 (CONFIG_CORRUPT → 从 bak 恢复)
  - [x] `lib.rs`: `mod doctor;` + 注册 `system_health_check`, `system_auto_fix`
  - [x] `tauri-bridge.js`: 新增 `healthCheck()`, `autoFix(code)`
  - [x] `App.vue`: `onMounted` 调用 `healthCheck()`，结果存入全局 reactive state
  - [x] `ChatView.vue`: 顶部 sticky 横幅 (32px高, warning=bg-tertiary, error=rgba(200,100,50,0.08))
  - [x] `ChatView.vue`: 横幅包含人话提示 + 可选"一键修复"按钮 + X 关闭 (localStorage 24h 内不重复)
  - [x] `index.css`: `.health-banner`, `.health-banner--warning`, `.health-banner--error`
  - *技术详情见 bob_v04_dev_guide.md T-1304*

- [x] T-1305: **聊天就绪守卫 (Chat Readiness)**
  - [x] `llm.rs`: 新增 `system_validate_chat_ready()` — 检查 provider/model/apiKey 本地配置完整性 (不做网络探测)
  - [x] `lib.rs`: 注册 `system_validate_chat_ready`
  - [x] `tauri-bridge.js`: 新增 `validateChatReady()`
  - [x] `ChatView.vue`: `onMounted` 调用一次，缓存 60s
  - [x] `ChatView.vue`: 不就绪时发送按钮 `disabled` (opacity 0.4) + 输入框下方一行提示 + "前往设置"链接
  - [x] Fail-open: 任何超时/不确定情况返回 `ready: true`
  - *技术详情见 bob_v04_dev_guide.md T-1305*

### Phase 3: 主动性升级 — 让 Bob "动起来" (3-4周)

- [x] T-1306: **对话自动提取行动项** (0.5天, 纯 Prompt Engineering)
  - [x] `llm.rs`: system prompt 追加"行动项捕捉"指令段 — bob-action-items 代码块格式
  - [x] `useChat.js`: 解析 bob-action-items 代码块，提取行动项并渲染 ActionItemCard 交互卡片
  - [x] `ActionItemCard.vue`: 独立组件，支持保存到日历/忽略
  - *已实现：Prompt + 前端解析 + 交互卡片*
  - *技术详情见 bob_v04_dev_guide.md T-1306*

- [x] T-1307: **智能待办跟进**
  - [x] `calendar.rs`: events 表新增 `last_notified INTEGER DEFAULT 0` 列 (ALTER TABLE)
  - [x] `scheduler.rs`: 主循环追加 `check_upcoming_todos()` — 查询今日到期 + pending 的事件
  - [x] `scheduler.rs`: 到期待办触发系统通知 (notification API) + emit `todo:reminder` 事件
  - [x] `scheduler.rs`: 提醒频率控制 — 同一待办 last_notified 当天不重复
  - [x] `InboxView.vue`: 监听 `todo:reminder`，今日到期项左侧边框变为 `var(--accent-primary)`
  - *技术详情见 bob_v04_dev_guide.md T-1307*

- [x] T-1308: **晨间简报增强**
  - [x] `dream.rs`: `getDreamReport` 返回数据扩展 — 新增 `today_events`, `today_todos` 字段 (查询 events 表)
  - [x] `MorningBriefing.vue`: 对话回顾区块之前插入"今日日程"和"待完成事项"区块
  - [x] `zh-CN.json` / `en-US.json`: 新增 `briefing.today_schedule`, `briefing.today_todos` (严禁 Emoji)
  - [x] 简报总长度控制: 不超过一屏 (~300字)
  - *技术详情见 bob_v04_dev_guide.md T-1308*

---

## 📍 目标 14: v0.5 — 认知引擎升级 (Cognitive Engine v2)
> 🎯 **目标**: 让 Bob 从"能记住事"进化为"会思考的记忆体"——自动去噪、自我纠错、成本自控。
> 📋 **来源**: `docs/分布式 Agent 认知系统审视.docx` 理论框架，提取出 5 个可落地到单 Agent 桌面产品的改进点。
> 🏗️ **核心原则**: 所有"智能"逻辑尽可能下沉到 Rust 确定性层（瘦智能体，胖平台），减少对 LLM 的依赖。

### Phase 0: 确定性防御层 (P0 — 立即可做，纯 Rust)

- [x] T-1401: **工具调用循环熔断器 (Tool Call Circuit Breaker)**
  - [x] `tools.rs`: 新增 `ToolCallHistory` 结构体，记录最近 N 次工具调用的 `(tool_name, args_hash)` 元组
  - [x] `tools.rs`: `execute_tool()` 入口处检测：如果连续 3 次调用同名工具且参数哈希相似度 > 80%，返回 `Err("循环检测: 该工具已连续调用 3 次且参数高度相似，已自动中止")`
  - [x] `llm.rs`: Tool Calling 循环收到熔断错误后，将错误信息作为 tool result 回注，让 LLM 自行决定下一步（换工具/放弃/告知用户）
  - [x] `llm.rs`: 新增全局计数器 `tool_call_budget`，单次对话工具调用总量上限 15 次（当前硬编码 5 轮×每轮多工具，实际可能超过）
  - *预期效果: 消除"搜索失败→换词再搜→再失败"的无限循环，直接省 Token 费用*
  - *理论依据: 论文命题 2 §3 "状态机异常探测"——在 Agent 推理回路之外引入确定性机制*

### Phase 1: 记忆质量升级 (P1 — 需 LLM 配合)

- [x] T-1411: **长对话上下文分级压缩 (Context Tiering)**
  - [x] `llm.rs`: `build_messages()` 改造 — 将对话历史分为三层:
    - **活跃层** (最近 6 轮): 原样保留
    - **摘要层** (7~20 轮): 由牛马模型压缩为 ≤200 字的段落摘要，作为单条 system message 注入
    - **废弃层** (20 轮以上): 不进入上下文
  - [x] `llm.rs`: 压缩触发条件 — 当活跃层消息总 Token 估算 > 4000 时，自动将最旧的活跃层消息推入摘要层
  - [x] `llm.rs`: 摘要缓存 — 压缩结果缓存在内存中 (HashMap<conversation_id, String>)，同一对话不重复压缩
  - [x] 被用户明确否决的方案（如"不要这个方案"之后的 assistant 回复）标记为废弃，不进入摘要
  - *预期效果: 50 轮对话后 Bob 依然头脑清晰，不会把第 3 轮的试探当作最终决策*
  - *理论依据: 论文脆弱点 1 "上下文污染与垃圾回收"*

- [x] T-1412: **记忆置信度与冲突消解 (Memory Confidence & Conflict Resolution)**
  - [x] `dream.rs`: 记忆文件元数据扩展 — 每条记忆附带 `confidence: f32` (0.0~1.0) + `source: enum {UserExplicit, Inferred, Corrected}` + `last_referenced: timestamp`
  - [x] `dream.rs`: Dream 整理时新增"冲突检测"步骤:
    - 用牛马模型对比同一主题的多条记忆
    - 如果存在矛盾 → 保留最新的 UserExplicit 来源，废弃旧条目
    - 如果从未被引用且超过 30 天 → confidence 衰减至 0，归档
  - [x] `llm.rs`: 对话中用户明确纠正 Bob 时（如"不对，项目 A 用的是 Vue"），触发即时记忆更新:
    - 搜索匹配的旧记忆 → 标记为 `Corrected` → 写入新记忆 `source: UserExplicit, confidence: 1.0`
  - *预期效果: Bob 不再说"你之前说过用 React，但也说过用 Vue"，直接给出最新正确答案*
  - *理论依据: 论文脆弱点 1 "信息分级机制"*

### Phase 2: 智能整理升级 (P2 — 需向量能力)

- [x] T-1421: **Dream 语义去重引擎 (Semantic Deduplication)**
  - [x] `dream.rs`: 新增 `deduplicate_memories()` 阶段，在 Dream 流程的 merge 步骤之前执行
  - [x] 实现方式 (两种方案择一):
    - **方案 A (轻量)**: 用牛马模型对每条记忆生成 ≤50 字的"语义指纹"，文本相似度 > 0.85 的归为同组
    - **方案 B (精确)**: 调用 embedding API（如 doubao-embedding）将记忆编码为向量，余弦相似度 > 0.9 的归为同组
  - [x] 同组记忆合并策略: 保留信息最完整的一条，其余标记为"已合并"并归档
  - [x] 合并日志写入 Dream 报告，晨报中显示"整理了 N 条重复记忆"
  - *预期效果: 晨报里不再出现三条措辞不同但含义相同的记忆条目*
  - *理论依据: 论文命题 2 §2 "信息增益衰减"——净增量知识逼近零时应自动收敛*

### Phase 3: 智能路由升级 (P3 — 研究性质)

- [x] T-1431: **任务复杂度感知路由 (Complexity-Aware Routing)**
  - [x] `llm.rs`: 新增 `estimate_task_complexity()` 函数，在发送请求前对用户输入进行快速评估:
    - **信号 1**: 输入长度 (> 2000 字 → 高复杂度)
    - **信号 2**: 附件类型 (代码/合同/学术论文 → 高复杂度)
    - **信号 3**: 用户历史 — 如果当前对话已经使用过 3+ 种工具 → 高复杂度
    - **信号 4**: 关键词检测 ("分析"/"审查"/"对比"/"重构" → 高复杂度)
  - [x] 路由逻辑:
    - 低复杂度 + 当前指定为牛马模型 → 保持牛马
    - 高复杂度 + 当前指定为牛马模型 → **自动升级为主力模型**，并在回复开头附带一条淡色提示"[已自动切换至主力模型]"
    - 主力模型永远不降级（用户显式选择的优先级最高）
  - [x] 配置项: `settings.auto_model_upgrade: bool` (默认 true)，可在设置页关闭
  - *预期效果: 拖入普通文件用便宜模型秒处理，拖入合同自动调用主力模型深度分析——用户什么都不用管*
  - *理论依据: 论文重构壁垒 1 "MoMA 泛化路由"的单 Agent 简化版*

---

## 📍 目标 15: v0.6 — 文档输出引擎 (Document Export Engine)
> 让 Bob 从"只会说"进化为"能交付"——对话结束后导出精排版 HTML 报告、PDF、Excel、Word、PPT。
> 核心策略: **HTML-first** — 精排 HTML 是主力输出，PDF 通过打印导出。
> 设计来源: o2_analysis 项目 + guizang-ppt-skill + mckinsey-designer + frontend-design

### Phase 0: 基建准备

- [x] T-1500: **输出目录与 Bridge 基建**
  - [x] `config.rs`: 新增 `exportsDir` 配置项 (默认 `~/Desktop/Bob-Exports/`)
  - [x] `lib.rs` + `tauri-bridge.js`: 注册所有新增 Rust Command
  - [x] `tools.rs`: 注册 `export_html`, `export_xlsx`, `export_docx`, `export_pptx` 为 Tool Calling 工具
  - [x] `llm.rs` system prompt: 追加文档导出能力描述段

### Phase 1: HTML 报告 + PDF 导出 (Tier 1, 核心)

- [x] T-1501: **HTML 报告模板系统** — `report.rs` 已实现，含 CSS + marked.js + print-to-PDF 按钮

- [x] T-1502: **`export_html` Tool Calling 接口** — 已注册并可用

- [x] T-1503: **PDF 导出路径** — HTML 内嵌 print 按钮方案

### Phase 2: XLSX 数据导出 (Tier 2, 数据刚需)

- [x] T-1511: **Rust XLSX 写入引擎** — `xlsx.rs` 已实现 (rust_xlsxwriter, 表头/冻结/交替行色)

- [x] T-1512: **`export_xlsx` Tool Calling 接口** — 已注册并可用

### Phase 3: DOCX 文档导出 (Tier 3, 基础文字)

- [x] T-1521: **Rust DOCX 写入引擎** — `docx.rs` 已实现 (docx_rs, H1-H3/列表/段落/A4)

- [x] T-1522: **`export_docx` Tool Calling 接口** — 已注册并可用

### Phase 4: PPTX 模板注入式生成 (Tier 4, 延后)

- [x] T-1531-1534: **PPTX 引擎** — `pptx.rs` 已实现 (383行, ZIP 组装, 封面/内容/章节/总结页, 暗/亮主题, 项目符号列表)

---

## 📍 目标 16: v0.4.1 — Shell 执行引擎 + 通讯渠道接入
> 🎯 **目标**: 补齐白领场景的"文件整理"与"移动端通讯"两个关键能力缺口。
> 📋 **来源**: 用户反馈 — 基础文件操作 + Telegram/Discord 后端接入。
> 🏗️ **预估工作量**: 2-3 天。

### Phase 0: 架构断裂修复 (Bug Fix — 最高优先级)

- [x] T-1601: **agentMode/globalFileAccess 透传修复** — 已完成，全链路已连通 (useChat.js → bridge → lib.rs → llm.rs → tools.rs)
  - [ ] `tauri-bridge.js`: sendChat/sendVision 将 `globalFileAccess`, `agentMode` 透传给 Rust invoke
  - [ ] `lib.rs`: llm_chat/llm_vision 命令签名新增 `global_file_access: bool`, `agent_mode: String`
  - [ ] `llm.rs`: stream_internal() 接收并使用这两个参数:
    - `global_file_access` → 传给 execute_tool() → resolve_write_path()
    - `agent_mode == "yolo"` → system prompt 附加"干活模式"指令
  - [ ] `tools.rs`: 移除 L1419-1420 的 TODO 硬编码 `let global_file_access = false`

### Phase 1: 文件操作工具集 (Shell-Lite, 5 个新工具)

- [ ] T-1611: **create_directory 工具**
  - [ ] `tools.rs`: Schema + execute 分支, 使用 `std::fs::create_dir_all()`
  - [ ] 安全: 复用 `resolve_write_path()` 白名单

- [ ] T-1612: **move_file 工具**
  - [ ] `tools.rs`: Schema + execute 分支, 使用 `std::fs::rename()` + 跨盘降级 copy+delete
  - [ ] 安全: 源路径需在 tracked_folders 内, 目标路径走 `resolve_write_path()`

- [ ] T-1613: **copy_file 工具**
  - [ ] `tools.rs`: Schema + execute 分支, 使用 `std::fs::copy()`
  - [ ] 安全: 同 move_file

- [ ] T-1614: **delete_file 工具 (回收站优先)**
  - [ ] `Cargo.toml`: 引入 `trash = "5"` 跨平台回收站 crate
  - [ ] `tools.rs`: Schema + execute 分支, 优先 `trash::delete()`, 降级 `std::fs::remove_file()`
  - [ ] 安全: 仅允许删除 tracked_folders / workspaceDir 内的文件

- [ ] T-1615: **rename_file 工具**
  - [ ] `tools.rs`: Schema + execute 分支, 使用 `std::fs::rename()` 同目录内
  - [ ] 安全: 复用 `resolve_write_path()`

- [ ] T-1616: **System Prompt 更新**
  - [ ] `llm.rs`: 工具列表注释区追加 5 个文件操作工具的描述

### Phase 2: Telegram Bot 后端

- [x] T-1621: **新建 `telegram.rs` 模块** — 136 行，长轮询 + stream_chat 回复 + Token 持久化

- [x] T-1622: **Telegram IPC 命令注册** — lib.rs + tauri-bridge.js 已对接

- [x] T-1623: **Telegram 前端 UI** — SettingsConnections.vue 已集成

### Phase 3: Discord Bot 后端

- [x] T-1631: **新建 `discord.rs` 模块** — 233 行，WebSocket Gateway + Heartbeat + REST 回复 + Bot 循环检测

- [x] T-1632: **Discord IPC 命令注册** — lib.rs + tauri-bridge.js 已对接

- [x] T-1633: **Discord 前端 UI** — SettingsConnections.vue 已集成

### Phase 4: 验证

- [ ] T-1641: cargo check + cargo clippy 编译通过
- [ ] T-1642: 端到端测试 — 对话中"帮我建个文件夹"/"移动文件" 验证
- [x] T-1643: Telegram Bot 测试 — Token 激活 → 手机发消息 → Bob 回复
- [x] T-1644: Discord Bot 测试 — Token 激活 → DM 发消息 → Bob 回复

---

## 📍 目标 17: v0.33 — 知识图谱融合 (Knowledge Graph)

> 核心目标：把 iknow 的语义图谱能力原生化到 Bob 中，闭合“拖拽文件夹 → 知识提取 → 图谱展示”的完整 UX 循环。

### Phase 0: 数据层 — SQLite 图存储 + Rust 图引擎

- [x] T-1701: `bob.db` 新增 `kg_nodes` + `kg_edges` 两张表
- [ ] T-1702: Rust 模块 `kg.rs` — Node/Edge CRUD（insert, upsert, delete）
- [ ] T-1703: `kg.rs` — BFS 子图查询 `kg_query(term, max_hops)` → 返回 JSON
- [ ] T-1704: `kg.rs` — 图统计 `kg_get_stats()` → 节点数/边数/类型分布

### Phase 1: 提取层 — LLM 实体+关系提取

- [ ] T-1711: `kb_indexer.rs` 扩展 — Prompt 追加 relations 字段
- [ ] T-1712: 索引完成后调用 `kg.rs` 写入节点和边（去重 upsert）
- [ ] T-1713: `brain_search` 升级 — FTS5 + 图谱子图 RRF 混合

### Phase 2: 前端 — KnowledgeGraphView

- [x] T-1721: 安装 `vis-network` npm 依赖
- [ ] T-1722: `KnowledgeGraphView.vue` — vis.js 力导向图主画布
- [ ] T-1723: 顶部工具栏 — 搜索框 + 类型筛选 chips + 节点统计
- [ ] T-1724: 右侧 Inspector 面板 — 节点详情 + 摘要 + 关联列表
- [ ] T-1725: 侧边栏新增“知识图谱”导航入口

### Phase 3: 流程串联 — 闭合 UX 循环

- [ ] T-1731: KB 构建完成消息添加“查看知识图谱” CTA 按钮
- [ ] T-1732: 进度消息分三阶段：提取文本 → 生成摘要 → 构建图谱
- [ ] T-1733: Tool Calling 新增 `query_knowledge_graph` 工具
- [ ] T-1734: 对话中右键 → “提取到知识图谱”

### Phase 4: 图谱维护

- [ ] T-1741: Dream V3 — 检测孤立/重复节点，标记 superseded
- [ ] T-1742: Inspector 支持手动编辑关系
- [ ] T-1743: 图谱导出（JSON / Markdown）

---

## 📍 目标 19: Goal Mode V2 — 双层裁判 + 自进化失败闭环

> 🎯 **核心理念**: "失败不是日志，而是进化请求"。为 Goal Mode 注入 Layer 1 确定性断言（零 Token 预筛）和 Dream Engine 失败模式分析（夜间自动提炼避坑指南写入 SOUL.md）。
> 📖 **设计来源**: [AI智能体深度分析与产品优化.docx](references/AI智能体深度分析与产品优化.docx) + [朋友的务实建议](references/20260627_bob_next_step_with_coderunner.txt)

### Phase 1: 数据层 — execution_errors 表
- [x] T-1901: `db.rs` — 新增 `execution_errors` 表建表语句 + 索引
- [x] T-1902: `db.rs` — 新增 `log_execution_error()` 写入函数
- [x] T-1903: `db.rs` — 新增 `get_unanalyzed_errors()` 查询函数
- [x] T-1904: `db.rs` — 新增 `mark_errors_analyzed()` 标记函数

### Phase 2: 断言引擎 + stream_internal 扩展
- [x] T-1911: `assertions.rs` — 新建模块，实现 6 条断言规则 (NO_EMPTY_EXECUTION / ERROR_RATIO / NO_ANALYSIS_PARALYSIS / FILE_WRITE_SUCCESS / COMMAND_EXIT_CODE / BUDGET_EFFICIENCY)
- [x] T-1912: `assertions.rs` — 7 个单元测试全部通过
- [x] T-1913: `llm.rs` — stream_internal 返回值新增 `tool_summary` 字段 (total_calls / total_failures / calls[])
- [x] T-1914: `lib.rs` — 注册 `mod assertions;` 模块

### Phase 3: Goal Mode 改造 + Dream Pipeline Phase 4
- [x] T-1921: `goal.rs` — 在 Maker 与 Checker 之间插入 Layer 1 断言层
- [x] T-1922: `goal.rs` — Checker Fail 时持久化错误到 execution_errors
- [x] T-1923: `goal.rs` — 3 轮耗尽时持久化 budget_exhausted 错误
- [x] T-1924: `evolution.rs` — 新增 `phase_failure_analysis()` 函数
- [x] T-1925: `evolution.rs` — 在 `run_dream_pipeline()` 中调用 Phase 4
- [x] T-1926: `evolution.rs` — SOUL.md 避坑区追加逻辑（已有则替换，无则追加）
- [x] T-1927: `evolution.rs` — 更新 DreamReport 结构体增加 failure_insights 字段

### Phase 4: 验证
- [x] T-1941: `cargo check` 编译通过 (57 warnings 全为预存, 无新 error)
- [x] T-1942: `cargo test assertions` 7 个单元测试全部通过

---


## 📝 v0.32.2 工作记录 (2026-06-26)

**完成**:
1. [Feature] **一键体检与自愈面板**: 在设置页面新增 "Doctor" 系统自检 Dashboard，包含关键配置、连通性检测，支持一键热修复。
2. [Fix] **消息横幅免打扰**: 聊天主界面的体检警告横幅增加 `localStorage` 的 24 小时休眠逻辑，防止频繁打扰。
3. [Arch] **毫秒级无感自愈防线**: 重构 `db.rs` 的数据库初始化。在开局 1 秒闪屏内植入探针连接执行 `PRAGMA quick_check;`。若查出损坏瞬间完成热回滚，并解除 SQLite 文件锁以防死锁，实现 "Fail-Open" 降级不死机策略。
4. [UI] **日程中心视图优化**: 调整 `WeekTimeline.vue` 展示逻辑。废除自然周（周一至周日）强绑定，改为始终以“今天”为中心的滚动 7 天视图（过去 3 天 + 未来 3 天），大幅提升聚焦体验。

**全部完成** 🎉

---

## 📝 v0.32.1 工作记录 (2026-06-12 ~ 2026-06-14)

**完成**:
1. [Fix] **Release 版日志修复**: 移除 `cfg!(debug_assertions)` 守卫，Release 构建现在输出日志至 `logs/bob.log` (2MB 轮转)
2. [Fix] **CDN 上传超时修复**: 固定 120s 超时替换为动态计算: `max(120s, size_in_MB * 30s)`
3. [Feature] **实时上传进度条**: stream-based 分块上传 (64KB/chunk)，前端实时显示文件名 + 百分比 + 字节计数
4. [Arch] **外层工具超时与 CDN 匹配**: send_wechat_file 外层 tokio timeout 120s -> 600s
5. [Verify] **T-1601 透传修复确认**: 全链路已连通 (useChat -> bridge -> lib -> llm -> tools)，关闭过期 TODO
6. [Verify] **T-1611~1616 文件操作工具确认**: 5 个工具 (create_directory/move/copy/delete/rename) 已完整实现
7. [Feature] **streamThinking 流式思考动画**: 脉冲圆点 + 可折叠面板 + 自动滚动 + i18n
8. [Feature] **工具结果缓存**: 会话级 HashMap (read_file/list_dir/list_skills/read_skill/system_time)，写操作自动清空

**全部完成** 🎉

## 📍 目标 18: Goal Mode (闭环执行引擎)
> 🎯 **目标**: 让 Bob 具备以结果为导向的闭环执行能力，在遇到复杂任务时自主拆解、评估和重试，直至任务完全成功。

- [x] T-1901: **Rust 后端: Core Setup**
  - [x] 修改 `tools.rs` 的 `ToolCallTracker` 增加 `with_budget(budget)`，支持 Goal 模式下调高 Token 预算（例如 50 轮）。
  - [x] 将 `llm.rs` 的 `stream_internal` 与 `call_clerk_oneshot` 提升为 `pub(crate)` 供跨文件调用。
- [x] T-1902: **Rust 后端: Evaluator Engine (Maker/Checker 架构)**
  - [x] 新建 `goal.rs` 模块并实现 `execute_goal_loop` 外层闭环。
  - [x] 引入独立评估器：执行后使用 Clerk 模型根据初始目标独立验证结果 (PASS / FAIL / PARTIAL)。
  - [x] 自动重试与上下文注回：若验证失败，将反馈无缝附加至消息流继续尝试（最多 3 次大循环）。
- [x] T-1903: **Rust 后端: 路由调度**
  - [x] 修改 `http_api.rs` 的 `ChatRequest` 注入 `agent_mode` 字段。
  - [x] 拦截 `stream_chat` 调用，若 `agent_mode == "goal"` 则路由给 `goal::execute_goal_loop` 引擎。
- [x] T-1904: **前端集成 (UI 与事件绑定)**
  - [x] 在 `ChatView.vue` 消息引擎模式选择器中增加 "目标 (Goal)" 选项卡。
  - [x] 引入 `Target` (靶心) 样式及文案 (`zh-CN.json` / `en-US.json`) 翻译。
  - [x] 前端天然继承 `llm:chunk` 事件，实现过程反馈文本无缝流式加载呈现。

## 🚀 T-1800: Bob 联邦网络与 Web Drop 引擎
- [x] **阶段一：信令层与基础设施 (bob-relay + coturn)**
  - [x] 使用 Node.js 编写 `bob-relay` WebSocket 中继服务，实现无落盘的内存管线（双向信令+数据盲传）。
  - [x] 在 VPS1 安装配置 `coturn` 作为 WebRTC STUN/TURN 服务器。
  - [x] 编写 `bob-services.sh` 脚本统管 `bob-relay` 和 `coturn` 的生命周期（setup/reset/status）。
  - [x] 在 VPS1 的 Caddy 中配置反向代理和静态目录。
- [x] **阶段二：Web Drop MVP (三级渐进式传输)**
  - [x] 前端 (`index.html`)：实现 WebRTC P2P (DTLS加密) 接收逻辑，并包含 AES-GCM 降级中继逻辑。
  - [x] Bob 桌面端 (`web_drop.rs`)：生成 URL，创建房间，尝试建立 DataChannel 传输，如果超时 10 秒无响应则自动触发回退协议。
- [ ] **阶段二 (Bug 修复 & 部署测试)**
  - [ ] **[高优]** Cloudflare 代理拦截了 WebSocket 握手，需将 DNS 的橙色云朵改成灰色 (DNS Only)，然后让 Caddy 自动申请证书。
  - [ ] **[UI]** 修复 URL 中分隔符 `|` 导致手机微信无法点击的问题（替换为 `.`）。
  - [ ] **[运维]** 修复 `bob-services.sh` 中 sudo 下执行 node 找不到命令的环境变量问题。
- [ ] **阶段三：联邦身份与 Agent Swarm (远景)**
  - [ ] 在 `SettingsView` 新增【联邦网络】面板，生成本地私有的 `Swarm Key` (AES 密钥种子)。
  - [ ] `bob.db` 新增 `pending_transfers` SQLite 队列表，实现设备异步离线传输。
  - [ ] 增加 LLM `send_to_device` 工具，实现跨设备大模型指令接力交互。


## 📍 目标 19: 智能笔记模块 Bob Notebook (T-1900) ✅
> ✅ **已在目标 17 知识图谱融合中全部实现**。后端 `notebook.rs` (888 行, 14 个 IPC)、前端 `TiptapEditor.vue` + `NoteExplorer.vue` 均已完成并集成到 KnowledgeGraphView。
> 包含：CRUD、标签管理、反向链接、全文搜索、Dream 笔记摘要、气泡存笔记、@note 上下文注入。

---

## 📍 目标 20: 内网穿墙隧道 (Proxy Tunnel) 与中继模式
> 🎯 构建一个纯粹的代理信息流管道（Proxy/Ladder），绕开公司内网防火墙。
> 📋 **核心逻辑**: 作为全局功能开关存在。对于受限网络环境一键开启穿墙透传，而无限制网络环境继续依赖现有的直连方式，互不干扰。

### Phase 1: 前端全局开关与 UI (SettingsConnections)
- [x] T-2001: 在 SettingsConnections.vue 增加【内网穿墙隧道】拨动开关。
- [x] T-2002: 开关状态与后端 config.json 双向绑定。
- [ ] T-2003: UI 面板显示当前隧道的连接状态（🟢代理已连接 / 🔴代理断开）与实时延迟。

### Phase 2: Rust 后端网络层重构 (Tunnel Client & Proxy)
- [ ] T-2011: src-tauri/src/tunnel.rs 实现到 VPS 的长连接隧道引擎（WebSocket或TCP中继）。
- [ ] T-2012: 改造微信模块 (wechat/req.rs)：请求发前判定，若穿墙开启则通过 Tunnel 管道透传包，否则正常走直连。
- [ ] T-2013: 改造 Telegram 模块 (	elegram.rs) 同上网络劫持逻辑。
- [ ] T-2014: 维持自动重连心跳，确保网络波动时隧道的高可用性。

### Phase 3: VPS 中继端配合 (Tunnel Server)
- [ ] T-2021: VPS (bob.bobbik.org) 上配置对应的解包与转发逻辑（直接用 Nginx Stream Proxy，或单独部署轻量 WebSocket Server 均可）。

---

## 📍 目标 22: Bob-Mobile 手机端 MVP (T-2200)
> 🎯 **目标**: 在同一个 bob-agent 仓库中，基于 Tauri V2 的 Mobile 支持，构建手机端极简入口。
> 📋 **核心定位**: 手机是"独立前哨站"——能独立调用云端 API 产出知识点和管理日程，但不跑图谱编织和重计算。
> 📖 **详细蓝图**: `docs/MOBILE_BLUEPRINT.md`

### Phase 0: 升级基建
- [ ] T-2201: 将 Tauri 从 RC (2.0.0-rc.17) 升级到正式版 2.x stable。
- [ ] T-2202: 运行 `tauri android init`，生成 Android 移动端脚手架。
- [ ] T-2203: 验证空白 WebView 在安卓模拟器上能正常打开。

### Phase 1: 代码重构——分层
- [ ] T-2211: 将现有 `src/views/` 和 `src/components/` 拆分到 `desktop/` 和 `shared/`。
- [ ] T-2212: 将现有 `src-tauri/src/*.rs` 拆分到 `core/` 和 `desktop/` 子目录。
- [ ] T-2213: 在 `Cargo.toml` 中引入 `desktop` / `mobile` features 条件编译。
- [ ] T-2214: 实现 `router.js` 的平台检测与动态路由 (Desktop Shell / Mobile Shell)。
- [ ] T-2215: **回归测试**：确保 PC 版所有功能不受分层重构影响。

### Phase 2: 手机端 MVP
- [ ] T-2221: 实现 `MobileShell.vue` (底部 TabBar + 基础导航 + 安全区域适配)。
- [ ] T-2222: 实现 `CaptureView.vue` (文字输入 + 语音按钮 + 主界面)。
- [ ] T-2223: 实现 `mobile/ChatView.vue` (轻量聊天，复用 `core/llm.rs`，无 Tool Calling 可视化)。
- [ ] T-2224: 实现 `PairView.vue` + `qr_scanner.rs` (扫码绑定 + Ed25519 密钥交换)。
- [ ] T-2225: 实现手机端 SQLite 缓存 (SyncPacket 表 + config 存储)。
- [ ] T-2226: 在安卓真机上跑通一次完整的"语音输入 → 云端 API 调用 → 收到回复"流程。

---

## 📍 目标 23: 跨端同步引擎 (T-2300)
> 🎯 **目标**: 复用现有 bob-relay + coturn + WebRTC 基建 (T-1800)，实现手机与 PC 的四级渐进式数据同步。
> 📋 **核心策略**: PC 主导唤醒，四级降级 (局域网直连 → WebRTC P2P 打洞 → TURN 中继 → 微信 Bot 推送)。
> 📖 **详细蓝图**: `docs/MOBILE_BLUEPRINT.md` §五

### Phase 3a: bob-relay 增加设备注册协议
- [ ] T-2301: 在现有 bob-relay (Node.js, VPS1) 中新增设备注册协议 (register/query/notify)。
- [ ] T-2302: PC 端 Bob 启动时自动向 bob-relay 注册 device_id + 在线状态。

### Phase 3b: 同步通道 (复用 Web Drop 引擎)
- [ ] T-2311: 复用 `web_drop.rs` 的 WebRTC 引擎，改造为持久化双向 DataChannel 同步通道。
- [ ] T-2312: 实现四级渐进式连接策略的完整决策链 (局域网 UDP → WebRTC → TURN → Bot)。
- [ ] T-2313: `http_api.rs` 新增 `/v1/sync` 端点，供手机局域网直连时使用。
- [ ] T-2314: 实现手机端 `lan_sync.rs`：被动监听 PC 连接 + 主动回连。

### Phase 3c: 同步协议
- [ ] T-2321: 实现 `sync_protocol.rs` (SyncPacket 序列化 + Ed25519 认证 + X25519-AES-GCM 加密)。
- [ ] T-2322: 实现 `config_version` 配置版本同步机制 (PC 改了 API Key → 手机下次同步自动更新)。
- [ ] T-2323: 实现 SyncPacket 批量传输与 ACK + synced 标记机制。

### Phase 3d: 端到端联调
- [ ] T-2331: 局域网同步联调 (同一 WiFi，UDP 广播发现 + HTTP 直连)。
- [ ] T-2332: 跨网 WebRTC 打洞联调 (手机 4G + PC WiFi，经 bob-relay 信令 + coturn STUN)。
- [ ] T-2333: Bot 推送唤醒联调 (手机被杀后台 → PC 通过微信 Bot 推送 → 用户打开手机端 → 同步)。

---

## 🔙 已后置的待办事项 (Backlog)

### 📍 目标 21: 知识图谱 Source-Hub 架构重构 (Implicit Provenance + Cascade GC)
> 🎯 **目标**: 彻底解决知识图谱中的"幽灵节点"问题，实现按来源批次成套导入/成套清除，并让跨项目的相同概念自动融合桥接。
> 📋 **核心设计**: 隐式溯源 (JSON 数组多血缘) + 实体去重融合 + 引用计数垃圾回收。
> 🏗️ **架构哲学**: 借鉴 Google Drive 的扁平化对象存储 + Capacities 的面向对象知识管理，实现"底层隐式血缘（用于生命周期管理）、上层纯粹语义网状关联（用于知识发现）"的混合架构。

#### Phase 1: 数据库 Schema 升级
- [x] T-3001: `db.rs` — 迁移逻辑：`ALTER TABLE kg_nodes ADD COLUMN source_batches TEXT DEFAULT '[]'`
- [x] T-3002: `db.rs` — 新增 `kg_source_batches` 批次注册表

#### Phase 2: 改造 `upsert_node` 多来源缝合
- [x] T-3010: `kg.rs` — `upsert_node` 新增 `batch_id` 参数
- [x] T-3011: `kg.rs` — ON CONFLICT 时用 `JSON_INSERT` 追加来源（含去重判断）
- [x] T-3012: `kg.rs` — 更新 `kg_backfill` 兼容新签名

#### Phase 3: 改造 `system_build_kb` — 批次注册 + Hub 节点
- [x] T-3020: `kb_indexer.rs` — 生成 `batch_id` 并写入 `kg_source_batches`
- [x] T-3021: `kb_indexer.rs` — 传递 `batch_id` 给所有 `upsert_node` 调用
- [x] T-3022: `kb_indexer.rs` — 创建 Source Hub 节点（类型 `"source"`）+ `contains_file` 边

#### Phase 4: 新增 `system_remove_source` 级联清除 API
- [x] T-3030: `kg.rs` — 新增 `remove_source_batch()` 引用计数 GC 函数
- [x] T-3031: `kb_indexer.rs` — 新增 `system_remove_source` Tauri Command
- [x] T-3032: `lib.rs` — 注册新命令到 `generate_handler![]`

#### Phase 5: 前端适配
- [x] T-3040: preload 桥接层暴露 `systemRemoveSource`
- [x] T-3041: `KnowledgeGraphView.vue` — Source 节点详情面板 + "清除整个批次"按钮
- [x] T-3042: 实现清除按钮逻辑：二次确认 → API 调用 → 图谱刷新
- [x] T-3043: 图谱统计区域增加"来源批次列表"

---


### ⚠️ 微信 CDN 文件传输缺陷（P2，后置）
> 需深度排查 reqwest 流式分块上传与微信 API 侧限制
- [ ] 检查 `wechat/cdn.rs` 分块流式上传逻辑 `build_progress_body`
- [ ] 排查 `ilink/bot/getuploadurl` 接口对大文件的签名超时问题
- [ ] 实现 >25MB 失败时自动降级至 Web Drop (T-1800) 的链接分享机制

### ⚠️ PDF 转图片功能失效（P2，后置）
> 需修改 Rust 接口使用 `app_handle.path().resource_dir()` 动态定位 pdfium.dll
- [ ] 修改 `pdf_renderer.rs` 与 `kb_extractor.rs` 绑定路径为 Tauri 资源目录

### 💡 界面体验清债与输入指令重构（P3，后置）
- [ ] Slash/Mention Command 智能悬浮补全菜单
- [ ] Chat 界面增加显性的"📌 作为笔记速记"按钮

### 💡 日程交互重构 (T-1801)（P3，后置）
- [ ] 拖拽事件 (Drag & Drop) 改变日程日期与时间
- [ ] 拖拽时长 (Resize Event) 改变事件开始/结束时间
- [ ] 自定义事件弹窗替代原始 `prompt()` 弹窗
