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

    # 2. 编写极其详尽的 Prompt 给云端 Agent
    prompt = """
    任务目标：完成 bob-agent 项目中 Sprint 6 剩余的三大核心架构任务（Renderer Sandbox 与 MCP 协议接入）。

    上下文要求：
    1. 目标目录：`electron/` 和 `src/` (前端)。
    2. 当前正在将 bob-agent 打造成一个高安全性的本地 AI Agent 客户端，同时需要接入 Model Context Protocol (MCP) 标准，以利用外部服务器的工具库。

    具体任务细节：

    1. [T-631] 启用 Renderer Sandbox (`sandbox: true`) 并完成安全剥离：
       - 修改 `electron/main.js`，在 `BrowserWindow` 的 `webPreferences` 中强制开启 `sandbox: true`。
       - 彻底审查 `electron/preload.js`，确保其中没有直接暴露 Node.js API (如 `fs`, `path`, `child_process` 等) 给 Renderer。
       - 如果前端 `src/` 目录中有任何直接使用 Node API 的地方，请重构为通过 `window.electronAPI` IPC 调用。

    2. [T-614] MCP 客户端接入 (Node 端核心实现)：
       - 引入官方 `@modelcontextprotocol/sdk`。你可能需要通过 npm install 安装此依赖。
       - 在 `electron/services/mcp-client.js` 中实现一个 MCP 客户端类，用于通过 `stdio` 方式连接外部的 MCP Servers。
       - 实现逻辑：读取本地的 `mcp/config.json`，启动这些 Server，调用其 `tools/list` 接口，并将返回的 tools 动态注册到我们的 `ToolRegistry` (`electron/tools/registry.js`) 中。

    3. [T-615] MCP 配置 UI (前端 Vue 实现)：
       - 在前端新增一个简单的 Vue 组件或在已有的设置界面中新增一块区域，用于管理 MCP 服务器配置。
       - 允许用户进行 CRUD 操作：添加、修改、删除 MCP Server 配置 (主要是 `server_name` 和 `command`/`args`)。
       - 修改配置后，应通过 IPC 写入到 `mcp/config.json` 中，并触发 Node 端 MCP Client 重新加载服务器。

    代码规范要求：
    - Vue 代码请遵循现有的深色工业风格 (Industrial Dark)。
    - Node.js 代码请注意捕获异常，不要让 MCP Client 的崩溃影响主进程的稳定性。

    ⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
    1. Do NOT start any long-running local development servers.
    2. Before you conclude the task and submit the Pull Request, you MUST explicitly verify your changes by using git diff. 
    3. Ensure you have actually written the code to disk.
    """

    # 3. 唤醒 FastMCP 客户端并派单
    print("Connecting to Jules MCP Server...")
    async with Client(mcp) as client:
        # 获取所有可用的 Github 代码库源
        sources_result = await client.call_tool("get_all_sources", {})
        
        if hasattr(sources_result, "content"):
            sources = json.loads(sources_result.content[0].text)
        else:
            sources = sources_result
            
        target_source = None
        for s in sources:
            if "bob-agent" in s.get("name", "").lower():
                target_source = s["name"]
                break
        
        if not target_source:
            print("ERROR: Target repository source not found in Jules! Make sure 'bob-agent' is imported in Jules console.")
            return
            
        print(f"Found target source: {target_source}")
        
        # 派发任务并创建 Session
        print("Dispatching task to Jules...")
        session = await client.call_tool(
            "create_session",
            {
                "prompt": prompt,
                "source": target_source,
                "starting_branch": "main",
                "title": "Bob-Agent Backend: Sandbox & MCP Client Implementation",
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
