import asyncio
from fastmcp import Client
from jules_mcp import mcp
import os
import json

async def dispatch_task():
    # 1. 加载并注入 Jules API Key
    registry_path = r"D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\api-registry\references\unified_api_registry.json"
    try:
        with open(registry_path, "r", encoding="utf-8") as f:
            registry = json.load(f)
        os.environ["JULES_API_KEY"] = registry["ai_and_agents"]["jules"]["bearer_token"]
    except Exception as e:
        print(f"Failed to load Jules API Key: {e}")
        return

    # 2. 编写 Prompt 给云端 Agent — 纯 Audit 模式，只输出报告不修改代码
    prompt = """
    ## Task: Full Architecture Audit of bob-agent (READ-ONLY — NO CODE CHANGES)

    You are performing a comprehensive code quality and architecture audit of the bob-agent Electron desktop application. 
    **You must NOT modify any source files. You must NOT create Pull Requests with code changes.**
    Your ONLY deliverable is a single Markdown report file: `AUDIT_REPORT.md` in the project root.

    ### What to Audit

    #### 1. Electron Security (CRITICAL)
    - Review `electron/main.js` and `electron/preload.js` for IPC security.
    - Check if `contextIsolation`, `nodeIntegration`, and `sandbox` are correctly configured.
    - Assess whether the `preload.js` exposes overly broad APIs to the renderer process.
    - Check for any `shell.openExternal()` without URL validation.

    #### 2. LLM Client Architecture (`electron/services/llm-client.js`)
    - Evaluate the streaming implementation: error handling, abort logic, memory leaks from unclosed streams.
    - Assess the tool call loop: is there proper protection against infinite loops?
    - Review the multi-provider abstraction: is it clean and extensible?
    - Check if API keys are handled securely (never logged, never sent to renderer).
    - Evaluate the `thinking` and `reasoning_effort` parameter injection for DeepSeek models.

    #### 3. Tool System (`electron/tools/`)
    - Review `registry.js`: is the dynamic tool scanning robust? Are errors handled gracefully?
    - Audit each built-in tool in `electron/tools/built-in/` for:
      - Input validation and sanitization
      - Error handling completeness
      - File system access safety (path traversal attacks?)
      - Consistent export format
    - Special attention to `wechat_reader.js` and `browser_automation.js` (newer tools).

    #### 4. Frontend Architecture (`src/views/ChatView.vue`)
    - Is the component too large / doing too much? Should it be decomposed?
    - Review state management approach.
    - Check for memory leaks in event listeners or watchers.
    - Assess accessibility and responsive design quality.

    #### 5. Build & Dependencies
    - Review `package.json` for outdated or vulnerable dependencies.
    - Check if `electron-builder` config is production-ready.
    - Evaluate the dev workflow (`scripts/wait-for-vite.js`, concurrently setup).

    #### 6. Code Quality & Patterns
    - Consistent error handling patterns across the codebase.
    - Proper use of `encoding: 'utf-8'` for all file I/O (Windows requirement).
    - Console.log pollution assessment: are there too many debug logs?
    - Dead code identification.

    ### Output Format
    Create `AUDIT_REPORT.md` with the following structure:
    ```
    # Bob-Agent Architecture Audit Report
    ## Executive Summary
    ## Critical Issues (P0)
    ## Important Issues (P1)  
    ## Nice-to-Have Improvements (P2)
    ## Security Assessment
    ## Performance Observations
    ## Recommendations Summary Table
    ```

    Each issue should include:
    - File path and line number(s)
    - Description of the problem
    - Severity (P0/P1/P2)
    - Suggested fix (description only, do NOT implement)

    ⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
    1. Do NOT modify any existing source files. Your ONLY output is AUDIT_REPORT.md.
    2. Do NOT start any dev servers or build processes.
    3. READ every file thoroughly before making observations. Do not guess at file contents.
    4. Before concluding, verify AUDIT_REPORT.md exists and is well-formatted by reading it back.
    """

    # 3. 唤醒 FastMCP 客户端并派单
    print("Connecting to Jules MCP Server...")
    try:
        async with Client(mcp) as client:
            sources_result = await client.call_tool("get_all_sources", {})
            
            if hasattr(sources_result, "content"):
                sources = json.loads(sources_result.content[0].text)
            else:
                sources = sources_result
                
            target_source = None
            for s in sources:
                if "bob-agent" in s.get("name", ""):
                    target_source = s["name"]
                    break
            
            if not target_source:
                print("ERROR: Target repository bob-agent not found in Jules!")
                return
                
            print(f"Found target source: {target_source}")
            
            print("Dispatching audit task to Jules...")
            session = await client.call_tool(
                "create_session",
                {
                    "prompt": prompt,
                    "source": target_source,
                    "starting_branch": "main",
                    "title": "Full Architecture Audit (Read-Only)",
                    "require_plan_approval": False,
                },
            )
            
            if hasattr(session, "content"):
                text = session.content[0].text
                try:
                    data = json.loads(text)
                    print(f"Jules Session created! URL: {data.get('url', 'Unknown URL')}")
                except Exception:
                    print("Session Info:", text.encode("gbk", errors="replace").decode("gbk"))
            else:
                print("Session Info:", str(session).encode("gbk", errors="replace").decode("gbk"))
    except Exception as e:
        print(f"Jules invocation failed: {e}")

if __name__ == "__main__":
    asyncio.run(dispatch_task())
