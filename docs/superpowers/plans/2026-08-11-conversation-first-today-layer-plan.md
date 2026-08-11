# Conversation-first Today Layer 实施计划

> 状态：核心纵切片已实施并通过自动化与 Tauri 原生启动验证；PC/Android 发布产物的真机与体积验收待下一次发布
>
> 日期：2026-08-11
>
> 设计依据：`docs/superpowers/specs/2026-08-11-conversation-first-today-layer-design.md`
>
> 产品边界：对话是默认入口；Today Layer 是派生的轻量工作层；Quick Note 始终优先服务灵感记录

### 2026-08-11 实施收口

- 已完成 Rust `daily_brief` 契约、只读来源、确定性排序、缓存、逐设备已读和三个 Commands；未修改 Cargo/package 依赖。
- 已完成 App 级唯一 Today Surface、对话首屏卡片、桌面/移动入口、结构化导航和 Quick Note 草稿无损交接；旧 `MorningBriefing` 已移除。
- 常规排序保持纯本地。可选 Clerk 没有接入，因为当前确定性 `1 + 2` 已满足产品预算；以后只能作为不改变事实与权限的可失败增强。
- 浏览器预览复用 `tauri-bridge.js` 的开发 fixture 和真实组件，没有创建第二套演示 UI。桌面 1280×800、手机 390×844 和草稿恢复已验证。
- `npm test` 9/9、`npm run build`、Daily Brief Rust 12/12、全量 Rust 140 passed / 0 failed / 1 ignored 均通过；`git diff --check` 通过。
- `npm run tauri dev` 已生成并启动响应正常的 `bob.exe`，开发页面返回 HTTP 200；真机键盘、Android Back、跨端已读与安装包体积保留到发布质量门。
- 当前工作树原本包含未提交 Phase 5 修改，重叠 hunk 不适合安全拆分提交，因此本轮不提交、不推送、不升版本。

## 1. 交付目标

在不新增用户侧依赖、不扩大客户端体积风险、不改变各业务真相源的前提下，交付一个可离线、可增量、可跨入口复用的 Today Layer：

- 启动和新对话能够用十秒可读的信息帮助用户继续工作；
- 普通对话、对话标题和 Quick Note 底部按钮打开同一个 `TodayLayerSurface`；
- 悬浮气泡仍然只打开灵感速记，Daily Brief 只是底部次级入口；
- Today Layer 先显示，Quick Note 后淡出，未提交草稿不丢失；
- L1 固定为一个主项、最多两个待确认项，其余只显示数量；
- Work Core、Goal Runtime、Calendar、Todo、Conversation 和 Dream 仍是各自数据的真相源；
- 断网、模型未配置和单一来源失败时仍能显示可靠的确定性简报。

## 2. 实施原则

- **One surface**：所有入口只打开一个 App 级 `TodayLayerSurface`，禁止复制对话版和速记版。
- **Derived view**：Brief 只读聚合，不复制或反向覆盖源对象状态。
- **Local first**：首屏由本地确定性查询生成，Clerk 和 Dream 都不能阻塞。
- **One plus two**：排序器在后端强制执行 `1 focus + 2 attention`，不能只靠前端隐藏。
- **Structured action**：卡片保存稳定对象引用和动作类型，不把整份 Brief 拼成聊天消息。
- **No data loss**：Quick Note 交接中的草稿必须保留，失败时保持输入界面可用。
- **No new runtime**：不新增 Python、Node、MCP、常驻服务或云端前提。
- **No dependency by default**：优先复用 Rust、SQLite、Vue、Vitest 和 Lucide 现有依赖。
- **Current language**：持久层保存 error/reason code，用户可见文字在渲染时按当前 locale 生成。
- **Controlled commits**：每个纵向切片通过对应门后才提交；不推送、不发布、不升版本。

## 3. 前置保护门：处理当前重叠工作区

当前工作区已有未提交的 Phase 5 修改，且与本功能重叠的文件包括：

- `src-tauri/src/db.rs`
- `src-tauri/src/lib.rs`
- `src/App.vue`
- `src/views/ChatView.vue`
- `src/composables/useChat.js`
- `src/locales/zh-CN.json`
- `src/locales/en-US.json`
- 多份架构与路线文档

实施前必须：

- [ ] 记录 `git status --short` 和上述文件的基线 diff；
- [ ] 不还原、不覆盖、不格式化无关代码；
- [ ] 确认 Phase 5 专用 Rust 回归门是否已通过；未通过时先定位失败，不把 Today Layer 当成修复载体；
- [ ] Today Layer 修改采用小块补丁，验证每个重叠文件的最终 diff 同时保留原有行为；
- [ ] 提交时只暂存本批次明确文件或明确 hunk，不把其他未提交修改混入；
- [ ] 若同一 hunk 已包含不可分离的未提交修改，则暂不提交该代码批次，先保留工作树并报告边界。

完成信号：能够说明现有修改和 Today Layer 新修改分别是什么，且没有任何用户文件丢失。

## 4. Batch 1：Daily Brief 契约、来源与确定性排序

### 4.1 新增 `src-tauri/src/daily_brief/models.rs`

- [ ] 定义版本化 `DailyBriefSnapshot`、`DailyBriefItem`、`DailyBriefAction`、`SourceHealth`；
- [ ] 状态限定为 `fresh | partial | stale`；
- [ ] Item 保存稳定 `item_id`、`source_type`、`source_id`、`source_revision`、kind、priority、reason codes、时间和 evidence refs；
- [ ] Action 只允许已注册类型：打开对象、继续会话、回应审批、查看日程、查看待办、打开详情；
- [ ] 不允许 Action 携带自由执行 prompt、密钥、完整原文或模型推理链；
- [ ] 为 Rust/JS 序列化字段固定 camelCase 契约并增加兼容测试。

### 4.2 新增 `src-tauri/src/daily_brief/sources.rs`

- [ ] 以只读适配器分别读取 Work Core、Goal Runtime、Calendar/Todo、Conversation Summary 和 Dream；
- [ ] Calendar 与 Todo 共用 `events` 表，但按 `type` 明确分开；
- [ ] Work/Goal 通过现有 repository API 或明确 SQL 查询获得状态，不调用会把数据库错误吞成空数组的 UI command；
- [ ] Goal 来源覆盖 `waiting_user`、blocked、即将超预算、进行中和最近完成；
- [ ] Conversation 来源读取现有 session 摘要文件，返回稳定 conversation/summary 引用；
- [ ] Dream 只读取已经生成的报告或结构化洞察，不因打开 Brief 触发做梦；
- [ ] 每个来源独立返回 `ok | unavailable | error` 和 revision/fingerprint；
- [ ] 来源失败不得转换为零项。

### 4.3 新增 `src-tauri/src/daily_brief/ranker.rs`

- [ ] 用固定规则完成去重、排序和 `1 + 2` 截取；
- [ ] 排序顺序：等待用户 → 今日到期/风险 → 新变化 → 建议下一步 → 信息；
- [ ] 同一源对象只生成一个主 Item，附属变化合并为 reason codes；
- [ ] focus 可以是最高优先级的审批、风险或建议，不强制是 AI 建议；
- [ ] 普通日程、进行中 Goal、Dream 条目不进入 L0 待处理数量；
- [ ] 相同输入必须产生相同 item ID 和顺序。

### 4.4 新增 `src-tauri/src/daily_brief/mod.rs`

- [ ] 暴露 models、sources、ranker、service 和 commands；
- [ ] 在 `src-tauri/src/lib.rs` 注册模块，但本批不开放 UI command；
- [ ] 不修改 Cargo manifests 或 lockfile。

### 4.5 Batch 1 测试

- [ ] 空数据库生成可靠空状态；
- [ ] 三个以上待确认项仍只得到一个 focus 和两个 attention；
- [ ] 相同对象的 Work Event、Goal Event 和审批提示不会重复；
- [ ] 一个来源报错时 Snapshot 为 partial，其他来源仍可用；
- [ ] Calendar 的 event 与 todo 被正确区分；
- [ ] 无 Dream、无 session 文件、断网和无模型不影响确定性输出；
- [ ] 稳定 ID、排序和序列化回放通过。

验证命令：

```powershell
$env:CARGO_TARGET_DIR='D:\ignore_sync\bob-agent-today-layer-target'
Set-Location src-tauri
cargo test daily_brief --lib
```

完成信号：Rust 测试通过，依赖文件无变化，尚未修改现有用户界面。

## 5. Batch 2：缓存、设备增量与 Commands

### 5.1 新增本地派生缓存

在 Daily Brief 模块内部初始化两张本地表：

```text
daily_brief_cache
  local_date
  revision
  source_fingerprint
  payload_json
  generated_at
  status

daily_brief_seen
  device_id
  local_date
  last_seen_revision
  updated_at
```

- [ ] 表是可重建缓存和设备阅读游标，不加入跨端业务同步真相；
- [ ] `device_id` 复用现有配置身份，缺失时使用稳定的本地 fallback，不创建第二套设备身份；
- [ ] source fingerprint 未变化时直接返回缓存，不固定调用模型；
- [ ] source fingerprint 变化时生成新 revision，并计算 `changed_since_last_seen`；
- [ ] 每台设备独立标记已查看；真实审批和状态变化仍通过源对象同步；
- [ ] 缓存损坏时删除单条缓存并重建，不执行全库重置。

### 5.2 新增 `src-tauri/src/daily_brief/service.rs`

- [ ] 接受经过校验的本地日期和 UTC offset，避免新增时区依赖；
- [ ] 协调来源、排序、缓存、stale/partial 和增量；
- [ ] 优先返回可用缓存，再在需要时重建；
- [ ] 空内容返回明确 empty snapshot，不生成虚假建议；
- [ ] 生成用户无关的 reason code，不在 Rust 持久化中英文文案。

### 5.3 新增 `src-tauri/src/daily_brief/commands.rs`

开放最小命令：

```text
daily_brief_get(dateContext)
daily_brief_refresh(dateContext)
daily_brief_mark_seen(snapshotId, revision)
```

- [ ] commands 返回可读 `Result`，不增加生产路径 `unwrap()` 或 `panic!()`；
- [ ] mark seen 校验 snapshot/revision，旧卡片不能覆盖新 revision；
- [ ] refresh 只刷新派生视图，不执行任何业务 Action；
- [ ] 在 `lib.rs` 注册命令并在 `src/tauri-bridge.js` 暴露同名语义 API；
- [ ] Bridge 继续是 Vue 使用 Tauri 的唯一入口。

### 5.4 Batch 2 测试

- [ ] SQLite 关闭重开后 cache 和 seen cursor 可恢复；
- [ ] PC 与手机 device ID 的 last-seen 相互隔离；
- [ ] 同一 revision 重复 mark seen 幂等；
- [ ] 旧 revision 标记被拒绝或安全忽略；
- [ ] 缓存损坏、数据库锁失败和单源失败返回正确状态；
- [ ] 无源变化时不会重复生成 revision。

完成信号：后端可以稳定返回 Brief 契约，前端尚未改变现有 Quick Note 行为。

## 6. Batch 3：唯一 Today Layer Surface 与可测试状态机

### 6.1 新增 `src/composables/useDailyBrief.js`

- [ ] 作为 App 级唯一控制器维护 snapshot、loading、visible、entrySource、returnContext 和错误；
- [ ] 暴露 `openTodayLayer(source)`、`closeTodayLayer()`、`refresh()`、`markSeen()` 和结构化 action 路由；
- [ ] 同时到来的多次打开请求合并，禁止重复 Surface；
- [ ] 优先显示缓存/骨架，再异步刷新 partial/stale 数据；
- [ ] Surface 关闭时根据 entrySource 恢复原焦点，不改变 active conversation。

### 6.2 新增纯状态机 `src/daily-brief/surface-state.js`

状态建议：

```text
closed → opening → visible → closing → closed
             ↘ failed
```

- [ ] 记录 entry source：`empty_chat | chat_header | quick_note`；
- [ ] `opening` 期间重复点击只保留第一次合法交接；
- [ ] `ready` 前不得关闭 Quick Note；
- [ ] `failed` 保持 Quick Note 可用并清除交接锁；
- [ ] 快速打开/关闭和 Android back 不产生双层残留。

新增 `src/daily-brief/surface-state.test.js`，用现有 Vitest 覆盖所有转换，不增加 Vue Test Utils。

### 6.3 新增 `src/components/TodayBriefCard.vue`

- [ ] 只渲染一个 focus、最多两个 attention 和分类数量；
- [ ] 不直接加载后端数据，不拥有 Surface 开关；
- [ ] 对事实、建议、partial/stale 使用不同的低干扰文案和语义；
- [ ] 只使用 Lucide、现有 CSS 变量和 compact 尺寸；
- [ ] 超长标题截断但可在 L2 查看完整内容。

### 6.4 新增 `src/components/TodayLayerSurface.vue`

- [ ] 所有入口只挂载这一个组件实例；
- [ ] L1 直接复用空对话使用的 `TodayBriefCard`，不重新实现卡片布局或文案；
- [ ] PC 为有边界的紧凑面板，手机为非全屏单列面板；
- [ ] 周围原界面保持可见，超长内容只在 Surface 内滚动；
- [ ] 支持 focus trap、Escape、Android back、关闭按钮和返回焦点；
- [ ] first paint 后发出 `ready`，供 Quick Note 无感交接；
- [ ] 展开 L2 时仍复用同一 snapshot，不重新创建另一种 Brief；
- [ ] 动画遵守 reduced-motion。

### 6.5 App 级接入 `src/App.vue`

- [ ] 在 `QuickNoteOverlay` 同级全局挂载唯一 `TodayLayerSurface`；
- [ ] provide `openTodayLayer` 给 ChatView 和其他入口；
- [ ] Android back 优先级调整为 Today Layer → Quick Note → 其他抽屉；
- [ ] Today Layer 不修改 `activeDrawer`，不创建新 conversation，不插入消息；
- [ ] PC/手机使用同一组件，只通过响应式样式调整。

完成信号：使用开发 fixture 可以从三个入口打开完全相同的 Surface，尚未替换 Morning Brief 和 Quick Note。

## 7. Batch 4：普通对话与现有 Morning Brief 收口

### 7.1 修改 `src/views/ChatView.vue`

- [ ] 空对话使用 `TodayBriefCard` 显示 L1；
- [ ] 对话标题或紧凑入口调用同一个 `openTodayLayer('chat_header')`；
- [ ] 空状态卡片展开调用 `openTodayLayer('empty_chat')`；
- [ ] 开始发送消息后 L1 收起为低干扰入口；
- [ ] 不新增固定 Daily Brief conversation。

### 7.2 修改 `src/composables/useChat.js`

- [ ] 删除把完整 Brief 拼接为用户输入再发送的 `onBriefingChat` 路径；
- [ ] “继续上次工作”使用结构化 conversation/summary/goal 引用；
- [ ] 只有用户选择延续或当前输入高置信相关时才注入对应摘要；
- [ ] 不把最近三个摘要无条件扩大到每个普通问题；如该旧逻辑仍由 `llm.rs` 注入，则本批先增加可关闭的相关性门并建立回归测试。

### 7.3 收口 `src/components/MorningBriefing.vue`

- [ ] 将 Dream 读取职责移入 Daily Brief source；
- [ ] 移除硬编码中文、Emoji、原始 Markdown 拼接和自身 dismiss 状态；
- [ ] 所有引用迁移完成后删除组件；若仍有兼容入口，只保留一层无状态 wrapper 并标明删除条件；
- [ ] `system_dismiss_dream` 不再等同于关闭 Today Layer，Dream 是否已处理由其自身状态维护。

### 7.4 普通入口一致性验证

- [ ] 空对话、对话标题和重新打开都显示同一 snapshot revision；
- [ ] 三个入口的 L1/L2、按钮、错误、动画和关闭行为一致；
- [ ] 关闭 Surface 不改变对话内容、标题或当前模型；
- [ ] “继续上次工作”不会出现伪造的用户消息。

完成信号：普通对话侧 Today Layer 完成，现有对话与 Goal 回复不回归。

## 8. Batch 5：Quick Note 无损交接

### 8.1 修改 `src/components/QuickNoteOverlay.vue`

- [ ] 保持悬浮气泡点击后的默认界面和 `captureQuickNote` 提交流程不变；
- [ ] 在现有底部控制栏增加小型 Daily Brief 按钮，可显示低干扰待处理数量；
- [ ] 按钮只发出 `open-today-layer`，不加载或渲染 Brief；
- [ ] 将 `close()` 扩展为明确的普通关闭与 `preserveDraft` 交接关闭；
- [ ] 交接关闭保存未提交草稿，下一次打开 Quick Note 恢复；
- [ ] 普通保存成功后仍清空草稿；普通 Escape/遮罩关闭维持现有产品语义；
- [ ] 防止连续点击、Enter 提交与交接同时发生。

### 8.2 修改 `src/App.vue`

- [ ] 接收 Quick Note 的交接事件并调用唯一 `openTodayLayer('quick_note')`；
- [ ] 等待 `TodayLayerSurface` 发出 ready 后再调用 Quick Note 的 preserve-draft close；
- [ ] Surface 打开失败时不关闭 Quick Note，并把本地化错误返回速记浮层；
- [ ] 交接完成后正确转移键盘焦点和屏幕阅读器上下文；
- [ ] 关闭 Today Layer 后回到原应用上下文，不自动重开 Quick Note。

### 8.3 Quick Note 验收

- [ ] 单击悬浮气泡仍只打开灵感输入；
- [ ] Daily Brief 按钮不会抢占输入框主视觉；
- [ ] Today Layer 先出现、Quick Note 后淡出，无白屏或遮罩残留；
- [ ] 输入一半的草稿在交接后再次打开仍存在；
- [ ] Today Layer 失败时输入、保存和扫码仍可用；
- [ ] PC Logo/快捷键和手机 FAB 的 Quick Note 行为一致。

完成信号：Quick Note 主职责未变化，独立 Today Layer 交接可恢复、可失败降级。

## 9. Batch 6：结构化 Action、可选 Clerk 与 Dream

### 9.1 结构化 Action 路由

- [ ] 打开对象类 Action 只导航到已有 Goal、Decision、Calendar、Todo 或 conversation；
- [ ] 审批类 Action 继续调用 Goal Runtime 的 revision/Policy Engine 命令；
- [ ] R2/R3 不因来自 Brief 而降低确认要求；
- [ ] Action 执行后重新读取源对象和 snapshot，不乐观伪造成功；
- [ ] revision 冲突保留卡片并提示刷新。

### 9.2 可选 Clerk

- [ ] 只在候选软排序或多条变化压缩确有必要、且 source fingerprint 已变化时调用已有 Clerk；
- [ ] Clerk 输入只含必要摘要和 reason codes，不含完整文件、密钥或私有推理；
- [ ] Clerk 输出使用严格 JSON，Rust 校验后只能调整软排序和摘要；
- [ ] Clerk 不能更改源状态、风险、截止时间、审批要求和 Action；
- [ ] 超时、无模型、断网和非法 JSON 立即使用确定性模板；
- [ ] 打开 Surface 本身不固定触发模型调用。

### 9.3 Dream 接入

- [ ] 读取已有结构化洞察；若当前报告只有 Markdown 字符串，先增加向后兼容的 `insights[]`，不解析展示层 HTML；
- [ ] 只有带来源、置信度和当前工作关联的洞察进入候选；
- [ ] Dream 只进入低优先级或详情，除非它识别到有证据的风险；
- [ ] 用户可查看来源、忽略或纠正，不把推断写成事实。

完成信号：模型能力提高表达质量但不是可靠性前提，Dream 不污染首屏。

## 10. Batch 7：i18n、预览、回归与文档

### 10.1 i18n

- [ ] `src/locales/zh-CN.json` 与 `src/locales/en-US.json` 同步所有标题、reason code、状态、操作和错误；
- [ ] Surface 打开时读取当前 locale，切换语言后立即重渲染；
- [ ] 持久 Snapshot 不保存最终显示语言；
- [ ] 删除相关路径中的 Emoji 和用户可见硬编码文案。

### 10.2 浏览器预览

- [ ] 新增 `src/daily-brief/preview-fixtures.js`，覆盖 fresh、partial、stale、empty、超长标题、两个待确认和交接失败；
- [ ] 仅在开发模式和明确 query 参数下启用，不影响生产构建；
- [ ] `npm run dev` 的 `localhost:5173` 可分别验证 PC 紧凑布局和手机窄屏；
- [ ] 预览使用与真实入口同一个 `TodayLayerSurface`，不另写演示 UI。

### 10.3 自动回归

```powershell
npm test
npm run build
$env:CARGO_TARGET_DIR='D:\ignore_sync\bob-agent-today-layer-target'
Set-Location src-tauri
cargo test daily_brief --lib
cargo test --lib
```

- [ ] Vitest 状态机与已有 sync reducer 测试通过；
- [ ] Rust daily_brief 专用测试通过；
- [ ] Rust 全量 lib tests 通过；
- [ ] 生产前端构建通过；
- [ ] `git diff --check` 通过；
- [ ] package/Cargo manifests 和 lockfiles无意外变化。

### 10.4 原生验证

- [ ] 先用 `npm run dev` 验证纯 UI 和 fixtures；
- [ ] 再用 `npm run tauri dev` 或等价 Tauri 开发命令验证 SQLite、Quick Note、键盘焦点和 Android back；
- [ ] 真机验证 360 px 左右宽度、输入法弹出、FAB 拖拽、草稿恢复和 reduced-motion；
- [ ] 编译产物只允许进入既有 `dist` 和映射后的 `src-tauri/target`，不创建新的仓库缓存目录。

### 10.5 文档同步

- [ ] `docs/ARCHITECTURE.md`：记录 Daily Brief 派生层和真相边界；
- [ ] `docs/FEATURES.md`：只写已通过验收的用户能力；
- [ ] `docs/USER_GUIDE.md`：说明对话、悬浮气泡、Quick Note 和 Today Layer 的使用方式；
- [ ] `LLM_WIKI.md`：更新模块、IPC 和代码导航；
- [ ] `docs/DECISIONS.md`：记录 one-surface 与 local-first 决策；
- [ ] `docs/BOB_EVOLUTION_ROADMAP.md`、`todo.md`、`progress.yaml`：按真实完成状态更新；
- [ ] `AGENTS.md`：只在 Daily Brief 成为新的 JIT 路由时补充。

完成信号：自动测试、原生体验、文档和真实状态一致。

## 11. 分批提交策略

在当前重叠工作区安全允许的前提下，建议提交边界为：

1. `feat(daily-brief): add deterministic snapshot service`
2. `feat(daily-brief): add cached device-aware commands`
3. `feat(ui): add shared today layer surface`
4. `feat(chat): connect conversation continuity to today layer`
5. `feat(quick-note): add lossless today layer handoff`
6. `feat(daily-brief): add structured actions and optional insights`
7. `docs: document conversation-first today layer`

每次提交前必须运行本批最小测试并检查 staged diff。若无法与现有未提交 Phase 5 hunk 安全分离，则暂停提交而不是把两条开发线混在一起。

## 12. 最终完成门

- [ ] 启动默认进入对话，Today Layer 不成为新的主导航页；
- [ ] L1 在所有数据条件下都满足 `1 + 2` 信息预算；
- [ ] 普通对话、对话标题和 Quick Note 打开同一个 Surface 和 snapshot；
- [ ] 悬浮气泡单击仍只打开灵感记录；
- [ ] Quick Note 到 Today Layer 交接没有闪屏、草稿丢失或焦点丢失；
- [ ] 离线、无模型和单一来源失败仍能生成可理解 Brief；
- [ ] 来源失败不会伪装为零项，Action 失败不会伪装为完成；
- [ ] 继续旧会话不再复制整份 Brief 为用户消息；
- [ ] 所有用户可见文字跟随当前语言，无 Emoji 和硬编码双语漂移；
- [ ] PC 紧凑模式与 Android 真机均通过；
- [ ] 未新增用户侧运行时，依赖和安装包体积变化有记录；
- [ ] Work Core、Goal Runtime、Calendar、Todo、Conversation 和 Dream 的真相边界未改变；
- [ ] 文档只陈述已经验证的能力；
- [ ] 未推送、未发布、未升版本，除非用户另行授权。
