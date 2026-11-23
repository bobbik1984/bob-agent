# BP4-05 有界原生 API 子代理（Bounded Native API Subagents）验证报告

**日期**：2026-11-01  
**依据**：[`docs/CURRENT_STAGE.md §3.8`](file:///D:/Code/bob-agent-next/docs/CURRENT_STAGE.md) · [`docs/wiki/kernel-schema.md §10.4`](file:///D:/Code/bob-agent-next/docs/wiki/kernel-schema.md) · [`todo.md`](file:///D:/Code/bob-agent-next/todo.md)

---

## 1. 交付概览

按施工合同与全生命周期规范完成 **BP4-05 有界原生 API 子代理** 内核与节点接线：
- **内核模块**：`crates/bob-runtime/src/subagent_pool.rs` 与 `crates/bob-runtime/src/lib.rs`
- **RPC 节点桥接**：`crates/bob-node/src/lib.rs`（暴露 `app_subagent_dispatch`、`app_subagent_execute`、`app_subagent_cancel`、`app_subagent_reconcile`、`app_subagent_status`、`app_subagent_list`）
- **前端调用桥接**：`apps/bob-app/src/kernel-bridge.mjs`

---

## 2. 核心架构约束与防护契约

1. **拓扑结构与有界深度（1 架构师 + 1 只读子代理）**：
   - 严格限定架构师主控与单只读子代理协作关系。
   - 最大递归深度严格限制为 1（`depth == 1`），子代理在调度或运行阶段若尝试再次派发子代理（嵌套生成），无条件拒绝并报错 `SUBAGENT_DEPTH_LIMIT_EXCEEDED`。
   - 单架构师并发上限严格为 1，当父任务已存在 `Dispatched` 或 `Running` 的子代理时，新任务拒绝并发派发，报错 `SUBAGENT_CONCURRENCY_LIMIT_EXCEEDED`。
2. **权限交集沙箱与读写隔离（只读铁律）**：
   - 子代理权限严格继承父授权的交集，且工作区写权限始终为 `false`（`write_authorized: false`）。
   - 任何试图调用写工具（`write_file`、`edit_file`、代码提交等）或携带写内容的操作立即终止，报错 `SUBAGENT_WRITE_FORBIDDEN`。
   - 路径限定：只能读取位于父级授权 `workspace_root` 内的文件；任何包含 `..` 或越出根目录的寻址立即拒绝，报错 `PATH_ESCAPES_WORKSPACE`；未在 `allowed_subpaths` 清单内的寻址拒绝，报错 `SUBAGENT_SUBPATH_UNAUTHORIZED`。
   - 凭据隔离：不向子代理暴露真实主密钥或 Vault Master Seed，避免凭据泄漏。
3. **资源配额与费用归属（防超支与防双重计费）**：
   - 严格限定 `max_tokens`、`max_budget_usd` 与 `max_timeout_ms`；超限立即标记 `BudgetExceeded` 或 `TimedOut` 并阻断。
   - API 提供方未上报费用或模型身份时，明确标注 `"cost_status": "unknown"` 与 `"identity": "unknown"`，不隐式估算假数据。
   - 重复请求哈希判定：相同请求哈希直接幂等返回已完成记录，不重复向外部模型派发计费。
4. **级联取消与重启防重放对账（Cascade Cancellation & Reconciliation）**：
   - 父任务取消信号向下即时传播，运行中的子代理立即终止并记录结构化取消证据 `CancellationEvidence { cancelled_at, reason: "PARENT_CANCELLED", process_terminated: true }`。
   - 节点崩溃或重启时，通过 `reconcile_on_restart` 扫描所有未决（`Dispatched`/`Running`）执行，将其原子转换为 `Interrupted`（保留已产生 token 与计费账本），**严禁在重启后自动重放派发模型调用**，从根本上杜绝二次扣费。
5. **结构化结论收敛与主会话防膨胀（No History Bloat）**：
   - 子代理执行过程的逐步详细工具输入、原始输出和思考帧独立落盘于隔离日志文件（`subagent-gate/audit/<run_id>.jsonl`），绝不向父主会话回填全量冗长日志。
   - 主会话与上层 Task 仅接收核验后的结构化收敛结论 `SubagentSynthesizedConclusion`：
     - `summary`：核心摘要；
     - `citations`：经本地磁盘校验的相对路径与 SHA-256 哈希列表（`verified_on_disk: true`）；
     - `discrepancies`：期望与实际不符的分歧点列表（含严重度）；
     - `model_claims`：模型推断声明与本地真实验证状态对照（明确分为 `verified`、`unverified`、`disproved`，杜绝把模型自报当实体验证）；
     - `evidence_hashes`：工具载荷指纹；
     - `usage`：Token、费用与耗时统计。

---

## 3. 测试验证证据

### 3.1 `bob-runtime` 核心单测（38/38 全部 PASS）
```text
running 38 tests
test subagent_pool::tests::test_depth_limit_enforced_at_dispatch_and_turn ... ok
test subagent_pool::tests::test_concurrency_limit_and_idempotent_dedup ... ok
test subagent_pool::tests::test_write_attempts_and_path_escapes_strictly_forbidden ... ok
test subagent_pool::tests::test_cascade_cancellation_and_restart_reconciliation_never_replays ... ok
test subagent_pool::tests::test_structured_synthesis_and_model_claim_verification_without_raw_bloat ... ok
...
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.60s
```

### 3.2 `bob-node` 节点端 RPC 集成测试（8/8 全部 PASS）
```text
running 8 tests
test tests::xp11_bounded_subagent_rpc_lifecycle_and_constraints ... ok
test claude_rpc::tests::live_x1t_claude_code_v1_to_v2_two_round_closed_loop ... ok
test context_capsule::tests::invalid_dispatch_never_writes_records ... ok
test context_capsule::tests::grants_are_host_only_and_fail_closed ... ok
test tests::xp10_import_legacy_nexus_db_is_idempotent_and_read_only ... ok
test tests::bob_node_rpc_and_client_v1_end_to_end_closed_loop ... ok
test context_capsule::task_card_tests::real_workspace_cards_pin_material_and_auto_refresh_does_not_retarget_retry ... ok
test claude_rpc::tests::unified_dsh_and_claude_rpc_handle_capsule_handoff_and_v1_v2_deliverables ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.17s
```

### 3.3 `apps/bob-app` 前端桥接单元测试（87/87 全部 PASS）
```text
# Subtest: BP4-05 bounded subagent bridge maps dispatch, execute, cancel, reconcile, status and list commands cleanly
ok 62 - BP4-05 bounded subagent bridge maps dispatch, execute, cancel, reconcile, status and list commands cleanly
...
1..87
# tests 87
# suites 0
# pass 87
# fail 0
# cancelled 0
# skipped 0
# todo 0
# duration_ms 727.6354
```
