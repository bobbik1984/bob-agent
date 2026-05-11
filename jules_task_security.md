# Security Hardening Task for Jules (Bob-Agent)

> **Context:** A recent architecture audit identified several critical security vulnerabilities in the `bob-agent` backend. This task document isolates the backend/Electron issues for you to resolve. **DO NOT modify anything inside the `src/` directory (Frontend Vue code)** as it has been heavily customized.

## 🎯 Objective
Harden the Electron Main process and IPC communications to resolve P0/P1 security vulnerabilities related to sandbox configuration, IPC state management, and path traversal.

## 🛠 Tasks

### 1. Enable Electron Sandbox
- **File:** `electron/main.js`
- **Action:** Find the `webPreferences` configuration for `mainWindow` and change `sandbox: false` to `sandbox: true`. Native modules like `better-sqlite3` are already contained within the Main process and expose their API via IPC, so disabling the sandbox in the Renderer is unnecessary and dangerous.

### 2. Secure Configuration IPC (`config:all`)
- **File:** `electron/main.js`
- **Action:** The `config:all` IPC handler currently sends the entire database configuration object to the Renderer. This exposes the `apiKey` to the frontend, which is a major XSS risk. Modify the `config:all` handler in `electron/main.js` to strip out `apiKey` and any other sensitive credentials before returning the object to the frontend.
- **Verification:** Ensure the frontend still receives non-sensitive configurations (like `model`, `workspaceDir`) but never the raw LLM API key.

### 3. Server-Side State Management for Privileges
- **Files:** `electron/main.js` and `electron/preload.js`
- **Action:** Currently, the Renderer process dictates the `globalFileAccess` and `agentMode` states when calling `llm:chat` or `llm:vision`. This means a compromised Renderer can arbitrarily elevate its own privileges.
- **Fix:** Move the storage and management of these two flags entirely to the Main process. The Renderer should only send requests to *toggle* or *set* these states via IPC (e.g., `ipcMain.handle('security:toggle-global-access')`), and the `llm:chat` / `llm:vision` handlers should read these states from the Main process variables or DB, not from the IPC payload.

### 4. Fix Path Traversal in File Tools
- **Files:** `electron/tools/built-in/read_file.js`, `electron/tools/built-in/write_file.js`, and the `workspace:list-dir` handler in `electron/main.js`.
- **Action:** The current path validation uses a simplistic `.startsWith()` check without a trailing path separator, which is vulnerable to traversal (e.g., `/data-secret` passes the check if workspace is `/data`).
- **Fix:** Implement robust path resolution. Ensure the resolved target path `startsWith(workspacePath + path.sep)` or is exactly the `workspacePath`. Apply this strict validation whenever `globalFileAccess` is disabled.

### 5. (Bonus) Sandbox External Tool Execution
- **File:** `electron/tools/registry.js`
- **Action:** The `scanDirectory` function dynamically uses `require()` on `.js` files found in the user-configured `externalSkillsDir`. This is arbitrary code execution. At a minimum, add a strict user confirmation mechanism or logging, though migrating to a safer sandbox execution environment is the ultimate goal. (If too complex for a quick fix, add a `TODO` comment noting the RCE risk).

## ⚠️ Constraints
- **NO FRONTEND CHANGES:** Under no circumstances should you edit `src/App.vue`, `src/views/*`, `src/components/*`, or `src/index.css`.
- **Maintain Compatibility:** Ensure that existing IPC method names remain intact so the frontend doesn't break, just change the underlying implementation and payload validation.
