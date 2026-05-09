# 派发给 Jules 的任务：T-406 Microsoft 365 日历同步移植

**说明**：由于 FastMCP 接口的 validation 发生变动（Invalid Argument），请复制以下提示词，直接在 [Jules 网页端](https://jules.google.com) 中发起新的 Session（选择 `bob-agent` 仓库）：

---

**【请复制以下内容发送给 Jules】**

任务目标：完成 `bob-agent` 的 T-406 任务：Microsoft 365 日历同步模块（将 Python 代码移植为 Node.js）。

上下文要求：
1. **目标目录**：`electron/services/`
2. **源文件参考**：你可以从 `../todolist/` 项目中找到现有的 `calendar_sync.py` 逻辑（如果你无法跨仓库读取，你可以参考标准的 Microsoft Graph API 流程：OAuth2 获取 Token -> GET `/me/events` -> 映射到本地 SQLite 格式）。
3. `bob-agent` 的定位是桌面应用，日历同步在主进程 (Node.js) 中运行。
4. 需要创建 `electron/services/calendar.js` 并实现类似的读取/同步日历事件功能。
5. 使用 `@azure/msal-node` 进行身份验证（需添加依赖）。需要将这些模块与现有的 `db.js` 结合起来，确保云端日历能和本地 `events` 表打通。
    
具体任务细节：
1. 在 `electron/services/calendar.js` 中用 JavaScript (Node.js) 实现 Graph API 同步逻辑。
2. 在 `electron/main.js` 中注册对应的 IPC handler (如 `calendar:sync`)，供渲染进程后续调用。
3. 更新 `package.json`，如果需要引入新的依赖（如 `@azure/msal-node`、`@microsoft/microsoft-graph-client`），请自动执行 `npm install`。
4. 确保代码风格符合本项目约定，不使用 TypeScript，直接使用现代 JS。
5. 任务完成后，创建一个 PR 到 `bob-agent`。

代码规范要求：
- 不要修改前端 UI 界面的内容，只完成 Electron 后端的服务与通信接口。
- 确保向后兼容和静默错误降级（比如用户在设置里没有配置 API Key 或者未授权，则静默跳过，不要引发应用崩溃）。

⚠️ ANTI-HALLUCINATION & WORKFLOW CONSTRAINTS (CRITICAL):
1. Do NOT start any long-running local development servers.
2. Before you conclude the task and submit the Pull Request, you MUST explicitly verify your changes by using git diff or reading the files you were supposed to modify. 
3. Ensure you have actually written the code to disk. Writing the logic in your thoughts or commit messages is NOT enough.
