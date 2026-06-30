# 📓 笔记 × 知识图谱系统重构 — 完整实施计划

> **版本**: v2 (设计决策已锁定)
> **日期**: 2026-06-30
> **目标**: 消灭三座数据孤岛，建立 Obsidian 级别的笔记互联体验 + AI 增值能力

---

## 已锁定的设计决策

| 决策项 | 选择 |
|--------|------|
| 双向链接语法 | Obsidian 风格 `[[笔记标题]]` |
| 标签管理 | 混合方案：低摩擦打标 + 侧边栏标签云 + Dream LLM 语义去重 |
| 笔记分类 | `daily` / `topics` / `projects`(新增) / `sources`(统一) / 用户自建 |
| 剪报 vs 文献 | 合并为统一的 `sources`，靠 frontmatter `source_type` 区分来源 |
| Phase 优先级 | P0 → P1 → P2 → P3 顺序执行 |

---

## 目标分类体系

```
notes/
├── daily/           速记 — 闪念、灵感、碎片 (📅)
├── topics/          笔记 — 深度思考、主题文章 (📝)
├── projects/        项目 — 按项目归档的工作笔记 (📁) [NEW]
│   ├── bob-agent/
│   ├── quant_lab/
│   └── ...
├── sources/         文献 — 所有外部参考资料 (📚) [统一]
│   │                (网页剪报/PDF/AI摘录/KB索引/微信文章)
│   │                靠 frontmatter source_type 区分
├── assets/          资产 — 拖入的图片等二进制文件
└── {用户自建}/      自定义文件夹 (📂)
```

---

## Phase 0: 基础设施修补 (1-2 天，纯 Rust)

> [!IMPORTANT]
> 不做就会腐烂的基础修复。无 UI 变动，纯后端。

### P0-1: 删除级联清理

#### [MODIFY] [notebook.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/notebook.rs)

`notebook_delete_note()` 从裸 `fs::remove_file()` 升级为级联清理：
- 如果路径匹配 `wiki/sources/*`：删除 `wiki_fts` 中 `source_path` 对应行
- 删除 `kg_nodes` 中 `source` 字段匹配的节点
- 删除关联 of `kg_edges`（source_id 或 target_id 匹配的行）
- 需要接收 `State<DbState>` 参数以访问数据库

#### [MODIFY] [lib.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/lib.rs)

更新 `notebook_delete_note` 命令签名，注入 `db: State<DbState>`。

---

### P0-2: 笔记专用 FTS 索引

#### [MODIFY] [db.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/db.rs)

新增 `notes_fts` FTS5 虚拟表：
```sql
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
    note_path,    -- 笔记路径 (如 'topics/架构设计.md')
    title,        -- 标题
    content,      -- 全文内容
    tags,         -- 标签 (空格分隔)
    tokenize='unicode61'
);
```

#### [MODIFY] [notebook.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/notebook.rs)

- `notebook_save_note()`: 保存后自动 upsert `notes_fts`
- `notebook_delete_note()`: 删除后自动删除 `notes_fts` 对应行
- `notebook_search()`: 从暴力 `contains()` 改为 FTS5 查询 `SELECT ... FROM notes_fts WHERE notes_fts MATCH ?`

---

### P0-3: 死代码清理

#### [MODIFY] [tauri-bridge.js](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/tauri-bridge.js)

- 删除废弃的 `appendQuickNote` 映射（L232，handler 未注册）
- 确认 `notebookAppendDaily` 是唯一的速记入口

#### [MODIFY] [ChatView.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/views/ChatView.vue)

- 修复 `handleSaveToNote()` 中对 `sendMessage` 的错误调用签名（当前直接传了 composable 内部参数）
- 确保 Slash Command 菜单的 `v-if` 与 `v-show` 一致性

---

## Phase 1: 笔记 ↔ 知识图谱联通 (3-5 天)

> [!IMPORTANT]
> 价值最高的改进：让用户笔记不再是孤岛。

### P1-1: 新增 `projects/` 目录 + 用户自建文件夹

#### [MODIFY] [notebook.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/notebook.rs)

- `init_notebook_dirs()`: 新增 `projects/` 子目录创建
- `notebook_list_notes()`: 扫描逻辑从硬编码 3 个目录改为动态扫描 `notes/` 下所有子目录
  - 预设目录 (`daily`, `topics`, `projects`, `sources`, `assets`) 按原有分组名展示
  - 非预设子目录统一归到 `custom` 分组，key 为文件夹名
  - 返回格式升级：`{ daily: [...], topics: [...], projects: { "bob-agent": [...], "quant_lab": [...] }, sources: [...], custom: { "我的分类": [...] } }`
- `notebook_create_note()`: 支持 `category` 参数，允许指定创建到哪个目录
- 新增 `notebook_create_folder(name)`: 在 `notes/` 下创建自定义子目录

#### [MODIFY] [NoteExplorer.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/components/NoteExplorer.vue)

- 树视图增加 `projects` 分组（嵌套子文件夹）和动态 `custom` 分组
- 底部增加"📁 新建分组"按钮
- 所有分组支持拖拽移动笔记

#### [MODIFY] [zh-CN.json](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/locales/zh-CN.json) / [en-US.json](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/locales/en-US.json)

新增翻译 key：`notebook.projects`, `notebook.custom`, `notebook.new_folder`, `notebook.empty_projects`

---

### P1-2: 笔记保存时自动同步 KG 节点

#### [MODIFY] [notebook.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/notebook.rs)

实现 L234-236 的 TODO 桩 (`T-1906`)：

`notebook_save_note()` 保存后触发副作用：
1. **笔记自身 → KG 节点**: 每篇 `topics/*` 和 `projects/*` 笔记自动创建/更新 `note` 类型 KG 节点
   - `id`: `note_{sanitized_path}`
   - `label`: frontmatter title 或文件名
   - `source`: 笔记路径
2. **标签 → KG 节点 + 边**: 解析 frontmatter `tags` 字段
   - 每个 tag 创建/查找 `tag` 类型节点
   - 创建 `tagged_as` 边 (笔记 → tag)
   - Diff 更新：比较旧 tags 和新 tags，删除移除的边，添加新的边
3. **实体提及 → KG 边**: 扫描笔记正文，如果包含已有 `kg_nodes.label` 中的文本
   - 创建 `mentions` 边 (笔记 → 实体)

需要接收 `State<DbState>` 参数。

---

### P1-3: 标签系统激活

#### [MODIFY] [NoteExplorer.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/components/NoteExplorer.vue)

- 新增第三个视图模式：`tags`（标签云）
- 工具栏增加 `Tag` 图标按钮
- 标签云视图：聚合所有笔记的 frontmatter tags，按出现频率排列
- 点击标签 → 筛选出所有包含该标签的笔记列表

#### [MODIFY] [TiptapEditor.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/components/TiptapEditor.vue)

- 工具栏末尾增加"🏷️ 标签"按钮
- 点击弹出小型 popover，显示当前笔记的标签列表 + 输入框可添加新标签
- 添加/删除标签时，修改当前笔记的 frontmatter `tags` 字段并触发保存

#### [MODIFY] [notebook.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/notebook.rs)

- 新增 `notebook_update_tags(path, tags: Vec<String>)`: 更新指定笔记的 frontmatter tags 字段
- 新增 `notebook_list_all_tags()`: 扫描所有笔记，返回去重的标签列表 + 每个标签 the 笔记数

---

### P1-4: KG Inspector 增加"相关笔记"

#### [MODIFY] [KnowledgeGraphView.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/views/KnowledgeGraphView.vue)

- Inspector 面板（点击节点后的右侧详情区）增加"📓 相关笔记"区块
- 查询 `kg_edges` 中所有 `relation='mentions'` 且 `target_id` 为当前节点的边
- 列出对应的笔记标题，点击可切换到笔记模式并打开该笔记

---

## Phase 2: 双向链接与反向链接 (5-7 天)

> [!WARNING]
> 核心竞争力：Bob 将成为唯一同时具备 AI 对话 + `[[双向链接]]` + 知识图谱的桌面应用。

### P2-1: `[[wikilink]]` 语法支持

#### [NEW] [WikilinkExtension.js](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/extensions/WikilinkExtension.js)

自定义 Tiptap Mark 扩展：
- 识别 `[[笔记标题]]` 语法，渲染为蓝色可点击内链
- 如果目标笔记存在 → 实线蓝色，点击跳转
- 如果目标笔记不存在 → 红色虚线，点击自动创建并跳转
- Markdown 序列化保持 `[[]]` 原样（`tiptap-markdown` 自定义 serializer）

#### [MODIFY] [TiptapEditor.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/components/TiptapEditor.vue)

- 导入并注册 `WikilinkExtension`
- 输入 `[[` 时触发笔记标题自动补全弹窗（复用 notes 列表数据）

#### [MODIFY] [notebook.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/notebook.rs)

- 新增 `notebook_get_backlinks(path)`: 扫描所有笔记，正则匹配 `\[\[{title}\]\]`
- 返回 `[{ path, title, context }]`（context = 链接所在行的前后各 50 字）
- 考虑缓存：用 `HashMap<title, Vec<path>>` 在内存中维护反向索引

---

### P2-2: 反向链接面板

#### [MODIFY] [KnowledgeGraphView.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/views/KnowledgeGraphView.vue)

- 笔记编辑器底部增加可折叠的"🔗 反向链接"面板
- 调用 `notebook_get_backlinks(currentNotePath)` 获取数据
- 每条显示：笔记标题 + 上下文片段 + 点击跳转

---

### P2-3: 链接 → KG 边自动同步

#### [MODIFY] [notebook.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/notebook.rs)

`notebook_save_note()` 副作用扩展：
- 正则解析所有 `[[X]]` 引用
- 为每个目标查找/创建 KG 节点
- 创建 `links_to` 类型边 (当前笔记 → 目标笔记)
- Diff 更新：比较旧链接和新链接，增删边

---

## Phase 2.5: Dream 标签语义去重引擎

> [!TIP]
> 把 Obsidian 标签系统的劣势变成 Bob 的独特优势。

#### [MODIFY] [dream.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/dream.rs)

新增 `phase_tag_dedup()` 阶段，在 Dream 流水线中执行：

1. **扫描**: 读取所有笔记的 frontmatter tags，汇总为去重列表
2. **聚类**: 调用 Clerk 模型，prompt 要求将语义相近的标签分组
   ```
   以下是用户笔记中使用的所有标签：[机器学习, ML, MachineLearning, 深度学习, DL, Rust, 锈]
   请将语义相同或极其相近的标签归为同组，每组推荐一个规范名称。
   输出 JSON: [{"canonical": "机器学习", "aliases": ["ML", "MachineLearning"]}, ...]
   注意：只合并真正同义的标签。"深度学习"和"机器学习"虽然相关但不是同义词，不要合并。
   ```
3. **存储提案**: 将合并提案写入 `dream_report.json` 的 `tag_merge_proposals` 字段
4. **排除列表**: 读取 `data/tag_merge_exclusions.json`，跳过用户之前拒绝过的合并对

#### [MODIFY] [MorningBriefing.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/components/MorningBriefing.vue)

- 晨间简报增加"🏷️ 标签整理建议"区块
- 显示合并提案列表，每条有"✅ 合并"和"❌ 跳过"按钮
- 确认合并 → 调用新的 IPC `notebook_merge_tags(canonical, aliases)`

#### [MODIFY] [notebook.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/notebook.rs)

新增 `notebook_merge_tags(canonical: String, aliases: Vec<String>)`:
- 遍历所有 `.md` 文件，在 frontmatter `tags` 中将 aliases 替换为 canonical
- 更新 `notes_fts` 中的 tags 字段
- 在 KG 中合并 tag 节点（`kg.rs` 的 `merge_nodes`）
- 将此次合并对写入 `tag_merge_history.json` 贴图日志

新增 `notebook_reject_tag_merge(tag_a: String, tag_b: String)`:
- 将排除对写入 `tag_merge_exclusions.json`

---

## Phase 3: AI 深度集成 (7-10 天)

### P3-1: 速记提升为独立笔记 (Promote)

#### [MODIFY] [NoteExplorer.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/components/NoteExplorer.vue)

- Daily 笔记的每个条目增加右键菜单 → "📤 提升为独立笔记"
- 弹窗让用户输入标题 and 选择目标分类 (topics/projects/自定义)
- 调用 `notebook_create_note()` 创建笔记，预填选中的速记内容
- 原始 daily note 中该行标记为 `~~已提升~~`

---

### P3-2: Dream 笔记扫描集成

#### [MODIFY] [dream.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/dream.rs)

新增 `phase_notebook_digest()` 阶段：
- 扫描过去 24 小时内修改的 `topics/` 和 `projects/` 笔记
- 用 Clerk 模型提取关键实体和关系
- 写入 KG（与 kb_indexer 同样的 upsert 逻辑）
- 晨间简报增加"📓 昨日笔记洞察"区块

---

### P3-3: 对话 ↔ 笔记双向联动

#### [MODIFY] [ChatView.vue](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src/views/ChatView.vue)

- AI 回复消息气泡增加"📌 存为笔记"按钮
- 点击 → 弹窗选择标题和分类 → 创建 `topics/` 或 `sources/` 笔记，预填 AI 回答内容
- `@note` 命令升级：输入 `@note` 后弹出笔记搜索面板（FTS 驱动）
- 选中笔记 → 将全文注入为 LLM 上下文

#### [MODIFY] Slash Command 菜单

`commandList` 扩展：
- `/memo` — 速记（已有）
- `/note` — 新建笔记并打开编辑器
- `/clip` — 将 AI 最近一条回复保存为文献笔记

---

### P3-4: Web Clipper (对标 Evernote)

#### [MODIFY] [tools.rs](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/tools.rs)

新增 `save_to_notes` 工具（LLM 可调用）：
- 参数：`title`, `content`, `source_url`, `source_type`, `tags`
- 将 `fetch_url` 抓取结果或 `web_search` 精华自动保存为 `sources/` 笔记
- 自动附带 frontmatter：`source_type`, `source_url`, `kb_indexed: false`

---

## 验证计划

### 自动化测试
```bash
# Phase 0 验证
cargo test --lib notebook   # 删除级联 + FTS upsert/delete
cargo test --lib kg          # 节点删除联动

# Phase 1 验证  
cargo test --lib notebook   # 新分类目录 + tag 同步
npm run build               # 前端编译通过

# Phase 2 验证
npm run build               # WikilinkExtension 编译
cargo test --lib notebook   # 反向链接查询 + 链接→KG同步
```

### 手动验证
- 创建笔记 → 检查 KG 中是否出现对应 `note` 节点
- 添加标签 → 检查 KG 中是否出现 `tag` 节点和 `tagged_as` 边
- 删除知识库文献 → 检查 `wiki_fts` 和 `kg_nodes` 是否同步清理
- 输入 `[[` → 检查笔记标题自动补全是否弹出
- 在笔记 A 中写 `[[笔记 B]]` → 打开笔记 B 检查反向链接面板
- Dream 运行后 → 晨间简报检查标签合并建议
