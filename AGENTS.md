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
│   ├── main.js                  # 应用入口 + 窗口管理 + IPC 注册
│   ├── preload.js               # IPC 安全桥接
│   ├── services/                # 后端服务层
│   │   ├── llm-client.js        # 多模型 LLM 引擎 (Chat + Vision + Tool Calling)
│   │   ├── parser.js            # 自然语言 → 结构化事件
│   │   ├── file-reader.js       # 文件读取引擎 (txt/md/csv/docx/xlsx/pdf)
│   │   └── db.js                # SQLite 持久化 (better-sqlite3)
│   └── tools/                   # 🔑 工具系统 (Sprint 6)
│       ├── base.js              # BaseTool 抽象基类 (参照 CodeRunner)
│       ├── registry.js          # ToolRegistry 工具注册表 + Schema 导出
│       ├── executor.js          # Tool Execution Loop (tool_call → execute → 回传)
│       ├── fs.js                # list_dir / read_file (沙箱化)
│       ├── calendar.js          # create_event / search_events
│       └── web.js               # web_search (可选)
│
├── src/                         # Vue 3 前端 (Renderer 进程)
│   ├── App.vue                  # 根组件 + 侧栏导航
│   ├── views/
│   │   ├── ChatView.vue         # 对话 + 视觉 + 工具调用展示
│   │   ├── InboxView.vue        # 智能收件箱（日程 + 待办）
│   │   └── SettingsView.vue     # 设置面板（含工作目录配置）
│   └── components/
│       ├── ConfirmCard.vue      # 事件确认卡片
│       ├── WeekTimeline.vue     # 周时间轴
│       ├── TodoList.vue         # 待办清单
│       └── SetupWizard.vue      # 首次启动向导
│
├── skills/                      # 🔑 内置基础技能 (随项目分发)
│   ├── daily_summary/
│   │   └── SKILL.md             # 每日总结
│   ├── file_organizer/
│   │   └── SKILL.md             # 文件整理建议
│   └── meeting_prep/
│       └── SKILL.md             # 会议准备助手
│   # 用户可在「设置→外部技能目录」配置额外的 skills 路径
│   # 例如指向 Assistant/common/knowledge/skills/ (49 个共享技能)
│   # 不同设备可指向各自的 Syncthing 同步路径
│
├── mcp/                         # 🔑 MCP 协议集成 (Sprint 6)
│   ├── config.json              # MCP Server 配置
│   └── client.js                # MCP 客户端
│
├── resources/                   # 静态资源
│   └── logos/                   # 模型品牌 PNG Logo
│       ├── deepseek.png
│       ├── openai.png
│       └── ollama.png
│
├── data/                        # ⛔ .gitignore 已忽略，不提交到 GitHub
│   ├── memory/                  # 主观记忆池
│   │   ├── SOUL.md              # Tier 1: 静态人格与偏好
│   │   └── sessions/            # Tier 2: 对话压缩总结 (≤ 7天自动注入)
│   │       └── <conv-id>.md
│   └── wiki/                    # 客观知识池 (Tier 3: 工具检索)
│       ├── sessions/            # 沉淀的旧对话总结 (> 7天自动迁移)
│       ├── projects/            # 工作区项目摘要
│       └── clippings/           # 外部知识剪报
│
├── docs/                        # 项目文档
├── tests/                       # 测试
├── todo.md                      # 开发待办
├── progress.yaml                # 进度追踪
└── package.json
```

### 数据流（当前 — 纯文本管道）

```
用户输入 (文字/图片/文件)
    ↓
Vue 前端 (Renderer) ── IPC ──→ Electron Main Process
                                    ↓
                            buildSystemPrompt() + 历史消息
                                    ↓
                              ┌─────┴─────┐
                              │ LLM Client │ ← OpenAI SDK (chat only)
                              └─────┬─────┘
                                    ↓
                             流式文本返回 → Markdown 渲染
```

### 数据流（Sprint 6 目标 — Agent 工具循环）

```
用户输入
    ↓
Main Process → buildSystemPrompt() + Tool Schemas
    ↓
LLM API (with tools=[list_dir, read_file, create_event, ...])
    ↓
┌─────────────────────────────────────────────┐
│              Tool Execution Loop            │
│                                             │
│  LLM 返回 tool_call? ─── Yes ──→ ToolRegistry.execute()
│       │                              ↓
│       No                     结果追加到 messages
│       ↓                              ↓
│  最终文本回复              ←── 再次调用 LLM ──┘
└─────────────────────────────────────────────┘
    ↓
ChatView 渲染（文本 + 工具执行状态 + 结果折叠）
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

### D-006: 双目录技能系统

**决策**：内置基础技能 + 外部可配置技能目录

**架构**（参照 CodeRunner 的 `ToolRegistry._resolve_skills_dirs()`）：
1. **内置目录** `skills/`：随项目打包分发，包含 3-5 个基础技能（每日总结、文件整理、会议准备等），保证开箱即用
2. **外部目录**：用户在「设置」中配置一个或多个外部技能路径（如 `D:\OneDrive\...\skills\`），ToolRegistry 启动时自动扫描注册

**跨设备场景**：
- 本机开发时指向 `Assistant/common/knowledge/skills/`（49 个共享技能）
- VPS 部署时指向各节点的 Syncthing 同步副本
- 纯净安装时仅用内置的 `skills/` 目录

**SKILL.md 规范**：完全复用现有 frontmatter 标准（name/description/parameters/entrypoint），不发明新格式。

### D-007: Agent 化工具系统

**决策**：通过 OpenAI Function Calling API + 工具执行循环，让模型拥有"手"

**参照**：CodeRunner `src/tools/`（BaseTool + ToolRegistry + 18 工具 + ToolsetProfile）

**关键差异**：
- CodeRunner 是 Python 后端，bob-agent 是 Node.js（Electron 主进程）
- CodeRunner 面向开发者（bash/file_edit/grep），bob-agent 面向普通用户（list_dir/create_event/web_search）
- bob-agent 的工具数量更少（5-8 个），不需要 ToolsetProfile 分层过滤
- bob-agent 增加 MCP 客户端层，可连接外部 MCP Server 扩展能力

### D-008: 三层记忆引擎 (Memory Engine)

**决策**：模拟人类记忆的三层衰减模型，实现跨会话的上下文连贯性

**架构（严格遵守，不得违反）：**

| 层级 | 存储位置 | 注入方式 | 触发时机 |
|:---|:---|:---|:---|
| **Tier 1: 灵魂** | `data/memory/SOUL.md` | 每次全文注入 System Prompt | 每次对话开始 |
| **Tier 2: 短期记忆** | `data/memory/sessions/<id>.md` (mtime ≤7天) | 自动压缩注入 System Prompt | 每次对话开始 |
| **Tier 3: 长期记忆** | `data/wiki/sessions/<id>.md` + `data/wiki/projects/` + `data/wiki/clippings/` | 通过 `brain_search` 工具检索 | Bob 主动调用 |

**Session 总结生命周期：**
1. 用户切换/新建对话时，后台静默调用廉价 LLM 压缩旧对话为 ≤100 字总结
2. 总结写入 `memory/sessions/<conv-id>.md`（热记忆）
3. 超过 7 天未被访问的总结自动迁移到 `wiki/sessions/`（冷记忆）
4. 用户删除对话时，级联删除 `memory/sessions/<id>.md` 和 `wiki/sessions/<id>.md`

**Session .md 文件必须包含元数据头：**
```markdown
---
conversation_id: <UUID>
title: <对话标题>
created: <YYYY-MM-DD>
---
<压缩总结内容>
```

**安全网：**
- 第一层：每条消息实时写入 SQLite（保证原始数据永不丢失）
- 第二层：切换对话时后台生成压缩总结
- 第三层：启动时补偿扫描（处理崩溃/强关导致的未总结对话）

**禁止事项：**
- 绝对不要把完整的原始对话记录注入 System Prompt（Token 爆炸）
- 绝对不要使用 XML 标签注入规则代替 Function Calling（已废弃）
- `data/` 目录绝不提交到版本控制（含用户私人记忆）

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
