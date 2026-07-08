# AGENTS.md — bob-agent AI 编码代理入职手册

> 继承根目录 Gemini/AGENTS.md 的全局准则
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

## 🔴 新增 UI 内容预检清单（Pre-flight Checklist）

**每次新增或修改任何用户可见的界面内容时**，Agent 必须逐项过检：

| # | 检查项 | 规则来源 |
|:--|:-------|:---------|
| 1 | **架构归属**：新内容放在正确的模块/组件中？是否需要新增 Rust Command + Bridge 适配？ | §架构铁律 |
| 2 | **配色语义**：使用了正确的 CSS 变量？Logo 用 `--logo-color`？图标/文字用 `--text-*`？交互用 `--accent-*`？**绝对不要**硬编码颜色或直接用 `--user-accent` 给文字/图标上色 | §Logo 配色 / §强调色 |
| 3 | **图标对齐**：所有包含 Lucide 图标的容器都用了 `display: flex; align-items: center`？ | §图标对齐 |
| 4 | **国际化**：所有用户可见文字都通过 `$t()` 调用？已在 `zh-CN.json` 和 `en-US.json` 中同步添加 key？ | §i18n 铁律 |
| 5 | **权限声明**：如用到新的原生能力，已在 `capabilities/default.json` 中注册？ | §权限声明 |

> **口诀**：架构 → 配色 → 对齐 → 多语种 → 权限

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

### 🔴 安装包编译工作流 (Bootstrapper Pipeline)

本项目的安装器使用的是**双 Tauri 嵌套架构**（Bootstrapper 模式），以实现极其定制化的暗黑风格安装引导（无边框、无系统灰条）。**绝对禁止**使用默认的 Tauri Bundle (NSIS/MSI) 直接分发主程序。

#### 一键发布（推荐）

```bash
scripts\release.bat
```

运行后会**自动依次执行**以下 6 步，最终产物统一收集到 `dist-release/` 目录并自动打开文件夹：

| 步骤 | 操作 | 说明 |
|:---:|------|------|
| 1/6 | `npm run tauri build` | 编译主程序 `bob.exe` (Release) |
| 2/6 | `node scripts/build_payload.mjs` | 将 bob.exe + pdfium.dll + skills 打包为 payload.zip |
| 3/6 | 复制 payload.zip → installer | 供安装器嵌入 |
| 4/6 | `cd installer && npm run tauri build` | 编译带 Bob Logo 的独立安装器 |
| 5/6 | 收集产物 → `dist-release/` | 归集最终可分发文件 |
| 6/6 | 清理中间文件 | 删除 payload.zip、bundle 临时目录 |

#### 最终产物

```
dist-release/
├── bob-installer.exe          # 带 Bob Logo 的独立安装器（~25MB）
└── bob-agent-portable.zip     # 绿色免安装版（~15MB）
```

> ⚠️ `dist-release/` 已被 `.gitignore` 排除，二进制产物不入版本控制。

### 🌐 官网部署工作流 (Marketing Website Pipeline)

`bob-agent/website/` 目录存放的是着陆页（Landing Page）和静态资源。
> **⚠️ 严禁假定自动同步**：该目录**没有**包含在 Syncthing 的同步范围内，必须通过脚本手动推送到指定的云端节点。
> *具体的部署节点 IP、外网域名映射以及 SSH Session 名称，请查阅 `Assistant/common/knowledge/skills/cluster_ops/references/node_inventory.md` 和 `service_map.md` 获取最新映射。*

#### 部署方式：
在项目根目录运行一键部署脚本：
```bash
deploy_website.bat
```
执行过程说明：
1. **打包**：将 `website/` 目录压缩为 `.zip`。
2. **传输**：通过 `pscp` 和预配置的 SSH Session 将文件推送到目标节点的 `/tmp/` 目录。
3. **部署**：通过 `plink` 远程执行 `sudo unzip` 解压到 Caddy 的目标目录 `/opt/bob/`。

---

## 编码规范

### Rust 侧（`src-tauri/`）

1. **UTF-8**：Rust 的 `String` 天然 UTF-8，但读写外部文件时仍需注意 BOM 和换行符。
2. **错误处理**：Tauri Command 必须返回 `Result<T, String>` 或使用 `thiserror`。**禁止在 Command 中 `unwrap()`/`panic!()`**——前端需要收到可读的错误信息而不是进程崩溃。
3. **模块化**：当 `lib.rs` 超过 200 行时，必须拆分为 `mod config;`、`mod database;`、`mod llm;` 等子模块。

### Vue 前端侧（`src/`）

1. **IPC 调用**：统一通过 `window.electronAPI.xxx()` 调用。**不要直接 import `@tauri-apps/api/core`**——这会破坏与 Electron 的兼容性。所有 Tauri 特有 API 仅在 `tauri-bridge.js` 中使用。
2. **组件风格**：Vue 组件使用 `PascalCase`，JS 函数使用 `camelCase`，文件名使用 `kebab-case`。
3. **响应式设计**：遵循 `frontend-design` Skill 中的响应式铁律（使用 `100dvh`，输入框 `≥16px` 防 iOS 缩放等）。
4. **🔴 静态资源 SSOT（唯一真相源）**：所有第三方动态图标/Logo（如各家大模型的品牌 Logo）**必须且只能**存放在 `public/logos/` 目录中。
   - 引用方式：由于这些 Logo 通常是根据模型 ID 动态加载的（例如 `getProviderLogo(id)`），前端代码应直接使用绝对路径 `/logos/xxx.png` 引用。
   - Vite 在编译时会自动将 `public/` 目录下的所有文件原封不动地复制到 `dist/` 中供 Tauri 打包，千万不要手动把源文件丢进 `dist/`！
   - 如果是与业务高度绑定的固定静态装饰图片，才建议放入 `src/assets/` 并使用 `new URL(..., import.meta.url)`。

---

## 安全红线

- **绝对不要**在代码中硬编码 API Key（Rust 侧使用 Stronghold 加密存储，或动态路由至 config.json）
- **绝对不要**在 Rust Command 中 `unwrap()` 或 `panic!()`（使用 `Result` 返回错误）
- **绝对不要**在 Vue 前端中直接 import `@tauri-apps/api/core`（仅 Bridge 层可用）
- **绝对不要**向 `electron/` 目录添加新功能
- **绝对不要**执行用户未确认的文件写入操作
- `data/` 目录绝不提交到版本控制（含用户私人记忆）
- `.env` 文件绝不提交到版本控制

---

## JIT 指针

- **全量功能与逻辑字典 (LLM-Wiki)**：详见 [LLM_WIKI.md](LLM_WIKI.md) (当需要修改、查找或调试具体功能如“闪念速记”时，请优先阅读此字典)
- **架构/目录树/IPC/依赖**：详见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- **UI 设计铁律 (配色/对齐/i18n)**：详见 [design_principles.md](design_principles.md)
- **灵魂定义**：详见 [data/memory/SOUL.md](data/memory/SOUL.md)
- **路线图**：详见 [todo.md](todo.md)
- **开源发布工作流**：详见 [OPEN_SOURCE_WORKFLOW.md](OPEN_SOURCE_WORKFLOW.md)
