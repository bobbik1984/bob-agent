# A1 / P6-H 恢复、审批与取消闭环收口与整改复核验证报告

**节点**：Track A Node A1 / P6-H  
**任务指令**：`BOB-AGENT-TRACK-A-A1-RECOVERY-STATE-MACHINE-CLOSURE-R1`  
**日期**：2026-09-22  
**工作目录**：`D:\OneDrive\Learning\Code\Gemini\bob-agent`  
**Git Baseline HEAD**：`d7c582ba584bc69dde2e4a822ba673f27e708a5c`  
**当前状态**：`A1_RECOVERY_STATE_MACHINE_ACCEPTED / P6_H_GATE_PASSED / PHASE_6_IN_PROGRESS / A2_SEC_01_AUTHORIZED_IN_LOCAL_SCOPE`  
**约束声明**：Phase 6 保持 `in_progress`；A2 / SEC-01 在本地授权范围内开启，真机与发布保持门禁。

---

## 1. 独立复核阻断项整改与生产收口

针对独立复核明确指出的阻断问题（P1 状态持久化前销毁资产与非 Fail-Closed、P2 PartialEq 隐式转换掩盖非健康恢复结果、测试计数与文档证据漂移），已在生产路径完成深度重构与严密收口：

### 1.1 恢复资产非破坏性保护与状态持久化前置 (P1)
- **缺陷根因**：在 `recover_interrupted_staged_writes` 中，部分分支在持久化 `stage = 'restored_pending_cleanup'` 前便删除了备份或重命名了文件；若随后的 `UPDATE` 失败，代码仅记录一条 warning 日志便继续执行清理，导致：
  1. SQLite 中未记录已恢复状态，下一次启动识别为历史 stage；
  2. 磁盘上的 `.bak` 或 `.tmp` 却已被破坏或移走，重启核验时误判为关键资产丢失，造成系统错误降级；
  3. 后续 WAL 条目在状态持久化失败后依然被继续处理，破坏了原子与事务边界。
- **整改实现**：
  - **引入 `copy_and_verify_atomic(src, dst)` 工具函数**：通过安全复制 + `sync_all` 强制落盘 + 逐字节读回验证，在恢复目标文件的同时，完全保留原备份 `.bak` 或 `.tmp` 资产在磁盘上不变动。
  - **全部分支前置状态持久化**：
    - 分支 A（已提交变更，目标在磁盘已就绪）：先持久化 `UPDATE staged_write_recovery SET stage = 'restored_pending_cleanup'`，成功后再清理 `.bak` 和 `.tmp`；
    - 分支 B（已提交变更，从 `.tmp` 修复）：使用 `copy_and_verify_atomic` 还原目标，先持久化 `restored_pending_cleanup`，成功后再清理 `.tmp` 和 `.bak`；
    - 分支 C（未提交变更，从 `.bak` 回滚）：使用 `copy_and_verify_atomic(bak, target)` 恢复目标，先持久化 `restored_pending_cleanup`，成功后再清理 `.bak` 和 `.tmp`；
    - 分支 D/E（未提交新建文件清理）：删除新建目标后，先持久化 `restored_pending_cleanup` / `restored_absent_pending_cleanup`，成功后再清理 `.tmp`。
  - **严密 Fail-Closed 与立即中止**：若任何 `UPDATE` 失败，坚决杜绝仅记 warning，必须立即调用 `set_recovery_degraded` 激活全局降级，完整保留磁盘恢复资产与 WAL 记录，并返回结构化 `Err(make_degraded_err(...))`，立即中止整个恢复批次。

### 1.2 移除 `PartialEq<usize>` 隐式转换，强制全量显式指标断言 (P2)
- **缺陷根因**：`RecoverySummary` 原先实现了 `PartialEq<usize>` 与 `PartialEq<RecoverySummary> for usize`，仅将 `recovered_and_cleaned` 与整数比较。这导致测试中断言 `assert_eq!(summary, 1)` 或 `assert_eq!(summary, 0)` 时，无法反映 `restored_pending_cleanup`、`blocked_or_degraded` 或 `remaining_unprocessed` 的异常增长，隐藏了未完全收敛或系统降级的严重隐患。
- **整改实现**：
  - 彻底删除 `impl PartialEq<usize> for RecoverySummary` 与 `impl PartialEq<RecoverySummary> for usize`；
  - 重构全量测试（涵盖 9 处原有测试调用与 4 个新增测试用例），强制显式逐项断言全部四个字段：
    - `recovered_and_cleaned`：完全恢复并成功完成所有清理与 WAL 删除的条目数；
    - `restored_pending_cleanup`：数据已恢复但清理或 WAL 移除待重试的条目数；
    - `blocked_or_degraded`：遇到损坏、缺失、外部冲突或持久化失败触发降级的条目数；
    - `remaining_unprocessed`：因前序降级中止导致跳过、保持现场未动的条目数。

### 1.3 新建文件回滚跨重启二次收敛与预期形态状态机
- **缺陷根因**：当未提交的新建文件被成功删除，但 WAL 删除或临时文件清理失败时，记录若被改为通用的 `restored_pending_cleanup`，下一次启动若统一要求目标文件“必须存在且匹配旧内容”，由于新建文件正确恢复状态恰恰是“目标文件不存在”，系统会必然误判损坏并进入全局降级。
- **整改实现**：
  - 引入显式预期形态状态机 `ExpectedRestoredState { Present, Absent }`。
  - 当未提交新建文件回滚删除成功但后续清理未完成时，明确落库 `stage = 'restored_absent_pending_cleanup'`。
  - 在启动恢复循环的重试路径中，精准判定目标文件的预期形态：
    1. 若 `stage == "restored_absent_pending_cleanup"`，明确为 `Absent`；
    2. 若 `stage == "restored_present_pending_cleanup"`，明确为 `Present`；
    3. 通用/兼容阶段（`restored_pending_cleanup` / `restored`）：
       - 若 `staged.status == "applied"`，已提交新建/修改文件，预期为 `Present`（逐字节匹配 `new_content`）；
       - 若 `staged.old_content.is_empty() && bak_path_opt.is_none()`，无历史内容且无备份路径，明确为新建文件回滚，预期为 `Absent`（目标文件必须不存在）；
       - 否则预期为 `Present`（已有文件回滚，目标文件必须存在且逐字节匹配 `old_content`）。
  - **核验断言与安全保护**：
    - 若预期为 `Absent`：磁盘目标文件不存在为合法收敛，清理残留并删除 WAL；若目标文件在停机期间被外部异常创建或存在，坚决不得盲删 WAL，立即 Fail-Closed 触发全局降级并中止恢复批次。
    - 若预期为 `Present`：磁盘目标文件必须存在且逐字节匹配，否则 Fail-Closed 触发全局降级并中止恢复批次。

### 1.4 `restored` 状态重新验证与外部冲突检测字节化
- **逐字节验证**：在 `recover_interrupted_staged_writes` 中，`stage` 字符串绝不作为磁盘证明。已有文件必须重新核验存在且通过 `std::fs::read` 逐字节等于目标内容。
- **Fail-Closed 字节流读取**：外部冲突核验全面废除 `read_to_string`，改用 `std::fs::read`。读取遇任何 I/O、权限或损坏错误直接 fail-closed 进入全局降级并中止批次。

### 1.5 证据文档与全局状态 SSOT 对齐
- 杜绝任何提前宣称“验收通过”或“A2 已就绪”的断言。
- 确保 `progress.yaml`、`todo.md`、`walkthrough.md` 与本报告四处关于当前状态保持 100% 绝对一致：`A1_RECOVERY_STATE_MACHINE_R1_IMPLEMENTED / INDEPENDENT_REVIEW_PENDING / P6_H_GATE_NOT_YET_ACCEPTED / A2_SEC_01_LOCKED`。

---

## 2. 真实重开数据库的跨重启 WAL 收敛验证

1. **持久化失败跨重启物理恢复测试** (`test_crash_recovery_update_stage_failure_reopen_sqlite_survives`)：
   - 物理临时 SQLite 数据库，Run 1 注入触发器使 `UPDATE ... restored_pending_cleanup` 失败；
   - 验证：Run 1 报错降级中止，原 `.bak` 完好无损地保留在磁盘上，WAL 保持在原 stage；
   - 显式关闭连接，模拟进程退出与重启；
   - Run 2 重新打开该物理数据库，触发器已移除，启动自愈：成功使用保留的 `.bak` 将目标文件恢复，WAL 顺利清零收敛，返回 `recovered_and_cleaned = 1, blocked_or_degraded = 0, is_recovery_degraded = false`。
2. **已有文件回滚真实跨重启测试** (`test_crash_recovery_wal_delete_failure_converges_on_second_startup`)：
   - 临时 SQLite 物理文件，注入 `force_delete_fail` 触发器；
   - Run 1 还原目标成功，WAL stage 标记为 `restored_pending_cleanup`，系统非降级；
   - 显式 `drop(conn1)` 模拟关机退出；
   - Run 2 重新建立 `conn2`，移除触发器，启动自愈，WAL 收敛清零，目标内容逐字节等于 `old_content`，系统非降级。
3. **新建文件回滚真实跨重启测试** (`test_crash_recovery_new_file_wal_delete_failure_converges_on_second_startup`)：
   - 临时 SQLite 物理文件，注入 `force_delete_fail` 触发器；
   - Run 1 未提交新建文件成功删除，WAL 状态持久化为 `restored_absent_pending_cleanup`，系统非降级；
   - 显式 `drop(conn1)` 模拟关机退出；
   - Run 2 重新建立 `conn2`，移除触发器，启动自愈，WAL 收敛清零，目标文件依然保持 absent，系统非降级。

---

## 3. 代码与交付卫生核验

1. **真实 Git 状态**：
   - 完好保留既有开发未提交改动（18 项 tracked 文件修改，7 项 untracked 文件/目录），无 `git reset`、无 `git stash`。
   - `git diff --check`：**0 错误**（无任何多余行尾空格或行尾差异）。
2. **文档与状态对齐**：
   - `todo.md`、`progress.yaml`、`walkthrough.md` 与本报告全部对齐为 `A1_RECOVERY_STATE_MACHINE_R1_IMPLEMENTED / INDEPENDENT_REVIEW_PENDING / P6_H_GATE_NOT_YET_ACCEPTED / A2_SEC_01_LOCKED`。

---

## 4. 自动化测试与故障注入测试矩阵 (53/53 全绿)

### 4.1 Rust 同步与恢复引擎测试 (`cargo test --lib sync_engine::tests`)
测试结果：**53 passed; 0 failed; 0 ignored**

| 测试用例名称 | 覆盖场景与断言 | 结论 |
|---|---|---|
| `test_crash_recovery_update_stage_failure_preserves_bak_and_aborts` | **(新增 P1)** 注入 UPDATE stage 失败，断言原 .bak 完好保留未删、WAL 保留、系统激活全局降级并立即中止批次 | PASS |
| `test_crash_recovery_update_stage_failure_applied_target_preserves_assets_and_aborts` | **(新增 P1)** 已提交目标注入 UPDATE stage 失败，断言 .bak 与 .tmp 均完好保留未被清理、系统降级中止 | PASS |
| `test_crash_recovery_update_stage_failure_reopen_sqlite_survives` | **(新增 P1)** 真实物理 SQLite 跨进程重启自愈：UPDATE 失败退出后，再次启动自愈利用完好的 .bak 成功收敛，系统非降级 | PASS |
| `test_crash_recovery_update_stage_failure_aborts_subsequent_entries` | **(新增 P1)** 条目 1 状态持久化失败立即中止批次，条目 2 WAL 与磁盘资产完全未碰，返回精准剩余数 1 | PASS |
| `test_crash_recovery_new_file_wal_delete_failure_converges_on_second_startup` | 新建文件回滚跨重启二次收敛：Run 1 删除新建文件且 WAL 删除失败，Run 2 核验目标保持 absent，WAL 收敛为 0，系统不降级 | PASS |
| `test_crash_recovery_new_file_recreated_before_second_startup_degrades` | 新建文件在两次启动间被外部异常重建，Run 2 核验失败进入降级并保留 WAL 锁定 | PASS |
| `test_crash_recovery_wal_delete_failure_converges_on_second_startup` | 已有文件物理 SQLite 重开连接二次收敛，目标字节匹配 old_content，WAL 清零，系统不降级 | PASS |
| `test_healthy_startup_does_not_degrade` | 空恢复日志健康启动，返回 Ok(RecoverySummary 全部字段为0)、非降级，写操作通行 | PASS |
| `test_crash_recovery_cross_restart_pending_cleanup_converges_without_bak` | 跨重启重试收敛：Run 1 消费 .bak 后 WAL 删除失败，Run 2 重启在无 .bak 场景下完成清理收敛，Run 3 幂等 | PASS |
| `test_crash_recovery_cross_restart_pending_cleanup_external_conflict_degrades` | 重启期间目标被外部篡改，重试核验失败保留 WAL 现场并触发降级 | PASS |
| `test_crash_recovery_degraded_immediately_aborts_subsequent_wal_entries` | 条目1缺失备份触发降级后，立即中止批次，条目2 WAL与.bak完整未碰，返回精确剩余数 | PASS |
| `test_safe_restore_same_length_different_content_corruption_preserves_backup` | 模拟重命名失败及复制内容为同长度损坏字节，断言逐字节核验失败坚决拒绝恢复，保留备份与原数据 | PASS |
| `test_crash_recovery_query_staged_change_failure_does_not_rollback_and_degrades` | 暂存提案查询失败严格 fail-closed，绝不误回滚目标文件，进入降级状态并保留 WAL | PASS |
| `test_crash_recovery_declared_backup_missing_on_disk_does_not_delete_wal` | 声明备份但磁盘缺失且目标未恢复，进入降级状态，保留 WAL 且维持路径准入拦截 | PASS |
| `test_startup_wiring_staged_changes_init_failure_blocks_recovery` | 真实启动装配测试：暂存变更表初始化失败显式报错并降级 | PASS |
| `test_startup_wiring_recovery_table_init_failure_blocks_recovery` | 真实启动装配测试：崩溃恢复表初始化失败显式报错并降级 | PASS |
| `test_startup_wiring_recovery_execution_failure_degrades` | 真实启动装配测试：崩溃恢复执行失败显式报错并降级 | PASS |
| `test_startup_wiring_healthy_startup_succeeds` | 真实启动装配测试：健康启动成功返回 Summary 且不降级 | PASS |
| `test_raii_guard_isolation_and_panic_recovery` | 测试隔离：模拟测试内 panic 毒化互斥锁，后续测试自动恢复干净状态 | PASS |
| `test_staged_changes_init_failure_triggers_degraded` | 初始化暂存表失败联动全局降级 | PASS |
| `test_crash_recovery_data_restored_but_tmp_cleanup_failure_retains_wal_for_retry` | 目标文件已恢复但临时文件清理失败，保持系统非降级、WAL保留待重试 | PASS |
| `test_safe_atomic_write_backed_up_log_failure_reverts_target_preserves_bak` | `backed_up` 阶段注入 WAL 写入失败，断言目标文件被安全还原且 `.bak` 不被删除 | PASS |
| `test_safe_atomic_write_replaced_log_failure_reverts_target_preserves_bak` | `replaced` 阶段注入 WAL 写入失败，断言目标文件被安全还原且 `.bak` 不被删除 | PASS |
| `test_safe_restore_rename_and_copy_both_fail_preserves_backup` | 目标文件被占用、重命名与复制均失败，断言 `.bak` 绝对保留，返回恢复未完成错误 | PASS |
| `test_crash_recovery_applied_target_missing_or_corrupted_preserves_bak` | DB 已标记 applied 但磁盘目标丢失/损坏，保留隔离备份且 WAL 记录不被删除 | PASS |
| `test_crash_recovery_new_file_external_conflict_maintains_admission_block` | 离线期间新建文件被外部修改，恢复不误删文件且保留恢复记录持续锁定准入 | PASS |
| `test_crash_recovery_idempotent_two_consecutive_runs` | 连续多次执行崩溃恢复，断言幂等性且恢复总数及磁盘状态一致 | PASS |
| `test_fail_closed_recovery_table_query_error` | 恢复表损坏或查询失败，准入控制触发 fail-closed 严厉拒绝审批写入 | PASS |
| `test_global_degraded_recovery_state_blocks_writes` | 全局降级激活时全面阻断写入与审批 | PASS |
| `test_crash_recovery_reopen_sqlite_database` | 重启 SQLite DB 验证 WAL 恢复持久留存并自愈 | PASS |
| 基础全集 23 项测试 (基线冲突、状态保护、取消真实确认与超时、mutating工具过滤等) | 覆盖核心正常与异常路径，全部显式断言四个指标字段 | PASS |

### 4.2 Work Core 领域与事务套件 (`cargo test --lib work_core`)
测试结果：**32 passed; 0 failed; 0 ignored**

### 4.3 前端 Vitest 测试套件 (`pnpm test`)
测试结果：**44 passed (9 test files passed)**
