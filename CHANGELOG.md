# Changelog

All notable changes to bob-agent will be documented in this file.

## [1.0.0-alpha] - Unreleased

### 🏗️ Architecture (Tauri Migration)
- Migration from Electron to Tauri (Rust) initiated to drastically reduce binary size and memory usage.
- See `todo.md` for migration Sprints.

## [0.2.0-beta.1] - 2026-05-13

### 🎉 First Internal Beta Release

#### Agent System (Sprint 6)
- **21 Built-in Tools**: file I/O, web search, browser automation, weather, calendar events, knowledge base operations
- **Tool Calling Pipeline**: LLM → tool_call → execute → result feedback → continue reasoning
- **MCP Protocol**: Connect external MCP servers for extensible capabilities
- **Dual-Model Architecture**: Main model for conversation + Clerk model for background tasks (cost optimization)
- **Browser Automation**: Obscura headless browser integration via CDP WebSocket
- **Agent Mode Toggle**: Q&A mode vs. Work mode switching in toolbar

#### Knowledge Base (Sprint 5)
- **Folder KB Pipeline**: Drag folder → confirm → manifest → convert to MD → semantic index
- **Document Parsing**: .docx, .xlsx, .pdf, .csv, .md, .txt support via mammoth/xlsx/pdf-parse
- **Semantic Index**: Auto-generated `index.md` with frontmatter tags for retrieval
- **Brain Search Tool**: Search across session memories and wiki knowledge

#### Memory Engine (Sprint 6)
- **Three-Tier Memory**: Soul (static persona) → Sessions (≤7 day summaries) → Wiki (long-term knowledge)
- **Dream Engine**: Overnight consolidation — dedup, merge, pattern discovery
- **Morning Briefing**: Welcome card with overnight insights on app launch
- **Cascade Delete**: Deleting a conversation removes associated memory files

#### UI/UX Polish (Sprint 5)
- **Theme System**: Dark/Light mode with `@property` CSS variable transitions (no snapping)
- **Custom Accent Colors**: Preset palette with real-time switching
- **i18n**: Full Chinese/English bilingual support (60+ translation keys)
- **FileCard**: Rich file reference cards in chat with thumbnails and click-to-open
- **Sidebar Resize**: Drag-to-resize + Notion-style collapse button
- **Week Timeline**: 7-day event view with 0/6/12/18/24 scale

#### Security (Sprint 6.8 — Audit-Driven)
- **DOMPurify**: XSS sanitization for all Markdown rendering
- **Renderer Sandbox**: Enabled `sandbox: true`
- **IPC Filtering**: Sensitive fields stripped from config broadcasts
- **Path Traversal Fix**: read_file/write_file restricted to authorized paths
- **Web Content Isolation**: External content wrapped in `<untrusted_web_content>` XML tags

#### Infrastructure
- **Credential Store (T-520)**: OS-level encrypted storage for API keys via Electron safeStorage
- **Tool Activation Status**: Settings UI shows which tools are active/missing credentials
- **Error Logging (T-703)**: Persistent log files via electron-log (`%APPDATA%/bob-agent/logs/`)
- **Cost Tracking**: Per-conversation cost persistence in SQLite
- **Version Bump**: 0.1.0 → 0.2.0-beta.1

### Known Issues
- Chunk size warning during build (>500KB) — acceptable for beta
- No auto-update mechanism yet (planned for T-702)
