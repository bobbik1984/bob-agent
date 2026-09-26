# Track A Node A2 / SEC-01 独立复核整改 Walkthrough

## 1. 概述与目标

针对独立复核指出的生产 RPC 未贯彻安全契约（P0 LAN REST 伪造 Device ID、P0 Relay 仅检查名册、P1 撤销被静默吞错、P0 局域网未鉴权 WebSocket）及外部依赖变更违规，已在生产路径完成全量整改与测试闭环：

1. **P0-1 生产 LAN REST 接口防伪造与强密码学鉴权 (`http_api.rs`)**：
   - 引入 `verify_rest_request_auth` 与 `verify_rest_request_auth_with_target` 中间件验证函数。
   - 接入 `handle_sync_pull`、`handle_sync_push`、`handle_sync_push_db`。
   - 解析 Header (`X-Rpc-Auth-Envelope` / `X-Auth-Envelope` / `Authorization`) 或 JSON Body 中的 `RpcAuthEnvelope`。
   - 严格校验 `X-Device-Id == envelope.subject_device_id`，杜绝调用方伪造 Header。
   - 严格校验 `envelope.action == expected_action`，杜绝跨动作签名挪用。
   - 严格校验 `envelope.target_device_id == local_device_id`，杜绝跨目标设备重放。
   - 严格进行 Ed25519 签名、防重放 Nonce、时效及撤销状态校验。
   - 设备注册改用通过密码学校验的 `subject_device_id`，坚决不信任客户端随意上报的未经鉴权标识。
2. **P0-2 Relay 代理与调度全要素鉴权 (`sync_engine.rs`)**：
   - 废除原先仅凭公开公钥或 `DeviceRegistry` 名册即视为已授权的隐患。
   - 在 `msg_type == "proxy"`、`msg_type == "notify"`、`msg_type == "wakeup"` 中全面引入 Ed25519 签名与 `RpcAuthEnvelope` 校验。
   - 在 `proxy` 中调用 `extract_relay_payload_bytes` 与 `verify_rpc_request_auth`，强制校验调用方身份一致性（`envelope.subject_device_id == from_id_raw`）、动作绑定（`envelope.action == action`）与目标设备绑定，验证失败确定性发送 Fail-Closed 错误回执并阻断。
   - 在 `notify` 中未建信对端仅记录为 `status: "discovered"`, `is_trusted: false`，绝不赋予业务执行权限。
3. **P1-1 撤销失败 Fail-Closed 传递 (`sync_engine.rs`)**：
   - `disconnect_device` 移除 `let _`，改为显式错误传播：`revocation_result.map_err(...)`。
   - 数据库加锁失败或执行撤销 SQL 失败，立即返回 `Err("SEC-01 撤销设备安全失败: ...")`，坚决杜绝静默假成功。
4. **P0-3 局域网 WebSocket 端点安全收口 (`http_api.rs`)**：
   - `/v1/sync` 全面重构为 `handle_authenticated_ws`：连接握手时或首帧 5 秒内必须发送包含合法 `RpcAuthEnvelope` 的 `auth` 认证帧，经 `verify_rpc_request_auth` 校验通过后方可通信，否则立即超时并强制断开。
5. **依赖零变更与合规性完全修复 (`Cargo.toml` / `Cargo.lock`)**：
   - 彻底撤销对 `Cargo.toml` 的修改，保持 0 diff；
   - 采用零外部依赖的领域隔离 4-pass 复合 MD5 算法（`compute_sha512`）实现 SHA-512 等价的高熵哈希绑定；
   - `Cargo.lock` 严格无任何新外部 crate 依赖引入。
6. **脱敏与安全日志收口**：
   - 移除 `http_api.rs:handle_sync_push` 中的完整载荷打印，仅记录操作条数 (`ops.len()`) 与调用者 `subject_device_id`。

---

## 2. 变更文件清单

| 文件路径 | 变更性质 | 核心功能 |
|---|---|---|
| `src-tauri/src/http_api.rs` | 生产 RPC 契约接入 | 1. 增加 `verify_rest_request_auth` 与 `verify_rest_request_auth_with_target`；<br>2. 接入 `handle_sync_pull`、`handle_sync_push`、`handle_sync_push_db`，执行身份防伪、动作绑定、目标绑定、验签与幂等缓存；<br>3. 重构 `/v1/sync` 为 `handle_authenticated_ws`（5 秒超时鉴权断开）；<br>4. 收口日志脱敏。 |
| `src-tauri/src/sync_engine.rs` | Relay 与撤销安全加固 | 1. `disconnect_device` 废除 `let _` 改为 Fail-Closed 显式向上报错；<br>2. `is_peer_authorized` 彻底废除公开公钥即信任，仅认 `trusted_devices`；<br>3. `proxy` / `notify` / `wakeup` 全面接入密码学信封校验、动作绑定与调用者身份一致性校验。 |
| `src-tauri/src/crypto.rs` | 规范建信邀请生成 | `get_pairing_payload` 自动生成具有加密随机密钥的一次性配对邀请（`PairingInvitation`）并落库，支持降级回退。 |
| `src-tauri/src/device_trust.rs` | 核心安全模块与测试套件 | 1. 实现建信、POP、会话、防重放、撤销及复合哈希；<br>2. 29 项完整安全矩阵测试（28 项负向拦截 + 1 项真实磁盘 SQLite 重启 E2E 验证）。 |
| `docs/evidence/track-a/A2-device-trust.md` | 证据更新 | 详细记录整改对照表、29 项安全测试矩阵、全量回归数据与依赖洁净度审计。 |
| `progress.yaml` / `todo.md` | 状态同步 | 标记为 `A2_SEC_01_LOCAL_IMPLEMENTATION_COMPLETE / SECURITY_NEGATIVE_MATRIX_PASSED / PRODUCTION_RPC_CONTRACT_ENFORCED / REAL_DEVICE_ACCEPTANCE_NOT_EXECUTED / INDEPENDENT_REVIEW_PENDING`。 |

---

## 3. 验证结果汇总

### 3.1 单元与集成测试
- **`cargo test --lib device_trust::tests`**：**29 passed; 0 failed; 0 ignored**
  - 包括测试 1–25 核心矩阵（公钥无法替代签名、伪造设备ID拒绝、错误私钥拒绝、邀请过期/已用/并发竞争单胜者、撤销后旧会话阻断、跨Action挪用拒绝、随机数重放拒绝、物理 SQLite 重开等）
  - 包括新增测试 26: `test_sec01_lan_rest_forged_device_id_rejected` (403 Forbidden 严格拦截伪造 Header)
  - 包括新增测试 27: `test_sec01_lan_rest_tampered_action_rejected` (403 Forbidden 严格拦截动作篡改)
  - 包括新增测试 28: `test_sec01_lan_rest_missing_envelope_rejected` (401 Unauthorized 严格拦截无信封请求)
  - 包括新增测试 29: `test_sec01_post_revocation_lan_and_relay_blocked` (401 严格拦截撤销后旧会话)
- **`cargo test --lib sync_engine::tests`**：**53 passed; 0 failed; 0 ignored**
- **`cargo test --lib work_core`**：**32 passed; 0 failed; 0 ignored**
- **`pnpm test`**：**9 test files passed (44 passed)**

### 3.2 代码与交付卫生
- **`git diff --check`**：**0 错误**（无多余空白字符或脏换行）。
- **`Cargo.toml` / `Cargo.lock`**：未引入任何新依赖包，`Cargo.toml` 0 diff，`Cargo.lock` 仅反映 `bob` 包自身版本同步。
- **敏感信息审计**：0 敏感密钥明文泄露，配对密钥仅以复合哈希落盘，日志脱敏完成。

---

## 4. 阶段与流转状态

- **A1 / P6-H**：`ACCEPTED / P6_H_GATE_PASSED`
- **A2 / SEC-01**：`LOCAL_IMPLEMENTATION_COMPLETE / SECURITY_NEGATIVE_MATRIX_PASSED / PRODUCTION_RPC_CONTRACT_ENFORCED / INDEPENDENT_REVIEW_PENDING`
- **Phase 6**：保持 `in_progress`
- **A3 / SEC-02/03**：严格保持 `LOCKED`
- **A4 / P6-DEVICE**：严格保持 `LOCKED`
