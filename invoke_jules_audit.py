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
    任务目标：全面深度审计从 Electron 迁移到 Tauri 的架构重构结果，确保系统安全、稳定、纯净。

    上下文要求：
    1. Bob-Agent 刚刚完成了一项巨大的架构重构：彻底废弃了 Electron 框架，将其完全重写为 Tauri (Rust) + Vue 3 的混合架构体系。
    2. 你需要对当前的代码库进行一次深度全局审查 (Comprehensive Code & Architecture Audit)。重点关注前后端的 IPC 桥接逻辑、Rust 引擎的健壮性以及是否存在冗余或潜在的安全漏洞。

    具体任务细节：
    1. 架构残余清理：全局扫描整个工作区，确认是否还有残存的 Electron 依赖、旧的 IPC 通信残余 (`ipcRenderer` 等)，或未废弃干净的 Node.js 原生包依赖。确保 package.json 中的构建脚本、依赖库纯净且指向 Tauri 环境。
    2. Tauri IPC 桥接与通信审查：审查 src/tauri-bridge.js 以及 src-tauri/src/lib.rs 的指令映射。检查是否存在前后端参数类型不匹配、调用后丢失错误处理 (Uncaught Errors)、或没有适当地通过 Result<T, E> 向前端冒泡异常的隐患。评估流式通信 (SSE / Tauri Events) 尤其是 llm:chunk 和 llm:stream_error 的订阅与退订机制是否存在内存/事件泄漏。
    3. Rust 引擎与业务逻辑审计：审查 src-tauri/src/llm.rs：Tool Calling 引擎的解析、流式返回的健壮性，以及防模型输出截断（如近期修复的 DSML 泄露）逻辑是否合理。审查 src-tauri/src/db.rs 与 kb_*.rs：检查文件 I/O 异常处理、并发死锁风险以及 Rust 生命周期所有权管理是否规范。安全性审查：本地文件系统的路径穿越 (Path Traversal) 漏洞是否存在？命令执行是否有限制？
    4. 前端视图与状态同步：审查 ChatView.vue 与 SettingsView.vue 的状态管理。前后端状态是否可能在某些极端情况下（例如快速多次点击发送、切换模型时网络中断）发生撕裂或失去同步。

    代码规范要求：
    - 请不要立刻开始大规模修改代码！这是一个纯审计任务。
    - 请输出一份详尽的 Audit Report，可以存放在 docs/postmortem_electron_to_tauri_audit.md 中。
    - 报告应包含：1. 高危漏洞与 Bug 列表；2. 技术债与代码异味；3. 架构优化建议；4. 具体的重构实施计划。

    ⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
    1. Do NOT start any long-running local development servers.
    2. Before you conclude the task and submit the Pull Request, explicitly verify your report is written to docs/postmortem_electron_to_tauri_audit.md.
    """

    # 3. 唤醒 FastMCP 客户端并派单
    print("Connecting to Jules MCP Server...")
    async with Client(mcp) as client:
        # 3.1 获取所有可用的 Github 代码库源 (Sources)
        sources_result = await client.call_tool("get_all_sources", {})
        
        # 兼容不同的返回格式
        if hasattr(sources_result, "content"):
            sources = json.loads(sources_result.content[0].text)
        else:
            sources = sources_result
            
        # 3.2 动态寻找当前项目对应的 Source
        target_source = None
        for s in sources:
            if "bob-agent" in s.get("name", "").lower():
                target_source = s["name"]
                break
        
        if not target_source:
            print("ERROR: Target repository source not found in Jules!")
            return
            
        print(f"Found target source: {target_source}")
        
        # 3.3 派发任务并创建 Session
        print("Dispatching task to Jules...")
        session = await client.call_tool(
            "create_session",
            {
                "prompt": prompt,
                "source": target_source,
                "starting_branch": "main",
                "title": "Tauri Migration Architecture Audit",
                "require_plan_approval": False,
            },
        )
        
        # 打印 Session URL 供用户点击监控
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
