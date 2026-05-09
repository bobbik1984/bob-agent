# bob-agent 开发计划

## 总览

| 阶段 | 时间 | 目标 | 交付物 |
|------|------|------|--------|
| Week 1 | Sprint 1 | 骨架 + 对话 + Vision | 能聊天+识别图片的 exe |
| Week 2 | Sprint 2 | 智能收件箱 | 能录事件+看日程的秘书 |
| Week 3 | Sprint 3 | 文件分析 + 联动 | 完整 MVP |
| Week 4 | Sprint 4 | 体验打磨 | 日常可用的 App |
| Week 5 | Sprint 5 | 打包发布 | 可分发安装包 |

---

## Sprint 1: 骨架 + 对话 + Vision（Week 1）

### 目标：能聊天 + 能识别图片的 .exe

| 任务 | 估时 | 说明 |
|------|------|------|
| T-101: 项目初始化 | 2h | `npm create vite` + Electron 集成 + 基础目录结构 |
| T-102: LLM Client | 4h | OpenAI SDK 封装，支持 DeepSeek/OpenAI，流式输出 |
| T-103: IPC 桥接 | 2h | preload.js + contextBridge 安全通道 |
| T-104: 基础对话界面 | 4h | ChatView.vue: 输入框 + 消息列表 + 流式渲染 |
| T-105: Markdown 渲染 | 2h | marked + highlight.js 集成 |
| T-106: Vision 支持 | 3h | 图片粘贴/拖拽 → Base64 → image_url 块 |
| T-107: 首次启动向导 | 3h | SetupWizard.vue: 选 Provider → 填 API Key |
| T-108: Micro-compact | 1h | 移植 CodeRunner 的截断逻辑 |
| T-109: 对话持久化 | 2h | SQLite 建表 + conversations/messages CRUD |

**Sprint 1 交付物**：双击 exe → 填 API Key → 可以聊天 + 粘贴图片识别

---

## Sprint 2: 智能收件箱（Week 2）

### 目标：能录事件 + 看日程的秘书

| 任务 | 估时 | 说明 |
|------|------|------|
| T-201: Parser 移植 | 3h | TodoList parser.py → parser.js（System Prompt + JSON mode）|
| T-202: 事件确认卡片 | 3h | ConfirmCard.vue: 解析结果展示 + 确认/取消按钮 |
| T-203: SQLite 事件表 | 2h | events 表 CRUD（与 TodoList schema 一致）|
| T-204: 对话→事件检测 | 2h | 聊天回复中自动检测日程意图 → 触发 Parser |
| T-205: 周时间轴 | 4h | WeekTimeline.vue（移植自 TodoList app.js）|
| T-206: 待办清单 | 3h | TodoList.vue: 简单 CRUD |
| T-207: InboxView 组装 | 2h | InboxView.vue: 时间轴 + 待办 + 快速录入 |
| T-208: 思维链折叠 | 2h | ThinkingCard.vue: DeepSeek thinking token 折叠显示 |

**Sprint 2 交付物**：输入 "下周三和李总开会" → 弹出确认卡片 → 本地日程时间轴

---

## Sprint 3: 文件分析 + Vision→日程联动（Week 3）

### 目标：完整 MVP

| 任务 | 估时 | 说明 |
|------|------|------|
| T-301: FileReader 纯文本 | 2h | .txt/.md/.csv/.json/.py 等读取 |
| T-302: FileReader Office | 4h | mammoth(.docx) + xlsx(.xlsx) + pdf-parse(.pdf) |
| T-303: FileDropZone | 2h | 拖拽区组件 + 文件类型检测 + 大小限制 |
| T-304: Vision→Parser 管线 | 3h | 图片识别结果 → 自动检测事件 → Parser → ConfirmCard |
| T-305: 全局截图 | 3h | Ctrl+Shift+S → desktopCapturer → 截取区域 |
| T-306: Calendar 同步 | 4h | 移植 TodoList calendar_sync.py → calendar.js (MSAL) |
| T-307: 侧栏导航 | 2h | 左侧: 对话/日程/设置 三个 tab 切换 |

**Sprint 3 交付物**：完整 MVP — 对话 + Vision + 日程 + 文件分析

---

## Sprint 4: 体验打磨（Week 4）

| 任务 | 估时 | 说明 |
|------|------|------|
| T-401: 系统托盘 | 2h | tray.js: 最小化到托盘 + 右键菜单 |
| T-402: 全局快捷键 | 1h | Ctrl+Shift+Space 唤起窗口 |
| T-403: 桌面通知 | 2h | Electron Notification API + 事件提醒 |
| T-404: 主题切换 | 3h | 暗色/亮色 + CSS 变量系统 |
| T-405: 模型切换 UI | 2h | 顶栏：⚡快速 / 🧠深度 / 🔄自动 三档 |
| T-406: Cost 追踪 | 2h | token 计数 → 估算费用显示 |
| T-407: 对话导出 | 1h | 导出为 .md 文件 |
| T-408: 错误处理美化 | 2h | 网络错误/API 错误的友好提示 |

---

## Sprint 5: 打包发布（Week 5）

| 任务 | 估时 | 说明 |
|------|------|------|
| T-501: electron-builder 配置 | 2h | NSIS 安装器 + 图标 + 元数据 |
| T-502: 自动更新 | 3h | electron-updater + GitHub Releases |
| T-503: 错误日志 | 2h | 本地日志文件 + 崩溃上报 |
| T-504: 内测分发 | 2h | 打包 + 分发给 5-10 个朋友 |
| T-505: 文档收尾 | 2h | README 更新 + 用户使用指南 |

---

## 关键路径

```
T-101 → T-102 → T-103 → T-104 → T-106 (Week 1 核心)
                  ↓
            T-201 → T-204 → T-304 (截图→日程管线)
                  ↓
            T-306 (日历同步，可延后)
```

**最小可演示路径**（3天）：T-101 → T-102 → T-103 → T-104 → T-106

---

## Jules 可委派的任务

以下任务相对独立，适合交给 Jules 执行：

| 任务 | 理由 |
|------|------|
| T-105 Markdown 渲染 | 纯前端，无 IPC 依赖 |
| T-108 Micro-compact | 纯函数，有 CodeRunner 参考 |
| T-201 Parser 移植 | 有 TodoList parser.py 作为精确参考 |
| T-206 待办清单 | 简单 CRUD 组件 |
| T-301/302 FileReader | 纯后端，使用已知库 |
| T-404 主题切换 | 纯 CSS 变量 |
