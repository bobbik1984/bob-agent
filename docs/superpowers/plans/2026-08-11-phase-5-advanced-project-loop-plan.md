# Phase 5 Advanced Project Loop 实施计划

> 状态：代码纵切片完成；前端与生产编译通过，专用 Rust 回归及本地提交待完成
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

- [x] 定义版本化 `GoalContract`、`EvidenceRule`、Scope、Budget、Risk、Blocker 和 Recovery policy；
- [x] 定义 `GoalRunStatus`、`GoalPhase`、`VerificationState`；
- [x] 定义 Run、Attempt、Evidence、Checkpoint、ApprovalRequest/Choice/Decision 和 Event；
- [x] 实现 Contract 校验、默认预算、状态转换白名单和终态判断；
- [x] 保持旧 `{ outcome }` Goal data 可读并可升级。

### 1.2 新增 `src-tauri/src/goal_runtime/repository.rs`

- [x] 创建 `goal_runs`、`goal_attempts`、`goal_evidence`、`goal_checkpoints`、`goal_approvals`、`goal_events` 与幂等回执表；
- [x] 一个 Goal 同时最多一个非终态 Run；
- [x] 所有写入使用事务、revision 和幂等键；
- [x] 实现个人工作区的幂等创建；
- [x] 实现 Run 创建、状态转换、预算更新、检查点、Evidence、审批与事件 API；
- [x] UI/API 事件查询默认限制最近 50 条；
- [x] 增加 SQLite reopen、冲突、重复请求、Evidence gate、租约和恢复测试；事务原子性由 Repository transaction 覆盖。

### 1.3 接入初始化

- [x] 在 `goal_runtime/mod.rs` 暴露初始化和恢复 API；
- [x] `db.rs` 在 Work Core 后初始化 Goal Runtime 表；
- [x] `lib.rs` 注册模块，不改变现有数据库位置。

验收：Repository 单测通过；数据库关闭重开后 Run、审批、Evidence 和 checkpoint 完整；依赖文件无变化。

## Batch 2：Compiler、审批与 Evidence

### 2.1 新增 `goal_runtime/compiler.rs`

- [x] 从原始请求和 RouteDecision 生成本地安全骨架；
- [x] Clerk 只输出严格 JSON 候选，不调用工具、不授予权限；
- [x] Rust 校验 outcome、EvidenceRule、Scope、预算和风险；
- [x] 模型不可用或 JSON 非法时保留原始请求与安全基线；只有关键字段确实不足才进入 draft/waiting_user；
- [x] 关键信息不足时生成 2–4 个互斥 ActionChoice；
- [x] 未明确项目时使用 `project_personal_inbox`。

### 2.2 新增 `goal_runtime/verifier.rs`

- [x] 确定性验证工具回执、文件/数据引用和用户验收；
- [x] Rubric verdict 解析器已提供，但 Phase 5 不把未注册的 Clerk rubric 设为必需完成门；
- [x] 任一必需规则未 verified 时拒绝 `done`；
- [x] 错误定位到 attempt、phase 和 EvidenceRule。

### 2.3 审批协议

- [x] 实现 approve、reject、defer、handoff、select_option；
- [x] R2 要求明确二次动作，R3 标记 trusted device required；
- [x] 第一份合法且 revision 匹配的决定生效；
- [x] 超时、断线和重复响应不默认批准；
- [x] 手表旋钮/触控/文字等 modality 只影响记录，不改变语义。

验收：Compiler 回放、非法 Contract、Evidence 缺失、多端重复审批和 R2/R3 权限测试通过。

## Batch 3：有限 Engine 与恢复

### 3.1 新增 `goal_runtime/engine.rs`

- [x] 实现 `observe → plan → act → verify → repair → finish` 状态推进；
- [x] 单次切片和全局运行预算均有上限；
- [x] 全局最多一个活动执行切片；
- [x] `act` 前写 intent checkpoint，真实工具回执后再写完成 checkpoint；
- [x] 只向 Runtime 自动执行路径暴露 R0/R1 工具；
- [x] R2/R3 生成持久 ApprovalRequest，不复用易失的 30 秒弹窗作为权威审批；
- [x] 不保存模型私有推理链，只保存可审计计划和结果摘要。

### 3.2 启动恢复

- [x] 使用短期 lease 防止重复执行；
- [x] 启动时扫描非终态 Run；
- [x] 过期 running/verifying 回到最后安全 checkpoint；
- [x] R0 可重试，结果未知的副作用进入 blocked/unverified；
- [x] waiting_user、blocked、cancelled 不自动执行；
- [x] 应用关闭期间不承诺继续工作。

### 3.3 LLM 接入

- [x] `llm.rs` 增加内部 `goal_runtime_read` / `goal_runtime_action` 安全执行上下文；
- [x] Auto Advanced 调用新 Runtime，而不是旧 `goal.rs`；
- [x] Direct/Deep 和显式旧 Goal 模式保持兼容；
- [x] Chat 返回 goalId、runId、状态、下一动作和 route；
- [x] 远程来源仍不得绕过 Policy Engine。

验收：故障注入覆盖调用前崩溃、调用后回执前崩溃、verify 崩溃、lease 过期、网络中断和预算耗尽。

## Batch 4：Commands、Bridge 与最小 UI

### 4.1 `goal_runtime/commands.rs` 与 `lib.rs`

- [x] Auto Advanced 作为唯一创建入口，并注册 list/get/continue/defer/cancel/decide/list-events commands；
- [x] Commands 返回可读 Result，不增加生产路径 unwrap/panic；
- [x] 状态变化发出 `goal:runtime-state`、`goal:approval-required` 和 `goal:evidence-updated`。

### 4.2 `src/tauri-bridge.js`

- [x] 通过 `window.appAPI` 暴露所有 Goal Runtime commands；
- [x] 复用通用 `listenEvent` 订阅 Runtime 事件并返回清理函数；
- [x] 不允许 Vue 直接 import Tauri API。

### 4.3 `src/composables/useChat.js` 与 `ChatView.vue`

- [x] 保存回复中的 Goal metadata；
- [x] 在 Advanced 回复旁显示 Goal 状态和下一动作；
- [x] waiting_user 显示结构化选项，不使用自由文本猜测批准；
- [x] 不展示 DAG、Agent 数量或内部 prompt。

### 4.4 `WorkView.vue`

- [x] 为 Goal 卡片加载对应 Runtime 摘要；
- [x] 显示 phase、预算、最新 checkpoint、Evidence 与阻塞原因；
- [x] 支持继续、暂缓、取消和选择 ApprovalRequest；
- [x] 小屏选项保持 2–4 个，协议支持未来旋钮/滚动适配。

### 4.5 i18n

- [x] `zh-CN.json` 与 `en-US.json` 同步所有状态、错误、选项和操作；
- [x] 使用稳定 error code + 当前 locale 渲染主要 Runtime 失败；
- [x] 只用 Lucide 图标，不新增 Emoji。

验收：前端测试与生产构建通过；桌面和窄屏布局可理解；旧消息和无 Runtime Goal 不报错。

## Batch 5：文档、回归与提交

- [x] 更新 `AGENTS.md`、`README.md`、`docs/ARCHITECTURE.md`、`docs/GOAL_RUNTIME.md`、`LLM_WIKI.md`；
- [x] 更新 `docs/BOB_EVOLUTION_ROADMAP.md`、`todo.md` 和 `progress.yaml`；
- [x] 在专用 Rust 回归通过前保持 Phase 5 为 validation pending；
- [ ] Rust 全量 lib tests；
- [x] 前端 tests 和生产 build；
- [x] `git diff --check`；
- [x] 确认 package/Cargo manifests 和 lockfiles无变化；
- [x] 确认未跟踪项只有新增 `goal_runtime/` 与用户原有 `docs/archive_todo_pre_capture_20260809.md`、`old_sync.rs`、`test.css`；
- [ ] 创建本地提交，不推送、不发布。

## 最终完成门

- [x] Auto Advanced 的 R0/R1 能创建、开始并持久化真实 Goal；
- [x] R2/R3 与关键歧义进入结构化持久审批；
- [x] SQLite reopen 和应用重启后可恢复；
- [x] 未知副作用不自动重放；
- [x] `done` 缺少 Evidence 时被拒绝；
- [x] 状态、失败位置、等待项和下一步对用户可见；
- [x] 不新增用户运行时或依赖；
- [x] 文档没有把 Phase 6+ 目标写成当前能力。
