# Conversation-first Today Layer 设计

> 状态：设计已确认，尚未实现
>
> 日期：2026-08-11
>
> 产品范围：Bob 对话首页、Daily Brief、跨对话工作延续
>
> 前置能力：会话摘要、Dream 报告、Calendar/Todo、Persistent Work Core、Goal Runtime

## 1. 目的

Bob 的默认入口回归对话，用户不需要先理解“工作”“Goal”“项目状态”或其他内部概念。系统把昨天进展、今天安排、待确认事项、进行中 Goal、会话延续和 Dream 洞察收敛为一个低密度的 **Today Layer**，帮助用户在十秒内回答：

1. 现在最值得做什么；
2. 有什么必须由我决定；
3. 自上次查看后发生了什么变化。

Today Layer 不是新的工作数据源，也不是独立的工作页面。对话是唯一自然语言入口；Work Core、Calendar、Todo、Conversation Summary 和 Dream 继续维护各自的权威状态，Today Layer 只负责聚合、排序、解释和跳转。

## 2. 已确认的产品选择

采用“方案 C：优先级卡片”，拒绝完整仪表盘和长篇 AI 日报。

### 2.1 信息预算

首次展开时只允许出现：

- 1 个“建议先做”的主项；
- 最多 2 个“需要你”的待确认项；
- 其余内容只显示数量和简短状态，不展开列表。

内容排序固定为：

1. 等待用户的审批、选择或补充；
2. 今天到期、即将开始或已经产生风险的事项；
3. 自上次查看后发生的变化；
4. Bob 建议的下一步；
5. 昨日回顾、Dream 洞察和其他信息。

Dream 洞察只有在与当前工作相关、可解释且能形成行动建议时才进入首层。维护统计、知识整理数量和一般性总结只能进入详情。

### 2.2 三层呈现

- **L0 提醒层**：一个入口图标和待处理数量；不展示段落。
- **L1 决策层**：十秒可读的主项、最多两个待确认项和其余分类数量。
- **L2 详情层**：按“需要你、今天、进行中、昨日变化、洞察”展开完整来源和操作。

用户关闭 L1 只是标记已查看，不删除简报。入口始终可重新打开；再次打开时优先显示“自上次查看后的变化”。

## 3. 用户体验

### 3.1 启动与新对话

- 启动 Bob 后默认进入对话，不进入独立 Work 页面。
- 当天首次打开一个空的新对话时，在输入框上方展示 L1 Today Layer。
- Today Layer 不创建固定的“每日日报对话”，也不自动向聊天记录插入一条消息。
- 用户开始输入或发送消息后，卡片收起为 L0；它不占据持续对话的正文空间。
- 新建或切换对话时，现有会话摘要流程继续运行，但摘要不会无条件注入每一次模型请求。

### 3.2 延续上一次工作

“继续上次工作”可以成为主项或详情项，但必须满足相关性和时效性门槛。用户点击后传递结构化的 `conversation_id`、`summary_id` 和可选 `goal_id`，而不是把整份简报复制成一条用户消息。

只有用户明确选择延续，或当前输入与旧会话形成高置信关联时，相关摘要才进入本轮模型上下文。其他最近会话只参与候选排序，不默认注入。

### 3.3 PC

- 空对话内展示紧凑 L1 卡片；宽屏可在右侧打开 L2 抽屉，但不永久占用主画面。
- 对话标题区保留“今日 · N 项待处理”入口；`N` 只统计确实需要用户处理的事项。
- 点击详情项可以定位到对应 Goal、Decision Review、Calendar、Todo 或原会话；WorkView 只作为深层管理与诊断界面，不再与对话争夺主入口。

### 3.4 手机

- 使用单列布局，不复制 PC 右侧栏，不使用撑满屏幕的大弹窗。
- L1 首屏只展示主项、最多两个待确认项和分类数量；详情使用可滚动的底部面板或当前页面内展开。
- 主要操作保持在拇指可达区域；展开详情后仍能随时回到对话输入。
- 点击移动端悬浮气泡仍然只打开现有“灵光一现”速记浮层，不直接展示 Daily Brief。
- 同一事项在 PC 和手机上使用相同的稳定对象 ID、状态和操作语义，仅改变排版。

### 3.5 悬浮气泡、速记与 Today Layer

悬浮气泡的首要且不可改变的职责是快速记录灵感。现有交互保持为：

```text
点击悬浮气泡 → 打开 QuickNoteOverlay → 输入灵感并快速保存
```

`QuickNoteOverlay` 的核心仍是紧凑输入条和保存反馈。Daily Brief 只作为底部快捷控制栏中的一个小型次级按钮，与模型选择、扫码等快捷入口并列；按钮可用主题色或灰阶显示待处理数量，但不能用高饱和错误色制造紧迫感。

点击 Daily Brief 按钮采用“浮层交接”，而不是页面跳转或浮层嵌套：

1. `QuickNoteOverlay` 发出打开 Today Layer 的结构化事件；
2. 全局 `TodayLayerSurface` 先挂载，并用缓存快照或轻量骨架进入可见状态；
3. 确认 Today Layer 已渲染后，Quick Note 再淡出并释放焦点；
4. 焦点进入 Today Layer 的第一个可操作元素；
5. 关闭 Today Layer 后回到原来的应用上下文，不自动重开 Quick Note。

Today Layer 是独立的轻量表层，但不是独立导航页，也不占满整个画面。PC 使用有最大宽度和高度的紧凑面板；手机使用保留周围界面可见的单列浮层或底部面板，内容超出时只在面板内部滚动。

如果用户在速记框中已经输入文字，浮层交接必须保留草稿且不得自动提交。用户下次打开速记时可以继续编辑。若 Today Layer 无法挂载，则 Quick Note 保持打开并显示可读错误，不能先关闭后留下空白画面。

### 3.6 时间语义

底层对象统一称 `DailyBrief`，界面根据时间和上下文显示“早上好”“今日简报”或“自上次查看后”。

- 当天首次查看：突出昨日至今的变化和今天的优先事项；
- 当天再次查看：突出增量变化；
- 没有增量：显示简短“暂无新变化”，仍允许进入详情；
- 跨日未查看：合并为最近一次有效变化，不逐日堆叠多份日报。

## 4. 数据来源与真相边界

Today Layer 只读聚合以下来源：

| 来源 | 提供内容 | 权威状态仍由谁维护 |
|---|---|---|
| Work Core | Project、Goal、Decision、Risk、Change、Commitment | Work Core |
| Goal Runtime | 运行阶段、阻塞、审批、Evidence 和下一步 | Goal Runtime |
| Calendar | 今天日程、临近事件和冲突 | Calendar |
| Todo | 到期、逾期和高优先级待办 | Todo |
| Conversation Summary | 可延续的旧对话与未完成承诺 | Conversation Store |
| Dream/Memory | 有证据的偏好、洞察和工作建议 | Dream/Memory |

Brief 中的“完成”“等待”“失败”“需要确认”必须来自权威对象状态，不能由摘要文案推断。某一来源不可用时，不得把“读取失败”显示为“零项”。

## 5. Daily Brief 契约

建议增加一个可序列化、可缓存但可重建的只读契约：

```text
DailyBriefSnapshot
  schemaVersion
  snapshotId
  localDate
  generatedAt
  sourceRevisions
  status                 fresh | partial | stale
  focusItem              0..1
  attentionItems         0..2
  sectionCounts
  detailItems
  changedSinceLastSeen
  warnings
```

每个 `DailyBriefItem` 至少包含：

```text
itemId
sourceType
sourceId
kind
title
summary
priority
reasonCodes[]
occurredAt
dueAt
action
evidenceRefs[]
```

`itemId` 必须稳定，避免同一事项因重新生成简报而重复出现。`action` 是结构化动作，例如打开对象、继续会话、回应审批或开始建议步骤；不得保存为待执行的自然语言提示词。

Daily Brief 是派生视图，不参与跨端冲突合并。跨端同步的是源对象和真实操作结果；每台设备只保存自己的 `lastSeenSnapshotRevision`，因此在 PC 看过不会让手机误以为用户已经在该设备查看过。用户作出的审批或状态更新则正常同步并从两端待处理数量中消失。

## 6. 聚合与排序

### 6.1 确定性优先

首轮聚合不依赖大模型：

- 读取本地 SQLite 和 Markdown 索引；
- 根据状态、日期、截止时间、风险和 revision 生成候选项；
- 去重同一源对象；
- 使用固定优先级和 reason code 排序；
- 截取 1 个 focus、2 个 attention，其余只计数。

这样在断网、API 余额不足或模型不可用时，用户仍能看到可靠简报。

### 6.2 可选 Clerk

只有候选项存在语义歧义、需要把多个相关变化压缩成一句话，或需要在接近的候选中做软排序时，才允许调用已配置的轻量 Clerk。Clerk 只能重写和建议排序，不能改变源状态、截止时间、风险等级或审批要求。

Clerk 超时、失败或未配置时直接使用确定性模板，不阻塞首屏。打开简报不应固定触发一次模型调用；只有源 revision 变化且确实需要语义压缩时才异步更新缓存。

### 6.3 Dream

Dream 继续独立运行。Today Layer 读取已经形成的洞察，不为打开页面临时触发“做梦”。进入 L1 的洞察必须：

- 有来源或近期行为证据；
- 与当前 Goal、项目或今天的安排相关；
- 可被用户查看、纠正或忽略；
- 不包含敏感原文和模型私有推理链。

## 7. 组件与模块边界

### 7.1 Rust

新增独立 `daily_brief` 边界，避免继续把聚合、Dream 和视图逻辑堆入现有组件：

```text
daily_brief/
├── models.rs       # Snapshot、Item、Action 与 reason code
├── sources.rs      # 各权威来源的只读适配器
├── ranker.rs       # 确定性筛选、去重和排序
├── service.rs      # 缓存、增量、partial/stale 状态
└── commands.rs     # 获取、刷新、标记设备已查看
```

该模块不得写入 Goal、Todo、Calendar 或 Dream。用户执行结构化 action 时，仍调用相应模块的既有命令和 Policy Engine。

### 7.2 Bridge

Vue 只通过 `window.electronAPI.*` 使用 Daily Brief。所有 Tauri `invoke` 继续集中在 `src/tauri-bridge.js`。Bridge 暴露获取快照、按需刷新和标记本设备已查看的最小接口，不暴露数据库细节。

### 7.3 Vue

将当前 `MorningBriefing.vue` 拆为职责清晰的组件：

- `TodayBriefCard`：L1 决策层；
- `TodayBriefDrawer`：L2 详情层；
- `TodayBriefLauncher`：对话标题等常规入口的 L0 图标和数量；
- `TodayLayerSurface`：由速记底部按钮或其他入口打开的独立轻量表层；
- `useDailyBrief`：加载、缓存、刷新、已查看和 action 路由。

现有 Morning Brief 的 Dream 内容成为一个数据来源，不再由单个大组件直接拼接所有报告。用户可见文字全部进入 `zh-CN.json` 和 `en-US.json`，只使用 Lucide 图标和现有语义化颜色变量。

`QuickNoteOverlay` 只增加一个小型 Daily Brief 按钮和无损交接事件，不接收 Brief 数据，也不渲染 Brief 内容。Quick Note 与 Today Layer 通过 App 级协调状态交接，避免两个组件互相调用或共享内部实现。

## 8. 交互规则

- “关闭”只收起，不清除；“稍后处理”可以更新提醒时间，但不能伪装成完成。
- 完成、审批、取消和推迟必须调用源对象动作，成功后依据新 revision 刷新 Brief。
- 点击主建议不会立即执行 R2/R3 行为；它只是进入对话或相应确认流程。
- 悬浮气泡点击语义保持“记录灵感”；只有用户再次点击速记底部的 Daily Brief 按钮才打开 Today Layer。
- Today Layer 必须先进入可见状态，Quick Note 才能关闭；任何未提交速记草稿在交接中保持原样。
- 来源不足的推荐显示“建议”而非“必须”；状态事实与 Bob 推断在视觉和文案上分开。
- L0 数量仅统计 `waiting_user`、明确业务选择和即将逾期的必要动作，不把普通日程、进行中 Goal 或 Dream 条目计入红点。
- 错误色仅用于真实错误；普通待处理和进行中状态使用主题色与灰阶，避免视觉抢夺。

## 9. 降级与错误处理

- **离线**：展示本地源数据；跳过 Clerk 和远程 Dream 更新。
- **单一来源失败**：快照标为 `partial`，保留其他来源并指出未更新的类别。
- **缓存过期**：先展示旧快照及“更新于…”标记，后台重建；不能出现空白首屏。
- **浮层交接失败**：保持 Quick Note 可见和可输入，显示简短错误；不得关闭速记后停留在遮罩或空白画面。
- **动作失败**：保留原条目，显示源模块返回的可读错误和重试入口。
- **跨端 revision 冲突**：重新拉取权威对象；不得按旧卡片重复执行。
- **无任何内容**：保持 Bob 品牌空状态，仅显示“今天没有需要你处理的新事项”，不生成虚假建议。

所有失败都使用当前 UI 语言。日志和用户可见错误保存结构化事件代码，显示时再本地化，不把英文写死到持久记录中。

## 10. 性能、依赖与隐私

- 首屏只依赖本地查询，目标是在普通设备上 300 ms 内得到可展示快照；可选语义压缩异步完成。
- 不新增 Python、Node、MCP、常驻服务或用户侧运行时。
- 优先使用现有 Rust、SQLite、Vue 和 Lucide 依赖；新增依赖必须单独证明必要性和体积影响。
- Snapshot 只保存摘要、稳定引用和 source revision，不复制完整对话、文件或知识正文。
- 不记录模型私有推理链；Dream 洞察必须遵守现有 Memory scope、evidence、confidence 和可删除边界。

## 11. 验收标准

### 11.1 产品验收

- Bob 启动后默认是对话；普通用户不需要进入 Work 页面才能继续工作。
- L1 永远不超过一个主项和两个待确认项。
- 用户关闭简报后能从对话标题重新打开，也能通过“悬浮气泡 → 速记底部小按钮”重新打开。
- 单击悬浮气泡只出现速记输入，不会直接展示或展开 Today Layer。
- 点击速记底部 Daily Brief 按钮后，Today Layer 先显示、速记后淡出，全程没有闪屏或空白状态。
- 已输入但未提交的速记草稿在交接后仍然保留。
- 同日再次查看优先展示增量，不重复整篇昨日总结。
- PC 适合快速扫读和侧向展开；360 px 宽手机保持单列、可滚动且输入框可返回。
- 点击“继续上次工作”不再把完整简报伪装成用户消息。
- Work、Calendar、Todo、Goal 和 Conversation 的真实状态不会被 Brief 复制或覆盖。

### 11.2 自动测试

- ranker 对相同输入产生稳定顺序，且强制执行 `1 + 2` 上限；
- 同一 source ID 不重复生成条目；
- `lastSeenSnapshotRevision` 正确计算增量，并按设备隔离；
- 一个来源失败时返回 `partial` 而非空数组或假零项；
- 离线、无模型、Clerk 超时和 Dream 缺失时仍能生成确定性 Brief；
- 过期卡片上的 action 遇到 revision 冲突时安全刷新，不重复副作用；
- 中英文 locale key 完整，无 Emoji 和用户可见硬编码文本；
- Vue 组件覆盖空状态、部分失败、超长标题、两项待确认、详情展开和收起后重开；
- 浮层测试覆盖仅打开速记、Today Layer 成功交接、交接失败、草稿恢复和连续快速点击；
- PC 紧凑模式与手机窄屏完成视觉回归；
- 生产构建通过，并记录 PC 安装版、绿色版和 Android APK 的体积变化。

## 12. 非目标

本设计不包含：

- 新建一个固定的 Daily Brief 对话；
- 把悬浮气泡改造成 Daily Brief 启动器，或在 Quick Note 内直接渲染 Brief；
- 让 Today Layer 成为全屏页面或新的主导航入口；
- 让 Today Layer 成为新的 Work 数据库或同步真相源；
- 删除 WorkView 的深层管理和诊断能力；
- 在首屏展示完整 Calendar、Todo、Goal 或 Dream 报告；
- 因打开简报自动执行外部操作；
- 新增多 Agent、Dynamic DAG、Runtime Host 或后台常驻能力；
- 为 Daily Brief 单独要求用户配置模型、MCP 或云服务。

## 13. 实施顺序建议

正式实施计划应按以下纵向切片展开：

1. 定义 Daily Brief 契约、来源适配器与确定性 ranker；
2. 建立缓存、按设备增量游标和 partial/stale 降级；
3. 以 `TodayBriefCard` 替换当前 Morning Brief 首层，并接入结构化 action；
4. 加入 PC 详情抽屉、手机单列详情、独立 `TodayLayerSurface` 和 Quick Note 无损交接；
5. 接入可选 Clerk 和已有 Dream 洞察；
6. 完成离线、冲突、i18n、视觉与包体积回归。

每一步都必须保持对话可用，不能等全部来源接完后才出现首个可测试版本。
