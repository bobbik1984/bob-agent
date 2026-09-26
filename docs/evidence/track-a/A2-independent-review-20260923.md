# Track A A2 / SEC-01 独立复核校准（2026-09-23，R13）

## 裁决

- 当前节点：`A2 / SEC-01`
- 独立复核结论：`CHANGES_REQUIRED`
- Phase 6：`in_progress`
- A3 / SEC-02/03：`LOCKED`
- A4 / P6-DEVICE：`LOCKED`

R13 真实修复了“有可信对端但旧私钥未解锁时仍销毁身份”与“Phase 2 回滚后重启误销毁旧身份”两个上轮阻断项。独立复跑 61 项 SEC-01 测试也全部通过。但真实生产链中，撤销 Outbox 没有排空入口，离线撤销证书会在旧私钥销毁后过期，且 Test 42 仍未与生产 Relay 主分发器共用处理函数。因此还不能宣告撤销闭环或通过 A2 Gate。

## 已确认改善

1. `reset_device_keys_core` 现在会先查询 `trusted_devices`；只要有可信对端且内存中无旧 `SigningKey`，就在写 journal、DB、配置或密钥文件前 Fail-Closed。
2. Phase 2 事务失败后会标记 `degraded_pre_db`，启动恢复将其收敛为 `cancelled`，保留旧私钥、配置、信任和会话。
3. Relay 撤销与 Ack 已有可重用 helper，并且 Test 42 使用真实本地 WebSocket 帧转发。
4. 独立执行 `cargo test --lib test_sec01_ -- --test-threads=1`：61 passed、0 failed、0 ignored。

## 阻断项

### P0：持久化撤销 Outbox 没有生产排空者

`reset_device_keys_core` 会把预签名撤销证书写入 `peer_revocation_outbox`，但全库中 `drain_peer_revocation_outbox_via_relay`、`drain_peer_revocation_outbox_via_lan`、`drain_peer_revocation_outbox` 和 `get_pending_peer_revocations` 只出现在 helper 与测试中，生产 `sync_engine` 的启动、重连和定时循环都没有调用它们。

影响：本机会先撤销本地记录并销毁旧私钥，但 `pending` 撤销证书不会被发送，远端仍持续信任旧公钥。

### P0：离线超过 5 分钟后，预签撤销证书永久不可用

撤销证书在重置 Phase 1 一次性预签，之后旧私钥被销毁。接收方 `sec01_verify_and_apply_peer_revocation` 却将该持久撤销事件限制为 5 分钟时钟窗口。对端只要离线超过 5 分钟，Outbox 内的唯一签名证书就会永久被拒绝，本机也已无旧私钥可重签。

### P1：Test 42 仍未与生产 Relay 主分发器复用同一 handler

`process_relay_device_revocation_frame` 和 `process_relay_device_revocation_ack_frame` 仅由 Test 42 调用。真实生产主循环调用 `dispatch_inbound_relay_message_core`，其中的 `device_revocation` / `device_revocation_ack` 仍是另一份手写逻辑，没有调用上述 helper。当前 Evidence 中的“100% 生产与测试复用”与 `PRODUCTION_RELAY_HANDLER_E2E_ACCEPTED` 不成立。

### P1：历史/未知恢复状态的事实查询未 Fail-Closed

`recover_device_identity_state` 处理历史或未知 journal state 时，对 Outbox 和 trusted device 的事实查询使用 `unwrap_or(0)`，然后又依赖 `error` 文本判断 Phase 2 是否提交。任一事实查询失败都应保留 journal 和旧密钥并返回降级/阻断，不得把错误当作零值继续销毁身份。

## 证据校准

- 当前工作树独立复跑的 SEC-01 数量是 **61**，不是交接文本中的 56。
- `staged_write_recovery(target_path)` 当前是普通 `CREATE INDEX`，不是交接文本所称的唯一索引。
- 本轮没有执行发布构建、真机验收、远程部署或 Git 推送。

## 最小整改与复验边界

1. 将 Outbox 排空纳入生产启动、Relay 连接/重连和 LAN 可用事件；只有经验签 CommitAck 才能进入 `delivered`。
2. 将长期持久撤销事件与 5 分钟短时 RPC 窗口分离，覆盖对端离线超过 5 分钟后上线撤销。
3. 让生产主分发器与 Test 42 复用唯一 handler，删除重复逻辑。
4. 对历史/未知恢复状态的事实查询进行故障注入，证明查询异常时不会删除旧私钥。
5. A2 继续保持未通过；`sha2 = "0.10"` 仍为 `DEPENDENCY_AUTHORIZATION_PENDING`；A3/A4 继续 `LOCKED`。
