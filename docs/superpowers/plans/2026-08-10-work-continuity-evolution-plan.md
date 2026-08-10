# Bob Work Continuity 全面演进计划

> 状态：待产品评审，不代表已实现能力
> 基线：`v0.8.0` / `8b7318c`
> 目标：在保留现有轻量客户端、Capture、知识、日历、同步和工具能力的前提下，把 Bob 演进为面向知识工作者的 Personal Work Orchestration Layer。

## 1. 北极星与验收问题

Bob 的北极星是：**让复杂工作不断线。**

每项新增能力必须至少改善以下一项：

- understanding：更准确理解用户负责的工作及其上下文；
- continuity：跨对话、跨时间、跨设备继续推进；
- traceability：说明结论、决定和状态来自哪里；
- execution：把信息转化为可验证的行动和结果；
- recovery：中断、失败或更换 Runtime 后能够恢复。

最终产品必须通过五项验收：

1. 一个月后重新进入项目，Bob 能准确说明目标、进展、决定、风险和下一步。
2. 新文件进入后，Bob 能指出它改变、补充或挑战了哪个旧结论。
3. 会议结束后，Bob 能提取并更新 Decision、Task、Owner 和 Deadline。
4. Advanced 工作中断或更换模型/Runtime 后，项目状态和证据不丢失。
5. 普通用户无需理解 DAG、MCP、模型路由或执行进程。

## 2. 路线选择

### 方案 A：在现有 Bob 中建立隔离的 Work Core（采用）

保留 Tauri、Android、Capture、Notes、Calendar、Sync、Relay、Tools 和现有 UI，在 Rust 中新增明确边界的 Work Core 与 Orchestration 模块，通过稳定 ID 和 Repository 接口逐步接入旧功能。

优势：复用成熟产品能力；迁移可逆；每一阶段都能交付用户价值；符合轻量、零依赖和 evolve-not-rewrite 原则。风险是过渡期存在兼容层，需要严格限制双写。

### 方案 B：新建 Bob 2

重新设计数据库、Runtime 和 UI，再迁移现有能力。代码初期更整洁，但会重复实现安装、移动端、同步、模型、技能和发布体系，在验证 Work Continuity 前产生长期双线维护。本阶段不采用。

### 方案 C：继续在现有大模块中直接叠加功能

开发速度最初最快，但 Project、Goal、Note、Graph 和 Chat 会继续共享隐式状态，难以形成可替换 Runtime 和持久项目状态。本阶段不采用。

## 3. 长期架构边界

```text
Bob Product UI
    ↓
Capture / Calendar / Notes / Files / Messages
    ↓
Work Core
    ├── Project State
    ├── Work Objects
    ├── Decision / Change / Evidence
    └── Work Event Journal
    ↓
Complexity Router
    ├── Direct
    ├── Deep
    └── Advanced Project Runtime
            ↓
        Goal / Dynamic Task Graph
            ↓
        Lead–Clerk Orchestration
            ↓
        Agent Runtime Adapter
            ↓
        API / Codex / AGY / Other
```

边界原则：越接近用户长期工作模型，越必须由 Bob 持有；越接近模型和执行环境，越必须可替换。

## 4. Canonical State 规则

为避免 Markdown 与 SQLite 再次形成两套互相竞争的真相源，按领域划分所有权：

| 数据 | 权威来源 | 可重建/镜像 |
|---|---|---|
| Source、Note、Knowledge Point、Memory | Markdown | SQLite 搜索、关系和图谱索引 |
| Project、Goal、Milestone、Task、Decision、Risk、Commitment | Bob SQLite + Work Event Journal | 可迁移 Markdown 项目快照 |
| Artifact 正文 | 原始文件或对应外部系统 | Bob 只保存 ID、版本、哈希、位置和证据 |
| Calendar Event | 对应日历系统；纯本地事件为 Bob SQLite | Project Link 和索引 |
| Email/Message | 原邮件或消息系统 | Bob 保存引用、摘要和关系 |
| Agent 对话 | 非权威上下文 | 只保留会话、引用和派生关系 |

现有 Markdown Project 对象不会立即删除。首次接入时以原稳定 ID 注册进 Work Core；之后运行状态以 SQLite 为准，Markdown 生成可读快照，不进行无约束双向覆盖。

## 5. 核心对象与关系

### 5.1 第一批对象

- Project：持续工作的容器，保存使命、状态和边界。
- Responsibility：用户长期负责的职责，可关联多个 Project。
- Goal：可验证终态，不等于任务列表。
- Milestone：Goal 的阶段性状态节点。
- Task：边界明确、可以执行或委派的工作单元。
- Decision：结论、理由、替代方案、证据和重审条件。
- Artifact：报告、表格、图纸、邮件草稿等工作产物的元数据。
- Evidence：支持完成、判断或决定的可检查事实。
- Risk：可能影响 Goal 的不确定因素。
- Change：新信息对现有工作状态造成的变化。
- Commitment：某人承诺在某时完成某事。

### 5.2 第一批关系

`belongs_to`、`owned_by`、`depends_on`、`blocks`、`supports`、`contradicts`、`supersedes`、`derived_from`、`decided_in`、`affected_by`、`due_at`、`assigned_to`、`produced_by`。

所有关系必须包含稳定 ID、来源、创建时间和置信度；推断关系与用户确认关系必须可区分。

## 6. 推荐模块结构

第一阶段只建立实际需要的文件，不一次性创建空目录：

```text
src-tauri/src/work_core/
├── mod.rs
├── models.rs          # Work Object 数据契约
├── repository.rs      # 唯一持久化边界与事务
├── project_state.rs   # 项目聚合与恢复
├── decisions.rs       # Decision 校验、版本和重审
├── events.rs          # append-only 工作事件
└── export.rs          # 可迁移 Markdown 快照

src-tauri/src/orchestration/       # 后续阶段按需创建
├── complexity_router.rs
├── project_runtime.rs
├── graph.rs
├── scheduler.rs
├── evidence.rs
└── runtime_adapter.rs
```

Vue 继续只通过 `window.electronAPI.*` 访问能力，Tauri `invoke` 不进入组件。新模块不增加 Python、Node、常驻 sidecar 或客户端运行时依赖。

## 7. 分阶段路线

### Phase 0：Re-anchor 与术语收口

目标：把 `references/bob_new` 中已确认的方向转化为项目权威约束。

工作：

- 更新 `docs/PRODUCT_VISION.md`，保留“普通白领、最小设置、轻量交付”，加入 Work Continuity 北极星；
- 更新 `docs/ARCHITECTURE.md` 和 `AGENTS.md`，明确 Bob Core、Orchestration、Runtime、Integration 边界；
- 将旧 Goal/Dream 文档中与新定义冲突的表述改为兼容或历史说明；
- 建立 Decision Log，记录 canonical state、Work Core 边界和不重写策略；
- 更新 `todo.md` 与 `progress.yaml`，避免 Capture 路线和 Work Continuity 路线并列竞争。

完成门槛：仓库只有一套正式产品北极星；目标架构与已实现能力明确分开。

### Phase 1：Persistent Work Core

目标：证明项目状态独立于聊天上下文而存在。

工作：

- 新增版本化 Work Object schema；
- 建立 Project、Goal、Milestone、Task、Decision、Artifact、Evidence、Risk、Change、Commitment 表；
- 建立 append-only `work_events`，任何状态改变可追溯；
- Repository 统一负责事务、乐观版本、幂等键和软删除；
- 建立项目聚合读取：目标、当前阶段、开放任务、决定、风险、近期变化和下一步；
- 支持从现有 Markdown Project 唯一 ID 注册，不移动真实文件；
- 生成可迁移的 Markdown 项目快照，快照不可反向覆盖运行状态；
- 新增最小 Project 页面：概况、目标、任务、决定；移动端只展示紧凑摘要和用户需关注项。

完成门槛：关闭应用并新建对话后，Bob 仍能通过 Project ID 恢复完整项目摘要；Decision 能解释理由和证据；所有写入有事件记录。

### Phase 2：现有入口关联 Work Core

目标：让已有 Capture 成为项目现实状态的可靠输入。

工作：

- Capture 分类增加 Project、Decision、Meeting、Change、Commitment 候选；
- Note 仍写 Markdown，但可以通过稳定 ID 关联一个 Project；
- Source/Knowledge Point 可被多个 Project 引用，不改变知识归属；
- Todo/Event 与 Work Task/Milestone 建立引用而不是复制；
- 文件拖拽产生 Document/Artifact 引用、哈希和版本；
- 低置信度项目关联进入低干扰确认；
- 一次 Capture 的多个派生对象在同一事务内提交，失败不产生半套项目状态。

完成门槛：会议记录、文章、笔记、任务、日程和文件能更新同一个 Project，且每项变化可追溯到原始 Capture。

### Phase 3：Decision Memory 与 Change Detection

目标：从“存资料”升级为“理解什么变了以及为什么”。

工作：

- Decision 原生保存 reason、alternatives、rejected alternatives、participants、owner、evidence、revisit condition；
- 新文件通过内容哈希、文件名、来源和显式关系识别版本；
- Change Detector 比较新信息与当前 Project State；
- 只自动更新确定性事实，冲突、替代和影响判断生成候选；
- 用户确认后写入 `supersedes`、`contradicts`、`affected_by` 等关系；
- 对 deadline、commitment、decision challenge 和风险变化产生用户可读通知。

完成门槛：给出新版文件时，Bob 能指出变化、受影响对象、证据和需要确认的决定。

### Phase 4：Complexity Router

目标：用户只自然表达，系统选择最轻量的可靠处理方式。

模式：

- Direct：一次回答或单步低风险操作；
- Deep：有限循环内的强推理、工具和验证；
- Advanced：需要持久项目状态、阶段、依赖、恢复或跨时间推进。

工作：

- 路由输出结构化原因、置信度、风险和预计持续性；
- 使用确定性信号优先，复杂语义才调用模型；
- 用户可以覆盖自动判断；
- Advanced 初期只使用一个 API Agent，不依赖外接订阅；
- 建立中英文回放集，防止普通问答被过度升级。

完成门槛：简单请求不进入复杂框架；需要持续推进的目标不会在一次长回复后虚假结束。

### Phase 5：Advanced Project Loop

目标：用单 Agent 证明持久执行框架本身有价值。

工作：

- Goal Compiler 生成 outcome、evidence、scope、constraints、budget、risk policy 和 blocker policy；
- Project Runtime 持久保存执行状态、尝试、证据和检查点；
- 实现 `observe → plan → act → verify → repair → finish`；
- Done 必须绑定 Evidence，缺少证据保持 `unverified`；
- 中断、超预算、等待用户和取消具有明确状态；
- R0–R3 权限继续由现有 Policy Engine 控制。

完成门槛：应用重启后能继续一个未完成 Goal；失败能定位到步骤和验证规则。

### Phase 6：Dynamic Task Graph

目标：管理多阶段依赖，而不是展示一张静态 DAG。

工作：

- Goal → Milestone → bounded Task；
- 任务状态采用 `pending/ready/running/blocked/needs_review/accepted/failed/superseded/cancelled`；
- 实现 ready queue、依赖归约、并行上限、局部重试、下游失效和动态重规划；
- 独立节点使用干净上下文；依赖节点只接收结构化 Artifact/Evidence；
- Graph 管项目，局部 Goal Loop 完成 bounded work。

完成门槛：一个节点失败只重跑受影响的节点和下游，不重放整个项目；计划可以合法变化并保留历史。

### Phase 7：Lead–Clerk 与 Evidence Review

目标：在确有收益时引入多个执行角色。

工作：

- 建立 bounded Task Contract；
- Clerk 返回结构化 Completion Report 和证据，不接受裸 `Done`；
- 按 low/medium/high risk 选择自动检查、独立复核、Specialist 或人工批准；
- 首批只支持同一 API provider 的不同角色；
- 记录每类任务的质量、成本、延迟和失败率。

完成门槛：多角色相比单 Agent 在可测任务上明显提高完成率或减少人工关注，否则不扩大。

### Phase 8：Agent Runtime Adapter

目标：执行环境可以替换，Project State 永远留在 Bob。

统一接口至少包括：

`capabilities`、`start`、`resume`、`status`、`interrupt`、`collect_artifacts`、`close`。

接入顺序：

1. API Agent；
2. Codex；
3. AGY；
4. 其他 Runtime。

Runtime 返回的 session ID、输出和 Artifact 都只是执行记录，不能成为项目权威状态。

完成门槛：同一个未完成 Task 更换 Runtime 后可以继续，Project、Decision 和 Evidence 不丢失。

### Phase 9：Runtime Host 与 Subscription-first

目标：在不增加普通用户设置摩擦的前提下复用订阅和远程能力。

工作：

- Runtime Host 提供 register、heartbeat、launch、resume、status、interrupt、collect 和 terminate；
- Worker 按 capability、环境、区域和并发能力注册，不按机器名硬编码；
- Scheduler 支持订阅优先、API fallback、成本上限和故障降级；
- 普通用户不需要安装 Host；只在高级设置中启用；
- 无任何 subscription 时 Advanced Mode 仍完整成立。

完成门槛：Host 离线只影响对应执行资源，不破坏 Bob Core、项目状态或基本 API 执行。

### Phase 10：Work Graph Intelligence 与 Dream

目标：Bob 从经过验证的工作结果中逐渐理解用户并主动跟进。

工作：

- Dream 输入改为 Goal Contract、Decision、执行轨迹、Evidence、用户修改和最终验收；
- 区分 identity、preference、episodic、procedural、project 和 correction memory；
- 推断记忆需要累积证据，用户明确纠正立即生效；
- 根据 deadline、commitment、blocked task、decision revisit condition 主动提醒；
- 推荐行动与自动外部行动严格分离；
- 用户可查看、纠正、删除及限制记忆范围。

完成门槛：Bob 的主动行为能够引用具体项目事实和用户偏好证据，错误推断可被纠正且不再重复。

## 8. 第一实施批次：Phase 0–1

第一批不实现 Complexity Router、DAG、多 Agent 或 Runtime Host，只完成以下纵切片：

1. 正式文档对齐与 Decision Log。
2. Work Object Rust 数据结构及 schema version。
3. SQLite migration 与 Repository。
4. Work Event Journal、幂等、乐观版本和软删除。
5. Project 聚合读取与 Markdown 可迁移快照。
6. Project/Goal/Task/Decision 最小 Tauri Commands 与 Bridge。
7. 最小 Project UI，只展示用户真正需要掌握的状态。
8. 新会话恢复测试、写入回滚测试和重启恢复测试。

建议拆成四个可独立验证的提交：

- Commit A：愿景、术语、架构决策与 schema 设计；
- Commit B：Work Core 数据层、迁移和单元测试；
- Commit C：Bridge、项目聚合 API 和恢复集成测试；
- Commit D：Project 最小 UI、i18n、文档和体积回归。

## 9. 数据可靠性要求

- 所有 ID 使用类型前缀和稳定 ULID，例如 `project_`、`goal_`、`decision_`；
- 所有可变对象包含 `revision`、`created_at`、`updated_at` 和软删除时间；
- 所有命令接受 `idempotency_key`，重复请求返回原回执；
- 跨多对象写入必须使用 SQLite 事务；
- 每次状态变化写 `work_events`，事件只追加不覆盖；
- Project 聚合可从业务表重建，Markdown 快照也可重新生成；
- 同步协议增加 schema/protocol version，旧客户端不能静默覆盖新对象；
- 首版不迁移真实用户数据，只提供 dry-run、fixture 和显式导入。

## 10. UI 原则

普通用户只看到：

- 这个项目要达成什么；
- 现在进行到哪里；
- 最近发生了什么变化；
- 哪些任务正在推进或被阻塞；
- 已经作出哪些重要决定；
- 哪些事情需要用户关注。

不在普通界面展示原始 DAG、prompt、token、进程 ID 或 Runtime 日志。开发者视图才展示事件、节点、证据和执行诊断。继续使用 Lucide、设计变量和双语 i18n，不使用 Emoji。

## 11. 测试矩阵

### 单元测试

- schema、状态转移、关系约束、Decision 完整性；
- Repository 事务、幂等、revision 冲突和软删除；
- Project 聚合和 Markdown 快照稳定性；
- Complexity Router 回放、Goal budget、Graph 依赖归约；
- Evidence 缺失不得完成；Memory scope 和 correction 优先级。

### 集成测试

- Capture → Project/Task/Decision；
- Note/Source → Project 引用；
- Calendar/Todo → Work Object 引用；
- 应用重启后恢复 Project 和 Goal；
- 模型超时、工具失败、网络中断、数据库冲突；
- PC/Android 同步状态和协议降级。

### 产品回放

- 月度项目回顾；
- 新版文件挑战旧 Decision；
- 会议形成 Decision/Task/Deadline；
- Advanced 工作中断恢复；
- 更换 Runtime 继续执行。

每一阶段记录 PC 安装版、绿色版和 Android APK 的体积；任何增长必须说明来源和用户价值。

## 12. 风险与控制

| 风险 | 控制 |
|---|---|
| Work Core 与旧项目笔记形成双真相源 | 按领域明确 canonical ownership；旧 Markdown 只注册或生成快照 |
| 一次创建过多对象导致过度设计 | 第一 UI 只开放 Project/Goal/Task/Decision；其他对象按真实场景启用 |
| LLM 错误关联项目或决定 | 唯一匹配、本地校验、置信度和用户确认 |
| 所有请求被升级为 Advanced | Direct/Deep 默认优先，Advanced 必须命中持续性条件 |
| 多 Agent 只增加成本 | API 单 Agent 先证明框架；有量化收益才扩展 Clerk Pool |
| Runtime 绑死 Codex/AGY | Adapter 契约先于具体接入，API-only 必须完整可用 |
| 同步协议破坏旧客户端 | schema version、单调状态、兼容读取和明确升级提示 |
| 客户端体积和依赖膨胀 | Rust/SQLite 复用现有依赖；每阶段体积门；Host 为可选高级能力 |
| Dream 把错误当用户偏好 | 来源、scope、confidence、evidence、correction 和可删除性 |

## 13. 发布节奏

- `v0.8.0`：可靠 Capture 与知识提交基线，已经封存。
- `v0.8.x`：文档收口、缺陷修复和 Capture/Source 收尾，不宣称 Work Core 已完成。
- `v0.9.0`：Persistent Project State、Decision 和最小 Project UI。
- `v0.10.0`：现有输入接入 Work Core 与 Change Detection。
- `v0.11.0`：Complexity Router 与单 Agent Advanced Project Loop。
- 后续版本：Dynamic Graph、Clerk Pool、Runtime Adapter、Runtime Host 和 Work Graph Intelligence；只有达到阶段完成门槛才命名发布。

版本号是能力质量门，不是日期承诺。

## 14. 明确非目标

第一周期不做：

- 重写客户端或更换 Tauri；
- 引入 Graph Database；
- 默认安装 Python、Node、Docker、MCP 或 Runtime Host；
- 同时接入多个订阅型 Agent；
- 用 prompt 或聊天历史冒充 Persistent Project State；
- 用模型自述代替 Evidence；
- 自动迁移或修改真实 AppData 数据；
- 为展示复杂性而暴露 DAG 或多 Agent UI。

## 15. 计划批准后的立即动作

1. 将 `bob_new` 愿景整合进正式 `PRODUCT_VISION.md`，保留本项目低摩擦与轻量交付定位。
2. 新建 Work Core 数据契约设计和 Decision Log。
3. 为 Phase 1 写逐文件、逐测试的实现计划。
4. 先提交文档设计，再开始数据库和 Rust 模块开发。
5. 每个提交通过相应测试后才进入下一提交，不提前发布目标能力。
