# bob-agent 开发待办

## Sprint 3: UI 打磨 + 工作目录 ← 当前

- [x] T-301: Industrial Dark UI 重构（Emoji→Lucide 全量替换）
- [x] T-302: System Prompt 注入（动态 buildSystemPrompt）
- [x] T-303: 工作目录配置 + IPC 通道（workspace:list-dir / read-file / select-dir）
- [x] T-304: System Prompt 动态注入目录树摘要（让模型真正"看到"文件列表）
- [x] T-305: Agent 头像改 "Bob" 文字标（替换 Cpu 图标）
- [x] T-306: 对话列表添加删除按钮（hover 显示 X，调用 deleteConversation）
- [ ] T-307: 模型品牌 PNG Logo（DeepSeek/OpenAI/Ollama → resources/logos/）
- [ ] T-308: 自动日程意图检测（对话流自动识别→弹出 ConfirmCard）

## Sprint 4: 文件分析 + 联动

- [x] T-401: FileReader 纯文本 (.txt/.md/.csv/.json/.py)
- [x] T-402: FileReader Office (.docx/.xlsx/.pdf)
- [x] T-403: FileDropZone 组件
- [ ] T-404: Vision→Parser 管线 (截图→识别→建日程)
- [ ] T-405: 全局截图 (Ctrl+Shift+S)
- [ ] T-406: Microsoft 365 日历同步 (移植 calendar_sync.py)

## Sprint 5: 体验打磨

- [ ] T-501: 系统托盘
- [ ] T-502: 全局快捷键 (Ctrl+Shift+Space)
- [ ] T-503: 桌面通知提醒
- [ ] T-504: 暗色/亮色主题
- [ ] T-505: 模型切换 UI
- [ ] T-506: Cost 追踪
- [ ] T-507: 对话导出 (.md)
- [ ] T-508: 错误处理美化

## Sprint 6: Agent 化（Function Calling + 工具系统）🔑 关键里程碑

> 当前 bob-agent 只是一个"聊天界面"，Sprint 6 将它变成真正的"Agent"。
> 参照 CodeRunner 的 `src/tools/` 架构（BaseTool + ToolRegistry + 18 个工具）。

### 6.1 工具系统 (`electron/tools/`)
- [ ] T-601: BaseTool 基类 (name/description/parameters/execute + get_schema)
- [ ] T-602: ToolRegistry 工具注册表（内置工具 + skills/ 外挂扫描）
- [ ] T-603: 内置工具 — list_dir / read_file（沙箱化，限工作目录）
- [ ] T-604: 内置工具 — create_event / search_events / update_event
- [ ] T-605: 内置工具 — web_search（可选，联网搜索）

### 6.2 LLM 调用链改造
- [ ] T-606: LLM Client 改造 — chatStream 支持 tools 参数 + tool_call 解析
- [ ] T-607: Tool Execution Loop（LLM 返回 tool_call → 执行 → 结果回传 → 继续推理）
- [ ] T-608: ChatView 工具调用 UI（执行中 spinner + 工具名称 + 结果折叠展示）

### 6.3 技能系统（双目录架构，参照 CodeRunner ToolRegistry._resolve_skills_dirs）
- [ ] T-609: 内置 skills/ 目录 — 随项目分发 3-5 个基础技能（每日总结/文件整理/会议准备）
- [ ] T-610: ToolRegistry 扫描内置 skills/ — 自动发现 SKILL.md frontmatter 并注册
- [ ] T-611: 设置→外部技能目录 — UI 配置 + 目录选择器（支持多路径）
- [ ] T-612: ToolRegistry 扫描外部目录 — 启动时加载，如 Assistant/common/knowledge/skills/
- [ ] T-613: 技能热重载 — 不重启应用即可刷新技能列表

### 6.4 MCP 协议
- [ ] T-614: MCP 客户端接入（连接外部 MCP Server）
- [ ] T-615: MCP 配置 UI（mcp/config.json 管理）

## Sprint 7: 打包发布

- [ ] T-701: electron-builder 打包配置
- [ ] T-702: 自动更新 (electron-updater)
- [ ] T-703: 错误日志
- [ ] T-704: 内测分发
- [ ] T-705: 文档收尾

---

## 已完成（归档）

### Sprint 1: 骨架 + 对话 + Vision ✅
- [x] T-101 ~ T-109: 全部完成

### Sprint 2: 智能收件箱 ✅
- [x] T-201 ~ T-208: 全部完成
