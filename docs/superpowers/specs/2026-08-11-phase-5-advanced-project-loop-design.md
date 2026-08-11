# Phase 5 Advanced Project Loop 设计

> 状态：设计已确认，尚未实现
>
> 日期：2026-08-11
>
> 阶段：Bob Work Continuity Phase 5
>
> 前置能力：Persistent Work Core、输入关联、Decision/Change Review、Complexity Router

## 1. 目的

Phase 5 让 Bob 第一次具备真实但有限的持久目标执行能力：用户在 Auto 模式提出需要跨时间、恢复、阶段推进或持续跟进的请求后，Bob 创建结构化 Goal，在应用运行期间自动推进低风险工作，并能在应用重启后从安全检查点继续。

本阶段必须证明四件事：

1. Goal 不依赖聊天历史也能恢复；
2. 执行过程有预算、有终止状态，不形成无限循环；
3. `done` 必须绑定通过验证的 Evidence；
4. R0–R3 Policy Engine 始终是唯一权限边界。

Phase 5 是单 Agent Advanced Project Loop，不是 Dynamic Task Graph、多 Agent 系统或常驻后台服务。

## 2. 用户价值与成功标准

用户只需表达目标，不需要理解 Goal、DAG、Agent、MCP 或模型角色。

成功标准：

- Auto 判断为 Advanced 后，明确且低风险的 Goal 自动创建并开始；
- R2/R3、缺少关键信息或存在互斥业务选择时进入 `waiting_user`；
- PC 按钮、手机触控、手表旋钮、眼镜语音都可映射到同一审批协议；
- 应用退出再启动后，未完成 Goal 可从最后一个安全检查点继续；
- 无法确认外部副作用是否成功时不盲目重放，而是进入 `blocked` 且标记 `unverified`；
- 用户能看见当前状态、下一步、预算、阻塞原因和最新证据；
- 没有新增 Python、Node、Docker、MCP 或其他用户侧运行时；
- 没有新增前端或 Rust 依赖，除非实现中证明现有依赖无法满足且另行批准。

## 3. 范围

### 3.1 本阶段包含

- Goal Contract 编译、校验和版本化；
- Work Core Goal 与 Goal Runtime 的稳定关联；
- Goal 运行、尝试、证据、审批、检查点和事件持久化；
- 单 Agent `observe → plan → act → verify → repair → finish` 有限循环；
- 运行租约、预算、暂停、取消、失败和重启恢复；
- 设备无关的结构化审批协议；
- Auto Advanced 接入和旧 Goal Loop 隔离；
- WorkView/Chat 的最小状态展示与控制；
- 中英文用户可见状态和错误；
- Rust 单元测试、数据库重启测试、故障注入、前端测试和生产构建。

### 3.2 本阶段不包含

- Phase 6 Dynamic Task Graph、节点依赖、并行执行和局部下游失效；
- 多 Agent、Lead–Clerk Pool、Specialist 角色编排；
- Codex、Claude Code、AGY 等 Runtime Adapter；
- Runtime Host、应用关闭后的常驻执行或云端托管；
- 智能手表、眼镜或新的移动客户端实现；
- 用 Knowledge Graph 代替执行状态机；
- 将旧 `goal.rs` 扩展为新 Runtime；
- 新增复杂 DAG UI。

应用重启可恢复的含义是：用户再次打开 Bob 后自动重建安全执行状态。应用完全关闭期间不持续运行；常驻执行属于 Phase 9。

## 4. 方案选择

### 4.1 方案 A：继续扩展旧 `goal.rs`

优点是改动少。缺点是旧实现以聊天消息和三轮 Maker–Checker 重试为中心，没有持久状态、审批、检查点和证据契约，继续扩展会把聊天循环误当成 Goal Runtime。

结论：拒绝。旧模块只保留显式实验入口兼容性。

### 4.2 方案 B：把全部运行状态写入 `work_objects.data_json`

优点是表少。缺点是高频运行状态、尝试、租约、审批和证据难以独立查询、事务更新和故障恢复，也会让 Work Core Goal 同时承担产品目标与执行日志。

结论：拒绝。

### 4.3 方案 C：Work Core Goal + 独立 Goal Runtime

Work Core Goal 保存用户要完成什么，独立 Goal Runtime 保存如何推进、执行到哪里、为什么暂停和凭什么完成。两者使用稳定 `goal_id` 关联。

结论：采用。该方案保持 Canonical Project State、Runtime 状态和未来 Dynamic Graph 的边界清晰。

## 5. 架构边界

新增 `src-tauri/src/goal_runtime/`：

```text
goal_runtime/
├── mod.rs          # 模块入口、初始化与恢复入口
├── models.rs       # Contract、Run、Attempt、Evidence、Approval、Checkpoint
├── repository.rs   # SQLite、事务、幂等、租约与事件
├── compiler.rs     # 原始请求到 Goal Contract
├── engine.rs       # 单 Agent 有限状态循环
├── verifier.rs     # 确定性验证与可选 Clerk 判断
└── commands.rs     # Tauri commands 与用户控制
```

边界规则：

- `complexity_router.rs` 只判断 Direct、Deep、Advanced，不执行 Goal；
- `goal_runtime` 只接受已经判定为 Advanced 的请求或用户显式创建请求；
- `work_core` 继续拥有 Project 和 Goal 权威对象；
- `goal_runtime` 不直接改变 Calendar、知识 Markdown 或文件，所有动作继续经过现有工具和 Policy Engine；
- `goal.rs` 不被 Auto Advanced 调用；
- Vue 组件只调用 `window.electronAPI.*`，所有 Tauri `invoke` 保留在 `src/tauri-bridge.js`。

## 6. Goal Contract

Goal Contract 使用版本化、可序列化结构，并保存在 Work Core Goal 的 `data_json` 中。现有只包含 `outcome` 的旧 Goal 保持可读。

```text
GoalContract
  schemaVersion
  originalRequest
  outcome
  evidenceRules[]
  scope
  constraints[]
  budget
  riskPolicy
  blockerPolicy
  recoveryPolicy
  createdFrom
```

### 6.1 EvidenceRule

每条完成规则包含：

- 稳定规则 ID；
- 可读描述；
- 类型：`deterministic | rubric | user_acceptance`；
- 是否必须；
- 允许的 Evidence 类型；
- 确定性验证参数或 rubric；
- 当前验证状态：`pending | verified | unverified | rejected`。

没有至少一条必需 EvidenceRule 的 Contract 不得进入 `ready`。

### 6.2 Scope 与 Constraints

Scope 明确允许读取和修改的项目、对象、路径或外部目标。Constraints 保存禁止事项和必须保留的产品边界。模型不能通过改写计划扩大 Scope。

### 6.3 Budget

预算至少包括：

- 最大总运行时间；
- 单次执行切片时间；
- 最大模型调用次数；
- 最大工具调用次数；
- 最大修复次数；
- 可选 token 上限。

预算耗尽进入 `failed` 或 `waiting_user`，由 blocker policy 决定；不能自动重置预算继续执行。

### 6.4 Risk 与 Blocker Policy

- R0：只读，可自动执行；
- R1：明确在 Scope 内、低风险且可撤销时可自动执行；
- R2：执行前必须获得明确审批；
- R3：必须转交受信任且已解锁的手机或 PC，不能用普通手表单击完成；
- 缺少关键事实、存在互斥业务选择、Scope 不足或 Evidence 规则不可验证时进入 `waiting_user`。

## 7. 未归属 Goal

用户不应在创建 Goal 前被迫选择项目。未找到明确项目时，Bob 延迟创建固定 `project_personal_inbox`，作为“个人工作区”。

- Goal 可以立即持久化和执行；
- WorkView 将其显示为待归属；
- 后续归入正式项目必须保留 Goal ID、运行记录、Evidence 和事件历史；
- 自动归属只能使用稳定项目 ID 或唯一名称匹配，歧义继续留在个人工作区。

项目迁移的具体 UI 可以简化，但数据层必须从一开始支持安全重归属。

## 8. 持久化模型

新增表不复制 Project 真相，只保存 Runtime 状态。

### 8.1 `goal_runs`

保存 `run_id`、`goal_id`、`project_id`、状态、当前 phase、verification state、预算使用、恢复次数、租约、最近错误、revision、创建和更新时间。一个 Goal 同时最多有一个非终态 Run。

### 8.2 `goal_attempts`

保存一次 plan/act/repair 尝试的输入摘要、执行器、工具回执引用、错误类型、开始和结束时间。不得保存模型私有推理链；只保存用户可审计的计划、结果摘要和必要诊断。

### 8.3 `goal_evidence`

绑定 `run_id`、EvidenceRule、Work Core Evidence 对象或外部回执引用、校验结果、校验器、来源和时间。二进制内容保留原位置，只存引用和 hash。

### 8.4 `goal_checkpoints`

保存 phase、可恢复输入、已确认副作用回执、下一动作、预算快照和创建时间。Checkpoint 是追加式的；Run 只引用最新安全 checkpoint。

### 8.5 `goal_approvals`

保存结构化 ApprovalRequest、选项、风险、目标设备要求、有效期、revision 和最终选择。审批响应必须有幂等键；第一个合法响应生效，后续重复响应返回同一结果。

### 8.6 `goal_events`

保存 Goal Runtime 不可变状态流。UI 和普通 API 默认只返回最近 50 条；持久事件不等同于设备活动日志，不受设备日志 50 条存储上限约束。事件不得存储密钥、完整模型思维链或大块二进制数据。

## 9. 状态机

主状态：

```text
draft → ready → running → verifying → done
                    ↘ repair ↗
```

旁路状态：

- `waiting_user`：等待审批、补充信息或业务选择；
- `blocked`：外部条件、未知副作用结果或不可恢复错误；
- `failed`：预算内无法修复；
- `cancelled`：用户明确取消；
- `done`：所有必需 EvidenceRule 均为 `verified`。

状态约束：

- `draft` 只有 Contract 校验通过才能进入 `ready`；
- `running` 必须持有未过期执行租约；
- `act` 前写 pre-action checkpoint；
- `verifying` 只读取已提交回执和 Evidence；
- Evidence 缺失时 verification state 为 `unverified`，Goal 不能进入 `done`；
- `cancelled` 是用户终态，不记录为执行失败；
- 终态不得自动重新打开，只能创建显式新 Run 或修订 Goal。

## 10. Goal Compiler

编译流程：

1. 保存原始请求和 Complexity Router 决策；
2. 本地规则预填风险、持续性、默认预算和已知 Project；
3. 配置的 Clerk 仅生成严格 JSON 候选，不调用工具、不获得权限；
4. Rust 校验 Contract schema、Scope、EvidenceRule 和预算；
5. 合法 Contract 进入 `ready`；
6. 缺少关键信息则创建 ApprovalRequest 并进入 `waiting_user`；
7. 断网或模型不可用时保留 `draft`，原始请求不丢失，恢复后可继续编译。

Clerk 的输出只能提出候选 Contract，不能授予 Action 权限、提升风险许可或直接宣布 Goal 完成。

## 11. 单 Agent 有限循环

每次 Runtime 只运行一个有预算的执行切片：

1. `observe`：读取 Contract、项目摘要、最新 checkpoint、审批和相关 Evidence；
2. `plan`：生成当前会话可完成的顺序步骤列表；
3. `act`：逐步调用现有工具，所有调用继续经过 Policy Engine；
4. `verify`：确定性规则优先验证回执和产物；
5. `repair`：只修复当前有界步骤，受修复预算限制；
6. `finish`：全部必需 Evidence 通过后才能结束。

本阶段全局并发上限为一个活动执行切片。其他 ready Goal 排队，避免在尚无 Dynamic Graph 和资源调度器时制造竞争。

Plan 只是当前 Run 的顺序步骤，不是 DAG。不得表达并行节点、长期依赖或下游失效；这些属于 Phase 6。

## 12. 自动启动策略

- Auto + Advanced + R0/R1 + Contract 完整：自动创建并开始；
- Auto + Advanced + R2/R3：创建 Goal 和审批，进入 `waiting_user`；
- Auto + Advanced + 关键信息缺失：创建 Goal 草稿和选择题，不执行；
- 用户显式“只分析”覆盖：即使语义 Advanced，也只生成只读分析或 Goal 草案；
- 用户显式取消：进入 `cancelled`，清除租约，不再自动恢复；
- 旧手动 Goal 模式仍标注实验性，不与新 Runtime 的产品状态混淆。

Chat 返回值携带 `goalId`、`runId`、状态和下一动作。界面必须明确区分“Goal 已创建”“正在执行”“等待选择”和“已验证完成”。

## 13. 设备无关审批协议

Runtime 生成结构化 ApprovalRequest，而不是 UI `Yes/No`。

```text
ApprovalRequest
  approvalId
  goalId
  runId
  summary
  risk
  choices[]
  expiresAt
  trustedDeviceRequired
  revision

ActionChoice
  choiceId
  labelKey
  semantic
  payload

ApprovalDecision
  approvalId
  choiceId
  actor
  deviceId
  inputModality
  decidedAt
  idempotencyKey
```

标准 semantic：`approve | reject | defer | handoff | select_option`。

交互原则：

- 选项短小、互斥，通常为 2–4 个；
- PC 使用按钮或文字，手机使用触控，手表优先使用旋钮/上下滚动和按键；
- 语音是可选适配器，不是手表第一输入方式；
- 无法在当前设备表达“其他方案”时，最后一项应为“转到手机或 PC”；
- R2 在小屏设备上要求长按、再次点击或等价明确动作；
- R3 必须转到受信任、已解锁且能力足够的设备；
- 超时、断线和无响应保持 `waiting_user`，绝不默认批准；
- 多设备同时响应时，第一个合法且 revision 匹配的决定生效。

Phase 5 实现协议以及现有 PC/聊天入口适配。手机 Relay、手表、眼镜和语音识别的完整 Adapter 不属于本阶段，但协议不得阻碍它们后续接入。

## 14. Evidence 与完成门

验证顺序：

1. 确定性回执、文件 hash、数据库状态、schema、计数或测试结果；
2. 业务规则；
3. Clerk rubric；
4. 主观或高风险交付的用户验收。

规则：

- 执行模型的自述不是 Evidence；
- Clerk 不得覆盖失败的确定性检查；
- 每个必需 EvidenceRule 都必须有可追溯 Evidence；
- 任何必需规则为 `pending`、`unverified` 或 `rejected` 时不得进入 `done`；
- 失败信息必须定位到 phase、attempt、EvidenceRule 和可执行的下一步；
- 用户明确接受主观交付物时，保存为 `user_acceptance` Evidence。

## 15. 重启、崩溃与幂等恢复

### 15.1 运行租约

每个活动 Run 使用短期 lease。应用启动时扫描非终态 Run：

- 未过期租约不启动第二执行器；
- 过期的 `running` 或 `verifying` Run 追加 `goal.recovered` 事件并回到安全 phase；
- R0/R1 在应用可用后继续；
- `waiting_user`、`blocked` 和 `cancelled` 不自动执行。

### 15.2 副作用安全

工具调用前写入 intent checkpoint，调用后写入真实回执。若进程在两者之间退出：

- 有工具幂等回执时读取原结果；
- 无法确定是否执行成功的 R1/R2/R3 操作进入 `blocked`，不得自动重放；
- 只读 R0 可安全重试；
- 所有恢复决定写入 goal_events。

### 15.3 多端审批

ApprovalDecision 使用 approval ID、revision 和幂等键。重复选择返回原结果；过期 revision 被拒绝并返回当前状态。Runtime 的权威写入仍发生在 Bob Core，远端设备只传递选择，不直接修改 Goal 状态。

## 16. 用户界面

Phase 5 只增加最小 Goal 状态能力：

- Chat 回复显示 Goal 已创建、当前状态和下一动作；
- WorkView 显示活动 Goal、当前 phase、预算摘要、最新 checkpoint、等待项和证据状态；
- 用户可以继续、暂缓、取消或选择 ApprovalRequest 选项；
- 状态文本通过中英文 i18n key 渲染；
- 图标只使用 Lucide，不使用 Emoji；
- 不展示 DAG、Agent 数量、内部 prompt 或模型思维链。

普通用户首先看到“正在处理什么”和“需要我做什么”。运行 ID、attempt 和验证细节放在可展开诊断中。

## 17. API 与事件

计划新增的 Tauri Commands：

- `goal_runtime_create`
- `goal_runtime_list`
- `goal_runtime_get`
- `goal_runtime_continue`
- `goal_runtime_defer`
- `goal_runtime_cancel`
- `goal_runtime_decide_approval`
- `goal_runtime_list_events`

计划新增事件：

- `goal:runtime-state`
- `goal:approval-required`
- `goal:evidence-updated`

所有前端调用经 `src/tauri-bridge.js` 暴露。Commands 返回可读 `Result`，生产路径不新增 `unwrap()` 或 `panic!()`。

## 18. 错误与用户日志

用户可见错误使用稳定错误码和 i18n 参数，不持久化翻译后的整句文本。至少区分：

- Contract 无效；
- 模型不可用；
- 预算耗尽；
- 审批过期或冲突；
- Policy 拒绝；
- 工具失败；
- 副作用结果未知；
- Evidence 缺失或验证失败；
- 恢复失败。

普通界面展示原因和下一步；高级诊断展示 goal/run/attempt/rule ID。UI 默认展示最近 50 条 Goal 事件。

## 19. 测试策略

### 19.1 模型与状态机测试

- 合法和非法 Goal Contract；
- 旧 Goal data 向前兼容；
- 每个合法状态转换和所有非法转换；
- `done` 无 Evidence 时被拒绝；
- 取消与失败严格区分；
- 预算不能被执行器自行重置。

### 19.2 Repository 测试

- 创建 Goal/Run 幂等；
- 一个 Goal 最多一个活动 Run；
- revision 冲突；
- ApprovalDecision 首次有效、重复回放和过期拒绝；
- checkpoint、attempt、evidence 和 event 事务回滚；
- SQLite 关闭重开后状态完整。

### 19.3 故障注入

- Contract 编译模型超时或返回非法 JSON；
- 工具调用前崩溃；
- 工具已执行但回执未写入时崩溃；
- verify 期间崩溃；
- 应用重启和租约过期；
- 网络中断；
- Evidence 缺失；
- R2/R3 未审批；
- 多设备重复或冲突审批。

### 19.4 集成与回归

- Auto Advanced 自动创建 R0/R1 Goal；
- R2/R3 进入 `waiting_user`；
- Direct/Deep 不创建 Goal；
- 旧显式 Goal 模式保持兼容且继续标注实验性；
- Bridge 不出现组件直接 `invoke`；
- 中英文 UI 文案齐全；
- 前端测试、Rust 测试和生产构建通过；
- `package.json`、lockfile、`Cargo.toml` 和 `Cargo.lock` 无非预期变化；
- 编译缓存继续写入 `D:\ignore_sync`。

## 20. 完成定义

Phase 5 只有同时满足以下条件才完成：

- Auto Advanced 能创建真实持久 Goal，R0/R1 自动开始；
- R2/R3 和关键歧义进入结构化审批；
- 一个未完成 Goal 在 SQLite 重开和应用重启后可恢复；
- 未知副作用不会自动重放；
- Goal 运行循环有明确预算和终态；
- `done` 必须绑定全部必需 Evidence；
- UI 能说明状态、等待项、失败位置和下一步；
- 测试覆盖重启、崩溃、权限、Evidence、幂等和预算；
- 权威文档反映真实能力；
- 没有新增用户侧运行时或未经解释的客户端体积增长。

未满足上述任一项时，对外仍称为 Advanced Project Loop 开发中，不能宣称 Bob 已具备完整 Goal Runtime。
