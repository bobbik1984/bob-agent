# Bob-Agent 开发全局路线图 (Roadmap)

> 🎯 **当前版本**: `v0.4.4` — Ghost Partner (幽灵副手) 阶段正式版。
> ♻️ **已完成**: Tauri 迁移、主体模式、微信/TG/Discord 通道、文档输出引擎、Goal 闭环执行引擎、Web Drop P2P 极传、知识图谱融合、智能笔记模块。
> 📋 **下一目标**: 目标 20 — 内网穿墙隧道 | 目标 22 — Bob-Mobile 手机端 MVP | 目标 23 — 跨端同步引擎 (详见 `docs/MOBILE_BLUEPRINT.md`)。

---

## 📍 目标 1: Tauri 基础脚手架 ✅

---

## 📍 目标 2: 前端完整性保障 (让所有页面能正常渲染) ✅
> ⚠️ 本阶段**不写任何 Rust 代码**，只在 `tauri-bridge.js` 中补全 Mock，以及在 `App.vue` 中添加窗口按钮。


### 2B: Bridge Mock 完整性 (补全所有 53 个接口)
> 以下是当前 `tauri-bridge.js` 中**完全缺失**的接口，按组件分类列出。


### 2C: 导航验证

---

## 📍 目标 3: Rust 原生化 — 配置与凭证
> 将 Bridge 中的 Mock 逐步替换为真正的 Rust 实现。


## 📍 目标 7: 收尾与发布

---

## 📍 目标 8: 声明式配置 + 单向调谐 (Outbox/Reconciler 架构) ✅
> 🎯 **目标**: 让 Bob 在获得第一个 API Key（点火）后，具备自主配置系统的能力。
> 🛡️ **安全原则**: AI 只写 Outbox 文件（"办公桌"），Rust 内部守护者单向读取、校验、生效，AI 永远碰不到核心配置的"保险柜"。


### Phase 3: 用户体验

- [ ] T-823: **端到端集成测试** (集成测试后期再检测) — 在对话中模拟"帮我配好这个 Key: sk-test123"，验证 Outbox 写入 → Reconciler 消费 → config 更新 → UI 自动刷新的完整链路。
- [ ] T-824: **防破坏测试** (集成测试后期再检测) — 手动写入格式错误/恶意字段的 `bob_outbox.json`，验证 Reconciler 不崩溃、程序正常运行、审计日志正确记录拒绝原因。

---

## 📍 目标 9: Tool Calling 引擎 (Agent 升级) ✅
> 🎯 **目标**: 让 Bob 从 ChatBot 升级为 Agent——能够主动调用工具（读文件、抓网页、查技能），而非仅靠用户拖拽喂数据。
> 🏗️ **方案**: Rust 侧实现（方案 A），全部在进程内完成，不依赖 Python/Node。


### Phase 2: 集成与测试


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


---

## 🔧 可完善项 (Improvement Backlog)


### 引擎层
- [ ] **Epic: 离线模型 Tool Calling (Offline Function Calling)**
  - [ ] 调研本地 `llama-server` (如 Llama-3/Qwen) 的 function calling 输出格式
  - [ ] 在 `llm.rs` 中针对 `provider == "offline"` 增加降级逻辑（如果模型不支持标准 JSON，则通过 Prompt 强制要求 XML 标签包裹）
  - [ ] 实现离线模式下的错误重试机制（解析 JSON 失败时，返回报错信息让小模型自行修正）
- [ ] **Epic: 排队纠偏 (Queue Correction)**
  - [ ] 在前端 `ChatView.vue` 添加“打断执行”按钮，点击后发送中止信号给 Tauri 后端
  - [ ] 在 `llm.rs` 中引入 `tokio::sync::mpsc` 监听前端中断信号
  - [ ] 修改 `stream_internal` 的 Tool Calling 循环：一旦收到中断信号，立刻跳出循环并丢弃挂起的工具
  - [ ] 将用户新输入的“纠正指令”作为新一轮上下文直接喂给 LLM 重新规划
- [ ] 引入 `tokio-cron-scheduler` 库，在 `lib.rs` 的后台守护线程中初始化
  - [ ] 在应用启动时，从 SQLite 读取用户的自动化日程（如每天 08:00 播报新闻）
  - [ ] 编写后台无头 (Headless) 唤醒逻辑：时间一到，自动后台组装 Prompt 并调用 `stream_internal`，将结果通过系统通知（Notification）或悬浮窗推给用户

### 体验层

---

## 📍 目标 10: 架构审计与安全加固 (Post-Migration Audit)
> 🎯 **目标**: 根据 Jules 提供的 Electron 到 Tauri 迁移审计报告，全面清理技术债并加固系统安全性。
> 🛡️ **安全原则**: 消除 Rust 后端 Panic 隐患，彻底清理弃用的 Node.js 依赖，封堵路径穿越漏洞。


### 第三阶段: 生产级加固 (v0.2.0 Sprint — 已完成)

---

## 目标 11: v0.3 — 微信接入 + HTTP API (已完成)

微信接入模块已在 Rust 侧原生实现 (wechat/ 9个文件 + http_api.rs)，桌面端 UI 已适配。

---

## 📍 目标 12: v0.4 — Ghost Partner (幽灵副手)
> 🎯 **目标**: 从"被动响应的聊天机器人"进化为"主动辅助的桌面幽灵副手"。
> 📋 **来源**: `docs/20260606_AI 桌面助手竞品与差异化战略.docx` 竞品分析 + 差异化战略梳理。
> 🏗️ **核心定位**: 「中国泛白领办公桌上的幽灵副手」— 极度轻量、原生体验、纯本地化，拒绝全能 IDE 叙事。


### Phase 1: 文件操作工具集 (Shell-Lite, 5 个新工具)


- [ ] T-1212: **文件目录监控 (Micro-Heartbeat File Watch)**
  - [ ] Cargo.toml 引入 `notify` crate（文件系统事件通知，比 walkdir 轮询更省电）
  - [ ] 监控 tracked_folders + 微信下载目录
  - [ ] 新文件出现时，通过系统托盘气泡或桌面通知轻提醒
  - [ ] 可选: 自动触发 LLM-Wiki ingest（需用户在提醒中确认）


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


### Phase 3: 主动性升级 — 让 Bob "动起来" (3-4周)


---

## 📍 目标 14: v0.5 — 认知引擎升级 (Cognitive Engine v2)
> 🎯 **目标**: 让 Bob 从"能记住事"进化为"会思考的记忆体"——自动去噪、自我纠错、成本自控。
> 📋 **来源**: `docs/分布式 Agent 认知系统审视.docx` 理论框架，提取出 5 个可落地到单 Agent 桌面产品的改进点。
> 🏗️ **核心原则**: 所有"智能"逻辑尽可能下沉到 Rust 确定性层（瘦智能体，胖平台），减少对 LLM 的依赖。


### Phase 3: 智能路由升级 (P3 — 研究性质)


---

## 📍 目标 15: v0.6 — 文档输出引擎 (Document Export Engine)
> 让 Bob 从"只会说"进化为"能交付"——对话结束后导出精排版 HTML 报告、PDF、Excel、Word、PPT。
> 核心策略: **HTML-first** — 精排 HTML 是主力输出，PDF 通过打印导出。
> 设计来源: o2_analysis 项目 + guizang-ppt-skill + mckinsey-designer + frontend-design


### Phase 4: PPTX 模板注入式生成 (Tier 4, 延后)


---

## 📍 目标 16: v0.4.1 — Shell 执行引擎 + 通讯渠道接入
> 🎯 **目标**: 补齐白领场景的"文件整理"与"移动端通讯"两个关键能力缺口。
> 📋 **来源**: 用户反馈 — 基础文件操作 + Telegram/Discord 后端接入。
> 🏗️ **预估工作量**: 2-3 天。

### Phase 0: 架构断裂修复 (Bug Fix — 最高优先级)

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


### Phase 4: 验证

- [ ] T-1641: cargo check + cargo clippy 编译通过
- [ ] T-1642: 端到端测试 — 对话中"帮我建个文件夹"/"移动文件" 验证

---

## 📍 目标 17: v0.33 — 知识图谱融合 (Knowledge Graph)

> 核心目标：把 iknow 的语义图谱能力原生化到 Bob 中，闭合“拖拽文件夹 → 知识提取 → 图谱展示”的完整 UX 循环。

### Phase 0: 数据层 — SQLite 图存储 + Rust 图引擎

- [ ] T-1702: Rust 模块 `kg.rs` — Node/Edge CRUD（insert, upsert, delete）
- [ ] T-1703: `kg.rs` — BFS 子图查询 `kg_query(term, max_hops)` → 返回 JSON
- [ ] T-1704: `kg.rs` — 图统计 `kg_get_stats()` → 节点数/边数/类型分布

### Phase 1: 提取层 — LLM 实体+关系提取

- [ ] T-1711: `kb_indexer.rs` 扩展 — Prompt 追加 relations 字段
- [ ] T-1712: 索引完成后调用 `kg.rs` 写入节点和边（去重 upsert）
- [ ] T-1713: `brain_search` 升级 — FTS5 + 图谱子图 RRF 混合

### Phase 2: 前端 — KnowledgeGraphView

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


### Phase 4: 验证

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


## 🚀 T-1800: Bob 联邦网络与 Web Drop 引擎
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
> 🎯 **目标**: 在同一个 bob-agent 仓库中，基于 Tauri V2 的 Mobile 支持，构建手机端极简入口，并最终进化为端侧 LLM 离线节点。
> 📋 **核心定位**: 手机是"独立前哨站"和"便携式离线推理节点"。
> 📖 **详细蓝图**: `docs/MOBILE_BLUEPRINT.md`


### Phase 2: 手机端 UI 适配与裁剪 (M2 Sprint)
- [ ] T-2221: (M2-01) 移动端布局彻底重构 (底部导航条 Bottom Navigation，完全替换 PC 侧边栏)
- [ ] T-2222: (M2-02) 避开手机状态栏（沉浸式处理 / SafeAreaPadding），最大化屏幕利用率
- [ ] T-2223: (M2-03) 移除或折叠微信、Telegram、Discord 等桌面端专属通道入口
- [ ] T-2224: (M2-04) 替换 Tauri 默认 Android 图标为专属 Bob Logo，缩放优化 SVG
- [ ] T-2225: (M2-05) 聊天视图双层级改造 (默认打开上一个对话记录，支持后退返回全局对话列表)
- [ ] T-2226: (M2-06) 知识库视图极简改造 (左上角汉堡包按钮等小面积控件，用于切换图谱与知识库状态)
- [ ] T-2227: (M2-07) 移动端专属 Onboarding 绑定流程 (在微信接入后，通过扫码 PC 端直接绑定，生成唯一设备标签，不选工作目录)

### Phase 3: 端侧本地大模型集成 (llama.cpp)
- [ ] T-2231: (M2-21) 调研 Tauri Mobile 下打包与拉起 `llama-server` Native 二进制的 Sidecar 方案
- [ ] T-2232: (M2-22) 集成 Gemma 4B / Qwen 等轻便开源模型至手机本地推理引擎

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
- [ ] T-2322: 运行时配置全量互通机制 (包括模型偏好选择、知识库状态、API Key，安装后自动双向同步更新)。
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


#### Phase 5: 前端适配

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

---


