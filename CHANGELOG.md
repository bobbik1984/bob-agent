# Changelog

All notable changes to bob-agent will be documented in this file.

## [0.32.2] - 2026-06-23

### 🐛 Bug Fixes & Improvements
- **[WebRTC] rustls CryptoProvider Panic 修复**：为 `rustls` 添加了 `rustls::crypto::ring::default_provider().install_default()` 初始化，防止在创建 `WebRtcTransport` 阶段由于未指定默认加密算法导致的进程崩溃。
- **[Web Drop] 前端哈希提取兼容性**：强化了 `index.html` 中的 hash 解析逻辑，能够安全地处理带有 `.`、`|` 的锚点参数，防止错误切分导致解密密钥遗失。
- **[Web Drop] 强缓存绕过**：在生成的分享链接后附加 `?v=2` 参数，强制使得微信内置浏览器和其他移动端浏览器跳过强缓存，加载最新版前端页面。
- **[UX/WeChat] 双排版兼容链接输出**：重构了 `tools.rs` 中文件分享工具生成的提示词逻辑。现在 AI 会提供纯净的 URL 超链接供电脑端直接点击，同时提供纯文本代码块内的 URL 供移动端无损长按复制，彻底解决了微信自动对带有标点符号的 Markdown URL 错误截断导致手机端无法打开的问题。
- **[Network] 双向对称NAT穿透与中继验证**：成功验证了在跨越多重代理隧道（发送端 Oracle VPN，接收端 Proton VPN）所产生的极端双重对称 NAT 网络环境下，底层传输通道能够稳定并自动 Fallback 到 VPS 部署的 TURN Server 中继（Coturn），保障了极端网络连通性。

## [0.3.2] - 2026-06-11

### 🐛 Bug Fixes
- **[Security] Content-Security-Policy (CSP) 放行本地资源**：修改了 `index.html` 的 CSP 限制，将 `http://127.0.0.1:*` 加入到 `img-src` 白名单中。修复了 WebView2 引擎静默拦截本地 HTTP 文件服务请求，导致所有基于 `file:///` 转换的本地图片和图标无法渲染的重大 Bug。
- **[WeChat] wxid 路由修复**：LLM 调用 `send_wechat_file` 时传入用户明文微信号（如 `wobushuai872834`），而 ilink CDN API 要求加密格式的 wxid（`xxx@im.wechat`），导致 CDN 上传失败。现在 `execute_tool` 接收 `from_user` 上下文，自动将非加密 wxid 覆盖为会话中的真实加密 ID。
- **[Icon] 桌面图标裁切修复**：原始 `icon.png` 横向内容几乎撑满画布（左边距 1px/0.2%，右边距 0px），缩小至 32x32 后因像素舍入和 Windows 渲染内边距导致左右被截。现在四周统一保留 ~12% 安全边距（61px），所有衍生尺寸（png/ico/icns）已重新生成。

### 🔧 Changes
- `useChat.js`: 清理了 Markdown 渲染管道（正则转换与 DOMPurify 阶段）的冗余调试日志，保持生产环境控制台整洁。
- `tools.rs`: `execute_tool()` 和 `execute_tool_inner()` 签名新增 `from_user: Option<&str>` 参数
- `tools.rs`: `send_wechat_file` 分支增加 wxid 格式检测 + `from_user` 覆盖逻辑
- `llm.rs`: `stream_internal()` 工具并行执行时透传 `from_user` 到每个工具调用
- `src-tauri/icons/`: 全部 16 个图标资产（png/ico/icns）用加边距的源图重新生成

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
