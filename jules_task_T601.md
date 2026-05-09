# 派发给 Jules 的任务：T-601 ~ T-602 工具系统与 Function Calling 执行循环

**说明**：请复制以下提示词，直接在 [Jules 网页端](https://jules.google.com) 中发起新的 Session（选择 `bob-agent` 仓库）：

---

**【请复制以下内容发送给 Jules】**

任务目标：为 `bob-agent` 实现核心的 Agentic Tool 系统架构，完成 `T-601` 和 `T-602`。

上下文要求：
1. 目标目录：`electron/tools/` 和 `electron/services/llm-client.js`。
2. 我已经创建了基础骨架文件 `electron/tools/base.js` (BaseTool) 和 `electron/tools/registry.js` (ToolRegistry)。
3. 目前 `bob-agent` 的 LLM 调用是单纯的文本聊天（在 `electron/services/llm-client.js` 中），它需要被改造为标准的“思考 -> 决定调用工具 -> 执行工具 -> 返回结果给大模型”的 Function Calling 执行循环 (Agent Execution Loop)。

具体任务细节：
1. 完善 `electron/tools/registry.js` 中的 `scanDirectory(dirPath)` 逻辑，使其能够递归或单层扫描目录下的 js 文件，并使用 `require` 动态加载，实例化所有继承自 `BaseTool` 的类并注册到 Map 中。
2. 在 `electron/main.js` 中初始化 `ToolRegistry` 单例，从 `db.getConfig('externalSkillsDir')` 读取外部扩展技能路径（如果有），并执行 `.init()`。
3. 实现第一个核心内部技能：创建 `electron/tools/built-in/fs-tool.js`，包含 `ListDirectoryTool` 和 `ReadFileTool` 两个工具（逻辑可基于现有的 `electron/services/file-reader.js` 和 `fs` 模块）。
4. 大改 `electron/services/llm-client.js` 的 `chatStream` / `chat` 方法：
   - 使用 registry 获取的工具 schema 注入到 OpenAI 的 `tools` 字段。
   - 当收到 `tool_calls` 响应时，解析执行对应的工具（通过 `registry.executeTool`），然后将 `tool_message` 追加到上下文再次请求 LLM，形成完整的执行循环（限制最大循环次数为 5 次以防死循环）。
5. 保持流式返回（如果是基于大模型的 final text 响应，依然返回给前端渲染）。
6. 完成后，创建一个 PR 到 `bob-agent` 仓库。

代码规范要求：
- 严格处理所有异常，在工具执行失败时返回带有错误信息的 tool_message 给大模型，让它自我纠正。
- 确保工具系统的安全性，不要使用危险的 eval 等操作。
- 日志输出应当清晰，方便在主进程终端跟踪工具执行情况。

⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
1. Do NOT start any long-running local development servers.
2. Before you conclude the task and submit the Pull Request, you MUST explicitly verify your changes by using git diff or reading the files you were supposed to modify. 
3. Ensure you have actually written the code to disk. Writing the logic in your thoughts or commit messages is NOT enough.
