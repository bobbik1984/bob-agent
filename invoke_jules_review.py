import asyncio
from fastmcp import Client
from jules_mcp import mcp
import os
import json
import time

REGISTRY_PATH = r"D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\api-registry\references\unified_api_registry.json"
TARGET_REPO_KEYWORD = "bob-agent"
BRANCH = "feature/document-export"

TASKS = [
    {
        "title": "[Audit 1/4] SetupWizard config protection",
        "prompt": """
## AUDIT ONLY - DO NOT MODIFY CODE

You are performing a read-only code audit on the `feature/document-export` branch of the bob-agent project.

### Scope
Review ONLY the file: `src/components/SetupWizard.vue`

### What changed
1. Language selection buttons were added to Step 1 (merged with theme/color selection)
2. A single `onMounted` hook now loads existing config into `tempConfig` before the wizard renders
3. An `initialSnapshot` is taken after loading, and `finishOnboarding()` uses diff-based comparison to only write fields the user actually changed
4. A `watch(step)` handler saves API Key when leaving Step 3

### Audit checklist
- [ ] Is there exactly ONE `onMounted` hook? (There was a bug with duplicate hooks)
- [ ] Does `onMounted` correctly apply the saved theme to DOM (not force 'dark')?
- [ ] Is `initialSnapshot` taken AFTER config loading completes?
- [ ] Does `finishOnboarding` correctly skip unchanged fields?
- [ ] Is the `watch(step)` for API Key saving present and correct?
- [ ] Could any race condition cause `initialSnapshot` to be empty?
- [ ] Are there any hardcoded Chinese strings that should use i18n?

### Output format
Produce a structured audit report with: PASS / WARN / FAIL for each checklist item, with brief explanation. Do NOT create a PR or modify any code.
"""
    },
    {
        "title": "[Audit 2/4] UI fixes: locale init + font stack",
        "prompt": """
## AUDIT ONLY - DO NOT MODIFY CODE

You are performing a read-only code audit on the `feature/document-export` branch of the bob-agent project.

### Scope
Review these 2 files:
1. `src/views/settings/SettingsAppearance.vue` (lines 155-175, the onMounted hook)
2. `src/index.css` (lines 293-381, font stack and UI scale rules)

### What changed
1. SettingsAppearance.vue: The `onMounted` hook was modified to NOT overwrite `locale.value` when `props.config.language` is undefined. Previously it would fallback to 'zh-CN' and force the entire app back to Chinese.
2. index.css: Font stack changed from `'Inter', 'Noto Sans SC'` to `system-ui, -apple-system, 'Segoe UI', Roboto, 'Microsoft YaHei', 'PingFang SC', sans-serif`
3. index.css: Added `html { font-size: 16px; }` as default baseline to match the 'compact' UI scale option

### Audit checklist
- [ ] In SettingsAppearance: Can `locale.value` still be accidentally overwritten to 'zh-CN' when config hasn't loaded yet?
- [ ] In SettingsAppearance: Is the fallback chain `props.config.language || locale.value || 'zh-CN'` correct?
- [ ] In index.css: Does the new font stack cover Windows, macOS, and Linux adequately?
- [ ] In index.css: Is the default `html { font-size: 16px }` consistent with the compact scale? Does it conflict with the `html, body` rule that sets `font-size: var(--text-base)`?
- [ ] Could the `html` font-size rule and the `html, body` font-size rule create a cascade conflict?

### Output format
Produce a structured audit report with: PASS / WARN / FAIL for each checklist item. Do NOT create a PR or modify any code.
"""
    },
    {
        "title": "[Audit 3/4] TodoList + completed_at DB migration",
        "prompt": """
## AUDIT ONLY - DO NOT MODIFY CODE

You are performing a read-only code audit on the `feature/document-export` branch of the bob-agent project.

### Scope
Review these files:
1. `src/components/TodoList.vue`
2. `src-tauri/src/calendar.rs` (the events table schema and update_event_status function)
3. `src/locales/zh-CN.json` and `src/locales/en-US.json` (the "todo" section only)

### What changed
1. TodoList.vue: Completed todos are now hidden by default. A toggle checkbox shows/hides them. Completion timestamps are displayed using i18n interpolation.
2. calendar.rs: Added `completed_at INTEGER DEFAULT 0` column via ALTER TABLE migration. The `system_update_event_status` function now sets `completed_at` to current unix timestamp when status becomes 'done', or resets to 0 otherwise. The SELECT query now includes `completed_at`.
3. i18n: Added `todo.show_completed` and `todo.completed_at` keys with `{time}` interpolation.

### Audit checklist
- [ ] Is the ALTER TABLE migration safe for existing databases? (It uses unwrap_or_default, but check for edge cases)
- [ ] Does the SELECT query column index (9) correctly map to `completed_at`?
- [ ] Is `completed_at` correctly returned as 0 (not null) for old records that don't have the column yet?
- [ ] In TodoList.vue: Does the filter logic correctly show all todos when `showCompleted` is true?
- [ ] In TodoList.vue: Is the `formatTime` function correct? (unix seconds * 1000 for JS Date)
- [ ] Are the i18n interpolation formats `{time}` consistent between template usage and locale definitions?
- [ ] Does unchecking a completed todo correctly reset `completed_at` to 0?

### Output format
Produce a structured audit report with: PASS / WARN / FAIL for each checklist item. Do NOT create a PR or modify any code.
"""
    },
    {
        "title": "[Audit 4/4] Screenshot + @ file attachment",
        "prompt": """
## AUDIT ONLY - DO NOT MODIFY CODE

You are performing a read-only code audit on the `feature/document-export` branch of the bob-agent project.

### Scope
Review these files:
1. `src/views/ChatView.vue` (the handleScreenshot, handleInput for @ mention, and handleMentionSelect functions)
2. `src/tauri-bridge.js` (the takeScreenshot and getClipboardImage exports)
3. `src-tauri/src/lib.rs` (the system_take_screenshot command registration)

### What changed
1. ChatView.vue: Added @ mention trigger that shows a floating menu to browse local files. Typing @ at the start of input or after a space shows the menu. Selecting a file clears the @ and attaches the file.
2. ChatView.vue: Added screenshot button that hides the window, invokes Windows Snipping Tool, waits for clipboard, then auto-pastes the captured image into `pendingImage` (WeChat-like flow, no Ctrl+V needed).
3. tauri-bridge.js: Added `takeScreenshot()` IPC call and `getClipboardImage()` using `navigator.clipboard.read()`.
4. lib.rs: Registered `system_take_screenshot` which invokes `SnippingTool.exe /clip`.

### Audit checklist
- [ ] Does the @ regex `/(?:^|\s)@$/` correctly trigger only when @ is typed at start or after whitespace? Could it interfere with typing email addresses?
- [ ] After selecting a file from the @ menu, is the @ character properly removed from the input?
- [ ] In the screenshot flow: Is there a timeout/fallback if the user cancels the snipping tool without capturing?
- [ ] Is `navigator.clipboard.read()` reliable in Tauri's WebView context? Are permissions handled?
- [ ] Does the window correctly show/hide during the screenshot flow?
- [ ] Is there error handling if SnippingTool.exe is not available on the user's Windows version?
- [ ] Could rapid repeated clicks on the screenshot button cause issues?

### Output format
Produce a structured audit report with: PASS / WARN / FAIL for each checklist item. Do NOT create a PR or modify any code.
"""
    }
]


async def dispatch_all():
    with open(REGISTRY_PATH, "r", encoding="utf-8") as f:
        registry = json.load(f)
    os.environ["JULES_API_KEY"] = registry["ai_and_agents"]["jules"]["bearer_token"]

    print("Connecting to Jules MCP Server...")
    async with Client(mcp) as client:
        # Find the target repo source
        sources_result = await client.call_tool("get_all_sources", {})
        if hasattr(sources_result, "content"):
            sources = json.loads(sources_result.content[0].text)
        else:
            sources = sources_result

        target_source = None
        for s in sources:
            if TARGET_REPO_KEYWORD in s.get("name", ""):
                target_source = s["name"]
                break

        if not target_source:
            print("ERROR: Target repository source not found!")
            return

        print(f"Found target source: {target_source}")

        # Dispatch each task with a small delay between them
        for i, task in enumerate(TASKS):
            print(f"\n--- Dispatching Task {i+1}/4: {task['title']} ---")
            session = await client.call_tool(
                "create_session",
                {
                    "prompt": task["prompt"],
                    "source": target_source,
                    "starting_branch": BRANCH,
                    "title": task["title"],
                    "require_plan_approval": False,
                },
            )
            if hasattr(session, "content"):
                text = session.content[0].text
                try:
                    data = json.loads(text)
                    print(f"Session created! URL: {data.get('url', 'N/A')}")
                except Exception:
                    safe_text = text.encode("gbk", errors="replace").decode("gbk")
                    print("Session Info:", safe_text)
            else:
                safe_text = str(session).encode("gbk", errors="replace").decode("gbk")
                print("Session Info:", safe_text)

            if i < len(TASKS) - 1:
                print("Waiting 5s before next dispatch...")
                await asyncio.sleep(5)

    print("\n=== All 4 audit tasks dispatched! ===")
    print("Please monitor progress at: https://jules.google.com")


if __name__ == "__main__":
    asyncio.run(dispatch_all())
