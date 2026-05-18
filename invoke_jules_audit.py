import asyncio
from fastmcp import Client
from jules_mcp import mcp
import os
import json

async def dispatch_task():
    registry_path = r"D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\api-registry\references\unified_api_registry.json"
    try:
        with open(registry_path, "r", encoding="utf-8") as f:
            registry = json.load(f)
        os.environ["JULES_API_KEY"] = registry["ai_and_agents"]["jules"]["bearer_token"]
    except Exception as e:
        print(f"Failed to load Jules API Key: {e}")
        return

    prompt_path = r"C:\Users\xm_bo\.gemini\antigravity\brain\c7f38ecf-9a4d-4b5d-b301-e711d7f4c9a4\jules_audit_prompt.md"
    with open(prompt_path, "r", encoding="utf-8") as f:
        prompt = f.read()

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
            print("ERROR: Target repository source not found in Jules!")
            return
            
        print(f"Found target source: {target_source}")
        
        print("Dispatching task to Jules...")
        session = await client.call_tool(
            "create_session",
            {
                "prompt": prompt,
                "source": target_source,
                "starting_branch": "main",
                "title": "Comprehensive Audit - Bob-Agent",
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
