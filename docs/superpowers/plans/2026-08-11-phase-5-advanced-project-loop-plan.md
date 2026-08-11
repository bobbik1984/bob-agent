# Phase 5 Advanced Project Loop 实施计划

> 状态：已批准，实施中
>
> 日期：2026-08-11
>
> 设计依据：`docs/superpowers/specs/2026-08-11-phase-5-advanced-project-loop-design.md`

## 实施原则

- Work Core Goal 是权威目标对象，Goal Runtime 只保存执行状态；
- Auto Advanced 取代旧 Goal Loop 的自动入口，但不删除显式实验兼容路径；
- 首版只运行一个单 Agent、有预算、顺序执行切片，不实现 DAG；
- R0/R1 可自动执行，R2/R3 和关键歧义必须进入持久审批；
- `done` 必须绑定通过验证的 Evidence；
- 无新用户侧运行时，无前端或 Rust 新依赖；
- 编译缓存写入 `D:\ignore_sync\bob-agent-phase2-target`；
- 不改动用户未跟踪文件。

## Batch 1：Contract、状态机与 Repository

### 1.1 新增 `src-tauri/src/goal_runtime/models.rs`

- [ ] 定义版本化 `GoalContract`、`EvidenceRule`、Scope、Budget、Risk、Blocker 和 Recovery policy；
- [ ] 定义 `GoalRunStatus`、`GoalPhase`、`VerificationState`；
- [ ] 定义 Run、Attempt、Evidence、Checkpoint、ApprovalRequest/Choice/Decision 和 Event；
- [ ] 实现 Contract 校验、默认预算、状态转换白名单和终态判断；
- [ ] 保持旧 `{ outcome }` Goal data 可读并可升级。

### 1.2 新增 `src-tauri/src/goal_runtime/repository.rs`

- [ ] 创建 `goal_runs`、`goal_attempts`、`goal_evidence`、`goal_checkpoints`、`goal_approvals`、`goal_events` 与幂等回执表；
- [ ] 一个 Goal 同时最多一个非终态 Run；
- [ ] 所有写入使用事务、revision 和幂等键；
- [ ] 实现个人工作区的幂等创建；
- [ ] 实现 Run 创建、状态转换、预算更新、检查点、Evidence、审批与事件 API；
- [ ] UI/API 事件查询默认限制最近 50 条；
- [ ] 增加 SQLite reopen、事务回滚、冲突和重复请求测试。

### 1.3 接入初始化

- [ ] 在 `goal_runtime/mod.rs` 暴露初始化和恢复 API；
- [ ] `db.rs` 在 Work Core 后初始化 Goal Runtime 表；
- [ ] `lib.rs` 注册模块，不改变现有数据库位置。

验收：Repository 单测通过；数据库关闭重开后 Run、审批、Evidence 和 checkpoint 完整；依赖文件无变化。

## Batch 2：Compiler、审批与 Evidence

### 2.1 新增 `goal_runtime/compiler.rs`

- [ ] 从原始请求和 RouteDecision 生成本地安全骨架；
- [ ] Clerk 只输出严格 JSON 候选，不调用工具、不授予权限；
- [ ] Rust 校验 outcome、EvidenceRule、Scope、预算和风险；
- [ ] 模型不可用或 JSON 非法时保留 draft 与原始请求；
- [ ] 关键信息不足时生成 2–4 个互斥 ActionChoice；
- [ ] 未明确项目时使用 `project_personal_inbox`。

### 2.2 新增 `goal_runtime/verifier.rs`

- [ ] 确定性验证工具回执、文件/数据引用和用户验收；
- [ ] Clerk rubric 只用于确定性规则无法判断的结果；
- [ ] 任一必需规则未 verified 时拒绝 `done`；
- [ ] 错误定位到 attempt、phase 和 EvidenceRule。

### 2.3 审批协议

- [ ] 实现 approve、reject、defer、handoff、select_option；
- [ ] R2 要求明确二次动作，R3 标记 trusted device required；
- [ ] 第一份合法且 revision 匹配的决定生效；
- [ ] 超时、断线和重复响应不默认批准；
- [ ] 测试手表旋钮/触控/文字等 modality 只影响记录，不改变语义。

验收：Compiler 回放、非法 Contract、Evidence 缺失、多端重复审批和 R2/R3 权限测试通过。

## Batch 3：有限 Engine 与恢复

### 3.1 新增 `goal_runtime/engine.rs`

- [ ] 实现 `observe → plan → act → verify → repair → finish` 状态推进；
- [ ] 单次切片和全局运行预算均有上限；
- [ ] 全局最多一个活动执行切片；
- [ ] `act` 前写 intent checkpoint，真实工具回执后再写完成 checkpoint；
- [ ] 只向 Runtime 自动执行路径暴露 R0/R1 工具；
- [ ] R2/R3 生成持久 ApprovalRequest，不复用易失的 30 秒弹窗作为权威审批；
- [ ] 不保存模型私有推理链，只保存可审计计划和结果摘要。

### 3.2 启动恢复

- [ ] 使用短期 lease 防止重复执行；
- [ ] 启动时扫描非终态 Run；
- [ ] 过期 running/verifying 回到最后安全 checkpoint；
- [ ] R0 可重试，结果未知的副作用进入 blocked/unverified；
- [ ] waiting_user、blocked、cancelled 不自动执行；
- [ ] 应用关闭期间不承诺继续工作。

### 3.3 LLM 接入

- [ ] `llm.rs` 增加内部 `goal_runtime` 安全执行上下文；
- [ ] Auto Advanced 调用新 Runtime，而不是旧 `goal.rs`；
- [ ] Direct/Deep 和显式旧 Goal 模式保持兼容；
- [ ] Chat 返回 goalId、runId、状态、下一动作和 route；
- [ ] 远程来源仍不得绕过 Policy Engine。

验收：故障注入覆盖调用前崩溃、调用后回执前崩溃、verify 崩溃、lease 过期、网络中断和预算耗尽。

## Batch 4：Commands、Bridge 与最小 UI

### 4.1 `goal_runtime/commands.rs` 与 `lib.rs`

- [ ] 注册 create/list/get/continue/defer/cancel/decide/list-events commands；
- [ ] Commands 返回可读 Result，不增加生产路径 unwrap/panic；
- [ ] 状态变化发出 `goal:runtime-state`、`goal:approval-required` 和 `goal:evidence-updated`。

### 4.2 `src/tauri-bridge.js`

- [ ] 通过 `window.appAPI` 暴露所有 Goal Runtime commands；
- [ ] 增加三个事件监听器和清理函数；
- [ ] 不允许 Vue 直接 import Tauri API。

### 4.3 `src/composables/useChat.js` 与 `ChatView.vue`

- [ ] 保存回复中的 Goal metadata；
- [ ] 在 Advanced 回复旁显示 Goal 状态和下一动作；
- [ ] waiting_user 显示结构化选项，不使用自由文本猜测批准；
- [ ] 不展示 DAG、Agent 数量或内部 prompt。

### 4.4 `WorkView.vue`

- [ ] 为 Goal 卡片加载对应 Runtime 摘要；
- [ ] 显示 phase、预算、最新 checkpoint、Evidence 与阻塞原因；
- [ ] 支持继续、暂缓、取消和选择 ApprovalRequest；
- [ ] 小屏选项保持 2–4 个，协议支持未来旋钮/滚动适配。

### 4.5 i18n

- [ ] `zh-CN.json` 与 `en-US.json` 同步所有状态、错误、选项和操作；
- [ ] 使用稳定 error code + 当前 locale 渲染；
- [ ] 只用 Lucide 图标，不新增 Emoji。

验收：前端测试与生产构建通过；桌面和窄屏布局可理解；旧消息和无 Runtime Goal 不报错。

## Batch 5：文档、回归与提交

- [ ] 更新 `AGENTS.md`、`README.md`、`docs/ARCHITECTURE.md`、`docs/GOAL_RUNTIME.md`、`LLM_WIKI.md`；
- [ ] 更新 `docs/BOB_EVOLUTION_ROADMAP.md`、`todo.md` 和 `progress.yaml`；
- [ ] 只在所有完成门通过后将 Phase 5 标记完成；
- [ ] Rust 全量 lib tests；
- [ ] 前端 tests 和生产 build；
- [ ] `git diff --check`；
- [ ] 确认 package/Cargo manifests 和 lockfiles无变化；
- [ ] 确认只剩用户原有未跟踪文件；
- [ ] 创建本地提交，不推送、不发布。

## 最终完成门

- [ ] Auto Advanced 的 R0/R1 能创建、开始并持久化真实 Goal；
- [ ] R2/R3 与关键歧义进入结构化持久审批；
- [ ] SQLite reopen 和应用重启后可恢复；
- [ ] 未知副作用不自动重放；
- [ ] `done` 缺少 Evidence 时被拒绝；
- [ ] 状态、失败位置、等待项和下一步对用户可见；
- [ ] 不新增用户运行时或依赖；
- [ ] 文档没有把 Phase 6+ 目标写成当前能力。
