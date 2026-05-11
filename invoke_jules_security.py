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

    # 2. 从本地读取刚才生成的安全加固任务文档
    task_file_path = r"D:\OneDrive\Learning\Code\Gemini\bob-agent\jules_task_security.md"
    try:
        with open(task_file_path, "r", encoding="utf-8") as f:
            prompt_content = f.read()
    except Exception as e:
        print(f"Failed to read task file: {e}")
        return

    prompt = f"""
    Please execute the following security hardening tasks for the bob-agent repository.
    
    {prompt_content}
    
    ⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
    1. Do NOT start any local development servers.
    2. Before you conclude the task and submit the Pull Request, you MUST explicitly verify your changes by using git diff or reading the files you were supposed to modify.
    3. DO NOT modify any frontend files under `src/`. ONLY touch backend Electron files as specified.
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
            
            print("Dispatching security hardening task to Jules...")
            session = await client.call_tool(
                "create_session",
                {
                    "prompt": prompt,
                    "source": target_source,
                    "starting_branch": "main",
                    "title": "Backend Security Hardening (P0/P1 Fixes)",
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
