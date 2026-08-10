<p align="center">
  <img src="public/bob_logo.svg" alt="Bob Agent Logo" width="120" style="margin-bottom: 10px;" />
  <h1 align="center">Bob Agent</h1>
  <p align="center">
    <strong>A zero-setup personal execution system that learns how you work</strong><br/>
    <strong>零设置、懂你的、以结果为单位工作的个人执行系统</strong>
  </p>
  <p align="center">
    <img src="https://img.shields.io/badge/version-v0.7.8-blue?style=flat-square" alt="Version" />
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

> **Verified outcome closure with minimum user friction.**<br>
> 在尽量少打扰用户的前提下，持续提高“用户认可、证据可查、失败可恢复”的任务闭环率。

当前主线是先建立可靠的 **Capture → 分类 → 跨端一致 → Memory/Dream** 数据闭环，再在可信数据底座上推进 Goal Runtime 与最小 DAG。现有 `Goal Mode` 仍是 Maker–Checker 三轮重试原型，不等同于完整的持久 Goal Runtime。阶段顺序与完成门槛见 [Bob Evolution Roadmap](docs/BOB_EVOLUTION_ROADMAP.md)。

---

## ✨ Core Capabilities / 核心能力

| | Feature / 功能 | Description / 描述 |
|:---:|---|---|
| 🎐 | **Quick Capture 极速交互** | Global hotkey (`Ctrl+Shift+B`), drag-and-drop file/screenshot processing — capture ideas without breaking your workflow. <br>全局快捷键一键唤起、文件/截图拖拽交互、托盘气泡常驻，不打断手头工作。 |
| 🎛️ | **Model Hub 模型中心** | Auto-discover 40+ models from any OpenAI-compatible provider. Main/Clerk dual-model roles. Offline Sidecar (GGUF). <br>自动检索任何兼容 API 服务商的 40+ 模型，内置 Maker/Clerk 双角色协作，支持本地离线大模型运行。 |
| 🔌 | **MCP Client 认知工具** | Native stdio JSON-RPC 2.0 client managing MCP Server child processes. Dynamic tool discovery & conflicts-free namespacing. <br>原生 stdio 异步管理 MCP 子进程，自动扫码注册认知技能，完全兼容开源 MCP 生态。 |
| 🧠 | **Memory & Dream 记忆进化** | Current: Soul, Session, Wiki, structured corrections and nightly compaction. Direction: separate identity, preference, episodic, procedural and project memory, then learn from verified Goal outcomes. <br>当前已具备三层记忆、结构化纠错与夜间整理；下一阶段将身份、偏好、经历、策略和项目状态分离，并从 Goal 的真实结果中学习。 |
| 🕸️ | **SQLite Graph 知识脑图** | Native SQLite graph database (nodes + edges). BFS sub-graph extraction and interactive Vis.js canvas visualization. <br>基于本地 SQLite 构建轻量知识网络，自动提取实体关系，并通过 Vis.js 脑图画布进行拖拽交互。 |
| 🎯 | **Goal Loop 闭环原型** | Current: high-budget Maker–Checker loop, deterministic assertions and up to three retries. Direction: automatic Goal compilation, durable state, task DAG, evidence-bound completion and restart recovery. <br>当前具备高预算执行、确定性断言与 Clerk 三轮验收；完整 Goal Runtime、任务图、持久恢复和证据闭环仍是下一阶段开发目标。 |
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
- [GOAL_RUNTIME.md](docs/GOAL_RUNTIME.md) — Goal, execution DAG, verification and personalized evolution target architecture / Goal、执行图、验证与个性化进化目标架构

---

## 📝 License

Distributed under the **MIT License**. See [LICENSE](LICENSE) for details.
