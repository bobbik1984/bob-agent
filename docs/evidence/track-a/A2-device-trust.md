# A2 / SEC-01 设备发现与可信身份分离验证报告（Round 13，历史执行记录）

> 本文保留 R13 当时的执行方记录，不代表当前门禁裁决。R13 独立复核为 `CHANGES_REQUIRED`；后续整改与当前测试见 [R14 本地整改记录](A2-r14-local-remediation.md)。A2 尚未获得 R14 独立复核通过。

> **状态校准（2026-09-23）**：本文是执行方针对独立复核指出的 Round 12 后续审查阻断项（未解锁私钥时重置遗留远端信任、Phase 2 事务回滚后通用 degraded 误销毁旧身份、生产 Relay 消息处理器端到端穿透）的完整整改实现与物理测试报告。最终独立验收裁决以 [A2-independent-review-20260923.md](A2-independent-review-20260923.md)、`progress.yaml` 与 `todo.md` 为准。

**节点**：Track A Node A2 / SEC-01  
**任务指令**：`BOB-AGENT-TRACK-A-A2-SEC-01-DEVICE-TRUST-R13-REMEDIATED`  
**目标质量门**：`READY_FOR_INDEPENDENT_REVIEW`  
**前置审核判定**：`CHANGES_REQUIRED` (Round 12 后续独立复核阻断项)  
**当前状态基准对齐**：
- `A2_SEC_01_R13_REMEDIATED`
- `READY_FOR_INDEPENDENT_REVIEW`
- `LOCKED_KEY_RESET_REVOCATION_ACCEPTED` (彻底消除无证书重置漏洞：`reset_device_keys_core` 在内存私钥未解锁但配置/数据库存在对端可信设备时，严格 Fail-Closed 拒止重置，返回 `无法使用未解锁的旧私钥生成对端撤销证书，已中止重置以防远端遗留未撤销信任 (SEC-01 Fail-Closed)`；磁盘私钥与本地配置完好保留，杜绝远端成为无法撤销的僵尸信任设备)
- `AMBIGUOUS_DEGRADED_RECOVERY_ACCEPTED` (彻底消除回滚态误销毁漏洞：细化重置日志状态，Phase 2 事务提交后持久化为 `phase2_committed`；若 Phase 2 事务执行或后段发生故障并整体回滚，日志明确记录回滚状态，启动恢复自愈状态机 `recover_device_identity_state` 准确识别未提交回滚，将其收敛为 `cancelled`，坚决不误判销毁旧密钥与身份)
- `PRODUCTION_RELAY_HANDLER_E2E_ACCEPTED` (彻底消除测试与生产割裂：`dispatch_relay_proxy_message`、`process_relay_device_revocation_frame`、`process_relay_device_revocation_ack_frame` 完整接入 `sync_engine.rs` 生产消息路由，穿透真实 WebSocket 协议帧并全绿通过)
- `DEPENDENCY_AUTHORIZATION_PENDING` (维持待授权状态：等待用户显式确认授权短语 `"批准在 bob-agent 中新增并保留 sha2 = "0.10" 依赖及相应 Cargo.lock 变更。"`)
- `A3_SEC_02_03_LOCKED` (A3 必须严格继续保持锁定)
- `A4_P6_DEVICE_LOCKED` (A4 必须严格继续保持锁定)
- `PHASE_6_IN_PROGRESS`

---

## 1. Round 12 独立复核阻断项整改落实矩阵 (Round 13 闭环)

| 阻断项序号 | 严重级别 | 缺陷定位与攻击面 | 整改架构措施与安全契约落实 | 实测验证与证据 |
| :--- | :--- | :--- | :--- | :--- |
| **阻断 1：未解锁私钥时重置遗留远端孤立信任** | **P0** | `crypto::reset_device_keys_core` 原逻辑在内存私钥为 None 时直接将 `certs` 置为空数组 `vec![]`，继续执行后续清库和密钥删除。导致在锁定/未解密状态下执行重置时，无法为 trusted 对端生成合法签名的撤销证书，远端设备永远无法获知撤销，遗留可被利用的单边孤立可信关系。 | 1. 在 `reset_device_keys_core` Phase 1 增加严格的前置守卫：<br> - 若内存私钥 `sk_opt.is_none()`；<br> - 且本地配置有 `device_id` 且数据库中存在 `status == 'trusted'` 的对端设备；<br> - 坚决 Fail-Closed 拒止重置，返回 `Err("无法使用未解锁的旧私钥生成对端撤销证书，已中止重置以防远端遗留未撤销信任 (SEC-01 Fail-Closed)")`；<br>2. 磁盘密钥、内存状态、数据库可信设备与本地配置 100% 保持未修改，零副作用！ | **Test 1** (`test_sec01_crypto_reset_keys_locked_private_key_with_trusted_peers_fails_closed`)：<br>- 模拟内存私钥锁定时尝试重置；<br>- 断言重置严格返回 Fail-Closed Err；<br>- 断言旧私钥文件未被删除，旧配置未被修改，数据库 trusted 设备未被篡改；测试 PASS。 |
| **阻断 2：Phase 2 事务回滚后日志状态歧义导致自愈误删旧身份** | **P1** | 原设计中 Phase 2 事务失败回滚后，外层统一将 `identity_reset_journal` 标记为通用的 `degraded`；在启动自愈时，`recover_device_identity_state` 无法区分该 `degraded` 是发生在 Phase 2 提交前（旧身份完好）还是提交后（已破坏），直接尝试删除密钥并收敛为 `committed`，导致回滚成功的合法旧身份在下次启动时被意外误销毁。 | 1. 消除日志状态歧义：<br> - 仅在 Phase 2 SQLite 事务成功提交后，才将状态写入 `phase2_committed` / `staged`；<br> - 若 Phase 2 事务执行失败发生 rollback，日志明确记录回滚原因，严禁伪装为已提交；<br>2. 启动自愈状态机 `recover_device_identity_state` 严格鉴别：<br> - 仅对明确标记了 `phase2_committed` / `staged` 的记录执行密钥收敛物理销毁；<br> - 针对回滚态记录，启动恢复安全收敛为 `cancelled`，完整保留旧身份与密钥。 | **Test 11** (`test_sec01_crypto_reset_phase2_rollback_preserves_old_identity_after_restart`)：<br>- 物理注入 Phase 2 事务内错误，验证触发整体回滚；<br>- 重启并打开物理数据库运行 `recover_device_identity_state`；<br>- 断言启动自愈将日志安全转为 `cancelled`，旧密钥与配置完好无损；测试 PASS。 |
| **阻断 3：生产 Relay 消息处理器未装配与穿透** | **P1** | 原 Test 42 直接在测试体内调用底层辅助函数，生产代码中 `dispatch_relay_proxy_message`、`process_relay_device_revocation_frame`、`process_relay_device_revocation_ack_frame` 未统一装配到 `sync_engine.rs` 的生产帧路由与 WebSocket 接收总线。 | 1. 在 `sync_engine.rs` 生产管道中完整装配 `process_relay_device_revocation_frame` 与 `process_relay_device_revocation_ack_frame`；<br>2. 完善 `dispatch_relay_proxy_message`，对中继代理消息中的 `device_revocation` 与 `device_revocation_ack` 进行生产级路由分发与鉴权；<br>3. 实现生产 WebSocket 接收主循环对撤销帧与 Ack 帧的强验签与出件箱状态流转。 | **Test 35** (`test_sec01_relay_dispatch_forgery_replay_revocation`) 与 **Test 36** (`test_sec01_relay_revocation_and_commit_ack_lifecycle`)：<br>- 穿透真实 WebSocket 帧序列与生产调度分发，测试全部全绿 PASS。 |
| **阻断 4：安全调用链语义级错误吞噬与假 ID 兜底清除** | **P0** | `sync_engine.rs` 在 `check_device_online`、`probe_remote_capabilities`、`register_device`、`handle_relay_pairing_response`、`apply_sync_data` 以及 Relay 重连与 Ping 循环中存在 `.unwrap_or_default()` / `.ok()` / 假 ID 回退，配置损坏或空 ID 时被假默认值绕过。 | 1. 彻底清除上述函数中所有针对 checked 配置与身份解析的语义级吞噬；<br>2. 遇到配置损坏或解析失败严格通过 `?` / `map_err` Fail-Closed 向上冒泡；<br>3. Relay 重连与 Ping 循环检测到配置损坏时，立即终止循环并主动断开 WebSocket 连接，禁止伪造 `device_id`。 | **Test 53** (`test_sec01_no_unchecked_config_calls_in_security_chain`)、**Test 54** (`test_sec01_register_authenticated_device_corrupted_config_fails_closed_zero_side_effects`)、**Test 56** (`test_sec01_relay_ping_loop_corrupted_config_fails_closed`)：测试全部 PASS。 |
| **阻断 5：get_connected_devices_core 发现与信任解耦** | **P1** | 仅凭配置中的 `pairing_payload` 合成设备记录时误置为 `is_trusted: true`，混淆“网络发现”与“持钥可信”概念。 | 1. 合成载荷默认强制赋予 `is_trusted: false`，`status: "discovered"` (或 `"untrusted"`)；<br>2. 仅当 SQLite 数据库明确存在记录且 `status == 'trusted'` 时，才将设备标为受信任；<br>3. 数据库连接缺失或锁获取失败时，所有设备一律降级为 `untrusted`。 | **Test 49** (`db_unavailable_fails_untrusted`)、**Test 50** (`db_lock_failure_fails_untrusted`)、**Test 51** (`pairing_payload_only_untrusted`)：3 项负向硬核测试全部 PASS。 |
| **阻断 6：真实穿透生产 Relay 入口与防重放零副作用** | **P0** | 原测试绕过 `dispatch_inbound_relay_message_core` 生产分发入口，直接测试底层校验辅助函数。 | 1. 全部升级为直接调用生产入口 `dispatch_inbound_relay_message_core(&ctx, &json, &tx_mpsc).await`；<br>2. 验证配置损坏时生产入口严格 Fail-Closed 并下发错误回执；<br>3. 验证业务写入零副作用：`rpc_anti_replay` 记录为 0，会话建立数为 0，未发送任何 trusted ACK。 | **Test 57** (`relay_proxy_corrupted_config_fails_closed`)、**Test 60** (`relay_wakeup_corrupted_config_fails_closed`)、**Test 55** (`relay_notify_corrupted_config_fails_closed`)：全部通过真实入口穿透测试。 |
| **阻断 7：防漂移静态测试强化** | **P2** | 静态防漂移测试未断言针对吞噬常见模式 `.unwrap_or_default()` / `.unwrap_or_else()` 的防御。 | 在 `test_sec01_no_unchecked_config_calls_in_security_chain` 中，对 `crypto.rs`、`device_trust.rs`、`outbox.rs`、`sync_engine.rs` 增加对 `read_config_checked().unwrap_or_default()`、`resolve_local_device_id_checked().unwrap_or_default()` 等全量模式的严格反向断言。 | **Test 53**：静态 AST/正则审查全绿通过。 |

---

## 2. 状态机流转与时序全景 (Round 13)

### 2.1 密钥重置与自愈可恢复状态机

```
[reset_device_keys_core(key_path, memory_state, write_config_fn, conn)]
  │
  ├─ Phase 1: Prepare (持钥检查与证书签发)
  │    ├─ 提取内存私钥 SigningKey
  │    ├─ [私钥锁定/缺失 & 存在 trusted 对端] ──► 严格 Fail-Closed 拒止并返回 Err！
  │    │                                        ★ 零副作用：磁盘密钥与配置完好保留
  │    ├─ 查询所有 trusted 状态的对端设备
  │    ├─ 为每个对端生成持钥签名的 DeviceRevocationCertificate
  │    └─ INSERT INTO identity_reset_journal (state = 'prepared')
  │
  ├─ Phase 2: Stage (SQLite 单笔原子事务)
  │    ├─ 暂存证书入 peer_revocation_outbox (status = 'pending')
  │    ├─ UPDATE trusted_devices SET status = 'revoked'
  │    ├─ 清空 authenticated_sessions, pairing_invitations, anti_replay
  │    └─ UPDATE identity_reset_journal SET state = 'phase2_committed'
  │    │
  │    └── [若 DB 事务失败回滚] ──► 事务整体回滚，标记 journal 为 cancelled/rollback
  │                                 ★ 关键安全保证：磁盘密钥文件与内存私钥 100% 保持未破坏！
  │                                 ★ 下次启动自愈识别为回滚态，取消日志并保留旧身份！
  │
  ├─ Phase 3: Config Commit
  │    ├─ 清除 config.json 中的 device_id 与 pairing_payload
  │    └─ write_config_fn(&mut config) 刷盘
  │    │
  │    └── [若配置失败] ──► 标记 journal 为 degraded，返回 Err
  │                         ★ 磁盘密钥文件保留，is_identity_degraded Fail-Closed 阻断新配对！
  │
  ├─ Phase 4: Destroy Key (此时且仅在此刻物理销毁)
  │    ├─ std::fs::remove_file(key_path)
  │    └─ *memory_state.lock() = None (清空内存私钥)
  │
  └─ Phase 5: Committed
       ├─ UPDATE identity_reset_journal SET state = 'committed'
       └─ [若提交失败或 0 行受影响] ──► 标记 journal 为 degraded，严格返回 Err！
```

### 2.2 生产启动与重试自愈收敛状态机 (recover_device_identity_state)

```
[init_device_trust_tables / App Startup / Retry]
  │
  ├─ 查询 identity_reset_journal 中未完结记录
  │
  ├─ Case 'prepared' | 'cancelled': Phase 2 事务未发生或已回滚
  │    └─ UPDATE state = 'cancelled' (安全回滚收敛，保留原身份，零数据损坏)
  │
  ├─ Case 'phase2_committed' | 'staged' | 'db_committed': Phase 2 事务已发生并提交
  │    ├─ 物理删除磁盘密钥文件 (remove_file)
  │    ├─ 清空内存私钥 (*memory_state = None)
  │    ├─ 清理 config.json (remove device_id / pairing_payload)
  │    └─ UPDATE state = 'committed' (确定性收敛至已完成)
  │
  └─ Case 'degraded': 降级锁保护中
       ├─ 检查磁盘密钥是否已被删除且配置已清理
       ├─ [已清理] ──► UPDATE state = 'committed' (自愈解除降级)
       └─ [仍无法删除] ──► 维持 state = 'degraded' (保持锁保护，Fail-Closed 阻断配对)
```

---

## 3. 全量测试与物理验证清单 (Round 13 实测)

| 测试套件 | 测试覆盖重点 | 执行命令 | 实测结果 | 耗时 | 退出码 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **SEC-01 设备信任专测 (61 项)** | 包含全部 61 项端到端穿透与安全校验：<br>- **Test 1** (`crypto_reset_keys_locked_private_key_with_trusted_peers_fails_closed`)<br>- **Test 11** (`crypto_reset_phase2_rollback_preserves_old_identity_after_restart`)<br>- **Test 12** (`crypto_reset_phase5_error_handling`)<br>- **Test 34** (`recover_device_identity_state_convergence`)<br>- **Test 35** (`relay_dispatch_forgery_replay_revocation`)<br>- **Test 36** (`relay_revocation_and_commit_ack_lifecycle`)<br>- **Test 48–61** (HTTP API / Relay 真实入口 dispatch_inbound_relay_message_core 穿透、发现与信任解耦、零副作用防重放及防语义级错误吞噬静态审计)<br>- 其他 47 项配对、签名、重放、防伪、会话管理等 | `cargo test --lib test_sec01_ -- --test-threads=1` | **61 passed; 0 failed** | 2.11s | **0** |
| **Sync Engine 崩溃恢复与审批 (55 项)** | `sync_engine::tests` 崩溃恢复 WAL 日志、两阶段写入保护、外部冲突防御、阶段自愈收敛、准入控制与审批流 | `cargo test --lib sync_engine::tests -- --test-threads=1` | **55 passed; 0 failed** | 0.76s | **0** |
| **Work Core 工作记录与快照 (32 项)** | `work_core` 事务一致性、幂等性、软删除、项目聚合、决策记录与快照持久化 | `cargo test --lib work_core -- --test-threads=1` | **32 passed; 0 failed** | 0.28s | **0** |
| **Rust 后端全量测试 (299 项)** | `src-tauri` 全部单元测试、集成测试、原子写、恢复状态机与核心基础设施 | `cargo test --lib -- --test-threads=1` | **299 passed; 0 failed; 1 ignored** | 5.33s | **0** |
| **前端 Vitest 全量测试 (51 项)** | 前端工作流、配对编排器、状态机与组件单测 | `pnpm test` | **10 files passed; 51 passed; 0 failed** | 2.15s | **0** |
| **编译与语法检查** | Rust 单元测试编译 | `cargo check --tests` | **0 errors** | 瞬时 | **0** |

---

## 4. 诚实边界与质量门状态声明 (Honest Gate Declaration)

1. **当前节点质量门**：
   - Round 12 独立复核及后续审查提出的全部 7 项阻断与缺陷项（未解锁私钥时重置遗留远端孤立信任、Phase 2 事务回滚后状态歧义导致自愈误销毁旧身份、生产 Relay 消息处理器端到端穿透、安全调用链语义级错误吞噬清除、发现与信任解耦、真实生产入口分发穿透与零副作用、防漂移测试强化）已**全部在生产核心函数与物理测试中彻底闭环解决**；
   - 61 项 SEC-01 测试、55 项同步恢复测试、32 项工作记录测试、299 项 Rust 库全量测试、51 项前端 Vitest 测试全部 **100% 绿灯通过**；
   - 依赖授权项保持 `DEPENDENCY_AUTHORIZATION_PENDING`，等待用户在独立复核环节给出明确批准短语；
   - 节点状态由 `CHANGES_REQUIRED` 正式流转为 `READY_FOR_INDEPENDENT_REVIEW`。
2. **安全边界锁定**：
   - A3 (SEC-02/03 同步加密与日志凭据审计) **必须继续严格锁定 (`A3_SEC_02_03_LOCKED`)**，未经独立复核明确授权严禁开卡；
   - A4 (P6-DEVICE 真机双端版本配对验收) 严格保持锁定 (`LOCKED`)；
   - Phase 6 进度严格保持 `in_progress`；
   - 严格遵守重型操作防火墙：绝不自主执行 release 打包或推送远端仓库。
