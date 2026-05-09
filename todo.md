# bob-agent 开发待办

## Sprint 1: 骨架 + 对话 + Vision

- [ ] T-101: 项目初始化 (Electron + Vue 3 + Vite)
- [ ] T-102: LLM Client (OpenAI SDK, Chat + Vision + 流式)
- [ ] T-103: IPC 桥接 (preload.js + contextBridge)
- [ ] T-104: 基础对话界面 (ChatView.vue)
- [ ] T-105: Markdown 渲染 (marked + highlight.js)
- [ ] T-106: Vision 支持 (粘贴/拖拽图片 → Base64 → image_url)
- [ ] T-107: 首次启动向导 (SetupWizard.vue)
- [ ] T-108: Micro-compact 移植 (来自 CodeRunner)
- [ ] T-109: 对话持久化 (SQLite conversations + messages)

## Sprint 2: 智能收件箱

- [ ] T-201: Parser 移植 (TodoList parser.py → parser.js)
- [ ] T-202: 事件确认卡片 (ConfirmCard.vue)
- [ ] T-203: SQLite 事件表 (与 TodoList schema 一致)
- [ ] T-204: 对话→事件检测 (自动触发 Parser)
- [ ] T-205: 周时间轴 (WeekTimeline.vue, 移植自 TodoList)
- [ ] T-206: 待办清单 (TodoList.vue)
- [ ] T-207: InboxView 组装
- [ ] T-208: 思维链折叠 (ThinkingCard.vue)

## Sprint 3: 文件分析 + Vision→日程联动

- [ ] T-301: FileReader 纯文本 (.txt/.md/.csv/.json/.py)
- [ ] T-302: FileReader Office (.docx/.xlsx/.pdf)
- [ ] T-303: FileDropZone 组件
- [ ] T-304: Vision→Parser 管线 (截图→识别→建日程)
- [ ] T-305: 全局截图 (Ctrl+Shift+S)
- [ ] T-306: Microsoft 365 日历同步 (移植 calendar_sync.py)
- [ ] T-307: 侧栏导航

## Sprint 4: 体验打磨

- [ ] T-401: 系统托盘
- [ ] T-402: 全局快捷键 (Ctrl+Shift+Space)
- [ ] T-403: 桌面通知提醒
- [ ] T-404: 暗色/亮色主题
- [ ] T-405: 模型切换 UI
- [ ] T-406: Cost 追踪
- [ ] T-407: 对话导出 (.md)
- [ ] T-408: 错误处理美化

## Sprint 5: 打包发布

- [ ] T-501: electron-builder 打包配置
- [ ] T-502: 自动更新 (electron-updater)
- [ ] T-503: 错误日志
- [ ] T-504: 内测分发
- [ ] T-505: 文档收尾
