import asyncio
from fastmcp import Client
from jules_mcp import mcp
import os
import json

REGISTRY_PATH = r"D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\api-registry\references\unified_api_registry.json"

SESSION_IDS = [
    "13193590901609681101",
    "6972522456367434361",
    "16737171875228192449",
    "8822146224514429255",
]

TITLES = [
    "[Audit 1/4] SetupWizard config protection",
    "[Audit 2/4] UI fixes: locale init + font stack",
    "[Audit 3/4] TodoList + completed_at DB migration",
    "[Audit 4/4] Screenshot + @ file attachment",
]

async def check_sessions():
    with open(REGISTRY_PATH, "r", encoding="utf-8") as f:
        registry = json.load(f)
    os.environ["JULES_API_KEY"] = registry["ai_and_agents"]["jules"]["bearer_token"]

    async with Client(mcp) as client:
        # List available tools first
        tools = await client.list_tools()
        tool_names = [t.name for t in tools]
        print("Available tools:", tool_names)

        for sid, title in zip(SESSION_IDS, TITLES):
            print(f"\n{'='*60}")
            print(f"Session: {title}")
            print(f"ID: {sid}")
            print(f"{'='*60}")
            try:
                result = await client.call_tool("get_session", {"session_id": sid})
                if hasattr(result, "content"):
                    text = result.content[0].text
                    try:
                        data = json.loads(text)
                        print(f"Status: {data.get('status', 'unknown')}")
                        if data.get('plan'):
                            print(f"\n--- Plan ---")
                            print(data['plan'][:2000])
                        if data.get('diff_summary'):
                            print(f"\n--- Diff Summary ---")
                            print(data['diff_summary'][:2000])
                        # Print any other useful fields
                        for key in ['resolution', 'error', 'result', 'output', 'report']:
                            if data.get(key):
                                print(f"\n--- {key} ---")
                                val = data[key]
                                print(str(val)[:3000])
                    except json.JSONDecodeError:
                        safe = text.encode("gbk", errors="replace").decode("gbk")
                        print(safe[:3000])
                else:
                    safe = str(result).encode("gbk", errors="replace").decode("gbk")
                    print(safe[:3000])
            except Exception as e:
                print(f"Error: {e}")

if __name__ == "__main__":
    asyncio.run(check_sessions())
