# bob-agent 参考资料索引

> 本文件列出所有对开发 bob-agent 有价值的外部参考路径。
> 另一个 Agent 接手时，请先阅读这些文件获取上下文。

---

## 核心参考项目

### TodoList（代码移植源）

bob-agent 的"智能收件箱"功能直接移植自此项目。

| 文件 | 路径 | 用途 |
|------|------|------|
| **Parser 引擎** | `D:\OneDrive\Learning\Code\Gemini\todolist\src\core\parser.py` | 自然语言→结构化事件的 LLM 调用逻辑。**核心资产，需移植为 JS** |
| **日历同步** | `D:\OneDrive\Learning\Code\Gemini\todolist\src\core\calendar_sync.py` | Microsoft 365 Graph API CRUD。**需移植为 JS** |
| **通知推送** | `D:\OneDrive\Learning\Code\Gemini\todolist\src\core\notifier.py` | Discord 推送逻辑（bob-agent 改为桌面通知，仅参考结构） |
| **数据模型** | `D:\OneDrive\Learning\Code\Gemini\todolist\src\models\event.py` | Pydantic 事件 Schema。bob-agent 的 events 表需与此对齐 |
| **数据库** | `D:\OneDrive\Learning\Code\Gemini\todolist\src\models\database.py` | SQLite 表定义和 CRUD。**表结构需保持一致** |
| **前端 UI** | `D:\OneDrive\Learning\Code\Gemini\todolist\static\app.js` | WeekTimeline 周时间轴组件（26K JS）。**可直接复用** |
| **前端样式** | `D:\OneDrive\Learning\Code\Gemini\todolist\static\style.css` | 暗色主题样式（23K CSS）。timeline 相关样式可参考 |
| **配置** | `D:\OneDrive\Learning\Code\Gemini\todolist\src\config.py` | 环境变量读取模式参考 |
| **AGENTS.md** | `D:\OneDrive\Learning\Code\Gemini\todolist\AGENTS.md` | TodoList 的编码规范 |

### CodeRunner（理念借鉴源）

bob-agent 的上下文管理和流式输出方案来自此项目。

| 文件 | 路径 | 用途 |
|------|------|------|
| **Micro-compact** | `D:\OneDrive\Learning\Code\Gemini\code_runner\src\core\micro_compact.py` | 工具返回 >8K 的零 LLM 截断逻辑。**需移植为 JS** |
| **流式输出** | `D:\OneDrive\Learning\Code\Gemini\code_runner\src\core\react_loop.py` | `run_stream()` 的 SSE 流式实现参考 |
| **Prompt 工程** | `D:\OneDrive\Learning\Code\Gemini\code_runner\src\core\prompt_builder.py` | System Prompt 构建模式参考 |
| **产品手册** | `D:\OneDrive\Learning\Code\Gemini\code_runner\references\可视化智能体产品手册与开发计划.md` | Tauri 桌面化方案分析（最终选了 Electron） |

### DeepSeek-TUI（工程理念参考）

bob-agent 不使用任何 TUI 代码，但其架构设计值得参考。

| 文件 | 路径 | 用途 |
|------|------|------|
| **工具系统** | `D:\OneDrive\Learning\Code\Gemini\DeepSeek-TUI\crates\tools\src\lib.rs` | `ToolCapability` 六维标签设计参考 |
| **审批引擎** | `D:\OneDrive\Learning\Code\Gemini\DeepSeek-TUI\crates\execpolicy\` | `Auto/Suggest/Required` 三档策略参考 |
| **多 Provider** | `D:\OneDrive\Learning\Code\Gemini\DeepSeek-TUI\config.example.toml` | 多模型供应商配置格式参考（508行详尽示例） |
| **架构图** | `D:\OneDrive\Learning\Code\Gemini\DeepSeek-TUI\DEPENDENCY_GRAPH.md` | 14 crate 分层依赖参考 |
| **AGENTS.md** | `D:\OneDrive\Learning\Code\Gemini\DeepSeek-TUI\AGENTS.md` | Session 管理、子智能体设计、长会话防崩策略 |

---

## 全局基础设施

| 资源 | 路径 | 用途 |
|------|------|------|
| **模型注册表** | `D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\model-registry\references\unified_model_registry.json` | 查询所有 LLM 的 API 端点、model_id、定价。**严禁硬编码模型名** |
| **API 注册表** | `D:\OneDrive\Learning\Code\Gemini\Assistant\common\knowledge\skills\api-registry\references\unified_api_registry.json` | 查询 Microsoft Graph 等非 LLM API 的 Key 和端点 |
| **项目注册表** | `D:\OneDrive\Learning\Code\Gemini\project_registry.yaml` | 全局项目索引（bob-agent 已注册） |
| **全局 AGENTS.md** | `D:\OneDrive\Learning\Code\Gemini\AGENTS.md` | 跨项目编码规范（UTF-8 铁律等） |

---

## 设计分析文档（本次对话产出）

这些文档记录了 bob-agent 的设计决策过程，存储在 Antigravity 会话目录中：

| 文档 | 路径 | 内容 |
|------|------|------|
| **TUI 对照分析** | `C:\Users\xm_bo\.gemini\antigravity\brain\42b23e6b-622b-44d5-87a4-2efebe0e73a8\tui_vs_coderunner.md` | DeepSeek-TUI vs CodeRunner 深度架构对比 |
| **同步策略** | `C:\Users\xm_bo\.gemini\antigravity\brain\42b23e6b-622b-44d5-87a4-2efebe0e73a8\bob_agent_sync_strategy.md` | bob-agent × TodoList 数据/代码同步策略决策 |
| **产品规划** | `C:\Users\xm_bo\.gemini\antigravity\brain\42b23e6b-622b-44d5-87a4-2efebe0e73a8\lightdesk_product_spec.md` | 完整产品规划（三大支柱 + 技术方案 + 竞品对比） |

---

## 技术栈文档

| 技术 | 官方文档 | 说明 |
|------|---------|------|
| Electron | https://www.electronjs.org/docs | 桌面应用框架 |
| Vue 3 | https://vuejs.org/guide/ | 前端框架 |
| Vite | https://vite.dev/guide/ | 构建工具 |
| OpenAI SDK (Node) | https://github.com/openai/openai-node | LLM API 客户端（兼容 DeepSeek） |
| better-sqlite3 | https://github.com/WiseLibs/better-sqlite3 | SQLite 绑定 |
| electron-builder | https://www.electron.build/ | 打包工具 |
| @azure/msal-node | https://github.com/AzureAD/microsoft-authentication-library-for-js | Microsoft 365 OAuth |
| mammoth | https://github.com/mwilliamson/mammoth.js | .docx 读取 |
| xlsx | https://github.com/SheetJS/sheetjs | .xlsx 读取 |
| pdf-parse | https://github.com/nickolasburr/pdf-parse | .pdf 读取 |
| marked | https://github.com/markedjs/marked | Markdown 渲染 |
| highlight.js | https://highlightjs.org/ | 代码高亮 |
