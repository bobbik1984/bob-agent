# 本地知识库引擎原生化 (LLM-Wiki 原生架构)

本计划结合了 `iknow` 项目的 **LLM-Wiki** 理念，与 Bob-Agent 的“彻底离线、纯本地化”架构决策。我们将抛弃传统的 RAG（每次检索原始文档碎片）模式，在本地建立一个由大模型（牛马模型）自主维护的、持续演进的结构化 Markdown 知识库。

## 🎯 核心理念 (The LLM-Wiki Paradigm)

传统 RAG 是“临时检索”，而 LLM-Wiki 是“知识沉淀”。
1. **三层架构**：
   - **原始源 (Raw Sources)**：用户拖入的本地文件夹（如 `D:\Documents\Public_Policy`）。绝对只读，不作修改。
   - **Wiki 层**：`data/wiki/` 目录下的 Markdown 文件网。由 LLM 全权编写和维护，包含概念页、实体页、对比分析等。
   - **Schema (指令集)**：Bob-Agent 系统提示词中对知识库维护的铁律。
2. **牛马主导的 Ingest**：扔进去一个新文件，不是简单的提词存向量，而是让便宜的本地/云端 Clerk 模型阅读、写摘要、更新全局 Index、更新实体关联，**把知识织入现有的网中**。

---

## 🛠️ Proposed Changes (架构设计)

### 1. 原生文本解析器 (Rust Native Extractors)

这是 Bob-Agent 能够读取 Raw Sources 的硬前提，必须完全在 Rust 中实现，抛弃 Python：

#### [NEW] `src-tauri/src/kb_extractor.rs`
引入轻量级 Rust 依赖，将文件暴力转换为纯文本：
- `.txt` / `.md`：原生读取。
- `.pdf`：使用 `pdf-extract` 剥离纯文本。
- `.docx` / `.pptx`：使用 `dotext` 或手动解压 `xml` 读取。
- `.xlsx`：使用 `calamine` 提取为 CSV 文本。
- 图片/音视频：仅提取文件名参与语义推测。

### 2. 知识库目录拓扑重构 (The Wiki Layout)

规范化 `data/wiki/` 的存储结构：
- `data/wiki/index.md`：**全局目录**。按类别列出所有概念、实体和文件摘要。LLM 在回答前首先检索它。
- `data/wiki/log.md`：**时间轴**。记录每次 Ingest 的操作历史（例如：`## [2026-05-15] ingest | 公共政策白皮书.pdf`）。
- `data/wiki/sources/`：每摄入一个原始文件，在这里生成一份百字级的“结构化摘要页”。
- `data/wiki/entities/`：跨文件提取出的核心概念、组织、人物的专题页面。

### 3. Ingest 工作流 (The Ingest Pipeline)

当你在 UI 点击“开始构建”后：
1. **预处理**：Rust `kb_extractor` 将拖入的文件逐一切片（Chunking），提取纯文本。
2. **分配任务**：将纯文本发送给 `clerk`（牛马模型）。
3. **LLM 编排 (Schema 执行)**：
   - Clerk 模型阅读文本片段。
   - 调用 `write_file` 创建/更新 `wiki/sources/<文件名>.md`。
   - 调用 `write_file` 提取文本中的新概念，写入 `wiki/entities/`。
   - 调用 `write_file` 将新文件和新概念追加登记到 `wiki/index.md`。
   - 在 `wiki/log.md` 中打卡记录。

### 4. Query 与 Lint 工作流

- **Query (查询)**：你问 Bob 问题时，Bob 调用现成的 `brain_search` 工具。他检索的是经过提炼的 `wiki` 层，而不是海量且充满废话的原始 PDF，速度和准度极高。
- **Lint (体检)**：你可以随时对 Bob 说：“做一次知识库体检”。Bob 会扫描 `wiki/` 目录，找出“孤儿页面（无链接）”、“冲突观点”和“过时信息”，主动提议修改。

---

## 🙋 User Review Required (需决策事项)

> [!IMPORTANT]
> **依赖增加评估**
> 引入 `pdf-extract`、`calamine` 等解析库，预计会让 Bob-Agent 安装包体积增加约 **3-5 MB**。完全脱离 Python 环境的代价，是否接受？

> [!TIP]
> **全自动还是半自动？**
> `iknow` 文档提到，Ingest 过程既可以“全自动批处理”，也可以“单文件带人工监督”。在当前的 UI 流程中，你拖入文件夹后是倾向于**让 Bob 在后台一口气处理完几百个文件**，还是**每处理完一个就在聊天框里给你汇报一句，让你确认重点**？

---

## ✅ 验证计划

1. 拖入一个包含 PDF 和 Word 的测试文件夹。
2. 观察 Rust 能否成功解析出纯文本。
3. 检查 `data/wiki/log.md` 是否新增了处理记录。
4. 检查 `data/wiki/index.md` 是否自动建立了对新知识的超链接目录。
