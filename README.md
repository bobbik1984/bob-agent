<p align="center">
  <img src="resources/bob_logo.svg" alt="Bob Agent Logo" width="120" style="margin-bottom: 10px;" />
  <h1 align="center">Bob Agent</h1>
  <p align="center">
    <strong>Your Ghost Co-Pilot on the Desktop</strong><br/>
    <strong>隐于桌面，使命必达的本地 AI 私人秘书</strong>
  </p>
  <p align="center">
    <img src="https://img.shields.io/badge/version-v0.4.0-blue?style=flat-square" alt="Version" />
    <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License" />
    <img src="https://img.shields.io/badge/platform-Windows_|_Android-0078D6?style=flat-square&logo=windows" alt="Platform" />
    <img src="https://img.shields.io/badge/Tauri-v2-FFC131?style=flat-square&logo=tauri" alt="Tauri v2" />
    <img src="https://img.shields.io/badge/Rust-%23000000?style=flat-square&logo=rust" alt="Rust" />
    <img src="https://img.shields.io/badge/Vue_3-4FC08D?style=flat-square&logo=vuedotjs&logoColor=white" alt="Vue 3" />
  </p>
</p>

---

## 📖 Introduction / 项目简介

**Bob Agent** is a privacy-first, self-evolving AI desktop & mobile secretary built on **Tauri v2 + Rust**. Bob lives in your system tray on PC and runs natively on your Android device. He weaves AI into your daily workflow—local file access, multi-model orchestration, extensible MCP tools, WeChat bridge, E2EE file transfer, and a native SQLite knowledge graph—all without a single byte of your private data leaving your machine.

**Bob Agent** 是基于 **Tauri v2 + Rust** 构建的「隐私优先、智能演化」跨端私人秘书。在 PC 端他常驻系统托盘，在手机端他是轻量前哨站。通过打通本地文件、多模型协作、MCP 协议扩展、极传与本地 SQLite 知识图谱——所有敏感数据永不出本地，极致安全。

---

## ✨ Core Capabilities / 核心能力

| | Feature / 功能 | Description / 描述 |
|:---:|---|---|
| 🎐 | **Quick Capture 极速交互** | Global hotkey (`Ctrl+Shift+B`), drag-and-drop file/screenshot processing — capture ideas without breaking your workflow. <br>全局快捷键一键唤起、文件/截图拖拽交互、托盘气泡常驻，不打断手头工作。 |
| 🎛️ | **Model Hub 模型中心** | Auto-discover 40+ models from any OpenAI-compatible provider. Main/Clerk dual-model roles. Offline Sidecar (GGUF). <br>自动检索任何兼容 API 服务商的 40+ 模型，内置 Maker/Clerk 双角色协作，支持本地离线大模型运行。 |
| 🔌 | **MCP Client 认知工具** | Native stdio JSON-RPC 2.0 client managing MCP Server child processes. Dynamic tool discovery & conflicts-free namespacing. <br>原生 stdio 异步管理 MCP 子进程，自动扫码注册认知技能，完全兼容开源 MCP 生态。 |
| 🧠 | **3-Tier Memory 记忆进化** | 3-tier memory (Soul → Session → Wiki). Compaction engine runs nightly to clean, summarize, and cold-condense facts. <br>三层记忆架构。后台 Dream 压缩线程在闲置时自动运行，做事实蒸馏、冷凝与去重，形成个人本地 Wiki。 |
| 🕸️ | **SQLite Graph 知识脑图** | Native SQLite graph database (nodes + edges). BFS sub-graph extraction and interactive Vis.js canvas visualization. <br>基于本地 SQLite 构建轻量知识网络，自动提取实体关系，并通过 Vis.js 脑图画布进行拖拽交互。 |
| 🎯 | **Goal Mode 死磕闭环** | Maker-Checker execution loop with 50-round tool calling budget. Clerk auto-evaluates output; auto-retry up to 3× until PASS. <br>面向复杂任务的自动循环机制，Clerk 模型严审工具执行结果，不达正确目标誓不罢休。 |
| 📲 | **WeChat Gateway 微信桥接** | Scan QR to pair. Remote control sessions, link prefetch, and AES-128 CDN encrypted file transfer. <br>扫码配对备用微信，在户外直接用微信向电脑投喂文件、查询日程、接收主动提醒。 |
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
- `bob-agent-portable.zip` — Portable green version (~15MB).

---

## 📂 Project Structure / 项目结构

```
bob-agent/
├── src-tauri/src/                  # Rust backend / Rust 后端源码
│   ├── main.rs                     # Tauri entry point / 程序入口
│   ├── lib.rs                      # App configuration, DB connection, Tray & IPC
│   ├── llm.rs                      # LLM Client (reqwest SSE streaming + Tool Calling)
│   ├── mcp.rs                      # MCP stdio client JSON-RPC 2.0 implementation
│   ├── kg.rs                       # SQLite-based Knowledge Graph engine
│   ├── evolution.rs                # Self-evolution memory core
│   ├── dream.rs                    # Nightly Dream compaction & SOUL engine
│   ├── goal.rs                     # Goal Mode (Maker-Checker loop)
│   ├── web_drop.rs                 # WebRTC P2P direct transmission
│   ├── wechat/                     # WeChat bridge channel adapter
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
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — Under-the-hood design details / 技术架构与设计决策

---

## 📝 License

Distributed under the **MIT License**. See [LICENSE](LICENSE) for details.
