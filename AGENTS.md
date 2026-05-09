# AGENTS.md — bob-agent AI 编码代理入职手册

> **适用范围**：所有在此项目工作的 AI 编码代理（Antigravity、Jules、CodeRunner、Cursor 等）

---

## 项目概述

**bob-agent** 是一个 **Windows 桌面 AI 私人秘书**，面向不喜欢折腾的普通用户。

- **产品定位**：开箱即用的桌面 AI 助手，核心能力是对话 + 图片识别 + 日程管理 + 文件分析
- **技术栈**：Electron + Vue 3 + Vite + Node.js
- **目标用户**：办公白领、非技术人员
- **血统**：融合了 CodeRunner 的上下文管理精华 + DeepSeek-TUI 的工程理念 + TodoList 的日程管理能力

### 与其他项目的关系

| 项目 | 关系 | 说明 |
|------|------|------|
| `todolist` | **代码移植源** | parser.py/calendar_sync.py 的业务逻辑移植为 JS 版本。WeekTimeline UI 直接复用。两个项目通过 Microsoft 365 日历天然数据同步 |
| `code_runner` | **理念借鉴** | micro-compact 上下文截断、流式输出、Markdown 渲染等精华概念移植 |
| `DeepSeek-TUI` | **参考项目（外部）** | 多 Provider 配置、审批策略等工程理念的来源，但不共享任何代码 |

---

## 命令

```bash
# 安装依赖
npm install

# 开发模式（热更新）
npm run dev

# 构建生产版本
npm run build

# 打包为 Windows 安装包
npm run pack

# 运行测试
npm test

# 代码检查
npm run lint
```

---

## 架构

```
bob-agent/
├── electron/                    # Electron 主进程 (Node.js)
│   ├── main.js                  # 应用入口 + 窗口管理
│   ├── preload.js               # IPC 安全桥接
│   ├── tray.js                  # 系统托盘 + 全局快捷键
│   └── services/                # 后端服务层
│       ├── llm-client.js        # 多模型 LLM 引擎 (Chat + Vision)
│       ├── parser.js            # 自然语言 → 结构化事件 (移植自 todolist)
│       ├── calendar.js          # Microsoft 365 日历同步 (移植自 todolist)
│       ├── file-reader.js       # 文件读取引擎 (txt/md/csv/docx/xlsx/pdf)
│       ├── micro-compact.js     # 上下文截断中间件 (移植自 code_runner)
│       └── db.js                # SQLite 持久化 (better-sqlite3)
│
├── src/                         # Vue 3 前端 (Renderer 进程)
│   ├── App.vue                  # 根组件 + 路由
│   ├── views/
│   │   ├── ChatView.vue         # 对话 + 视觉（主界面）
│   │   ├── InboxView.vue        # 智能收件箱（日程 + 待办）
│   │   └── SettingsView.vue     # 设置面板
│   ├── components/
│   │   ├── MessageBubble.vue    # 消息气泡 (支持 Markdown + 图片)
│   │   ├── ThinkingCard.vue     # 思维链折叠卡片
│   │   ├── ConfirmCard.vue      # 事件确认卡片
│   │   ├── WeekTimeline.vue     # 周时间轴 (移植自 todolist)
│   │   ├── TodoList.vue         # 待办清单
│   │   ├── FileDropZone.vue     # 文件/图片拖拽区
│   │   └── SetupWizard.vue      # 首次启动向导
│   └── composables/
│       ├── useLLM.js            # LLM IPC 封装
│       ├── useCalendar.js       # 日历 IPC 封装
│       └── useTheme.js          # 主题切换
│
├── docs/                        # 项目文档
│   ├── REQUIREMENTS.md          # 产品需求文档
│   ├── ARCHITECTURE.md          # 技术架构详解
│   ├── DEVELOPMENT_PLAN.md      # 开发计划
│   └── TODOLIST_INTEGRATION.md  # TodoList 集成指南
│
├── tests/                       # 测试
├── todo.md                      # 开发待办
├── progress.yaml                # 进度追踪
└── package.json
```

### 数据流

```
用户输入 (文字/图片/文件)
    ↓
Vue 前端 (Renderer) ── IPC ──→ Electron Main Process
                                    ↓
                              ┌─────┴─────┐
                              │ LLM Client │ ← OpenAI SDK
                              └─────┬─────┘
                                    ↓
                            ┌───────┴────────┐
                      纯对话回复          检测到事件
                            ↓                ↓
                      Markdown 渲染    Parser → ConfirmCard
                                             ↓ (用户确认)
                                    Calendar Sync + SQLite + 通知
```

---

## 编码规范

### 强制规则

1. **UTF-8**：所有文件 I/O 使用 UTF-8。Node.js 默认 UTF-8，但写入文件时仍需显式指定：
   ```javascript
   fs.writeFileSync(path, content, 'utf-8');
   ```

2. **IPC 安全**：Renderer 进程**不允许**直接访问 Node.js API。所有系统操作通过 `preload.js` 暴露的 IPC 通道：
   ```javascript
   // ✅ 正确：通过 IPC
   const result = await window.electronAPI.sendChat(messages);

   // ❌ 错误：Renderer 中直接 require
   const fs = require('fs'); // 绝对禁止
   ```

3. **API Key 安全**：
   - API Key 只存在于主进程内存和本地加密存储中
   - 绝不通过 IPC 传递 API Key 到 Renderer
   - 绝不在日志中打印完整 API Key

4. **代码风格**：
   - Vue 组件使用 `PascalCase`
   - JS 函数使用 `camelCase`
   - 文件名使用 `kebab-case`
   - 中英文注释均可

### 设计原则

1. **用户零配置**：除了 API Key，不应要求用户配置任何技术参数
2. **静默降级**：功能不可用时静默降级，不弹错误对话框。例：日历未配置 → 只保存本地不同步
3. **反极客**：不要暴露任何技术概念（token、context window、model ID）给用户

---

## 核心技术决策记录

### D-001: 为什么选 Electron 而不是 Tauri？

**决策**：使用 Electron

**理由**：
- 开发者精通 Vue + Node.js，不会 Rust
- Electron 允许 3-4 周出 MVP，Tauri 需要 2-3 个月
- 80-120MB 包体积对 Windows 桌面用户完全可接受（VS Code 也是 Electron）
- AI 代理写 JS/Vue 的成熟度远高于 Rust
- 未来如有必要可迁移至 Tauri v2

### D-002: 为什么不做 Web App？

**决策**：桌面原生应用

**理由**：
- 需要读取本地文件（拖拽 .docx/.pdf）
- 需要系统托盘 + 全局快捷键
- 需要桌面通知提醒
- 目标用户（非技术人员）更接受"安装一个软件"

### D-003: 与 TodoList 的同步策略

**决策**：定期整合，不做实时同步

**理由**：
- TodoList = Python，bob-agent = JavaScript，语言不同无法直接共享代码
- 数据层通过 Microsoft 365 Calendar API 天然同步（两个项目操作同一个日历）
- 代码层：parser/calendar/db 逻辑从 Python 移植为 JS（总量 ~450 行）
- Parser 的 System Prompt 两边保持一致，每月对齐一次

### D-004: LLM 多模型支持

**决策**：通过 OpenAI SDK 兼容协议支持多供应商

**供应商优先级**：
1. DeepSeek (deepseek-v4-pro / deepseek-v4-flash) — 默认，支持 Vision
2. OpenAI (gpt-4.1-mini / gpt-4.1) — 备选
3. Ollama (本地模型) — 离线场景
4. 自定义 (OpenAI 兼容端点) — 高级用户

### D-005: 图片识别实现

**决策**：使用 DeepSeek V4 Vision API（OpenAI 兼容的 `image_url` 内容块）

**图片来源**：
- `Ctrl+V` 剪贴板粘贴 → `clipboard.readImage()`
- 拖拽到窗口 → HTML5 drag & drop
- 全局截图 `Ctrl+Shift+S` → `desktopCapturer`
- 文件选择 → `dialog.showOpenDialog`

**联动**：图片识别结果可自动触发 Parser 管线 → 提取事件 → 确认卡片 → 写入日历

---

## 依赖清单

### 核心依赖
| 包 | 用途 |
|---|------|
| `electron` | 桌面应用框架 |
| `electron-builder` | 打包为 exe/msi |
| `vue` (3.x) | 前端框架 |
| `vite` | 构建工具 |
| `@anthropic-ai/sdk` 或 `openai` | LLM API 客户端 |
| `better-sqlite3` | 本地数据库 |
| `marked` | Markdown 渲染 |
| `highlight.js` | 代码高亮 |

### 文件处理
| 包 | 用途 |
|---|------|
| `mammoth` | .docx 读取 |
| `xlsx` | .xlsx 读取 |
| `pdf-parse` | .pdf 读取 |

### 日历集成
| 包 | 用途 |
|---|------|
| `@azure/msal-node` | Microsoft 365 OAuth |

---

## 测试要求

- 新增模块必须附带基础测试
- 使用 Vitest 作为测试框架
- 至少覆盖正常路径 + 异常处理
- LLM 调用使用 mock，不实际调 API

---

## 安全红线

- **绝对不要**在代码中硬编码 API Key
- **绝对不要**在 Renderer 进程中直接调用 Node.js API
- **绝对不要**执行用户未确认的文件写入操作
- **绝对不要**向外部发送用户本地文件内容（除 LLM API 调用外）
- `.env` 文件绝不提交到版本控制
