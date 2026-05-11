# Bob-Agent Architecture Audit Report

## Executive Summary
The architecture audit of bob-agent reveals a functional and feature-rich Electron application with a Vue 3 frontend. The integration of local tools, multi-provider LLMs, and desktop APIs is implemented smoothly. However, there are significant security vulnerabilities, primarily related to IPC configurations, missing input sanitization (XSS), and exposed API keys. The application architecture is generally sound, but the main ChatView component is monolithic and would benefit from decomposition.

## Critical Issues (P0)

### 1. Renderer Exposing API Keys & Config (IPC Security)
- **File:** `electron/main.js` (Lines 371-383) and `electron/preload.js` (Lines 44-46)
- **Description:** The `config:all` and `config:get` IPC handlers expose all configuration items to the Renderer process, including the sensitive `apiKey`. If the Renderer is compromised via XSS, an attacker can steal the user's API key.
- **Severity:** P0
- **Suggested Fix:** Do not send sensitive keys to the Renderer. Filter the configuration object in `main.js` before returning it, or remove the `getAllConfig` IPC handler and only allow fetching specific non-sensitive keys.

### 2. Cross-Site Scripting (XSS) via Markdown Rendering
- **File:** `src/views/ChatView.vue` (Lines 45, 62)
- **Description:** The LLM output is parsed with `marked` and injected directly into the DOM using `v-html="renderMarkdown(msg.content)"` without any HTML sanitization. A malicious LLM or prompt injection could cause arbitrary JavaScript execution in the Renderer.
- **Severity:** P0
- **Suggested Fix:** Integrate a library like `DOMPurify` to sanitize the HTML string returned by `marked` before injecting it via `v-html`.

### 3. Renderer Controls Global File Access and Agent Mode
- **File:** `electron/main.js` (Lines 140, 185) and `electron/preload.js` (Lines 11-12)
- **Description:** The Renderer process explicitly passes `globalFileAccess` and `agentMode` to the main process for `llm:chat` and `llm:vision`. A compromised Renderer could maliciously enable global file access or YOLO mode to perform destructive operations on the user's disk.
- **Severity:** P0
- **Suggested Fix:** Maintain the state of `globalFileAccess` and `agentMode` securely in the Main process instead of trusting the Renderer to provide these critical security parameters.

### 4. Electron Sandbox Disabled Globally
- **File:** `electron/main.js` (Line 104)
- **Description:** The `webPreferences.sandbox` is explicitly set to `false`. The comment cites `better-sqlite3` as the reason, but since `better-sqlite3` is only used in the Main process, disabling the sandbox for the renderer is completely unnecessary and significantly weakens Electron's security model.
- **Severity:** P0
- **Suggested Fix:** Set `sandbox: true`. If native modules are used, they should remain exclusively in the Main process (which is currently the case) and be accessed entirely via IPC.

## Important Issues (P1)

### 1. Insecure Path Traversal Validation in Workspace API
- **File:** `electron/main.js` (Lines 291-293, 321-323)
- **Description:** The path validation uses `.startsWith()`, e.g., `targetPath.startsWith(path.resolve(workspaceRoot))`. If `workspaceRoot` is `/app/data`, an attacker requesting `/app/data-secret` would bypass the check because `/app/data-secret` starts with `/app/data`.
- **Severity:** P1
- **Suggested Fix:** Ensure a path separator is appended when doing string prefix matching, or use path validation libraries to guarantee the resolved target path is strictly a descendant of the workspace directory.

### 2. Arbitrary Code Execution via External Skills Directory
- **File:** `electron/tools/registry.js` (Lines 47-51)
- **Description:** `scanDirectory` requires `.js` files dynamically from the user-configured `externalSkillsDir`. If a user points this to a download folder or is tricked into doing so, any downloaded `.js` file will be executed in the Node.js Main process context.
- **Severity:** P1
- **Suggested Fix:** Use a sandboxed execution environment (like `vm2` or `isolated-vm`) for external tools, or strictly require explicit user approval before loading any new script from the external directory.

### 3. Server-Side Request Forgery (SSRF) Risk in Web Tools
- **File:** `electron/tools/built-in/wechat_reader.js` (Line 16)
- **Description:** The `wechat_reader` uses `fetch(url)` without validating if the URL is pointing to a local/internal IP address. It only checks `url.includes('mp.weixin.qq.com')`, which an attacker could bypass with URLs like `http://127.0.0.1/mp.weixin.qq.com`.
- **Severity:** P1
- **Suggested Fix:** Use the `URL` API to rigorously parse the URL, ensuring the protocol is `https:` and the hostname exactly matches `mp.weixin.qq.com`. Disallow requests to private IP ranges.

### 4. Monolithic Chat Component (ChatView.vue)
- **File:** `src/views/ChatView.vue` (Lines 1-840)
- **Description:** The component is over 800 lines long, handling everything from DOM events, drag-and-drop, API polling, cost calculation, markdown configuration, and tool invocation logic. This makes it difficult to maintain and test.
- **Severity:** P1
- **Suggested Fix:** Decompose into smaller sub-components (e.g., `MessageList`, `ChatInput`, `MessageBubble`) and extract business logic into composables (e.g., `useChat`, `useDragAndDrop`).

## Nice-to-Have Improvements (P2)

### 1. Console Log Pollution
- **File:** `electron/services/llm-client.js` (Lines 131, 135, 178) and `electron/tools/registry.js`
- **Description:** There are many debug `console.log` statements left in the production code, which pollutes the terminal and can potentially leak sensitive arguments during tool execution.
- **Severity:** P2
- **Suggested Fix:** Wrap logs in a debug utility or check for `!app.isPackaged` / `isDev` before logging verbose information.

### 2. Hardcoded Model Pricing
- **File:** `electron/services/llm-client.js` (Lines 13-44)
- **Description:** Model pricing is hardcoded in the `PROVIDERS` object. Pricing changes frequently, which will make this data quickly outdated.
- **Severity:** P2
- **Suggested Fix:** Fetch pricing data dynamically from an external config or API, or allow users to update it via the UI.

### 3. Dead Code
- **File:** `electron/main.js` (Line 115)
- **Description:** There is commented-out code like `// mainWindow.webContents.openDevTools();` left in the source file.
- **Severity:** P2
- **Suggested Fix:** Remove commented-out dead code.

## Security Assessment
The security posture of the application requires immediate attention. The combination of XSS vulnerabilities in the markdown renderer, exposed API keys to the renderer, and the ability for the renderer to request global file access creates a dangerous attack chain. If an LLM is poisoned to output malicious markdown, it can execute JavaScript in the renderer. This script could then read the API key, toggle global file access, and use IPC to read/write arbitrary files on the user's machine. The disabled Electron sandbox exacerbates this risk. Fixing the P0 issues should be the top priority before any public release.

## Performance Observations
- **Memory Management**: The streaming logic uses async generators correctly and handles AbortControllers gracefully, which prevents memory leaks on canceled requests.
- **Frontend State**: The Vue frontend maintains all messages in memory. For extremely long conversations, this could lead to DOM bloat and high memory usage. Virtual scrolling for the message list is not implemented but would be beneficial for performance.
- **File Reading**: The `file-reader.js` correctly imposes a `MAX_FILE_SIZE` of 500KB, preventing the application from crashing or hanging when attempting to read enormous files.

## Recommendations Summary Table

| Issue | File | Severity | Recommendation |
|---|---|---|---|
| API Key Exposure | `electron/main.js`, `electron/preload.js` | P0 | Remove sensitive keys from renderer IPC responses. |
| XSS via Markdown | `src/views/ChatView.vue` | P0 | Use DOMPurify to sanitize `v-html` input. |
| Renderer Controls File Access | `electron/main.js`, `electron/preload.js` | P0 | Move `globalFileAccess` state management to the Main process. |
| Disabled Sandbox | `electron/main.js` | P0 | Set `sandbox: true` in `webPreferences`. |
| Path Traversal in Workspace | `electron/main.js` | P1 | Fix `startsWith` check to include path separators. |
| RCE via External Skills | `electron/tools/registry.js` | P1 | Sandbox external `.js` execution or require strict opt-in. |
| SSRF in Web Tools | `electron/tools/built-in/wechat_reader.js` | P1 | Validate URL scheme and exact hostname. |
| Monolithic ChatView | `src/views/ChatView.vue` | P1 | Decompose the 800+ line Vue component. |
| Hardcoded Pricing | `electron/services/llm-client.js` | P2 | Externalize model pricing config. |
| Console Log Pollution | Multiple | P2 | Remove or guard debug logs. |
