# Bob-Agent 技能可用性清单 (Skill Index)

本文档整理自 `Assistant/common/knowledge/skills` 目录下的 54 个现有技能，针对 **Bob-Agent (桌面端)** 的执行环境进行了可用性分类。

通过此清单，可以明确哪些技能 Bob-agent 能直接理解并使用，哪些需要通过额外的后端开发（原生化或接入 MCP）才能获得支持。

---

## 🟢 第一类：纯"大脑"技能（直接可用）
这类技能本质上是高质量的 Prompt 模板或工作流约束。Bob-agent 只需读取它们的文档（或将路径告知大模型），大模型就能理解并立即执行。

| 技能名称 | 领域 | 说明 |
| :--- | :--- | :--- |
| `brainstorming` | 思考/决策 | 多维度头脑风暴分析框架（SWOT、PEST 等） |
| `document-structurer` | 文档处理 | 麦肯锡金字塔原理，结构化长文本重组 |
| `frontend-design` | 编码/设计 | 高质量前端界面代码生成规范 |
| `mckinsey-consultant` | 思考/决策 | 麦肯锡顾问式问题解决与分析框架 |
| `requirement-analyst` | 编码/管理 | 编码前的需求分析与意图探索 |
| `research-planner` | 研究/规划 | 深度调研前的结构化大纲生成框架 |
| `semantic-distiller` | 文本处理 | 超长文本的高密度语义提取与摘要 |
| `travel_planner` | 生活助理 | 深度定制、带备选方案的旅行行程规划 |
| `web-style-analyzer` | 设计分析 | 提取目标网页视觉风格和 CSS 的思考规范 |
| `api-registry` | 系统资料 | 所有非 LLM API 的端点与 Key 获取规则 |
| `model-registry` | 系统资料 | 统一的大模型计价与模型名查询源 |
| `cluster_ops` | 系统资料 | VPS 集群节点和部署分布信息的知识库 |
| `mcp-builder` | 开发指南 | 编写优秀 MCP Server 的规范 |
| `invoke-jules` | 开发指南 | 调用 Jules 代理的规范文档 |
| `skill-creator` | 开发指南 | 创建与优化 Skill 的元工作流 |

---

## 🟡 第二类：建议“原生重写”的技能（需要 Node.js 开发支持）
这类技能逻辑并不复杂，可以通过在 Bob-agent 后端用 Node.js (JS) 编写几行代码来实现。重写后，这些将成为 Bob-agent 自带的“核心原生工具”。

| 原技能名称 | 建议重写方式 | 难度 |
| :--- | :--- | :--- |
| `weather` | **原生 Fetch** - 调用公共天气 API (如和风天气) | ⭐ |
| `web_search` | **原生 Fetch** - 调用 Tavily API 或 DuckDuckGo API | ⭐ |
| `system_time` | **Node.js API** - `new Date()` | ⭐ |
| `system_info` | **Node.js OS 模块** - 查询 CPU/内存/磁盘 | ⭐ |
| `link_extractor` | **Node.js 爬虫** - `fetch` 结合 `cheerio` 解析 DOM 提取 `<a>` 标签 | ⭐⭐ |
| `project_tracker` | **Node JS 文件读写** - 读取本地 YAML/JSON 生成看板 | ⭐⭐ |
| `todo_sorter` | **Node JS 文件读写** - 提取、分类并写入指定的 Markdown | ⭐⭐ |
| `todo_sweeper` | **Node JS 文件操作** - 归档已完成的任务并清理源文件 | ⭐⭐ |
| `assign_task` | **Node JS JSON 读写** - 追加记录到任务账本 | ⭐⭐ |
| `submit_feedback` | **Node JS JSON 读写** - 更新任务账本状态 | ⭐⭐ |

---

## 🔵 第三类：建议寻找“开源 MCP Server”替代的技能
这类技能涉及极度复杂的环境依赖（如浏览器沙盒、底层文件遍历）。自己开发极其耗时，建议直接让 Bob-agent 实现 MCP 协议，直接连上优秀的第三方开源 MCP 服务器。

| 技能意图 | 推荐替代方案 | 说明 |
| :--- | :--- | :--- |
| `agent-browser` | **Browser-Use MCP** 或 **Puppeteer MCP** | 专业的开源无头浏览器驱动服务，能安全且精准地执行网页自动化操作。 |
| `grounded_research` | **Brave Search MCP** 或 **Tavily MCP** | 专业的聚合搜索引擎 MCP，自带深度搜素和内容抓取管道。 |
| `code_agent` | **Anthropic Filesystem MCP** | 提供极度安全且高性能的本地文件遍历、读写和跨文件分析能力。 |

---

## 🔴 第四类：严重依赖云端/VPS 的技能（桌面端无法使用）
这些技能包含深度的定制业务逻辑、巨型 C/Python 依赖（爬虫、图表、视频、向量数据库）。**绝对不应该在桌面端实现**。它们是你个人的云端微服务资产。

| 技能名称 | 无法运行的原因 |
| :--- | :--- |
| `pptx-translate` | 强依赖 `python-pptx` 处理内存模型。 |
| `mckinsey-designer` | 强依赖重度的后端图表渲染环境（ECharts 服务端渲染）。 |
| `youtube_intelligence` | 强依赖 `yt-dlp` 不断更新的反爬签名破解系统，JS 环境无解。 |
| `china_intelligence` | 后台静默抓取管道，需要代理池和 RSS 持续运行。 |
| `global_intelligence` | 同上。 |
| `AKP_Link_Harvester` | 重度异步网页清洗工作流。 |
| `akp_digest` | 周期性的内容消化，属于批处理守护进程任务。 |
| `semantic-graph-weaver` | 强依赖重型 Embedding 模型推理，必须有 GPU/强算力支持。 |
| `note_graphify` | 图数据库（Neo4j）的复杂遍历逻辑。 |
| `knowledge_rag_injector`| 向量数据库连通以及 Dify 生态打通。 |
| `semantic_clustering` | 依赖文本向量化降维和聚类算法（如 KMeans, DBSCAN）。 |
| `history_map_*`系列 | 这是你个人的定制数据洗刷脚本集群。 |
| `heartbeat_monitor` | 典型的系统运维服务，需要在外网节点持续运行。 |
| `sync_cron_jobs` | OS 级别的任务调度器。 |
