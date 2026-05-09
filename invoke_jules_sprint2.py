"""
bob-agent — Jules 派单脚本
Sprint 2: 智能收件箱 (WeekTimeline, TodoList, ConfirmCard, InboxView)

包含多个 UI 组装与业务流打通任务。
"""

import asyncio
import json
import os

from fastmcp import Client
from jules_mcp import mcp


async def dispatch_task():
    # 1. 安全加载 Jules API Key
    registry_path = r"D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\api-registry\references\unified_api_registry.json"
    try:
        with open(registry_path, "r", encoding="utf-8") as f:
            registry = json.load(f)
        os.environ["JULES_API_KEY"] = registry["ai_and_agents"]["jules"]["bearer_token"]
    except Exception as e:
        print(f"Failed to load Jules API Key: {e}")
        return

    # 2. 详尽的任务 Prompt
    prompt = """
任务目标：为 bob-agent (Electron + Vue 3 桌面应用) 完成 Sprint 2 (智能收件箱) 的核心任务。

上下文要求：
1. 项目是一个 Windows 桌面 AI 助手，前端使用 Vue 3 + Vite。
2. 数据持久层 SQLite 和 IPC 桥接已在 Sprint 1 就绪。
3. 请参考 docs/ARCHITECTURE.md 和 AGENTS.md 了解项目结构。

─────────────────────────────────────────────────
任务 1: IPC 通道扩展 (为 T-206 准备)
─────────────────────────────────────────────────
目标文件：electron/main.js, electron/preload.js

为了支持 Todo 状态切换，你需要：
1. 在 electron/main.js 中添加 `calendar:update-status` handler：
   调用 `db.updateEventStatus(id, status)` 并在成功时返回 `{ ok: true }`。
2. 在 electron/preload.js 中暴露该接口：
   `updateEventStatus: (id, status) => ipcRenderer.invoke('calendar:update-status', id, status)`

─────────────────────────────────────────────────
任务 2: 事件确认卡片 (T-202)
─────────────────────────────────────────────────
目标文件：src/components/ConfirmCard.vue (新建)

创建一个 Vue 组件，用于展示通过自然语言解析出来的日程/待办事件信息：
1. Props: `event` (对象，包含 title, type, start_time, end_time, location, confidence 等)。
2. UI 应该优美地展示这些信息，并提供 "确认保存" 和 "取消" 两个按钮。
3. Emits: `confirm`, `cancel`。

─────────────────────────────────────────────────
任务 3: 对话→事件检测 (T-204)
─────────────────────────────────────────────────
目标文件：src/views/ChatView.vue

打通将文本解析为事件的业务流：
1. 在文本输入框区添加一个快捷按钮（例如 "📅 解析为日程"）。
2. 点击后，提取 `inputText.value` 调用 `window.electronAPI.parseEvent(text)`。
3. 由于 `parseEvent` 会连接 LLM 耗时几秒，期间需展示 Loading 状态。
4. 解析成功后，弹出一个 Modal 层或在消息流中展示新建的 `ConfirmCard.vue`。
5. 当用户点击 ConfirmCard 的 confirm 时，调用 `window.electronAPI.confirmEvent(event)` 写入 SQLite。

─────────────────────────────────────────────────
任务 4: 周时间轴 (T-205)
─────────────────────────────────────────────────
目标文件：src/components/WeekTimeline.vue (新建)

1. 这是 Sprint 2 最大的工程挑战。请首先使用 read_file 工具读取：
   - `D:\OneDrive\Learning\Code\Gemini\todolist\static\app.js` (找到 renderWeekUI 函数，了解比例计算逻辑)
   - `D:\OneDrive\Learning\Code\Gemini\todolist\static\style.css` (找到 WEEK TIMELINE 相关的 CSS)
2. 将 TodoList 中 Vanilla JS 的 renderWeekUI 逻辑完美重构为 Vue 3 组件。
3. Props 接收 `weekEvents` 数组。
4. 在横向的时间轴上准确计算每个事件的 startPct 和 spanPct，并渲染带样式的 block。

─────────────────────────────────────────────────
任务 5: 待办清单组件 (T-206)
─────────────────────────────────────────────────
目标文件：src/components/TodoList.vue (新建)

1. Props 接收 `todos` 数组。
2. 展示待办列表，每项前方有复选框。
3. 点击复选框时，调用我们在任务 1 中新加的 `window.electronAPI.updateEventStatus(id, 'done')`。
4. 样式复用 TodoList 项目或 bob-agent 暗色设计规范。

─────────────────────────────────────────────────
任务 6: 收件箱视图组装 (T-207)
─────────────────────────────────────────────────
目标文件：src/views/InboxView.vue (新建), src/App.vue

1. 创建 InboxView.vue 作为完整页面。
2. 在 onMounted 时调用 `window.electronAPI.listEvents()` 加载数据。
3. 过滤出 `type === 'event'` 的事件传入 WeekTimeline；`type === 'todo'` 的传入 TodoList。
4. 在 App.vue 的侧边栏添加 "📥 智能收件箱" 导航项，并将视图路由打通。

⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
1. 在修改文件前，一定要读取现有文件以了解其状态。
2. 特别是对于 WeekTimeline，由于涉及数学计算（8点到24点的时间映射），千万不要自己凭空猜测，必须先读取 TodoList 的参考代码。
3. 提交前，运行 `npx vite build` 确保没有 Vue 编译错误。
"""

    # 3. 唤醒 FastMCP 客户端并派单
    print("Connecting to Jules MCP Server...")
    async with Client(mcp) as client:
        # 获取所有可用的 GitHub 代码库源
        sources_result = await client.call_tool("get_all_sources", {})

        if hasattr(sources_result, "content"):
            sources = json.loads(sources_result.content[0].text)
        else:
            sources = sources_result

        # 寻找 bob-agent 仓库
        target_source = None
        for s in sources:
            if "bob-agent" in s.get("name", ""):
                target_source = s["name"]
                break

        if not target_source:
            print("ERROR: bob-agent repository source not found in Jules!")
            print("Available sources:", [s.get("name") for s in sources])
            return

        print(f"Found target source: {target_source}")

        # 派发任务
        print("Dispatching task to Jules...")
        session = await client.call_tool(
            "create_session",
            {
                "prompt": prompt,
                "source": target_source,
                "starting_branch": "main",
                "title": "[Sprint 2] 智能收件箱 (Inbox, Timeline, Parser UI)",
                "require_plan_approval": False,
            },
        )

        if hasattr(session, "content"):
            safe_text = session.content[0].text.encode("gbk", errors="replace").decode("gbk")
            print("Session Info:", safe_text)
        else:
            safe_text = str(session).encode("gbk", errors="replace").decode("gbk")
            print("Session Info:", safe_text)


if __name__ == "__main__":
    asyncio.run(dispatch_task())
