# bob-agent 开发待办

## Sprint 3: UI 打磨 + 工作目录 ✅

- [x] T-301: Industrial Dark UI 重构（Emoji→Lucide 全量替换）
- [x] T-302: System Prompt 注入（动态 buildSystemPrompt）
- [x] T-303: 工作目录配置 + IPC 通道（workspace:list-dir / read-file / select-dir）
- [x] T-304: System Prompt 动态注入目录树摘要（让模型真正"看到"文件列表）
- [x] T-305: Agent 头像改 "Bob" 文字标（替换 Cpu 图标）
- [x] T-306: 对话列表添加删除按钮（hover 显示 X，调用 deleteConversation）
- [x] T-307: 模型品牌 PNG Logo（DeepSeek/OpenAI/Ollama → resources/logos/）
- [x] T-308: 自动日程意图检测（对话流自动识别→弹出 ConfirmCard）

## Sprint 4: 文件分析 + 联动

- [x] T-401: FileReader 纯文本 (.txt/.md/.csv/.json/.py)
- [x] T-402: FileReader Office (.docx/.xlsx/.pdf)
- [x] T-403: FileDropZone 组件
- [ ] T-404: Vision→Parser 管线 (截图→识别→建日程)
- [ ] T-405: 全局截图 (Ctrl+Shift+S)
- [ ] T-406: Microsoft 365 日历同步 (移植 calendar_sync.py)

## Sprint 5: 体验打磨 ← 当前

- [ ] T-501: 系统托盘
- [ ] T-502: 全局快捷键 (Ctrl+Shift+Space)
- [ ] T-503: 桌面通知提醒
- [x] T-504: 暗色/亮色主题（含 Electron titleBarOverlay 实时同步）
- [x] T-505: 模型切换 UI（品牌 Logo + 弹出选择器）
- [x] T-506: Cost 追踪（会话级累计费用指示器）
- [ ] T-507: 对话导出 (.md)
- [ ] T-508: 错误处理美化
- [ ] T-509: 产物文件卡片与外接打开（对话中强化展示 Agent 生成的文件，点击直接调用系统默认应用如浏览器/Notepad/Word打开）
- [x] T-510: 侧边栏拖拽调宽 + Notion 风格折叠按钮
- [x] T-511: 空状态品牌水印（5% 透明度 Bob Logo）
- [x] T-512: 日程时间轴重写（0/6/12/18/24 刻度 + 以今天为中心的 7 天 + 窄屏竖向降级 + 事件点击详情弹窗）
- [x] T-513: 对话列表项等高修复（新对话无消息时占位）
- [x] T-514: 设计规范文档化（design_principles.md 9 节完整规范）

## Sprint 6: Agent 化（Function Calling + 工具系统）🔑 关键里程碑

> 当前 bob-agent 只是一个"聊天界面"，Sprint 6 将它变成真正的"Agent"。
> 参照 CodeRunner 的 `src/tools/` 架构（BaseTool + ToolRegistry + 18 个工具）。

### 6.1 工具系统 (`electron/tools/`)
- [x] T-601: BaseTool 基类 (name/description/parameters/execute + get_schema) — PR 已关闭，改由本地实现
- [x] T-602: ToolRegistry 工具注册表（内置工具 + skills/ 外挂扫描）
- [x] T-603: 内置工具 — list_dir / read_file / write_file 等（沙箱化，受工作目录/全局权限控制）
- [ ] T-604: 内置工具 — create_event / search_events / update_event
- [x] T-605: 内置工具 — web_search（可选，联网搜索）

### 6.2 LLM 调用链改造
- [x] T-606: LLM Client 改造 — chatStream 支持 tools 参数 + tool_call 解析
- [x] T-607: Tool Execution Loop（LLM 返回 tool_call → 执行 → 结果回传 → 继续推理）
- [ ] T-608: ChatView 工具调用 UI（执行中 spinner + 工具名称 + 结果折叠展示）

### 6.3 技能系统（双目录架构，参照 CodeRunner ToolRegistry._resolve_skills_dirs）
- [x] T-609: 内置 skills/ 目录 — 随项目分发 15+ 个基础纯大脑技能
- [x] T-610: ToolRegistry 扫描内置 skills/ — 自动发现 SKILL.md frontmatter 并注册
- [x] T-611: 设置→外部技能目录 — UI 配置 + 目录选择器（支持多路径）
- [x] T-612: ToolRegistry 扫描外部目录 — 启动时加载外部技能
- [ ] T-613: 技能热重载 — 不重启应用即可刷新技能列表

### 6.4 MCP 协议
- [ ] T-614: MCP 客户端接入（连接外部 MCP Server）
- [ ] T-615: MCP 配置 UI（mcp/config.json 管理）

### 6.5 资产迁移与基元能力
- [x] T-616: 原生读写基元支持 (write_file)
- [x] T-617: 网络代理基元支持 (tinyfish_fetch, weather 等)
- [x] T-618: 纯大脑技能全量迁移 (15 个 Markdown 技能导入)
- [x] T-619: 会话级全局文件授权 (Global File Access Toggle)
### 6.6 高级代理能力与浏览器自动化
- [x] T-621: Obscura 隐身浏览器底层集成 (WebSocket CDP, DOM 提取)
- [x] T-622: browser_automation 内部工具 (navigate, get_html, click, type, evaluate)
- [x] T-623: 修复打包后 obscura.exe 的动态路径解析与自动捆绑
- [x] T-624: 代理模式切换 UI (问答/干活模式弹出切换器)
- [x] T-625: 全局文件授权 UI 优化 (Lock/Unlock 图标 + 工具栏内嵌)
### 6.7 本地工作区记忆引擎 (Local Workspace Memory Engine)
> 架构：`data/wiki/`（客观知识）+ `data/memory/`（主观记忆）双存储
>
> - **wiki/**：项目/文件夹的结构化摘要、AKP 收割的外部知识剪报。用户问"XX 项目怎么样"时检索注入。
> - **memory/**：用户偏好、行为模式、对话摘要日志。每次对话启动时自动注入 System Prompt。
>
> ⚠️ 等待 Jules 安全加固 PR 合并后再启动开发，避免后端文件冲突。

- [ ] T-626: `data/` 目录初始化 — 创建 `data/wiki/projects/`、`data/wiki/clippings/`、`data/memory/journal/` 目录结构，确保 `.gitignore` 排除个人数据。
- [ ] T-627: Wiki 扫描器 (Workspace Indexer) — 扫描工作目录及用户指定的外部路径，为每个项目/文件夹生成一份 Markdown 摘要存入 `data/wiki/projects/`。支持增量更新（只处理新增/修改的文件）。
- [ ] T-628: Wiki 检索工具 (wiki_search) — 注册为 Agent 内置工具，接收关键词，在 `data/wiki/` 下做文件名+内容匹配，返回最相关的摘要片段。让 Bob 能主动查找知识。
- [ ] T-629: Memory 沉淀器 — 每次对话结束时，自动提取关键信息（用户偏好、重要决定、提到的日期/人名）写入 `data/memory/preferences.json` 和 `data/memory/journal/YYYY-MM-DD.md`。
- [ ] T-630-mem: Memory 注入 — 启动对话时，自动将 `preferences.json` + 最近 3 天的 journal 摘要注入 System Prompt，实现跨会话记忆。
- [ ] T-631-mem: 记忆自动刷新管线 — 基于 chokidar 文件监听或用户手动触发，后台增量更新变动的 wiki 条目。


### 6.8 安全加固（Audit 驱动）
- [x] T-630: XSS 消毒 — ChatView Markdown 渲染接入 DOMPurify
- [ ] T-631: 启用 Renderer Sandbox (`sandbox: true`) ← Jules 执行中
- [ ] T-632: config:all IPC 过滤 apiKey 等敏感字段 ← Jules 执行中
- [ ] T-633: globalFileAccess/agentMode 状态锁定到 Main 进程 ← Jules 执行中
- [ ] T-634: read_file/write_file 路径穿越修复 ← Jules 执行中
- [ ] T-635: 外部技能目录 require() 安全隔离

## Sprint 7: 打包发布

- [ ] T-701: electron-builder 打包配置
- [ ] T-702: 自动更新 (electron-updater)
- [ ] T-703: 错误日志
- [ ] T-704: 内测分发
- [ ] T-705: 文档收尾

## Sprint 8: 情绪价值与陪伴系统 (Desktop Pet / Companion) 🧸
> 远期愿景：让 Bob 从"冷冰冰的效率工具"变为"有温度的桌面伙伴"（极简文字流派）。
- [ ] T-801: 桌宠引擎预研 — 基于无边框透明窗口 (Transparent Frameless Window) 实现浮动微组件。
- [ ] T-802: 状态机联动 — 监听 Agent 状态（Idle / Thinking / Working / Error）。
- [ ] T-803: 极简纯文本动效渲染 — 摒弃笨重的图形动画，采用纯 CSS 文字呼吸特效（如闲置时飘出 `Zzz...`，思考时 `Think Think Think...`，执行工具时 `Run Run Run...`）。
- [ ] T-804: 极简交互 — 支持拖拽停靠，点击时唤起快捷对话框或一键打断执行。

---

## 已完成（归档）

### Sprint 1: 骨架 + 对话 + Vision ✅
- [x] T-101 ~ T-109: 全部完成

### Sprint 2: 智能收件箱 ✅
- [x] T-201 ~ T-208: 全部完成
