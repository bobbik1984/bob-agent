# Bob 统一信息与知识生命周期实施计划

> 日期：2026-08-10
> 依据：`docs/superpowers/specs/2026-08-10-unified-knowledge-lifecycle-design.md`
> 原则：先建立新标准和只读审计，再双写验证，最后迁移；真实知识数据在验证前不移动、不删除。

## 1. 完成定义

实施完成时必须同时满足：

1. 所有长期知识对象都有稳定 ID 和可移植 Markdown。
2. SQLite 可以从 Markdown 重建搜索、关系和图谱索引。
3. 同一来源、笔记或知识点不会因出现在不同视图而复制文件。
4. 明确待办和日程自动分流；灵感留在速记；完整内容自动形成笔记。
5. 收藏文章或文件后生成来源卡、概要、知识点、实体和证据关系。
6. 通用笔记不属于项目；项目笔记只能属于一个项目。
7. 项目可引用通用来源和知识点，不改变其归属。
8. 知识库默认展示概要和知识点，完整原文按需展开。
9. 做梦机制更新既有对象，不再无条件制造 `wiki/learned` 碎片。
10. PC 与 Android 同步对象 ID、状态和关系，不同步无效的本机绝对路径。
11. 现有数据完成可恢复迁移，旧内容仍可搜索和追溯。
12. PC 与 Android 不增加 Python 或新的客户端运行时依赖。

## 2. 实施纪律

- 每一阶段单独提交，前一阶段通过测试后才进入下一阶段。
- 先增加兼容能力，再切换读取路径，最后归档旧结构。
- 迁移工具默认 `--dry-run`；真实写入必须显式指定目标和确认参数。
- 迁移不永久删除文件，只移动到带时间戳的归档目录。
- 所有迁移操作写 manifest，并支持重复执行和中断恢复。
- 保留用户现有未提交修改，避免改动无关文件。
- 新增用户可见文本必须进入 `zh-CN.json` 和 `en-US.json`。
- 新增图形继续使用 Lucide 图标，不使用 Emoji。

## 3. 阶段一：知识对象契约

### 目标

建立所有 Agent 都能理解的 Markdown schema、稳定 ID、对象类型和关系词表，不改变现有读取逻辑。

### 主要文件

- 新建 `src-tauri/src/knowledge_schema.rs`
- 修改 `src-tauri/src/lib.rs`
- 修改 `src-tauri/src/notebook.rs`
- 修改 `src-tauri/src/db.rs`
- 新建 `docs/KNOWLEDGE_SCHEMA.md`
- 更新 `docs/ARCHITECTURE.md`
- 更新 `LLM_WIKI.md`
- 更新 `AGENTS.md`

### 任务

1. 定义对象类型：`capture`、`source`、`note`、`knowledge_point`、`project`、`entity`、`memory`、`session`、`collection`。
2. 定义稳定 ID 前缀和生成规则。
3. 定义共享 frontmatter 字段、各类型必需字段和默认值。
4. 定义关系词表：`derived_from`、`cites`、`belongs_to`、`related_to`、`mentions`、`supports`、`contradicts`、`supersedes`、`merged_into`。
5. 实现 Markdown frontmatter 的解析、规范化、验证和序列化。
6. 实现原子文件写入：临时文件、刷盘、替换；失败时不产生半文件。
7. 对旧 Markdown 采用宽松读取，对新写入采用严格 schema。

### 测试

- 每种对象的最小合法 frontmatter。
- 未知字段可保留，缺少必需字段可诊断。
- 文件重命名不改变对象 ID。
- 中英文、空格、特殊文件名和 CRLF/LF 往返不丢数据。
- 原子写入失败不覆盖旧文件。

### 完成门槛

- 新 schema 文档可由其他 Agent 独立理解。
- 现有 Markdown 仍能正常读取。
- `cargo test --lib`、`npm test`、`npm run build` 通过。

## 4. 阶段二：只读盘点与迁移审计

### 目标

在不修改用户数据的前提下，精确识别现有对象、重复、冲突、失效引用和建议去向。

### 主要文件

- 新建 `src-tauri/src/knowledge_audit.rs`
- 修改 `src-tauri/src/lib.rs`
- 修改 `src/tauri-bridge.js`
- 新建 `tests/fixtures/knowledge_migration/`
- 更新 `docs/KNOWLEDGE_SCHEMA.md`

### 任务

1. 扫描 `notes/` 和 `wiki/` 下所有 Markdown。
2. 记录路径、哈希、标题、frontmatter、对象类型、链接和来源。
3. 输出完全重复、同 URL、同原始文件、近似重复候选。
4. 将 `wiki/learned` 候选分类为知识、用户记忆、项目记忆、程序经验、纠错或一次性噪声。
5. 识别 `notes/topics` 与 `wiki/sources` 的跨目录副本。
6. 识别失效 wikilink、失效 source path、空文件和不完整 frontmatter。
7. 生成 JSON 机器报告和 Markdown 人类报告。
8. 审计命令只读，禁止在扫描阶段写入知识目录。

### 测试

- 用固定 fixture 验证完全重复、近似重复、冲突观点和失效引用。
- 在真实目录执行只读审计，比较扫描前后文件哈希，确保零修改。
- 相同输入重复审计产生稳定报告。

### 完成门槛

- 真实知识目录零变更。
- 报告覆盖所有现有 Markdown，并对每个文件给出建议去向。

## 5. 阶段三：可重建 SQLite 派生索引

### 目标

统一现有 `wiki_fts`、`memory_entries` 和知识图谱的对象身份，使数据库成为可删除重建的加速层。

### 主要文件

- 修改 `src-tauri/src/db.rs`
- 新建 `src-tauri/src/knowledge_index.rs`
- 修改 `src-tauri/src/kg.rs`
- 修改 `src-tauri/src/tools.rs`
- 修改 `src-tauri/src/notebook.rs`
- 修改 `src-tauri/src/sync_engine.rs`

### 任务

1. 建立统一对象索引表和关系索引表，以 Markdown 稳定 ID 为主键。
2. 保留 Capture Journal、活动日志和同步重试等运行表。
3. 实现全量重建和单文件增量更新。
4. `brain_search` 同时返回对象类型、概要、领域、主题和来源证据。
5. 图谱从统一关系索引生成，不再从目录和标签临时猜测身份。
6. 重建失败时保留旧数据库可用状态，避免半重建。
7. 数据库索引版本写入 `wiki/system/schema.md` 或对应 manifest。

### 测试

- 删除测试数据库后从 fixtures 完整重建。
- 重建前后搜索结果、对象数和关系数一致。
- 单文件更新只影响对应对象和关系。
- 同名不同 ID 不合并；同 ID 不同路径正确更新。

### 完成门槛

- 测试环境可仅凭 Markdown 恢复搜索和图谱。
- Bob 运行不依赖旧 `wiki/index.md` 的准确性。

## 6. 阶段四：统一 Capture 自动分流

### 目标

让悬浮窗、聊天、Android分享和文件拖拽共用一套分类与提交状态机。

### 主要文件

- 修改 `src-tauri/src/capture.rs`
- 新建 `src-tauri/src/capture_router.rs`
- 修改 `src-tauri/src/tools.rs`
- 修改 `src-tauri/src/notebook.rs`
- 修改 `src-tauri/src/llm.rs`
- 修改 `src/components/QuickNoteOverlay.vue`
- 修改 `src/composables/useChat.js`
- 修改 `scripts/android_patches/ShareActivity.kt`

### 任务

1. 将分类结果结构化为 `intent`、`confidence`、`targets`、`reason_codes`。
2. 明确待办进入待办，明确日程进入日历。
3. 灵感和随想进入 `notes/quick`。
4. 完整独立思考进入通用笔记。
5. 明确项目内容进入唯一项目笔记。
6. 收藏链接或文件进入来源管线。
7. 低置信度输入保留速记，不阻断捕获成功。
8. 一次 Capture 支持多个派生对象引用。
9. 保持现有幂等、重试、跨端状态合并和 50 条活动日志上限。
10. 捕获提交必须离线可用；断网或 Clerk 不可用时先持久化原文，再进入 `pending_enrichment`。
11. 建立轻量组合式时间解析快车道；明确表达本地解析，复杂口语才调用 Clerk。
12. Clerk 只输出结构化意图与时间约束，不直接写数据库或调用日历工具。
13. Clerk 结果必须通过 schema、日期、时区、冲突和幂等校验，再由本地工具层提交。
14. 恢复联网后按原失败阶段续跑；已有派生对象 ID 时禁止重复创建。
15. 只有收到真实工具提交回执后才记录成功，并保留可撤销的活动记录。

### 测试

- 待办、日程、灵感、完整思考、项目记录、收藏和模糊输入各一组中英文样本。
- 同一输入重试不重复创建对象。
- Markdown失败时不得标记 committed。
- Android与PC生成相同稳定 ID 语义和兼容同步记录。
- 断网时速记、明确待办和明确日程可在本地落地，复杂输入进入待整理队列且原文不丢失。
- 联网恢复后队列自动续跑；在 Clerk 成功但工具提交失败的 fixture 中，只重试提交阶段。
- “明天三点”“下周一”“三天后”等明确表达不调用 Clerk；复杂相对事件由 Clerk 结构化后交给本地日期引擎计算。
- Clerk 输出非法日期、缺失字段或低置信度候选时不得写入日历。

### 完成门槛

- 所有入口使用同一分流协议。
- 失败诊断能指出捕获、分类或提交阶段。
- 无网络和无 Clerk 时仍能可靠捕获；恢复网络后可幂等完成延迟整理。

### 2026-08-10 实施状态

- 已完成 Capture Journal、本地时间快车道、Clerk 结构化候选、Todo/Event 确定性提交和延迟重试。
- 已完成 QuickNote/Note/Source 权威 Markdown 提交、稳定 ID、Source 去重、项目名称唯一匹配与澄清分支。
- 尚需补齐多派生对象事务、低干扰纠正交互，以及真实 PC/Android 三入口回放；Source 正文提取与知识点蒸馏进入阶段五。

## 7. 阶段五：来源、知识点与实体管线

### 目标

将文章和文件从“单份摘要”升级为“唯一来源 + 多个知识点 + 实体关系”。

### 主要文件

- 新建 `src-tauri/src/source_pipeline.rs`
- 新建 `src-tauri/src/knowledge_distiller.rs`
- 修改 `src-tauri/src/browser.rs`
- 修改 `src-tauri/src/kb_indexer.rs`
- 修改 `src-tauri/src/tools.rs`
- 修改 `skills/AKP_Link_Harvester/SKILL.md`
- 修改 `skills/folder-to-wiki/SKILL.md`

### 任务

1. 统一 `browse_page`、`save_to_notes`、文件索引和 Link Harvester 的写入路径。
2. 移除同名技能对 `wiki/raw/article` 与 `wiki/sources` 的冲突约定。
3. 来源按 URL 规范化值或文件内容哈希去重。
4. 标记正文 `complete`、`partial`、`failed`。
5. 分离来源、领域、主题、实体和内容类型，不再全部压入扁平 tags。
6. 提取多个原子知识点并保存证据片段。
7. 相同知识点汇聚不同证据；冲突结论建立关系。
8. 微信文章清洗无效 `Image` 占位；有实际图片时使用受管资源和可追溯命名。
9. 不把外部作者观点自动升级为用户观点。

### 测试

- Eaton Centre 微信文章 fixture：生成商业地产来源和多个知识点。
- Anthropic Playbook fixture：生成 AI 领域来源，不与商业地产混类。
- 同 URL 再次收录不产生副本。
- 正文截断、反爬、网络失败和文件解析失败有明确状态。

### 完成门槛

- 每个知识点可追溯到来源和证据。
- 来源可被多个项目引用但只有一个权威文件。

## 8. 阶段六：速记成熟、笔记更新与做梦治理

### 目标

让 Bob 主动把成熟灵感转化为笔记，并停止无约束制造 `wiki/learned` 碎片。

### 主要文件

- 修改 `src-tauri/src/evolution.rs`
- 修改 `src-tauri/src/dream.rs`
- 新建 `src-tauri/src/note_maturation.rs`
- 修改 `src-tauri/src/notebook.rs`
- 修改 `src-tauri/src/llm.rs`

### 任务

1. 评估单条速记完整度和多条速记主题聚合度。
2. 自动创建或更新权威笔记，保留来源 Capture 引用。
3. 更新已有笔记时写 revision，不静默覆盖。
4. 将 `wiki/learned` 输出改为结构化知识或记忆对象。
5. 区分用户、项目、程序经验和纠错记忆。
6. 收藏行为只增加兴趣证据；确认、编辑或反复采用才提高观点置信度。
7. 做梦合并重复、发现跨领域方法论、处理过期与冲突。
8. 生成可解释的“Bob自动整理了什么”活动记录。

### 测试

- 单条完整速记自动成文。
- 多条相关速记形成一篇笔记。
- 新内容补充旧笔记，不生成副本。
- 冲突观点不被覆盖。
- 重复运行做梦结果幂等。

### 完成门槛

- 新对话不再持续向 `wiki/learned` 产生无治理碎片。
- 自动整理不要求用户逐条确认，但可追溯和撤销。

## 9. 阶段七：知识库、项目和图谱统一界面

### 目标

提供 iKnow 式易读入口，同时隐藏底层对象复杂度。

### 主要文件

- 修改 `src/components/NoteExplorer.vue`
- 修改 `src/views/KnowledgeGraphView.vue`
- 新建或重构知识库卡片与详情组件
- 修改 `src/tauri-bridge.js`
- 修改 `src/locales/zh-CN.json`
- 修改 `src/locales/en-US.json`

### 任务

1. 知识库首页显示标题、领域说明、概要、主题、来源和日期。
2. 详情页依次显示概要、知识点、用户思考、相关内容和折叠原文。
3. 普通笔记列表不再显示来源副本。
4. 项目页面聚合唯一项目笔记、待办、日程以及引用的来源和知识点。
5. 图谱使用统一对象和关系，不复制 Markdown。
6. 自动整理采用非打扰活动提示。
7. 桌面与手机保持功能一致，移动端详情保持紧凑。

### 测试

- 组件测试覆盖卡片、详情、原文展开和引用跳转。
- 390px、768px和桌面宽度视觉检查。
- 中英文切换后所有标签和状态一致。
- 同一对象从搜索、项目和图谱打开时 ID 一致。

### 完成门槛

- 用户无需理解内部目录即可完成浏览、搜索和引用。
- 不出现同名内容的重复卡片。

## 10. 阶段八：真实数据迁移

### 目标

安全迁移当前 `bob.agent` 数据，验证后切换主读取路径。

### 主要文件

- 新建 `src-tauri/src/knowledge_migration.rs` 或独立 Rust 辅助命令
- 新建迁移 fixture 与恢复测试
- 更新 `docs/KNOWLEDGE_SCHEMA.md`
- 更新 `docs/ARCHITECTURE.md`
- 更新 `docs/FEATURES.md`
- 更新 `todo.md`、`progress.yaml`

### 任务

1. 先执行真实目录 dry-run，输出逐文件建议。
2. 用户确认报告后建立带时间戳的可恢复归档。
3. 为保留对象分配 ID 并写入新结构。
4. 将完全重复副本映射到单一权威对象。
5. 将 `wiki/learned` 分流到知识、记忆或隔离区。
6. 重建 SQLite 和图谱。
7. 对比迁移前后数量、哈希、引用和搜索召回。
8. 切换 Bob 读取新结构。
9. 旧目录仅归档，不永久删除。

### 首批真实数据验证

- `notes/topics` 与 `wiki/sources` 中完全相同的 Anthropic 文件。
- `wiki/projects` 与 `wiki/sources` 中合理的聚合页和来源页。
- `wiki/learned` 中重复的 Google Calendar、PDF、CDN 和路径知识。
- 现有实体、冷会话和索引文件。

### 完成门槛

- 迁移报告无未解释的数据损失。
- Bob 可搜索旧内容、打开权威文件并追溯来源。
- 回滚演练能够恢复旧读取路径。

## 11. 发布与回滚

1. 先在测试数据目录启用新 schema 和索引。
2. 使用 feature flag 双读，旧结构为兜底。
3. 真实数据迁移后观察至少一个完整 Capture 和 Dream 周期。
4. 完成 PC、Android、局域网和 Relay 同步验证。
5. 发布前运行：
   - `cargo test --lib`
   - `npm test`
   - `npm run build`
   - `git diff --check`
6. 发布后保留旧目录归档和迁移 manifest。
7. 回滚只切换读取路径和恢复旧数据库，不反向覆盖 Markdown。

## 12. 第一执行批次

第一批只实施阶段一和阶段二：

- 新 schema 与稳定 ID；
- 兼容旧 Markdown 的解析器；
- 原子写入；
- 只读知识盘点器；
- fixtures 和单元测试；
- 真实目录 dry-run 审计报告。

第一批明确不做：

- 不移动 `C:\Users\xm_bo\AppData\Roaming\bob.agent` 中的文件；
- 不切换现有 UI 读取路径；
- 不改变做梦写入位置；
- 不发布新版本；
- 不推送远程仓库。
