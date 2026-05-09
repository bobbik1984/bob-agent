---
name: invoke-jules
description: Guide for invoking Google Jules asynchronously to perform autonomous coding tasks via the FastMCP integration.
keywords: [jules, mcp, fastmcp, async task, delegation, cloud agent, co-pilot, autonomous coding]
---

# 🤖 Invoke Jules (Cloud AI Co-pilot)

## 📌 触发场景 (When to use)
当你（AI 代理）判断当前用户的需求极其复杂、包含大量的枯燥编码、重构、或者可以通过多线程并行开发来节约时间时，你应当主动向用户提议：**"我们可以把这个任务打包派发给云端的 Jules 去处理。"**
一旦用户同意，你需要使用本技能提供的代码架构，编写一个脚本来唤醒 Jules。

---

## 🔐 前置安全要求 (Prerequisites)
**绝对不要硬编码 JULES_API_KEY！**
在调用 Jules 之前，你必须从系统的全局 API 注册表中读取密钥，并将其注入到环境变量中，否则 MCP 客户端将拒绝连接。

注册表路径：
`D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\api-registry\references\unified_api_registry.json`

JSON 路径为：`["ai_and_agents"]["jules"]["bearer_token"]`

---

## 🛠️ 标准调用蓝图 (Implementation Blueprint)

你可以直接在目标工程目录下创建一个形如 `invoke_jules.py` 的脚本，复制并根据项目需求微调以下标准模板。

```python
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
    prompt = \"\"\"
    任务目标：[在这里描述宏观目标，例如：重构色彩命名服务]

    上下文要求：
    1. 目标目录：src/example/
    2. [详细说明需要读取哪些文件，当前的项目逻辑是什么]

    具体任务细节：
    1. [步骤 1]
    2. [步骤 2]

    代码规范要求：
    - [例如：不要修改 main.py]
    - [例如：使用绝对路径，捕捉异常等]

    ⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
    1. Do NOT start any long-running local development servers (like `npm run dev`, `vite`, `python -m http.server`) because you cannot visually see the browser rendering anyway. It only creates useless log files and distracts you.
    2. Before you conclude the task and submit the Pull Request, you MUST explicitly verify your changes by using git diff or reading the files you were supposed to modify. 
    3. Ensure you have actually written the code to disk. Writing the logic in your thoughts or commit messages is NOT enough.
    \"\"\"

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
        # 请修改下方的 "your-project-name"，例如 "shades-of-city"
        target_source = None
        for s in sources:
            if "your-project-name" in s.get("name", ""):
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
                "starting_branch": "main", # 默认从 main 分支拉取
                "title": "Your Task Title Here", # 在云端 UI 显示的任务名称
                "require_plan_approval": False, # 如果设为 True，Jules 在写代码前会暂停等待用户网页端确认
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
```

## 📝 派发后的标准流程
1. 运行该脚本 `python invoke_jules.py`。
2. 从控制台输出中提取 `url`（例如：`https://jules.google.com/session/123456789`）。
3. 告诉用户：“任务已派发，请点击上述链接在云端监控 Jules 的进度。当它显示 Ready for review 并生成 PR 后，请在网页端点击 Merge，然后告诉我，我会立刻使用 `git pull` 拉取它的劳动成果并进行后续组装。”
