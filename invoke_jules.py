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

    # 2. 编写 Prompt 给云端 Agent
    prompt = """
    任务目标：重构 bob-agent 的内置工具系统 (T-601 和 T-604)

    上下文要求：
    1. 目标目录：`electron/tools/` 和 `src/views/`
    2. 请严格按照工作区根目录下的 `jules_refactor_plan.md` 的步骤执行。我已经为你准备好了所有的架构设计，你只需要执行具体的代码重构。

    具体任务细节：
    1. 参考 `electron/tools/built-in/create_event.js` 的写法，将 `electron/tools/built-in/` 目录下所有的纯对象工具转换为继承自 BaseTool 的类。
    2. 移除 `registry.js` 中的 `DynamicTool` 动态包装逻辑。
    3. 完成 `search_events.js` 和 `update_event.js` 的编写。
    4. 移除 `electron/main.js` (buildSystemPrompt) 中要求输出 `<calendar_event>` 的强制规则。
    5. 移除 `src/views/ChatView.vue` (430行附近) 的 `eventRegex` 解析逻辑。
    6. 更新 `todo.md` 将 T-601 和 T-604 打上 [x]。

    ⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
    1. Do NOT start any long-running local development servers.
    2. Before concluding, verify your changes by using git diff or reading the files.
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
            
            print("Dispatching task to Jules...")
            session = await client.call_tool(
                "create_session",
                {
                    "prompt": prompt,
                    "source": target_source,
                    "starting_branch": "main",
                    "title": "T-601 BaseTool Refactor",
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
