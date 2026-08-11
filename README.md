<p align="center">
  <img src="public/bob_logo.svg" alt="Bob Agent Logo" width="120" style="margin-bottom: 10px;" />
  <h1 align="center">Bob Agent</h1>
  <p align="center">
    <strong>A zero-setup personal execution system that learns how you work</strong><br/>
    <strong>零设置、懂你的、以结果为单位工作的个人执行系统</strong>
  </p>
  <p align="center">
    <img src="https://img.shields.io/badge/version-v0.9.0-blue?style=flat-square" alt="Version" />
    <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License" />
    <img src="https://img.shields.io/badge/platform-Windows_|_Android-0078D6?style=flat-square&logo=windows" alt="Platform" />
    <img src="https://img.shields.io/badge/Tauri-v2-FFC131?style=flat-square&logo=tauri" alt="Tauri v2" />
    <img src="https://img.shields.io/badge/Rust-%23000000?style=flat-square&logo=rust" alt="Rust" />
    <img src="https://img.shields.io/badge/Vue_3-4FC08D?style=flat-square&logo=vuedotjs&logoColor=white" alt="Vue 3" />
  </p>
</p>

---

## 📖 Introduction / 项目简介

**Bob Agent** is a privacy-first, self-evolving personal execution system built on **Tauri v2 + Rust**. Its product direction is simple: users express intent in ordinary language; Bob turns it into a safe, resumable and verifiable result without asking them to manage models, tools, MCP or agent orchestration. Bob lives in the Windows tray and extends to Android as a lightweight companion.

**Bob Agent** 是基于 **Tauri v2 + Rust** 构建的「隐私优先、持续进化」个人执行系统。用户只需表达意图，Bob 的目标是自动判断应该回答、执行、规划、持续推进还是定时处理，并以可验证结果完成闭环。PC 端保持安装简单与绿色免安装，Android 端保持轻量；模型、工具、记忆和跨端协作的内部复杂性不应转嫁给普通用户。

### Product North Star / 产品北极星

> **Bob keeps complex work moving without losing context.**<br>
> **Bob 让复杂工作不断线。**

`v0.8.0` 已封存可靠 Capture 与知识提交基线。`v0.9.0` 已完成 **Persistent Work Core、现有输入接入、Decision/Change Review、Complexity Router** 与 Phase 5 **单 Agent Advanced Project Loop** 的首个可靠纵切片。对话首屏新增 Today 摘要：用一个焦点和最多两个关注项汇总日程、待办、项目、Goal、审批、最近会话与 Dream，并可从速记浮层无损打开。Dynamic DAG、多 Agent、常驻后台与可信跨端 R3 最终确认仍未实现；显式 `Goal Mode` 继续作为旧 Maker–Checker 实验入口，不等同于新 Runtime。

---

## ✨ Core Capabilities / 核心能力

| | Feature / 功能 | Description / 描述 |
|:---:|---|---|
| 🎐 | **Quick Capture 极速交互** | Global hotkey (`Ctrl+Shift+B`), drag-and-drop file/screenshot processing — capture ideas without breaking your workflow. <br>全局快捷键一键唤起、文件/截图拖拽交互、托盘气泡常驻，不打断手头工作。 |
| 🧭 | **Persistent Work Core 持续工作核心** | Project state, goals, tasks, decisions, revisions and append-only activity survive chat changes and app restarts, with portable Markdown snapshots. <br>项目状态、目标、任务、决定和活动记录不依赖某次对话，并可生成供其他 Agent 阅读的 Markdown 快照。 |
| 🔎 | **Decision & Change Review 决策与变更审查** | Decisions retain alternatives, evidence, participants, owners and revisit conditions. File revisions preserve prior artifacts and surface evidence-backed impact for confirmation. <br>决策原生保留备选、证据、参与者、负责人和重访条件；新版文件不覆盖旧成果，只把有依据的影响交给用户确认。 |
| 🧭 | **Complexity Router 复杂度路由** | Auto selects Direct, Deep or Advanced with deterministic signals first and a short Clerk fallback only for ambiguity. Routing never bypasses tool permissions. <br>自动模式优先用本地规则选择直接处理、深度处理或持续任务；只有真正模糊的语义才调用 Clerk，且路由永远不能绕过工具权限。 |
| 🎛️ | **Model Hub 模型中心** | Auto-discover 40+ models from any OpenAI-compatible provider. Main/Clerk dual-model roles. Offline Sidecar (GGUF). <br>自动检索任何兼容 API 服务商的 40+ 模型，内置 Maker/Clerk 双角色协作，支持本地离线大模型运行。 |
| 🔌 | **MCP Client 认知工具** | Native stdio JSON-RPC 2.0 client managing MCP Server child processes. Dynamic tool discovery & conflicts-free namespacing. <br>原生 stdio 异步管理 MCP 子进程，自动扫码注册认知技能，完全兼容开源 MCP 生态。 |
| 🧠 | **Memory & Dream 记忆进化** | Current: Soul, Session, Wiki, structured corrections and nightly compaction. Direction: separate identity, preference, episodic, procedural and project memory, then learn from verified Goal outcomes. <br>当前已具备三层记忆、结构化纠错与夜间整理；下一阶段将身份、偏好、经历、策略和项目状态分离，并从 Goal 的真实结果中学习。 |
| 🕸️ | **SQLite Graph 知识脑图** | Native SQLite graph database (nodes + edges). BFS sub-graph extraction and interactive Vis.js canvas visualization. <br>基于本地 SQLite 构建轻量知识网络，自动提取实体关系，并通过 Vis.js 脑图画布进行拖拽交互。 |
| 🎯 | **Advanced Project Loop 持续目标闭环** | Auto compiles persistent Goals, stores attempts, checkpoints, approvals and evidence in SQLite, resumes safe R0/R1 work after restart, and refuses unverified completion. Dynamic DAG and multi-agent orchestration remain later phases. <br>Auto 可把持续任务编译为持久 Goal，在 SQLite 中保存尝试、检查点、审批与证据；重启后只恢复安全的 R0/R1 工作，证据不足不得完成。动态 DAG 与多 Agent 仍属后续阶段。 |
| — | **Conversation-first Today Layer 今日概览** | A compact, model-free daily surface inside chat: one focus, up to two attention items, expandable detail, per-device seen state, and lossless handoff from Quick Note. <br>以对话为主入口，用一个焦点、最多两个关注项和可展开明细汇总当天状态；速记草稿切换时不丢失，PC 与移动端分别记录已读。 |
| 📲 | **Native Android App 原生安卓端** | Scan QR to sync via local network. SQLite bi-directional synchronization, offline availability, and PC as SSOT. <br>原生安卓端，PC作为唯一真相源，局域网扫码双向同步，断网也可用。 |
| 🛜 | **Web Drop 极传** | WebRTC P2P cross-device file transfer. 3-tier fallback (loopback → P2P → relay) with zero-knowledge AES-GCM E2EE. <br>基于 WebRTC 的点对点多端文件传输，零知识证明加密，不经由云服务器缓存。 |
| 🩺 | **Doctor 自检自愈** | Health checks across API connection, SQLite integrity, sandboxes, and Sidecars. One-click auto-fix and rollback. <br>全面自检网络、数据库锁、环境依赖，遇到异常一键回滚配置、解锁数据库，零折腾。 |

---

## 🏗️ Technical Architecture / 技术底座

- **Desktop Shell**: **Tauri v2** (Rust) — native system calls, secure Sandbox, memory footprint ~50MB.
- **Frontend**: **Vue 3** + **Vite 6** — elegant, responsive CSS variables, light/dark mode auto-sync.
- **Local DB**: **SQLite** via `rusqlite` — localized storage, FTS5 full-text indexing, multi-table schema.
- **P2P Transport**: `webrtc-rs` — end-to-end encrypted direct tunnels.
- **HTTP Gateway**: `axum` local server — manages WeChat CDN Webhooks on `127.0.0.1:3721`.
- **Credential Storage**: System OS-level **Keychain / Stronghold** integration — no plaintext API keys in config files.

---

## 🚀 Quick Start for Developers / 开发者快速上手

### Prerequisites / 前置要求
- **Node.js** 18+ (with `npm`)
- **Rust** Toolchain (`rustc`, `cargo` 1.71+)
- **Windows C++ Build Tools** (via Visual Studio Installer)

### 1. Clone & Install / 克隆与依赖安装
```bash
git clone https://github.com/bobbik1984/bob-agent.git
cd bob-agent
npm install
```

### 2. Development Mode / 开发模式
```bash
# Start Vite frontend dev server and compile Rust in debug mode
# 启动前端开发服务器并实时热编译 Rust 后端
npm run dev:tauri
```

### 3. Build & Release / 编译发布安装包
To bundle the application, **always use the project-standard Bootstrapper pipeline** instead of default Tauri NSIS:
编译和打包发布版本，请**严格运行官方一键发布脚本**，该脚本会编译主程序并嵌套打包为暗黑风格安装器：

```bash
# Run one-click bootstrapper build pipeline
# 运行一键发布脚本（输出到 dist-release/）
scripts\release.bat
```

**Output Artifacts / 生成产物** (`dist-release/`):
- `bob-installer.exe` — Custom borderless dark-themed installer (~25MB).
- `bob-agent-portable.zip` — Portable green version (~38MB).

---

## 📂 Project Structure / 项目结构

```
bob-agent/
├── relay/                         # VPS-only Node.js Relay source of truth (not bundled in clients)
│   ├── src/server.js              # relay.bobbik.org production protocol source
│   └── tests/                     # Relay compatibility and fault-injection tests
├── src-tauri/src/                  # Rust backend / Rust 后端源码
│   ├── main.rs                     # Tauri entry point / 程序入口
│   ├── lib.rs                      # App configuration, DB connection, Tray & IPC
│   ├── llm.rs                      # LLM Client (reqwest SSE streaming + Tool Calling)
│   ├── mcp.rs                      # MCP stdio client JSON-RPC 2.0 implementation
│   ├── kg.rs                       # SQLite-based Knowledge Graph engine
│   ├── evolution.rs                # Self-evolution memory core
│   ├── dream.rs                    # Nightly Dream compaction & SOUL engine
│   ├── capture.rs                  # Reliable Capture envelope, journal & recovery boundary
│   ├── goal.rs                     # Goal Mode (Maker-Checker loop)
│   ├── goal_runtime/               # Persistent bounded Goal execution, evidence and recovery
│   ├── web_drop.rs                 # WebRTC P2P direct transmission
│   ├── wechat/                     # Mobile Android sync channel adapter
│   ├── doctor.rs                   # System health self-diagnostics & auto-fix
│   └── keychain.rs                 # Secure Keychain credential encryption
├── src/                            # Vue 3 frontend / Vue 3 前端源码
│   ├── App.vue                     # Sidebar & core shell framework
│   ├── views/                      # Interactive panels (Chat, Inbox, Graph, Settings)
│   └── locales/                    # i18n dictionaries (zh-CN.json / en-US.json)
├── installer/                      # Bootstrapper Tauri installer project / 引导安装器工程
├── skills/                         # Pre-bundled cognitive tool definitions / 内置认知工具集
├── docs/                           # Documentation / 相关架构设计与开发文档
└── website/                        # promotional showcase site / 宣发主页静态源码
```

---

## 📖 Related Documents / 更多文档

- [FEATURES.md](docs/FEATURES.md) — Detailed feature list / 功能列表与说明
- [USER_GUIDE.md](docs/USER_GUIDE.md) — User user manual / 用户操作手册
- [PRODUCT_VISION.md](docs/PRODUCT_VISION.md) — Product north star, users, pain and differentiation / 产品愿景
- [BOB_EVOLUTION_ROADMAP.md](docs/BOB_EVOLUTION_ROADMAP.md) — Ordered development phases and quality gates / 分阶段演进路线
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — Under-the-hood design details / 技术架构与设计决策
- [UI_SYSTEM.md](docs/UI_SYSTEM.md) — Shared responsive layout and button hierarchy / 跨终端布局与按钮层级
- [GOAL_RUNTIME.md](docs/GOAL_RUNTIME.md) — Goal, execution DAG, verification and personalized evolution target architecture / Goal、执行图、验证与个性化进化目标架构

---

## 📝 License

Distributed under the **MIT License**. See [LICENSE](LICENSE) for details.
