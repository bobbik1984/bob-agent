# AGENTS.md — bob-agent AI 编码代理入职手册

> **适用范围**：所有在此项目工作的 AI 编码代理（Antigravity、Jules、CodeRunner、Cursor 等）
> **架构状态**：✅ **Electron → Tauri 迁移已完成**（进入 Agent 化深化阶段）
> **历史归档**：Electron 时代的完整入职手册已保存至 `docs/agents_electron.md`

---

## 项目概述

**bob-agent** 是一个 **Windows 桌面 AI 私人秘书**，面向不喜欢折腾的普通用户。

- **产品定位**：开箱即用的桌面 AI 助手，核心能力是对话 + 图片识别 + 日程管理 + 文件分析
- **技术栈**：**Tauri v2 (Rust)** + Vue 3 + Vite（正在从 Electron 迁移）
- **目标用户**：办公白领、非技术人员
- **血统**：融合了 CodeRunner 的上下文管理精华 + DeepSeek-TUI 的工程理念 + TodoList 的日程管理能力

---

## ⚠️ 架构铁律（每个 Agent 必读）

### 🔴 纯 Tauri 架构

项目目前为纯 **Tauri (Rust)** 后端架构。旧的 `electron/` 目录已被彻底移除。**所有的底层系统能力、API 请求、文件操作必须在 Rust 侧实现**。

### 🔴 前端零改动原则

前端 `src/` 中的 Vue 组件**不知道**自己跑在 Tauri 还是 Electron 里。它们统一调用 `window.electronAPI.xxx()`。所有适配工作由 `src/tauri-bridge.js` 这个垫片层完成。**绝对禁止**修改 Vue 组件中的 IPC 调用方式。

### 🔴 Bridge 适配器模式

`tauri-bridge.js` 是连接前端和 Rust 后端的唯一网关：
- 已实现的 Rust Command → `invoke('rust_command_name', { args })`
- 尚未实现的接口 → 返回 Mock 假数据（如 `sendChat: async () => { ... }`）
- **每完成一个 Rust Command，就去 Bridge 中把对应的 Mock 替换为真实的 `invoke()` 调用**

---

## 命令

```bash
# ─── Tauri 开发（主线，日常使用这个） ───
npm run dev:tauri        # 启动 Tauri 开发模式（Vite 热更新 + Rust 编译）
npm run build:tauri      # 构建 Tauri 生产版本（~15MB 安装包）

# ─── Rust 侧独立命令 ───
cd src-tauri && cargo build          # 仅编译 Rust 后端
cd src-tauri && cargo check          # 快速语法检查（不生成二进制）
cd src-tauri && cargo test           # 运行 Rust 单元测试

# ─── 前端与测试 ───
npm run build            # 仅构建前端 (vite build)
npm test                 # 运行 Vitest 测试
npm run lint             # ESLint 检查
```

---

## 架构

```
bob-agent/
├── src-tauri/                       # Tauri 后端 (Rust) — 所有新代码写这里
│   ├── src/
│   │   ├── main.rs                  # Tauri 入口
│   │   ├── lib.rs                   # 🔑 Config + DB 初始化 + IPC 注册 + 系统托盘
│   │   ├── llm.rs                   # LLM 引擎 (SSE 流式 + Tool Calling 循环)
│   │   ├── tools.rs                 # 🔑 12 个原生工具 + 执行调度器
│   │   ├── calendar.rs              # 日程/待办管理 (SQLite)
│   │   ├── outbox.rs                # Outbox/Reconciler 声明式配置
│   │   ├── filesystem.rs            # 文件读取/扫描/跟踪
│   │   ├── web.rs                   # 网页抓取 (reqwest + scraper)
│   │   ├── plugins.rs               # 技能/插件扫描
│   │   ├── dream.rs                 # 做梦引擎 V1
│   │   ├── sidecar.rs               # Sidecar 子进程 (llama-server)
│   │   ├── kb_extractor.rs          # 知识库文件提取
│   │   └── kb_indexer.rs            # 知识库索引构建
│   ├── capabilities/
│   │   └── default.json             # 🔑 原生能力权限声明
│   ├── Cargo.toml                   # Rust 依赖管理
│   └── tauri.conf.json              # Tauri 应用配置
│
├── src/                             # Vue 3 前端 (Renderer) — 不直接改动 IPC 调用
│   ├── tauri-bridge.js              # 🔑 适配器：拦截 electronAPI → invoke()
│   ├── App.vue                      # 根组件 + 侧栏导航
│   ├── views/
│   │   ├── ChatView.vue             # 对话 + 视觉 + 工具调用展示
│   │   ├── InboxView.vue            # 日程面板（时间轴 + 待办）
│   │   └── SettingsView.vue         # 设置面板（含工作空间配置）
│   └── components/
│       ├── SetupWizard.vue          # 首次启动向导
│       ├── WeekTimeline.vue         # 周时间轴（可拖拽）
│       ├── SearchCard.vue           # 搜索结果卡片（与 FileCard 统一设计）
│       └── ...
│
├── skills/                          # 内置基础技能
├── data/                            # ⛔ .gitignore — 用户隐私数据
├── docs/                            # 架构、用户手册、历史归档
├── todo.md                          # 🔑 路线图（里程碑 1-10）
└── progress.yaml                    # 进度追踪
```

---

## IPC 实现状态速查表

### 🟢 已用 Rust 实现（真实调用）

| 分类 | 前端调用 | Rust Command | 说明 |
|:---|:---|:---|:---|
| 配置 | `isSetupComplete()` | `system_is_setup_complete` | config.json 判断 |
| 配置 | `getConfig/setConfig/getAllConfig` | `config_get/set/get_all` | 键值 CRUD |
| 对话 | `getConversations/create/delete/rename` | `db_conversation_*` | rusqlite CRUD |
| 消息 | `getMessages/addMessage` | `db_messages/db_message_add` | rusqlite |
| LLM | `sendChat/sendVision` | `llm_chat/llm_vision` | reqwest SSE + Tool Calling |
| LLM | `getModelPool/getActiveModels/assignModelRole` | `llm_get_*` | ModelHub |
| 凭证 | `getApiKeys/setApiKey` | `system_get/set_api_key` | config.json 存储 |
| 日程 | `listEvents/confirmEvent/deleteEvent/...` | `system_list/confirm/delete_event` | calendar.rs SQLite |
| 文件 | `readFile/scanFolder/getFileMeta` | `filesystem::system_*` | walkdir + fs |
| 文件夹 | `getTrackedFolders/add/remove` | `filesystem::system_*_tracked_*` | config 持久化 |
| 网页 | `fetchUrl` | `web::system_fetch_url` | reqwest + scraper |
| 知识库 | `estimateKB/buildKB` | `kb_extractor/kb_indexer` | PDF/DOCX/XLSX 提取 |
| 插件 | `getPlugins` | `plugins::system_get_plugins` | 技能扫描 |
| 做梦 | `summarizeSession/getDreamReport/dismissDream` | `dream::system_*` | 记忆引擎 |
| Outbox | `writeOutbox` | `system_write_outbox` | 声明式配置 |
| 系统 | `openFile/showInFolder/getVersion/factoryReset/...` | `system_*` | Rust 原生 |

### 🔴 仍为 Mock（Bridge 中硬编码）

| 接口 | 说明 |
|:---|:---|
| `updateTheme` | 主题热切换（目前 console.log） |
| `getClipboardImage` | 剪贴板图片读取（返回 null） |
| `showNotification` | 桌面通知（console.log） |
| `getMcpConfig/setMcpConfig` | MCP 服务器配置 |
| `installPlugin` | 插件安装逻辑 |

---

## 编码规范

### Rust 侧（`src-tauri/`）

1. **UTF-8**：Rust 的 `String` 天然 UTF-8，但读写外部文件时仍需注意 BOM 和换行符。
2. **错误处理**：Tauri Command 必须返回 `Result<T, String>` 或使用 `thiserror`。**禁止在 Command 中 `unwrap()`/`panic!()`**——前端需要收到可读的错误信息而不是进程崩溃。
3. **模块化**：当 `lib.rs` 超过 200 行时，必须拆分为 `mod config;`、`mod database;`、`mod llm;` 等子模块。
4. **依赖引入**：新增 crate 前必须在 `Cargo.toml` 中添加，并说明用途。推荐的核心 crate：

   | 功能 | Crate | 状态 |
   |:---|:---|:---|
   | JSON 序列化 | `serde` + `serde_json` | ✅ 已引入 |
   | SQLite | `rusqlite` (bundled) | ✅ 已引入 (M4) |
   | HTTP 客户端 | `reqwest` (stream + json) | ✅ 已引入 (M5) |
   | 异步运行时 | `tokio` (full) | ✅ 已引入 (M5) |
   | 文件遍历 | `walkdir` | ✅ 已引入 (M6) |
   | DOM 解析 | `scraper` | ✅ 已引入 (M6) |
   | PDF 解析 | `pdf-extract` | ✅ 已引入 (M6) |
   | Excel 解析 | `calamine` | ✅ 已引入 (M6) |
   | 单实例锁 | `tauri-plugin-single-instance` | ✅ 已引入 |
   | 全局快捷键 | `tauri-plugin-global-shortcut` | ✅ 已引入 (Ctrl+Shift+B) |
   | 加密存储 | `tauri-plugin-stronghold` | 🔜 计划中 (M3) |

### Vue 前端侧（`src/`）

1. **IPC 调用**：统一通过 `window.electronAPI.xxx()` 调用。**不要直接 import `@tauri-apps/api/core`**——这会破坏与 Electron 的兼容性。所有 Tauri 特有 API 仅在 `tauri-bridge.js` 中使用。
2. **组件风格**：Vue 组件使用 `PascalCase`，JS 函数使用 `camelCase`，文件名使用 `kebab-case`。
3. **响应式设计**：遵循 `frontend-design` Skill 中的响应式铁律（使用 `100dvh`，输入框 `≥16px` 防 iOS 缩放等）。

### 权限声明（Capabilities）

Tauri v2 采用**权限白名单**机制。每当你需要使用新的原生能力（如 shell、fs、notification），**必须**在 `src-tauri/capabilities/default.json` 的 `permissions` 数组中注册：

```json
{
  "permissions": [
    "core:default",
    "dialog:default",
    "stronghold:default",   // 加密存储
    "shell:default",        // 打开外部链接/文件
    "notification:default"  // 系统通知
  ]
}
```

**不注册 = 运行时权限拒绝 = 功能静默失败**。这是 Agent 最容易踩的坑。

---

## 模型生态扩展指南 (LLM Provider Extension)

未来当需要在 `bob-agent` 中新增模型或全新大模型供应商时，必须遵循以下 Tauri 后端扩展规范：

### 1. 新增现有厂商的模型
若仅新增（如 `deepseek-v5` 或 `qwen-4`），只需修改 `src-tauri/src/llm.rs` 中的 `get_model_pool()`：
- 在 JSON 数组中添加模型条目。
- 必须包含 `provider`（如 `"deepseek"`）和 `pricing` 字段（如 `{"input": 1.0, "output": 2.0}`）。
- 后端会自动依据 `provider` 将请求路由至官方 `baseURL`。

### 2. 接入全新的大模型厂商 (Provider)
若要接入如 `anthropic` 等全新厂商，需要修改 `src-tauri/src/llm.rs` 的 3 个位置：
1. **模型池注册**：在 `get_model_pool()` 中添加模型，声明新的 `provider`（如 `"anthropic"`）。
2. **防误判路由注册**：在 `read_llm_config_for_model` 方法的 `is_custom_proxy` 白名单中，将该厂商的官方域名（如 `!base_url.contains("anthropic.com")`）加入排除名单，以防止智能路由将其误判为用户的自定义反向代理。
3. **官方 Base URL 映射**：在 `read_llm_config_for_model` 的 `match provider.as_str()` 块中，添加 `"anthropic" => "https://api.anthropic.com/v1"` 的映射分支。
*(注意：如遇特殊厂商不支持标准参数，需在 `stream_internal` 构建请求体时添加特定的分支逻辑。)*

---

## 核心技术决策记录

### D-001: 从 Electron 迁移至 Tauri（决策变更）

**原决策**：使用 Electron（因为开发者不会 Rust）
**新决策**：迁移至 Tauri v2 (Rust)

**变更理由**：
- 代码完全由 AI Agent 编写，Rust 不再是门槛
- Rust 编译器的极致类型检查 = AI 最完美的反馈循环（编译通过 ≈ 无 Bug）
- 打包体积从 ~120MB 骤降至 ~15MB
- 内存占用从 ~300MB 降至 ~30MB
- 彻底告别 `node_modules` 黑洞和 `better-sqlite3` 的 C++ 编译地狱

**迁移策略**：Adapter 隔离法（`tauri-bridge.js`），前端零感知，后端逐模块替换。

### D-002: 为什么不做 Web App？

（不变）桌面原生应用。需要本地文件读取、系统托盘、全局快捷键、桌面通知。

### D-003: 双目录技能系统

（不变）内置 `skills/` + 用户可配置外部技能目录。SKILL.md 规范复用。

### D-004: LLM 多模型支持

**供应商优先级**：
1. DeepSeek (deepseek-v4-pro / deepseek-v4-flash) — 默认
2. ModelScope 免费层 (DeepSeek-V4-Flash / GLM-5.1) — Clerk 任务
3. Qwen (qwen3.5-flash / qwen3.5) — 备选
4. Ollama (本地模型) — 离线场景
5. 自定义 (OpenAI 兼容端点) — 高级用户

### D-005: 三层记忆引擎

（不变）Tier 1 灵魂 (SOUL.md) → Tier 2 短期记忆 (sessions/) → Tier 3 长期记忆 (wiki/)。详见 `docs/agents_electron.md` 中 D-008 的完整描述。

---

## 依赖清单

### Rust 后端 (`src-tauri/Cargo.toml`)

| Crate | 用途 | 状态 |
|:---|:---|:---|
| `tauri` v2.0.0-rc | 桌面应用框架 | ✅ |
| `serde` + `serde_json` | JSON 序列化 | ✅ |
| `dirs` | 跨平台用户数据目录 | ✅ |
| `rusqlite` (bundled) | SQLite 数据库 | ✅ |
| `reqwest` (stream + json) | HTTP 客户端 (LLM/搜索/天气) | ✅ |
| `tokio` (full) | 异步运行时 | ✅ |
| `walkdir` | 文件夹递归扫描 | ✅ |
| `scraper` | HTML DOM 解析 | ✅ |
| `pdf-extract` + `calamine` + `quick-xml` + `zip` | 知识库文件解析 | ✅ |
| `chrono` | 时间处理 | ✅ |
| `tauri-plugin-dialog` | 原生文件对话框 | ✅ |
| `tauri-plugin-log` | 日志 | ✅ |
| `tauri-plugin-shell` | 打开外部文件/链接 | ✅ |
| `tauri-plugin-single-instance` | 防双开 | ✅ |
| `tauri-plugin-stronghold` | 加密凭据存储 | 🔜 计划中 |

### 前端 (`package.json`)

| 包 | 用途 |
|:---|:---|
| `vue` 3.x | 前端框架 |
| `vite` 6.x | 构建工具 |
| `@tauri-apps/api` | Tauri 前端 IPC SDK |
| `@tauri-apps/plugin-dialog` | 对话框前端绑定 |
| `marked` + `highlight.js` | Markdown 渲染 |
| `lucide-vue-next` | 图标库 |
| `vue-i18n` | 国际化 |

### 待清理的 Electron 遗留依赖

以下 `package.json` 中的依赖是 Electron 时代遗留，Tauri 不使用但尚未清理：
`electron`, `electron-builder`, `electron-log`, `better-sqlite3`, `openai`, `mammoth`, `xlsx`, `pdf-parse`, `cheerio`, `concurrently`, `dotenv`, `ws`, `officeparser`

> 在 M7 (T-703 打包发布) 时应彻底清理。

---

## 测试要求

- Rust 后端：使用 `#[cfg(test)]` 内置测试模块，`cargo test` 运行
- Vue 前端：使用 Vitest，`npm test` 运行
- LLM 调用使用 mock，不实际调 API
- 每完成一个 Rust Command 的真实实现，必须同步验证对应的 Bridge 调用正常工作

---

## 安全红线

- **绝对不要**在代码中硬编码 API Key（Rust 侧通过 Stronghold 加密存储）
- **绝对不要**在 Rust Command 中 `unwrap()` 或 `panic!()`（使用 `Result` 返回错误）
- **绝对不要**在 Vue 前端中直接 import `@tauri-apps/api/core`（仅 Bridge 层可用）
- **绝对不要**向 `electron/` 目录添加新功能
- **绝对不要**执行用户未确认的文件写入操作
- `data/` 目录绝不提交到版本控制（含用户私人记忆）
- `.env` 文件绝不提交到版本控制
