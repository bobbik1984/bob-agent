# Bob-Agent 开发全局路线图 (Roadmap)

> 🎯 **当前版本**: `v0.3.1-pre` — 微信接入完成、API Key Keychain 加密、ChatView 架构拆分、安全加固全量完成。
> ♻️ **已完成**: Tauri 迁移、主题系统、记忆引擎、安全加固、IPC 防抖、僵尸进程修复、路径穿越修复、微信接入、HTTP API、Composable 架构拆分。
> 📋 **下一目标**: v0.4 — 离线模型 Tool Calling / 排队纠偏 / Cron 自动化 / 生产打包发布。

---

## 📍 里程碑 1: Tauri 基础脚手架 ✅
- [x] T-101: 初始化 Tauri V2 环境 (`tauri init`)。
- [x] T-102: 配置 `tauri.conf.json`（无边框、透明、380x750）。
- [x] T-103: 跑通 `npm run dev:tauri` 前端热更新。
- [x] T-104: 配置国内 Cargo 镜像源（清华 TUNA）。
- [x] T-105: 注入 `tauri-bridge.js` 适配器层，拦截 `window.electronAPI`。
- [x] T-106: 安装 `@tauri-apps/plugin-dialog` 并授权 `dialog:default` 能力。

---

## 📍 里程碑 2: 前端完整性保障 (让所有页面能正常渲染) ✅
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

## 📍 里程碑 3: Rust 原生化 — 配置与凭证
> 将 Bridge 中的 Mock 逐步替换为真正的 Rust 实现。
- [x] T-301: Rust `config_get/set` 读写 `config.json`。
- [x] T-302: Rust `config_get_all` 返回完整配置对象。
- [x] T-303: ~~Stronghold~~ → 使用 `keyring` crate 实现 OS 级 Keychain 加密存储（Windows DPAPI / macOS Keychain）。
- [ ] T-304: 注册全局快捷键 (`Ctrl+Shift+B`) 唤醒窗口。

## 📍 里程碑 4: Rust 原生化 — 数据库引擎 (SQLite)
- [x] T-401: 引入 `rusqlite`，初始化 `conversations` 和 `messages` 表。
- [x] T-402: 实现 `db_conversations`, `db_conversation_create/delete/rename/get`。
- [x] T-403: 实现 `db_messages`, `db_message_add`。
- [ ] T-404: 历史数据无损继承 (读取 `%APPDATA%/bob-agent/db`)。

## 📍 里程碑 5: Rust 原生化 — 大模型通信 (LLM Engine)
- [x] T-501: 用 Rust `reqwest` 发送 Chat 请求。
- [x] T-502: 实现 SSE 流式解析，通过 `app.emit("llm:chunk")` 推送。
- [x] T-503: 图片视觉请求 (`sendVision`) 的 Base64 传输。
- [x] T-504: ModelHub 模型池扫描与角色指派的 Rust 实现。
- [x] T-505: **Panic 防御加固** — 消除 `lib.rs` 中 13 处裸 `.unwrap()` 和 `llm.rs` 中 2 处 UTF-8 字节切片越界，改用 `match` 优雅降级，防止中文/Emoji 内容导致整个 Tauri 进程崩溃退出。

## 📍 里程碑 6: Rust 原生化 — 工具链与知识库 ✅
- [x] T-601: 文件夹递归扫描 (`walkdir`)。
- [x] T-602: **网页抓取引擎** — 新建 `web.rs`，`reqwest` + `scraper` 实现 HTML 获取与 DOM 解析，智能提取 `<article>/<main>` 正文（2MB/10s 安全限制）。
- [x] T-603: **技能与插件扫描引擎** — 新建 `plugins.rs`，原生扫描 `externalSkillsDir`，解析 SKILL.md YAML frontmatter，注册内置系统能力。
- [x] T-604: **做梦引擎 V1** — 新建 `dream.rs`，对话结束时提取核心话题存入 `memory/sessions/`，聚合生成晨间简报。
- [x] T-605: **日程管理引擎** — 新建 `calendar.rs`，SQLite 持久化的事件/待办系统，支持 `listEvents`, `parseEvent`, `confirmEvent`, `deleteEvent`, `updateEventStatus`, `updateEventTime` 全部 6 个接口。
- [x] T-606: **文件读取引擎** — `system_read_file` 原生读取文本文件（500KB 安全上限 + UTF-8 校验）。
- [x] T-607: **文件夹跟踪** — `system_get/add/remove_tracked_folders` 持久化到 config.json。
- [x] T-608: **系统工具原生化** — `openFile`, `showInFolder`, `getVersion`, `getLogPath`, `openLogDir`, `openDataDir`, `getToolStatuses`, `factoryReset` 全部 Rust 原生。剩余 Mock: `updateTheme`, `getClipboardImage`, `showNotification`。

## 📍 里程碑 7: 收尾与发布
- [x] T-701: **彻底移除 `electron/` 目录** — 42 个遗留文件已清除，Bridge 中的 Electron 引用注释已更新。
- [ ] T-702: 集成 `llama-cpp-rs` 离线推理。
- [x] T-703: 执行 `npm run tauri build`，验证打包流程，生成 `.msi` 或 `.exe` 安装包。

---

## 📍 里程碑 8: 声明式配置 + 单向调谐 (Outbox/Reconciler 架构) ✅
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
- [x] T-822: **SetupWizard i18n + 引导优化** — 硬编码中文替换为 $t() 调用，zh-CN/en-US 双语同步。
- [ ] T-823: **端到端集成测试** — 在对话中模拟"帮我配好这个 Key: sk-test123"，验证 Outbox 写入 → Reconciler 消费 → config 更新 → UI 自动刷新的完整链路。
- [ ] T-824: **防破坏测试** — 手动写入格式错误/恶意字段的 `bob_outbox.json`，验证 Reconciler 不崩溃、程序正常运行、审计日志正确记录拒绝原因。

---

## 📍 里程碑 9: Tool Calling 引擎 (Agent 升级) ✅
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
- [ ] T-822: SetupWizard 体验统一提升（暂缓，计划整体重做）

## 📍 里程碑 10: 认知与记忆引擎升级 (Phase 2)
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
- [ ] **工具结果缓存** - 避免同一对话中重复读取同一文件
- [x] **工具执行超时** - 每个工具 30秒超时保护 (tokio::time::timeout)

### 安全层
- [x] **read_file 路径沙箱** - 限制在 workspaceDir + externalSkillsDir 内
- [x] **Tool Calling 审计日志** - 记录每次工具调用到 logs/tools.log

### 前端 UX
- [x] **Tool Calling 进度指示** - ChatView 显示"正在搜索..."可视化反馈
- [ ] **工具结果折叠** - 太长的结果折叠显示
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
- [ ] **Epic: 无人值守自动化 (Cron Automations)**
  - [ ] 引入 `tokio-cron-scheduler` 库，在 `lib.rs` 的后台守护线程中初始化
  - [ ] 在应用启动时，从 SQLite 读取用户的自动化日程（如每天 08:00 播报新闻）
  - [ ] 编写后台无头 (Headless) 唤醒逻辑：时间一到，自动后台组装 Prompt 并调用 `stream_internal`，将结果通过系统通知（Notification）或悬浮窗推给用户

### 体验层
- [x] **T-822: SetupWizard i18n 收尾** — 硬编码中文替换为 $t() 调用，zh-CN/en-US 双语同步完成
- [x] **T-304: 全局快捷键** — Ctrl+Shift+B 硬编码版已实现并稳定运行，后续可考虑可配置化
- [x] **搜索/文件/文件夹卡片统一设计** — 抽取了 `.bob-card-inline` 和 `.bob-card-block` 组件基类和样式 token，SearchCard/FileCard/FolderDropCard/ConfirmCard 共享 CSS，消除冗余
- [x] **技能固化 (Skill Solidification)** — `skill-creator` 已存在于 `skills/` 目录，Bob 可通过 `list_skills` + `read_skill` 调用，无需额外开发

---

## 📍 里程碑 10: 架构审计与安全加固 (Post-Migration Audit)
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

## 里程碑 11: v0.3 — 微信接入 + HTTP API (已完成)

微信接入模块已在 Rust 侧原生实现 (wechat/ 9个文件 + http_api.rs)，桌面端 UI 已适配。

---

## 📍 里程碑 12: v0.4 — Ghost Partner (幽灵副手)
> 🎯 **目标**: 从"被动响应的聊天机器人"进化为"主动辅助的桌面幽灵副手"。
> 📋 **来源**: `docs/20260606_AI 桌面助手竞品与差异化战略.docx` 竞品分析 + 差异化战略梳理。
> 🏗️ **核心定位**: 「中国泛白领办公桌上的幽灵副手」— 极度轻量、原生体验、纯本地化，拒绝全能 IDE 叙事。

### Phase 1: 低成本高感知

- [ ] T-1201: **富文本剪贴板交接 (Rich-Text Clipboard Handoff)**
  - [ ] Cargo.toml 引入 `tauri-plugin-clipboard-manager`
  - [ ] lib.rs 注册插件，capabilities/default.json 增加 `clipboard-manager:default` 权限
  - [ ] ChatView 消息气泡增加"复制为富文本"按钮（图标: `ClipboardCopy`）
  - [ ] 实现 Markdown → HTML 序列化（复用 `marked` 渲染结果），写入 `text/html` 剪贴板格式
  - [ ] 用户按 Ctrl+V 即可在 Outlook / 微信公众号编辑器 / WPS 中粘贴为带表格、高亮、加粗的富文本
  - *设计理念: 拒绝视觉模拟 (GUI 自动化)，用优雅的"剪贴板交接"保留人类点击"发送"的安全控制感*

### Phase 2: 核心调度基建

- [ ] T-1211: **Cron 调度引擎 (Heartbeat Scheduler)**
  - [ ] Cargo.toml 引入 `tokio-cron-scheduler`
  - [ ] 新建 `scheduler.rs` 模块
  - [ ] SQLite 新增 `schedules` 表 (id, cron_expr, prompt_template, enabled, last_run, created_at)
  - [ ] lib.rs setup() 中初始化调度器，从 DB 恢复已有任务
  - [ ] IPC 命令: `system_list_schedules`, `system_add_schedule`, `system_remove_schedule`, `system_toggle_schedule`
  - [ ] 执行逻辑: 时间到 → 后台组装 Prompt → 调 `stream_internal` → 结果写入通知/Inbox
  - *对齐 todo.md L333-336 原有的 Cron Automations Epic*

- [ ] T-1212: **文件目录监控 (Micro-Heartbeat File Watch)**
  - [ ] Cargo.toml 引入 `notify` crate（文件系统事件通知，比 walkdir 轮询更省电）
  - [ ] 监控 tracked_folders + 微信下载目录
  - [ ] 新文件出现时，通过系统托盘气泡或桌面通知轻提醒
  - [ ] 可选: 自动触发 LLM-Wiki ingest（需用户在提醒中确认）

### Phase 3: 交互升级

- [ ] T-1221: **快速输入气泡窗 (Quick-Capture Bubble)**
  - [ ] Tauri 多窗口: 新建一个透明无边框小窗口（约 400×60px），贴边悬浮 + 始终置顶
  - [ ] 点击气泡展开单行输入框，回车后气泡立即收起
  - [ ] 输入内容通过 Tauri IPC 发送给主窗口处理
  - [ ] 后端语义路由: 结合现有 Tool Calling 和日历系统，判断输入是日历事件、待办、还是普通对话
  - *注: 此功能在竞品分析中被称为"桌宠"，实际上是一个极简气泡 + 快速对话框，不需要动画宠物形象*

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

## 里程碑 13: v0.4 — 体感/防御/主动性升级 (借鉴 Hermes Desktop)
> **目标**: 把 Bob 已有的强大后端能力"浮出水面"，让用户真正感知到 Bob 在背后做了什么。
> **来源**: Hermes Desktop 竞品分析 + `bob_optimization_plan.md` + `bob_v04_dev_guide.md`
> **设计红线**: 14px 最大圆角 / 纯灰度 / 无蓝色 / **严禁 Emoji** / 无技术术语 / 所有通知可关闭

### Phase 1: 体感升级 — 让用户"看得见" (1-2周)

- [ ] T-1301: **对话历史全文搜索**
  - [ ] `db.rs`: 新建 `messages_fts` FTS5 虚拟表，对 `messages.content` 建索引
  - [ ] `db.rs`: 创建 INSERT/DELETE 触发器，自动同步 FTS 索引
  - [ ] `db.rs`: 启动时回填存量消息到 FTS 索引 (INSERT OR IGNORE)
  - [ ] `db.rs`: 新增 IPC 命令 `db_search_messages(query)` → 返回 `Vec<{id, conversation_id, conv_title, snippet, created_at}>`，LIMIT 30
  - [ ] `lib.rs`: 注册 `db_search_messages` 命令
  - [ ] `tauri-bridge.js`: 新增 `searchMessages(query)` 映射
  - [ ] `ChatView.vue`: 侧边栏顶部新增搜索输入框 (32px高, bg-secondary, 300ms debounce)
  - [ ] `ChatView.vue`: 搜索结果卡片列表 (单行标题 + 双行 snippet, `<mark>` 用 accent-primary 高亮)
  - [ ] `ChatView.vue`: 点击搜索结果 → 加载对应对话并滚动到匹配消息
  - [ ] `index.css`: 搜索结果样式 (`.search-result-item`, `.search-highlight`)
  - *技术详情见 bob_v04_dev_guide.md T-1301*

- [ ] T-1302: **记忆透明化 — "Bob 的记忆"**
  - [ ] 新建 `memory.rs` (或在 `dream.rs` 中扩展): `system_get_memory_entries()` 读取 sessions/ 和 wiki/ 目录
  - [ ] `memory.rs`: `system_delete_memory_entry(type, id)` 安全删除记忆文件
  - [ ] `lib.rs`: 注册 `system_get_memory_entries`, `system_delete_memory_entry`
  - [ ] `tauri-bridge.js`: 新增 `getMemoryEntries()`, `deleteMemoryEntry(type, id)`
  - [ ] `SettingsView.vue`: 在主题区块之后新增"Bob 的记忆"区块 (卡片列表，每条一行 + X 删除按钮)
  - [ ] `zh-CN.json` / `en-US.json`: 新增 `settings.bob_memory`, `settings.bob_memory_hint` 翻译 (严禁 Emoji)
  - *技术详情见 bob_v04_dev_guide.md T-1302*

- [ ] T-1303: **Cron 执行结果通知**
  - [ ] `Cargo.toml`: 启用 `tauri` 的 `notification` feature
  - [ ] `capabilities/default.json`: 添加 `notification:default` 权限
  - [ ] `scheduler.rs`: `execute_cron_job()` Step 6 后追加 `app.notification().builder().title().body().show()`
  - [ ] `InboxView.vue`: `scheduler:completed` 事件处理中为最新完成项添加 `.cron-result-new` 高亮类 (3s 后移除)
  - [ ] `App.vue`: 全局监听 `scheduler:completed`，若当前不在 InboxView → 日程导航项显示红点 badge
  - [ ] 通知文案: 直接使用任务 title，正文为结果前 100 字 (严禁 Emoji)
  - *技术详情见 bob_v04_dev_guide.md T-1303*

### Phase 2: 防御升级 — 让用户"不出错" (2-3周)

- [ ] T-1304: **启动自检医生 (Bob Doctor)**
  - [ ] 新建 `doctor.rs`: 定义 `CheckResult` 结构体 (code, severity, message, fixable)
  - [ ] `doctor.rs`: `system_health_check()` 检查 config.json 可读性、bob.db 可读性、API Key 存在性、磁盘可写性
  - [ ] `doctor.rs`: `system_auto_fix(code)` 自动修复逻辑 (CONFIG_CORRUPT → 从 bak 恢复)
  - [ ] `lib.rs`: `mod doctor;` + 注册 `system_health_check`, `system_auto_fix`
  - [ ] `tauri-bridge.js`: 新增 `healthCheck()`, `autoFix(code)`
  - [ ] `App.vue`: `onMounted` 调用 `healthCheck()`，结果存入全局 reactive state
  - [ ] `ChatView.vue`: 顶部 sticky 横幅 (32px高, warning=bg-tertiary, error=rgba(200,100,50,0.08))
  - [ ] `ChatView.vue`: 横幅包含人话提示 + 可选"一键修复"按钮 + X 关闭 (localStorage 24h 内不重复)
  - [ ] `index.css`: `.health-banner`, `.health-banner--warning`, `.health-banner--error`
  - *技术详情见 bob_v04_dev_guide.md T-1304*

- [ ] T-1305: **聊天就绪守卫 (Chat Readiness)**
  - [ ] `llm.rs`: 新增 `system_validate_chat_ready()` — 检查 provider/model/apiKey 本地配置完整性 (不做网络探测)
  - [ ] `lib.rs`: 注册 `system_validate_chat_ready`
  - [ ] `tauri-bridge.js`: 新增 `validateChatReady()`
  - [ ] `ChatView.vue`: `onMounted` 调用一次，缓存 60s
  - [ ] `ChatView.vue`: 不就绪时发送按钮 `disabled` (opacity 0.4) + 输入框下方一行提示 + "前往设置"链接
  - [ ] Fail-open: 任何超时/不确定情况返回 `ready: true`
  - *技术详情见 bob_v04_dev_guide.md T-1305*

### Phase 3: 主动性升级 — 让 Bob "动起来" (3-4周)

- [ ] T-1306: **对话自动提取行动项** (0.5天, 纯 Prompt Engineering)
  - [ ] `llm.rs`: system prompt 追加"行动项捕捉"指令段 — 识别时间+动作组合时主动调用 `add_calendar_event`
  - [ ] `ChatView.vue`: `tool_end` 事件中 `tool_name === "add_calendar_event"` 时渲染 `.bob-card-inline` 日程卡片
  - [ ] 卡片样式: 复用 ConfirmCard 设计 (一行标题 + 一行时间)，不新建组件
  - *零后端代码改动，仅 Prompt + 前端渲染*
  - *技术详情见 bob_v04_dev_guide.md T-1306*

- [ ] T-1307: **智能待办跟进**
  - [ ] `calendar.rs`: events 表新增 `last_notified INTEGER DEFAULT 0` 列 (ALTER TABLE)
  - [ ] `scheduler.rs`: 主循环追加 `check_upcoming_todos()` — 查询今日到期 + pending 的事件
  - [ ] `scheduler.rs`: 到期待办触发系统通知 (notification API) + emit `todo:reminder` 事件
  - [ ] `scheduler.rs`: 提醒频率控制 — 同一待办 last_notified 当天不重复
  - [ ] `InboxView.vue`: 监听 `todo:reminder`，今日到期项左侧边框变为 `var(--accent-primary)`
  - *技术详情见 bob_v04_dev_guide.md T-1307*

- [ ] T-1308: **晨间简报增强**
  - [ ] `dream.rs`: `getDreamReport` 返回数据扩展 — 新增 `today_events`, `today_todos` 字段 (查询 events 表)
  - [ ] `MorningBriefing.vue`: 对话回顾区块之前插入"今日日程"和"待完成事项"区块
  - [ ] `zh-CN.json` / `en-US.json`: 新增 `briefing.today_schedule`, `briefing.today_todos` (严禁 Emoji)
  - [ ] 简报总长度控制: 不超过一屏 (~300字)
  - *技术详情见 bob_v04_dev_guide.md T-1308*

