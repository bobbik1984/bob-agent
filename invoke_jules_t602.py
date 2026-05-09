import asyncio
from fastmcp import Client
from jules_mcp import mcp
import os
import json

async def dispatch_task():
    # 1. 安全地加载并注入 Jules API Key
    registry_path = r"D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\api-registry\references\unified_api_registry.json"
    try:
        with open(registry_path, "r", encoding="utf-8") as f:
            registry = json.load(f)
        os.environ["JULES_API_KEY"] = registry["ai_and_agents"]["jules"]["bearer_token"]
    except Exception as e:
        print(f"Failed to load Jules API Key: {e}")
        return

    # 2. 极其详尽的 Prompt
    prompt = """
    Task ID: T-602
    Task Title: Agentic Tool System - Function Calling Loop + Built-in Tools

    ## Context
    bob-agent is an Electron + Vue 3 desktop AI assistant. The codebase was SIGNIFICANTLY updated
    since the last task (T-601). You MUST read the current files before making ANY changes.

    The main branch now contains:
    - `electron/services/llm-client.js`: Updated with `pricing` data per model, `getPricing()` method,
      `stream_options: { include_usage: true }` for token tracking, and `usage`/`model` fields in
      the `done` chunk yield. The PROVIDERS config now has pricing fields.
    - `electron/main.js`: Updated IPC handlers for `llm:chat` and `llm:vision` that return
      `{ content, thinking, usage, model, pricing }`. Also has strengthened system prompt.
    - `electron/tools/registry.js`: Existing scaffolding for tool registration.
    - `electron/tools/built-in/`: Directory for built-in tool implementations.

    ## Your Mandatory First Step
    Before writing ANY code, you MUST:
    1. `cat electron/services/llm-client.js` — Read the ENTIRE current file
    2. `cat electron/main.js` — Read the ENTIRE current file
    3. `cat electron/tools/registry.js` — Read the current scaffolding
    4. `ls electron/tools/built-in/` — Check what exists

    ## Task Requirements

    ### A. Refactor `chatStream()` in `llm-client.js` to support Function Calling loop
    - Add a `while` loop (max 5 iterations) around the stream creation
    - On each iteration, check if the LLM returned `tool_calls` in the delta chunks
    - If tool_calls are present: accumulate them, execute via `this.registry.executeTool()`,
      append tool results as `role: 'tool'` messages, and loop again
    - If no tool_calls: break and yield `done`
    - **CRITICAL**: You MUST preserve the existing `stream_options: { include_usage: true }` parameter
    - **CRITICAL**: You MUST preserve the `usage` and `model` fields in the `done` yield:
      `yield { type: 'done', content: '', usage: usageData, model: modelId }`
    - **CRITICAL**: You MUST preserve the `usageData` capture logic (`if (chunk.usage) { usageData = chunk.usage; }`)
    - **CRITICAL**: You MUST preserve `let usageData = null;` initialization
    - The constructor must accept an optional `registry` parameter: `constructor({ ..., registry = null })`
    - Store it as `this.registry = registry`
    - When building request params, if `this.registry` has schemas, add `tools` to the request

    ### B. Update `electron/main.js`
    - Import ToolRegistry from `./tools/registry.js`
    - In `initServices()`, create a ToolRegistry instance, scan `electron/tools/built-in/`,
      and pass the registry to LLMClient constructor
    - **CRITICAL**: Do NOT modify the IPC handler return values. They must keep returning
      `{ content, thinking, usage, model, pricing }` exactly as they are now.
    - **CRITICAL**: Do NOT modify `buildSystemPrompt()`. Leave it exactly as-is.

    ### C. Update `electron/tools/registry.js`
    - Implement `scanDirectory(dirPath)` to recursively find and require `.js` tool files
    - Each tool file exports `{ name, description, parameters, execute }`
    - Implement `getAllSchemas()` to return OpenAI-compatible tool definitions
    - Implement `executeTool(name, params)` to find and run the tool's execute function

    ### D. Implement built-in tools in `electron/tools/built-in/`
    Create these tool files:
    1. `list-directory.js` — Lists files in a directory (uses Node.js `fs`)
    2. `read-file.js` — Reads a text file's content (uses Node.js `fs`, encoding='utf-8')
    3. `web-search.js` — Searches the web using Tavily API
       - Read the API key from the environment variable `TAVILY_API_KEY`
       - Use Node.js native `fetch()` to POST to `https://api.tavily.com/search`
       - Parameters: `{ query: string, max_results?: number }`
       - Return the search results as a JSON string

    ## Code Standards
    - All file I/O MUST use `encoding: 'utf-8'` explicitly
    - Use `const` and `let`, never `var`
    - Add JSDoc comments for public methods
    - Error handling: wrap tool execution in try/catch, return error as string

    ⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
    1. Do NOT start any long-running local development servers (like `npm run dev`, `vite`).
    2. Before concluding, you MUST run `git diff` to verify your changes are actually on disk.
    3. Do NOT modify any Vue components, CSS, or frontend files.
    4. Do NOT modify the PROVIDERS config or pricing data in llm-client.js.
    5. Do NOT remove or modify `getPricing()`, `getAvailableModels()`, or `visionStream()` methods.
    """

    # 3. 唤醒 FastMCP 客户端并派单
    print("Connecting to Jules MCP Server...")
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
            print("ERROR: Target repository 'bob-agent' not found in Jules!")
            return

        print(f"Found target source: {target_source}")

        print("Dispatching task to Jules...")
        session = await client.call_tool(
            "create_session",
            {
                "prompt": prompt,
                "source": target_source,
                "starting_branch": "main",
                "title": "T-602: Agentic Tool System (Function Calling + Web Search)",
                "require_plan_approval": False,
            },
        )

        if hasattr(session, "content"):
            text = session.content[0].text
            try:
                data = json.loads(text)
                print(f"Jules Session created! URL: {data.get('url', 'Unknown URL')}")
            except Exception:
                safe_text = text.encode("gbk", errors="replace").decode("gbk")
                print("Session Info:", safe_text)
        else:
            safe_text = str(session).encode("gbk", errors="replace").decode("gbk")
            print("Session Info:", safe_text)

if __name__ == "__main__":
    asyncio.run(dispatch_task())
