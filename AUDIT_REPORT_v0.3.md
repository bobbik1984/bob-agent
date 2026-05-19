# bob-agent v0.3.0 Architecture & Security Audit Report

## Executive Summary
This report presents a comprehensive, read-only audit of the `bob-agent` project, following its recent migration to Tauri V2 + Vue 3 and the integration of a WeChat bot system. The audit focused on evaluating logical integrity between Rust and Vue components, security implementations (particularly IPC and API key management), UX/Design consistency based on established `AGENTS.md` guidelines, and general code quality.

While the new Rust backend demonstrates solid structural improvements (e.g., proper error handling), several high-priority security concerns and design inconsistencies remain.

---

## 1. Logical Integrity (Rust Backend & Vue Frontend)
The core interactions between the Rust backend and Vue frontend are mediated by `src/tauri-bridge.js`, which securely wraps Tauri's `invoke` API.

**Findings:**
*   **WeChat Integration:** The WeChat bot system utilizes a local HTTP server (`src-tauri/src/http_api.rs`) listening on `127.0.0.1:3721` to receive messages. The session and state are managed explicitly in the Rust backend via `WechatState` (`src-tauri/src/wechat/mod.rs`), which is registered as a managed Tauri state (`src-tauri/src/lib.rs` line 294). This ensures memory safety across threads.
*   **Error Handling:** The codebase correctly adheres to the `AGENTS.md` rule forbidding `unwrap()` or `panic!()` in command logic. Tools like the Reconciler (`src-tauri/src/outbox.rs`) strictly return `Result` types mapped to JSON strings for the LLM to process.

**Severity:** Low / Information Only

---

## 2. Security Assessment
Tauri v2 enforces a strict capability system, and the project aims to secure sensitive user data locally. However, key implementation gaps exist.

**Findings:**
*   **API Key Management (Critical):** Despite `AGENTS.md` specifying that API keys must be encrypted using `tauri-plugin-stronghold`, the current implementation in `src-tauri/src/llm.rs` (`set_api_key`) directly inserts plaintext API keys into the `config.json` file. The Stronghold implementation is marked as "Planned" but is not yet active.
*   **Path Traversal Mitigations (High):** File system access via tools (e.g., `tool_write_file` and `tool_append_file`) routes through `resolve_write_path` (`src-tauri/src/tools.rs`). This function attempts to prevent path traversal by checking `path.contains("..")` and routing relative paths (`!p.is_absolute()`) to the safe `get_data_dir()` or `get_wiki_dir()`. However, the lack of strict canonicalization for all file inputs leaves a theoretical attack surface for edge cases.
*   **Tauri Capabilities (Medium):** The `src-tauri/capabilities/default.json` explicitly whitelists only `core:default`, `dialog:default`, and `global-shortcut:*`. It properly excludes the dangerous `shell:default` (arbitrary command execution) capability, limiting the blast radius of any potential RCE vulnerability.

**Recommendations:**
1.  **Immediate:** Implement `tauri-plugin-stronghold` to encrypt the `config.json` payload or move API keys to the system keychain.
2.  **Short-term:** Strengthen `resolve_write_path` using `fs::canonicalize` to guarantee that the final resolved path is strictly within the allowed workspace boundary.

---

## 3. User Experience & Design Consistency
The UI relies on strict CSS variable usage and specific layout directives defined in `AGENTS.md`.

**Findings:**
*   **i18n Hardcoded Fallbacks (Medium):** The `AGENTS.md` explicitly bans using logical OR fallbacks for translation keys (e.g., `{{ $t('key') || 'Fallback' }}`). Violations of this rule were discovered in:
    *   `src/components/ConfirmCard.vue` (line 10)
    *   `src/components/KBEstimateCard.vue` (line 9)
    *   `src/views/SettingsView.vue` (lines 154, 372)
*   **Icon Alignment (Low):** The codebase strictly uses Flexbox properties (`display: flex; align-items: center;`) for icon alignments as required by `AGENTS.md`. A global search confirmed zero usage of the forbidden `vertical-align` property in the Vue templates.

**Recommendations:**
1.  Refactor Vue templates to rely entirely on the `$t()` function without JavaScript logical OR fallbacks.

---

## 4. Code Quality & Performance

**Findings:**
*   **Component Bloat (High):** The primary view component, `src/views/ChatView.vue`, consists of exactly 2293 lines of code. It manages chat states, markdown rendering, tool call UI interactions, and scrolling logic. This monolithic structure is a significant bottleneck for testing and future feature additions.
*   **Legacy Dependencies (Low):** The `package.json` file still retains multiple dependencies from the previous Electron era (e.g., `cheerio`, `mammoth`, `openai`, `pdf-parse`, `ws`, `xlsx`, `officeparser`), inflating the `node_modules` size unnecessarily since Rust now handles most file parsing.

**Recommendations:**
1.  **Short-term:** Decompose `ChatView.vue` by extracting logic into Vue composables (e.g., `useChat.ts`) and UI elements into smaller components (e.g., `ChatMessage.vue`, `ToolAction.vue`).
2.  **Short-term:** Prune unused npm packages matching the legacy Electron stack.