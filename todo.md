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
- [x] T-407: 剪贴板图片粘贴支持 (Clipboard Paste) — 支持直接在输入框 Ctrl+V 粘贴内存截图或本地图片文件，并将其作为附件/预览图加入对话流

## Sprint 5: 体验打磨 ← 当前

- [x] T-501: 系统托盘
- [ ] T-502: 全局快捷键 (Ctrl+Shift+Space)
- [ ] T-503: 桌面通知提醒
- [x] T-504: 暗色/亮色主题（含 Electron titleBarOverlay 实时同步 + did-finish-load 初始化 + Settings applyTheme 联动）
- [x] T-505: 模型切换 UI（品牌 Logo + 弹出选择器）
- [x] T-506: Cost 追踪（会话级累计费用指示器）
- [x] T-507: 对话导出 (.md)
- [x] T-508: 错误处理美化
- [x] T-509: 产物文件卡片与外接打开（对话中强化展示 Agent 生成的文件，点击直接调用系统默认应用如浏览器/Notepad/Word打开）
- [x] T-510: 侧边栏拖拽调宽 + Notion 风格折叠按钮
- [x] T-511: 空状态品牌水印（5% 透明度 Bob Logo）
- [x] T-512: 日程时间轴重写（0/6/12/18/24 刻度 + 以今天为中心的 7 天 + 窄屏竖向降级 + 事件点击详情弹窗）
- [x] T-513: 对话列表项等高修复（新对话无消息时占位）
- [x] T-514: 设计规范文档化（design_principles.md 9 节完整规范）
- [x] T-515: 启动画面 (Splash Screen) — 在应用启动加载资源时显示，缓解长时间等待的焦虑
- [x] T-516: 全局多语言支持 (i18n) — vue-i18n 框架 + zh-CN/en-US 双语包 + Settings 语言切换 + 导航 computed 响应 + WeekTimeline tm() 数组解析
- [x] T-517: 品牌 Logo 滚动视差动效 (Scroll-Driven Animation) — 新对话的背景 Logo 在输入首条消息后，平滑缩小悬浮至顶部作为 Section 标题；向下滚动对话时，Logo 相对位置固定但根据滚动距离平滑渐隐 (Fade-out)，回滚至顶部时重新渐显，实现类似 Apple 官网的高级互动感
- [ ] T-518: 对话内文件引用富卡片 (FileCard) — 将 LLM 回复中的本地文件路径从裸链接升级为带图标/缩略图的交互式卡片
  - [ ] T-518a: FileCard.vue 基础组件 — 文件类型图标 + 文件名 + 大小/修改时间 + 打开/在文件夹中显示
  - [ ] T-518b: system:file-meta IPC — fs.stat() 元数据 + system:show-in-folder
  - [ ] T-518c: ChatView Markdown 后处理 — 识别 file:// 链接并替换为 FileCard block
  - [ ] T-518d: 图片/视频缩略图 — nativeImage 缩略图生成替代文件类型图标
  - [ ] T-518e: LLM System Prompt 格式引导 — 引导模型输出标准 file:// Markdown 链接

## Sprint 6: Agent 化（Function Calling + 工具系统）🔑 关键里程碑

> 当前 bob-agent 只是一个"聊天界面"，Sprint 6 将它变成真正的"Agent"。
> 参照 CodeRunner 的 `src/tools/` 架构（BaseTool + ToolRegistry + 18 个工具）。

### 6.1 工具系统 (`electron/tools/`)
- [x] T-601: BaseTool 基类 (name/description/parameters/execute + get_schema) — PR 已关闭，改由本地实现
- [x] T-602: ToolRegistry 工具注册表（内置工具 + skills/ 外挂扫描）
- [x] T-603: 内置工具 — list_dir / read_file / write_file 等（沙箱化，受工作目录/全局权限控制）
- [x] T-604: 内置工具 — create_event / search_events / update_event
- [x] T-605: 内置工具 — web_search（可选，联网搜索）

### 6.2 LLM 调用链改造
- [x] T-606: LLM Client 改造 — chatStream 支持 tools 参数 + tool_call 解析
- [x] T-607: Tool Execution Loop（LLM 返回 tool_call → 执行 → 结果回传 → 继续推理）
- [x] T-608: ChatView 工具调用 UI（执行中 spinner + 工具名称 + 结果折叠展示）

### 6.3 技能系统（双目录架构，参照 CodeRunner ToolRegistry._resolve_skills_dirs）
- [x] T-609: 内置 skills/ 目录 — 随项目分发 15+ 个基础纯大脑技能
- [x] T-610: ToolRegistry 扫描内置 skills/ — 自动发现 SKILL.md frontmatter 并注册
- [x] T-611: 设置→外部技能目录 — UI 配置 + 目录选择器（支持多路径）
- [x] T-612: ToolRegistry 扫描外部目录 — 启动时加载外部技能
- [x] T-613: 技能热重载 — 不重启应用即可刷新技能列表

### 6.4 MCP 协议
- [x] T-614: MCP 客户端接入（连接外部 MCP Server）
- [x] T-615: MCP 配置 UI（mcp/config.json 管理）

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
### 6.7 三层记忆引擎 (Three-Tier Memory Engine)
> 架构参见 AGENTS.md D-008。
> - **Tier 1 灵魂**：`data/memory/SOUL.md`（静态人格，每次注入）
> - **Tier 2 短期记忆**：`data/memory/sessions/`（≤7天对话总结，自动压缩注入）
> - **Tier 3 长期记忆**：`data/wiki/`（>7天对话 + 文件夹知识，工具检索）

#### Phase 1-2: 对话记忆系统 ✅
- [x] T-626: `data/` 目录初始化 — `memory/sessions/`, `wiki/sessions/`, `wiki/folders/`, `wiki/clippings/`
- [x] T-627: SOUL.md 静态人格注入 + Session 总结（切换对话时后台自动压缩）
- [x] T-628: brain_search 工具 — 搜索冷记忆和 Wiki 知识库
- [x] T-629: 启动补偿扫描 — 处理崩溃/强关导致的未总结对话
- [x] T-630: 7天冷热迁移 — `memory/sessions/` → `wiki/sessions/` 自动迁移
- [x] T-631: 级联删除 — 删除对话时同步清除热记忆和冷记忆文件

#### Phase 3: 智能拖拽与文件夹记忆 ✅
- [x] T-632: 全局智能拖拽分发 (Smart Drop Zone) — 增强 ChatView 拖拽区域，识别拖入内容类型：文件夹→进入跟踪流、图片→Vision 附件、文档→上下文附件
- [x] T-633: track_folder / untrack_folder 工具 — Bob 的内置工具，支持通过对话口头指令或拖拽触发文件夹跟踪
- [x] T-634: 语义化文件夹速读 (folder-tracker.js) — 读取文件名列表 + 调用廉价 LLM 生成 ≤100字语义摘要 → 存入 `data/wiki/folders/<id>.md`
- [x] T-635-mem: tracked_folders 持久化 — SQLite 存储关注列表，支持增删查
- [x] T-636: Settings 文件夹管理面板 — 设置界面新增「关注的文件夹」专区，支持手动添加目录 + 查看/删除列表


### 6.8 安全加固（Audit 驱动）
- [x] T-630: XSS 消毒 — ChatView Markdown 渲染接入 DOMPurify
- [x] T-631: 启用 Renderer Sandbox (`sandbox: true`)
- [x] T-632: config:all IPC 过滤 apiKey 等敏感字段 (Jules 已修复)
- [x] T-633: globalFileAccess/agentMode 状态锁定到 Main 进程 (Jules 已修复)
- [x] T-634: read_file/write_file 路径穿越修复 (Jules 已修复)
- [x] T-635: 外部 Web 内容防注入隔离 (XML `<untrusted_web_content>` 包裹 + System Prompt 约束)

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
