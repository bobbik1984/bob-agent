# bob-agent — AI Desktop Assistant / AI 桌面私人秘书

> A zero-config, privacy-first AI assistant for Windows. All data stays local.
> 
> 一个零配置、隐私优先的 Windows AI 桌面助手。所有数据存储在本地。

## ✨ Features / 功能亮点

| Feature | Description |
|---------|-------------|
| 💬 **Multi-Model Chat** | 40+ models auto-discovered (DeepSeek, OpenAI, Qwen, Doubao, GLM, Kimi, MiniMax...) with SSE streaming & Markdown rendering |
| 🤖 **Agent + Tool Calling** | 12 Rust-native tools (file I/O, web search, weather, calendar, knowledge base...) with 5-round auto-invocation |
| 📸 **Vision** | Paste/drop images for AI analysis |
| 📅 **Calendar** | Natural language → event creation, drag-and-drop scheduling on weekly timeline |
| 📁 **Knowledge Base** | Drag a folder → auto-index → full-text search across docs (.docx/.xlsx/.pdf/.md) |
| 🔧 **Outbox Config** | AI can self-configure (API keys, model roles) via a sandboxed write-validate-apply pipeline |
| 🌙 **Dream Engine** | Session memory consolidation and morning briefing |
| 🎨 **Theme System** | Dark/Light mode with smooth CSS transitions + custom accent colors |
| 🌍 **i18n** | Full Chinese/English bilingual support |
| 🔌 **Skill System** | Internal `skills/` + user-configurable external skill directories |

## 🚀 Quick Start / 快速开始

### For Developers / 开发者

```bash
# Prerequisites: Node.js 18+, Rust toolchain, Visual Studio C++ Build Tools

# Install dependencies
npm install

# Development mode (Vite hot-reload + Rust compile)
npm run dev:tauri

# Production build (~15MB installer)
npm run build:tauri

# Rust check only
cd src-tauri && cargo check

# Run tests
npm test
```

### First-Time Setup / 首次配置

1. 启动后进入 **设置 → API 密钥管理**
2. 填入至少一个模型供应商的 API Key（如 DeepSeek）
3. 返回对话界面，开始聊天！

> 💡 也可以直接在对话中告诉 Bob："帮我配好这个 Key: sk-xxx"，他会通过 Outbox 系统自主完成配置。

## 🏗️ Tech Stack / 技术栈

| Component | Technology |
|-----------|------------|
| Desktop Shell | **Tauri v2** (Rust) |
| Frontend | Vue 3 + Vite 6 |
| LLM Communication | `reqwest` + SSE stream parser (Rust native) |
| Database | SQLite (`rusqlite`, bundled) |
| Tool Engine | 12 Rust-native tools + OpenAI Function Calling protocol |
| File Parsing | `pdf-extract` + `calamine` (xlsx) + `quick-xml` (docx) |
| Web Scraping | `reqwest` + `scraper` (DOM extraction) |
| Memory | Three-tier engine (Soul → Sessions → Wiki) |
| Security | Path whitelist engine + Outbox validation firewall + Tool audit log |
| Process Control | `tauri-plugin-single-instance` + System Tray + Global shortcut (`Ctrl+Shift+B`) |

## 📂 Project Structure / 项目结构

```
bob-agent/
├── src-tauri/                   # Rust backend (all system capabilities)
│   ├── src/
│   │   ├── main.rs              # Tauri entry point
│   │   ├── lib.rs               # Config + DB init + IPC registration + Tray
│   │   ├── llm.rs               # LLM engine (SSE + Tool Calling loop)
│   │   ├── tools.rs             # 12 native tools + dispatcher + audit log + path whitelist
│   │   ├── calendar.rs          # Event/todo management (SQLite)
│   │   ├── outbox.rs            # Declarative config engine (AI self-config)
│   │   ├── filesystem.rs        # File read/scan/track
│   │   ├── web.rs               # Web scraping (reqwest + scraper)
│   │   ├── plugins.rs           # Skill/plugin scanner
│   │   ├── dream.rs             # Dream engine V1
│   │   ├── sidecar.rs           # Offline model process management
│   │   ├── kb_extractor.rs      # Knowledge base file extraction
│   │   └── kb_indexer.rs        # Knowledge base index builder
│   ├── Cargo.toml               # Rust dependencies
│   └── tauri.conf.json          # App config (window, bundle, CSP)
│
├── src/                         # Vue 3 frontend (shared renderer)
│   ├── tauri-bridge.js          # Adapter: window.electronAPI → invoke()
│   ├── App.vue                  # Root + sidebar navigation
│   ├── views/
│   │   ├── ChatView.vue         # Chat + Vision + Tool Calling display
│   │   ├── InboxView.vue        # Calendar (weekly timeline + todos)
│   │   └── SettingsView.vue     # Settings (models, keys, workspace)
│   ├── components/              # WeekTimeline, TodoList, SetupWizard...
│   └── locales/                 # zh-CN.json / en-US.json
│
├── skills/                      # Built-in skills (shipped with app)
├── data/                        # ⛔ .gitignore — user private data
│   ├── memory/                  # Hot session summaries (≤7 days)
│   └── wiki/                    # Cold knowledge base
├── docs/                        # Architecture, user guide, archives
├── todo.md                      # Roadmap (Milestones 1-10)
└── progress.yaml                # Progress tracking
```

## 🔧 Configuration / 配置

All configuration is managed through the Settings UI. No manual file editing required.

所有配置均通过设置界面管理，无需手动编辑文件。

- **Model Hub**: Auto-discovers 40+ models, assign Main/Clerk roles
- **API Keys**: Stored in local config (Stronghold encryption planned)
- **Workspace**: Configure wiki directory, tracked folders, external skills
- **Theme**: Dark/Light with custom accent colors
- **Language**: 中文 / English

## 📝 License

MIT

## 🔗 Related Projects / 相关项目

- [CodeRunner](../code_runner) — Autonomous AI development platform (DAG engine)
- [Assistant](../Assistant) — Shared knowledge base & skill system
