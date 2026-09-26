# Track A A3 / SEC-02 & SEC-03 传输边界审计与同步数据脱敏复核报告 (Round 9 / Startup Sync Explicit Contract, Production Step Transition Wire-in & Vitest Sandbox Exit Code 0 Fix)

日期：2026-09-25  
范围：本地传输通道审计、LAN/Relay Outbox 耐久性协议重构、非对象配置阻断自愈、正向白名单、部分同步失败阻断与报错、远程操作契约与执行器统一、接收端合法畸形 JSON 阻断、投递状态契约化 (applied vs pending_apply) 与重试收敛、查询投递状态失败降级 Fail-Closed、缺失 stage 回执强验拒斥、幂等缓存更新防吞错、Relay 生产接收端真实落盘驱动与回执透传、Relay 消费闭环与 pending_apply 拦截、活跃同步返回值语义升级 (ActiveSyncOutcome)、上层诊断与历史记录彻底阻断虚报成功、前端配对与首次同步状态严格解耦 (paired vs applied / failed / skipped / unexpected)、步骤单向推进与迟到事件防御直连生产函数、App.vue 启动自动同步仅明确 applied 判定成功与更新时间戳、Vitest 沙箱 Exit Code 0 保障、真实生产处理路径测试全覆盖；未触及双端真实硬件配对、未触发打包与生产构建。

---

## 1. 核心裁决与结论

- **A3 (SEC-02 & SEC-03) 经生产级全链路状态收口整改后，本地安全边界审计、执行器契约统一、持久化状态机、接收端语义漏洞、Relay 生产接收端真落盘回执与 Fail-Closed 补强、ActiveSyncOutcome 显式状态机、调用方诊断与历史防虚报成功、配对流与首次同步严格解耦、App.vue 启动自动同步强类型收口、迟到事件防御直连生产状态机、Vitest 沙箱退出码修复及真实生产路径穿透测试全部验证通过。结论：A3 本地复核通过；A4 真机验收尚未执行（保持 LOCKED）。明确说明：未宣称公网 Relay、物理手机或发布验收已通过，未擅自推送云端看板。**
- **针对代码复核提出的全部缺陷与同范围缺口已 100% 根治**：
  1. **LAN / Relay 推送待同步操作可能丢失风险**：彻底废除“发出即删”策略，重构为必须收到对端确认提交回执 (`status: "ok"` / `"committed"` / `commit_ack`) 才删除 outbox。在传输失败、HTTP 401/500 或无明确回执时本地文件严格保留待重试。
  2. **异常非对象配置绕过合并白名单风险**：`read_config_checked_at` 统一实施 Fail-Closed 校验，任何非 JSON Object (字符串、数组、数字、布尔) 强制报错阻断；`merge_synced_config` 在本地配置非对象时直接返回本地原样，绝不退回或返回全量远端配置。
  3. **黑名单与包含匹配盲区**：全面废弃关键字匹配黑名单，改为**正向白名单 (Constructive Allowlist)**。`export_sync_data_from_conn` 从零构造公开配置项与剥离 apiKey 的自定义模型元数据，settings 采用 `WHERE key IN (...)` 显式白名单。
  4. **推送失败仍显示“同步完成”缺陷**：在 LAN 与 Relay 双通道中，Pull 成功后继续执行的 Outbox 推送或 `push_db` 数据推送一旦失败（网络错误、HTTP 非 200、或无提交回执），全面拦截并归集到 `push_errors`，本地文件严格保留待重试，通过 `sync:progress` 显式向前端发出 `status: "error"` 与 `"部分同步失败: <detail>"`，绝对不推进成功状态，底层函数向上返回 `Err(...)`。前端配对工作流与 UI 捕获该错误并同步标红。
  5. **远程操作白名单与实际执行器不一致缺陷**：消除 `device_trust.rs` 允许虚构操作 `create_item`、`add`、`delete` 但 `outbox.rs` 生产执行器不支持的契约割裂。`ALLOWED_REMOTE_PUSH_OPS` 统一收敛为 `&["set_config"]`。在 `drain_staged_outbox` 增加执行防护（若所有操作均被执行器拒斥则记录错误绝不标为 delivered），并通过端到端测试验证 `set_config` 在经过白名单校验后真正驱动调谐落地 `config.json` 磁盘文件并转入 `delivered`。
  6. **无法识别的合法 JSON 漏洞闭环**：彻底解决接收端 `/v1/sync/push` 原先对无法解析为数组或不含 `ops`/`data` 的合法对象（如 `{"foo": "bar"}`）静默转换为空操作并返回 `status: "ok"` 导致发送端误删本地暂存的漏洞。实现强类型结构校验与 Fail-Closed 拒斥（HTTP 400 Bad Request），确保发送端在面对格式不符或畸变载荷时坚决保留本地 outbox 绝不删除。
  7. **“提交回执”与“配置已生效”语义分离与契约化**：确立强类型提交回执契约 `PushCommitReceipt`，严格区分 `stage: "applied"`（已成功应用生效）与 `stage: "pending_apply"`（已安全入队暂存但待应用，例如受配置锁竞争、异步消费或写入延迟影响）。事务提交后实时查询投递状态；`push_db` 同样纳入该契约。发送端感知到 `pending_apply` 时保留本地 outbox 并阻断完成打勾，向前端上报 `pending_apply` 状态（显示“待对端应用配置”）。针对配置锁或磁盘写入异常场景，通过重试收敛驱动最终落盘并转入 `applied`。
  8. **回执消费路径虚报成功缺陷根治 (Round 5 针对性整改)**：
     - **查询投递状态失败误报 applied**：修复 `device_trust.rs` (line 1816) 与 `sync_engine.rs` (line 3776) 中 `check_res` 的 `Err(_)` 分支，彻底废除“查询失败仍宣称 applied”的漏洞，统一 Fail-Closed 降级返回 `stage: "pending_apply"` 并携带具体错误原因，杜绝在无法核实磁盘落盘状态时宣称生效。
     - **强制校验完整回执与杜绝静默吞错**：实现 `parse_push_commit_receipt` 工具函数，全面废除发送端 `unwrap_or("applied")` 宽容默认值，回执缺失或非 `"applied"`/`"pending_apply"` 一律返回 `Err` (Fail-Closed)；在更新 `rpc_idempotency_cache` 的 `response_json` 时校验影响行数，影响 0 行立即报错，杜绝吞错或重放无 stage 旧回执。
     - **Relay 通道消费新回执语义与闭环**：Relay 活跃同步路径的 outbox 推送与 `push_db` 全面接入 `parse_push_commit_receipt`；遇 `pending_apply` 严格保留本地 outbox，向前端发出 `sync:progress` (`status: "pending_apply"`) 并**坚决阻断发射 `status: "done"`**。
  9. **Relay 接收端回执虚报与未落盘宣称生效根治 (Round 6 针对性整改)**：
     - **Relay push 接收端真实落盘驱动**：彻底废弃直接调用 `write_outbox` 且盲目返回 `stage: "applied"` 的漏洞，全面接入 `atomic_commit_push_outbox`，根据持久化投递落盘结果动态返回 `applied` 或 `pending_apply`，失败时 Fail-Closed 更新幂等为 failed 并发送 signed error frame，绝不发送 `commit_ack`。
     - **Relay push_db 接收端投递回执透传**：`import_sync_data` 与内部 `import_sync_data_to_conn_atomic` 完整保留并向上透传 `outcome.receipt`；Relay 分发循环提取真实 `receipt.stage`、`applied_count`、`pending_count` 及 `delivery_error` 返回对端，在配置仅为暂存状态时如实上报 `pending_apply`，写入失败时 Fail-Closed 更新幂等为 failed 并发送 signed error frame，绝不发送 `commit_ack`。
     - **穿透真实生产分发入口的自动化回归**：使用全新的 `RelayDispatchContext::for_test_with_signer` 穿透生产分发函数 `dispatch_inbound_relay_message_core`，取代 Mock 凭据验证，验证了在恶意越权/非法载荷下的 Fail-Closed 拦截、磁盘故障下的 pending_apply 回执与发送端本地文件保留、以及故障恢复后后台收敛与后续 Push 达到 applied 且发送端删除 outbox 的全生命周期闭环。
  10. **全链路状态收口与诊断对齐 (Round 7 针对性整改)**：
      - **`do_active_sync` 返回值强类型语义化 (`ActiveSyncOutcome`)**：LAN 与 Relay 活跃同步分支中，若配置落盘回执为 `pending_apply`，返回 `ActiveSyncOutcome::PendingApply { transport, reasons }`；唯有配置落盘确认 `applied` 时才返回 `ActiveSyncOutcome::Applied { transport }`。
      - **调用方诊断与历史记录彻底防虚报**：`trigger_mobile_sync` 消费 `ActiveSyncOutcome`，遇到 `PendingApply` 时，在诊断事件与运行记录中严格写入 `DiagnosticStatus::Pending` 与 `"配置变更已入队待应用: ..."`，**严禁**记录 `Success` 或“同步完成”；仅在 `Applied` 时记录 `Success` 与“同步完成”；发生故障时明确记录 `Failed`。
      - **`import_sync_data` 与接收端完成事件精准发射**：`import_sync_data` 检查真实 `receipt.stage`，只有在 `applied` 时才发射 `sync:completed`；在 `pending_apply` 时发射 `sync:progress` (`status: "pending_apply"`)，杜绝广播虚假完成。
  11. **配对与首次同步解耦、历史真实化与真实生产路径测试 (Round 8 针对性整改)**：
      - **配对流严格区分设备配对与首次同步状态**：`pairing-flow.js` 彻底根除首次同步异常、缺失或非预期响应仍返回 `success: true, stage: 'applied'` 的漏洞。严格解耦返回 `{ success: isApplied || isPending, paired: true, applied: isApplied, pending_apply: isPending, stage: syncStage, error: syncError }`。失败时 stage 为 `failed`，缺失接口时为 `skipped`，非预期时为 `unexpected`。
      - **连接面板 UI 准确映射**：`SettingsConnections.vue` 引入 `pairingPairedSyncFailed` 与 `pairingPairedSyncSkipped`。设备已配对但同步失败时弹窗标题明确展示“设备已配对，但首次同步失败”；未同步时展示“设备已配对，未执行首次同步”；只要设备完成持钥配对验证，即刻拉取并展示已连接设备。
      - **步骤单向推进与迟到事件防御**：`updateStep` 实现终态防护，处于 `error` 状态的步骤不再被迟到的 `done` / `running` 进度事件覆盖；处于 `pending_apply` 状态的步骤不再被迟到的 `done` / `running` 覆盖，保证异步总线上的滞后消息不破坏状态真实性。
      - **本地导入历史精准反映暂存状态**：`import_sync_data` 生产路径接入 `build_sync_import_history_entry`，在 `stage: "pending_apply"` 时历史日志落盘 `status: "pending_apply"`，明细详细说明暂存待应用原因，杜绝在历史记录中提前写入“成功合并云端数据”或“同步更新已应用”。
      - **测试切入真实生产处理路径**：`test_sec03_active_sync_outcome_and_caller_diagnostics_closure` 废除测试体手写 match，改为直接调用生产处理函数 `record_trigger_sync_outcome` 与 `build_sync_import_history_entry`，全覆盖验证 pending_apply、applied、failed 分支及落盘记录；前端补齐测试覆盖各种同步异常与迟到事件防御。
  12. **启动自动同步契约收口、迟到防护直连生产状态机与测试沙箱退出码修复 (Round 9 针对性整改)**：
      - **App.vue 启动自动同步彻底废除“非 pending 即 success”的宽容分支**：提取强类型回执判定函数 `handleStartupSyncOutcome`，严格定义契约：唯有明确的 `res?.status === 'applied'` 才允许判定为 success、标记 `lastSyncStatus = 'success'` 并更新 `bob-last-sync-time` 时间戳；`res?.status === 'pending_apply'` 明确置为 pending，绝不更新时间戳；任何空值、非对象、未知状态或缺少明确 applied 的返回值，一律判定为 error 报错并启动静默 UDP 监听，**绝不更新同步时间戳**。
      - **消除迟到事件测试的逻辑复制，直连生产状态机**：将步骤状态转移与防覆盖保护逻辑独立收敛至 `src/sync/pairing-flow.js` 的 `applyStepTransition(steps, id, status, detail)` 生产函数，并在 `SettingsConnections.vue` 的 `updateStep` 中直接调用；在 `pairing-flow.test.js` Test 15 中彻底消除模拟函数复制，改为直接执行 `applyStepTransition` 生产逻辑；新增 Test 16–18 完整覆盖 `handleStartupSyncOutcome` 对 applied、pending_apply 以及空/未知回执的判定防护与时间戳更新阻断。
      - **Vitest 沙箱只读目录导致 Exit Code 1 缺陷修复**：Windows 或受限沙箱中 `DEV_CACHE_ROOT` 目录不可写会导致 Vitest 在测试全部通过后尝试写入 `results.json` 触发 `EPERM` 并退出码 1。在 `vite.config.js` 中增加目录可写性前置探测（若不可写回退至系统临时目录），配置 `test: { cache: false }`，并在 `package.json` 的 test 脚本中注入 `--no-cache`，确保测试在纯内存运行，进程 0 退出码 (Exit 0)。
- **真实加密边界判定**：
  - **Relay 与 LAN 绝不可宣传为“端到端加密 (E2EE)”**。
  - Relay 依靠 TLS 传输至 `relay.bobbik.org`，但 TLS 终结于中继服务器，中继节点能够完全解析明文 JSON 载荷（仅有 Ed25519 PoP 签名保障防篡改和防伪造，无端到端载荷加密）。
  - LAN 为内网明文 HTTP (3722 端口)，虽有 Ed25519 签名及防重放，但在局域网内明文可见。
  - 只有 Web Drop (WebRTC DTLS / AES-128-GCM，密钥位于 URL Hash Fragment) 具备端到端加密特性。
- **密钥物理隔离保证**：所有 LLM API 密钥与第三方 Bot Token (Discord, Telegram, WeChat 等) 严格驻留执行设备，跨端同步与 Outbox Push 彻底剔除敏感字段并拒绝远程注入。
- **Gate A4 状态**：严格保持 **LOCKED**。本地自动化测试不代替手机—PC 真实硬件表现，待进入 A4 阶段固定版本由用户参与实测。


---

## 2. 四大通信通道审计矩阵 (SEC-02)

| 通道 | 发送方 / 接收方 | 身份验证锚点 | 传输加密 | 谁能读取明文内容？ | 失败/异常处理机制 |
|---|---|---|---|---|---|
| **LAN 直连 (HTTP / WS)** | 移动端 ↔ PC 本地端 (`0.0.0.0:3722`) | `RpcAuthEnvelope` (Ed25519 PoP 签名 + 认证会话 + Nonce 防重放)；REST 请求头与 WS 初始帧强验 | 无（局域网明文 HTTP / WS） | **同一局域网监听者**、通信双方双端 | 缺少信封返回 401；身份不匹配/动作挪用/目标越权返回 403；无法识别载荷返回 400；**Fail-Closed，绝不回退至未鉴权通信** |
| **Relay 中继 (WSS)** | 移动端 ↔ 中继服务 ↔ PC 端 (`relay.bobbik.org`) | TLS 传输层握手；消息体内嵌 `RpcAuthEnvelope` 双向签名，中继节点根据信封路由 | HTTPS/WSS (客户端到中继服务 TLS 加密) | **Relay 中继服务器**、通信双方双端（**非 E2EE**） | 握手失败直接断连；信封校验失败抛弃代理帧并记录警告；**Fail-Closed** |
| **远程 RPC (Agent 委派)** | 移动端发起指令 → PC 端无头执行 | 移动端 Ed25519 私钥签名 `rpc_request`；PC 校验会话信任与执行设备权限 | 依赖 LAN 或 Relay 通道底座 | 依赖底座通道可见性（同上）；但**执行密钥永远不传输** | 签名失效或目标不匹配直接拒执行；执行工具受 `read_only` / `staged_mode` 强管控 |
| **Web Drop (文件投送)** | PC 端发起 → 接收方浏览器打开 | 密钥位于 URL `#hash` 锚点（根据 RFC 3986 锚点不传中继服务器） | Tier 2: WebRTC (DTLS-SRTP/SCTP)；Tier 3: AES-128-GCM 加密分块 WSS | **仅发送方与接收方**（中继服务器仅见密文块，**具备真正 E2EE**） | 5分钟超时自动销毁房间；解密失败直接报错并终止传输 |

---

## 3. 本轮复核整改落实清单 (Remediation Details)

### 3.1 LAN Push Outbox 耐久性保证与确认回执协议
- **实现文件**：`src-tauri/src/sync_engine.rs` (`execute_lan_outbox_push`)
- **协议逻辑**：
  1. 移动端/客户端向 `/v1/sync/push` 发出带签名的 outbox 批次。
  2. 强验网络状态与 HTTP 状态码：若网络断开、超时、返回 HTTP 400/401/500，**立即终止并完整保留本地 outbox 文件**。
  3. 强验返回内容结构：必须解析出 JSON 且满足 `status == "ok"` 或 `status == "committed"`（确认服务端已成功执行原子落盘与 Fencing 提交）。
  4. 差异化回执消费：若对端返回 `stage == "pending_apply"`，发送端判定对端尚未生效落地，必须保留本地 outbox 待后续周期重试，并向上返回 `PendingApply`。
  5. 只有在收到明确已应用的提交回执 (`stage == "applied"`) 后，才调用 `fs::remove_file(&outbox_path)` 清除本地暂存。
  6. 后续重试自动带上未删除的 outbox 文件继续重试投递。
  7. 同路径下 `push_db` 增加对响应状态码与 `PushCommitReceipt` 的显式校验与告警记录。

### 3.2 异常非对象配置阻断策略与合并安全
- **实现文件**：
  - `src-tauri/src/lib.rs` (`read_config_checked_at`)
  - `src-tauri/src/sync_engine.rs` (`merge_synced_config`, `import_sync_data_to_conn_atomic`)
- **防御逻辑**：
  1. `read_config_checked_at` 在 JSON 反序列化后强验 `!json.is_object()`。若不是对象（如包含字符串、数字、数组或布尔值），立即返回 `SEC-01/03 Fail-Closed: Config file must contain a JSON Object`。
  2. `merge_synced_config` 增加双端对象防御：若本地配置非对象，记录安全告警并返回本地配置副本，**绝不回退或返回对端配置 (`remote.clone()`)**，杜绝未清洗的对端敏感字段倒灌；若对端非对象，则保持本地原样。
  3. `import_sync_data_to_conn_atomic` 校验本地和远端配置，若任意一方不是对象，立即在事务前 Fail-Closed 终止。

### 3.3 严格正向白名单 (Constructive Allowlists) 与契约统一
- **真理源常量**：`src-tauri/src/device_trust.rs`
  - `ALLOWED_REMOTE_CONFIG_KEYS`: `["model", "clerkModel", "visionModel", "provider", "theme", "uiScale", "language", "accentColor", "weatherCity"]`
  - `ALLOWED_REMOTE_PUSH_OPS`: `["set_config"]`（**契约统一**：生产 Outbox 执行器仅支持 `set_config`，彻底废弃非生产/虚构操作 `create_item`、`add`、`delete`，实现入口准入与底层调谐器 100% 契约一致）。
- **数据导出端 (`export_sync_data_from_conn`)**：
  - 彻底抛弃从原配置 `remove(...)` 的黑名单模式，改为创建全新的空白 Map，仅从本地配置提取属于 `ALLOWED_REMOTE_CONFIG_KEYS` 的**标量值**。
  - `customModels` 若存在，仅提取 `["id", "name", "provider", "model", "contextWindow", "maxTokens"]` 等安全公开元数据，彻底剥离 `apiKey`、敏感提示词与任何未知字段。
  - `settings` 表查询改为正向白名单：`SELECT key, value FROM settings WHERE key IN ('last_sync_ts', 'last_routine_date', 'theme', 'language', 'weather_city')`。
- **数据接收端 (`atomic_commit_push_outbox`)**：
  - 强制操作必须为 JSON Object。
  - `set_api_key` 强制阻断拒绝（凭据驻留本地）。
  - 操作类型必须属于 `ALLOWED_REMOTE_PUSH_OPS`（仅允许 `set_config`）。
  - `set_config` 的 `key` 必须严格在 `ALLOWED_REMOTE_CONFIG_KEYS` 内，且 `value` 必须为标量（禁止对象或数组嵌套凭据）。
  - 禁止在 `set_config` 中夹带任何额外字段（仅允许 `op`, `action`, `key`, `value`, `event_id`, `request_id`）。
- **执行消费端 (`drain_staged_outbox` & `outbox::apply_operations`)**：
  - 调谐器执行时逐条校验操作，若暂存队列中的操作经校验全部被拒斥（`applied == 0`），将其视作投递失败并记录 `last_error`，绝不标记为 `delivered`。

### 3.4 推送失败阻断与部分同步失败显式报告
- **实现文件**：`src-tauri/src/sync_engine.rs` (`execute_lan_outbox_push`, `execute_lan_push_db`, `do_active_sync`)
- **防御机制**：
  1. **全链路失败拦截**：在 LAN 与 Relay 双通道中，Pull 成功后继续执行 Outbox 推送与 `push_db` 数据推送。若 Outbox 或 `push_db` 遭遇签名序列化失败、网络中断、HTTP 非 200 响应、或接收端缺少明确的 `status: "ok"` / `"committed"` / `commit_ack` 提交回执，所有失败信息被归集至 `push_errors`。
  2. **数据保留待重试**：本地 Outbox 文件严格保留在磁盘，绝不提前删除。
  3. **阻断成功推进**：一旦存在推送失败，绝不发出 `{"status": "done"}` 事件，向前端发出 `sync:progress` 事件携带 `{"status": "error", "detail": format!("部分同步失败: {}", ...)}`，底层函数向上返回 `Err(...)`。
  4. **前端状态对齐**：`pairing-flow.js` 与 `SettingsConnections.vue` 捕获该错误并将同步步骤标红 (`error`)，阻止将整体配对或同步标记为成功。

### 3.5 接收端载荷强校验与 Fail-Closed 格式拒斥
- **实现文件**：`src-tauri/src/http_api.rs` (`handle_sync_push`)
- **防御机制**：
  1. 对 `/v1/sync/push` 的载荷进行强类型结构匹配：允许顶层 JSON 数组（`[op1, op2, ...]`）或顶层 JSON 对象中包含 `ops` 或 `data` 数组。
  2. 任何非数组且无 `ops`/`data` 数组的载荷（如 `{"foo": "bar"}` 或畸形标量），绝不静默当做空操作，而是立即通过 `invalidate_execution_token_on_malformed_payload` 标记幂等失败并直接返回 HTTP 400 Bad Request (`Invalid push payload: expected array or object containing 'ops'/'data' array`)。
  3. 发送端捕获 HTTP 400 判定为投递失败，完好保留本地 outbox 绝不删除，彻底杜绝数据静默丢失。

### 3.6 投递状态契约化 (`applied` vs `pending_apply`) 与重试收敛
- **实现文件**：
  - `src-tauri/src/device_trust.rs` (`PushCommitReceipt`, `atomic_commit_push_outbox`)
  - `src-tauri/src/sync_engine.rs` (`execute_lan_outbox_push`, `execute_lan_push_db`, `do_active_sync`)
  - `src-tauri/src/http_api.rs` (`handle_sync_push_db`)
  - `src/views/settings/SettingsConnections.vue`
- **防御机制**：
  1. **回执状态契约**：定义包含 `status`, `type: commit_ack`, `stage: "applied" | "pending_apply"`, `applied_count`, `pending_count`, `delivery_error` 的强类型回执。
  2. **落盘感知**：`atomic_commit_push_outbox` 与 `import_sync_data_to_conn_atomic` 在事务完成后查询 `rpc_staged_outbox` 实际投递结果。若由于文件锁竞争或磁盘写入失败导致配置暂存处于 pending，回执明确返回 `stage: "pending_apply"` 与具体错误。
  3. **发送端差异化消费**：若回执为 `pending_apply`，发送端保留本地 outbox，阻止发出 `status: "done"`，发出 `sync:progress` 携带 `pending_apply`。
  4. **前端阻断完成**：前端识别 `pending_apply`，显式显示“待对端应用配置”，阻止提前打勾。
  5. **重试收敛**：下一次同步或后台调谐执行时，未应用的暂存操作被重新消费落盘，成功写入后状态收敛为 `applied` 并清除本地暂存。

### 3.7 回执消费路径与状态查询 Fail-Closed 加固 (Round 5 针对性整改)
- **实现文件**：
  - `src-tauri/src/device_trust.rs` (`atomic_commit_push_outbox`)
  - `src-tauri/src/sync_engine.rs` (`parse_push_commit_receipt`, `import_sync_data_to_conn_atomic`, `execute_lan_outbox_push`, `execute_lan_push_db`, `do_active_sync`)
- **防御机制**：
  1. **状态查询 Fail-Closed 降级**：
     - 在 `device_trust.rs` (line 1816) 与 `sync_engine.rs` (line 3776) 中，事务提交后执行投递状态查询。
     - 彻底废除 `check_res` 遇 `Err(_)` 时仍返回 `applied` 的误报漏洞。
     - 若状态查询失败（无法核实磁盘落盘状态），严格 Fail-Closed 降级返回 `("pending_apply".to_string(), 0, count, Some(format!("无法确认落盘状态 (Fail-Closed): {}", e)))`。
  2. **强制校验完整回执契约与废除默认值**：
     - 提炼核心回执解析函数 `parse_push_commit_receipt(&Value) -> Result<PushDeliveryOutcome, String>`。
     - 废除发送端所有 `unwrap_or("applied")` 宽容兜底，回执中缺失 `stage` 或非 `"applied"`/`"pending_apply"` 时一律 Fail-Closed 报错拒斥。
     - LAN 发送端 (`execute_lan_outbox_push`, `execute_lan_push_db`) 与 Relay 活跃同步发送端统一调用该函数强验对端回执。
  3. **幂等更新影响行数强验防静默吞错**：
     - 在 `device_trust.rs` (line 1848) 与 `sync_engine.rs` (line 3796) 中，向 `rpc_idempotency_cache` 更新最终 `response_json` 时，校验 `update_rows`。
     - 若 `update_rows == 0` 或 SQL 更新报错，立即返回 `Err(...)` 终止并告警，杜绝因并发冲突、缓存缺失或静默吞错导致客户端重放无 stage 的旧回执。
  4. **Relay 消费路径新回执语义对齐与闭环**：
     - 在 `sync_engine.rs` (`do_active_sync` Relay 路径) 中引入 `relay_pending_apply` 状态标记。
     - Outbox 推送与 `push_db` 均通过 `parse_push_commit_receipt` 消费对端回执。若对端返回 `PushDeliveryOutcome::PendingApply`，本地待同步文件**严格保留在磁盘绝不删除**，并置位 `relay_pending_apply = true`。
     - 若检测到 `relay_pending_apply`，向前端发射 `sync:progress` (`status: "pending_apply"`)，并且**坚决阻断发射 `status: "done"`**。
     - Relay 接收端生成回执处（lines 5635, 5657, 6323, 6333）补齐 `"stage": "applied"`。

### 3.8 Relay 生产接收端真实落盘驱动与回执透传 (Round 6 针对性整改)
- **实现文件**：
  - `src-tauri/src/sync_engine.rs` (`dispatch_inbound_relay_message_core`, `RelayDispatchContext`)
- **防御机制**：
  1. **Relay Push 接收端彻底废弃盲目 applied 回执**：
     - 彻底删除此前 Relay push 接收端只调用 `write_outbox(arr)` 并写死回传 `stage: "applied"` 的漏洞代码。
     - 改为直接进入 `crate::device_trust::atomic_commit_push_outbox` 执行原子化校验、暂存入队与实际投递。
     - 根据持久化投递落盘结果动态返回 `applied` 或 `pending_apply`，确保对端发送端绝不会因虚假的 `applied` 而误删未落盘的本地 outbox 文件。
     - 校验或入库失败时，调用 `fail_rpc_idempotency` 标记幂等缓存为 `failed`，发送签名 error 帧，并返回 `Err`（Fail-Closed，绝不发送 `commit_ack`）。
  2. **Relay Push_DB 接收端投递回执透传**：
     - 改造 `import_sync_data` 及内部 `import_sync_data_to_conn_atomic`，向上传递 `outcome.receipt`。
     - Relay 分发循环接收到 `outcome.receipt` 后，真实透传 `receipt.stage`、`applied_count`、`pending_count` 及 `delivery_error` 回传给对端。
     - 配置落盘受阻时如实报告 `pending_apply`，发送端保留本地数据；导入失败时 Fail-Closed 更新幂等为 `failed` 并发送签名 error 帧。
  3. **高保真生产分发测试穿透**：
     - 在 `RelayDispatchContext` 中引入 `test_signing_key` 与 `test_local_device_id`，打通在测试中对生产级入站函数 `dispatch_inbound_relay_message_core` 的直接调用与真实签名回传能力，彻底替换原先在内存中手工构造 Mock JSON 回执的低保真测试模式。

---

### 3.9 全链路投递阶段收口与虚报消除 (Round 7 针对性整改)
- **实现文件**：
  - `src-tauri/src/sync_engine.rs` (`ActiveSyncOutcome`, `trigger_mobile_sync`, `do_active_sync`, `import_sync_data`, `dispatch_inbound_relay_message_core`)
  - `src/sync/pairing-flow.js` (`executePairingWorkflow`)
  - `src/views/settings/SettingsConnections.vue`
  - `src/App.vue`
- **防御机制**：
  1. **强类型结果契约 (`ActiveSyncOutcome`)**：
     - 定义 `ActiveSyncOutcome::Applied { transport: TransportKind }` 与 `ActiveSyncOutcome::PendingApply { transport: TransportKind, reasons: Vec<String> }`。
     - `do_active_sync` 在 LAN 与 Relay 分支中，捕获 `import_sync_data` 回执中的 `receipt.stage` 与 outbox 投递结果。一旦存在 `pending_apply`，坚决压制 `config:reconciled`（避免发射 `applied: 1`），并向上返回 `ActiveSyncOutcome::PendingApply`。
  2. **诊断事件与历史记录防虚报对齐**：
     - `trigger_mobile_sync` 消费 `ActiveSyncOutcome`。遇 `PendingApply` 时，在诊断事件与 `sync_history::SyncRun` 中严格写入 `DiagnosticStatus::Pending` 与详细待应用原因，**绝不记录 `Success` 或“同步完成”**。
  3. **接收端事件精准化 (`import_sync_data` & Relay)**：
     - `import_sync_data` 检查 `outcome.receipt.stage`，仅在 `applied` 时发射 `sync:completed`，在 `pending_apply` 时发射 `sync:progress` (`status: "pending_apply"`)。
     - Relay 生产接收端在 `receipt.stage == "pending_apply"` 时，压制 `sync:completed`，记录 `DiagnosticStatus::Pending`，并带有“配置暂存待应用”前缀。
  4. **前端配对与指示器防护**：
     - `pairing-flow.js` 检测 `syncOutcome.status === 'pending_apply'`，调用 `onStepUpdate(..., 'pending_apply', ...)` 并返回 `{ success: true, pending_apply: true, stage: 'pending_apply' }`。
     - `SettingsConnections.vue` 在配对中如遇到 `pending_apply`，弹窗展示 `$t('settings.pairing_pending_apply')` 并高亮提示待应用，`updateStep` 设置防篡改哨兵防止被晚到的 `done` 信号覆盖。设备卡片指示灯针对 `pending` 显示黄色警示与“待应用”。
     - `App.vue` 启动同步遇 `pending_apply` 时标为 `pending` 状态且不记录 `lastSyncTime`。

---

## 4. 可复现自动化测试集 (SEC-02 & SEC-03)

在 `src-tauri/src/http_api.rs` 内置针对安全边界、状态机耐久性与契约一致性的自动化测试。
> **测试环境与边界校准声明**：对于 LAN 发送端耐久性测试 (`test_sec02_lan_outbox_push_failure_retention_and_retry` 及 `test_sec02_lan_push_db_failure_and_receipt_verification`)，测试通过本地 Axum 服务与模拟接收处理器验证发送端在 HTTP 500、401、缺少回执时的文件保留与失败阻断行为，这证明了发送函数的状态机与持久化逻辑，但不等同于无需对端参与的全链路穿透验收（全链路真机验收由 Gate A4 负责）。

1. **`test_sec03_export_sync_data_strips_credentials` (PASS)**
   - 构造含 `apiKeys`、`customModels[].apiKey`、`mcpServers`、未知攻击字段 `attacker_injected_unknown_field`、嵌套凭据 `nested_credentials` 以及数据库中各第三方 Bot Token、未知插件字段。
   - 验证导出结果经正向白名单过滤后，未知字段与敏感凭据 100% 被丢弃，自定义模型仅保留脱敏元数据，settings 仅保留白名单 key。
2. **`test_sec03_merge_synced_config_refuses_remote_api_keys` (PASS)**
   - 本地已有 key，对端传入替换 key 与新增 key。
   - 验证合并后本地 key 保持不变，对端 key 被坚决丢弃，仅漫游安全模型与主题偏好。
3. **`test_sec03_push_outbox_rejects_credential_modification_fail_closed` (PASS)**
   - 模拟推送包含 `set_api_key` 及恶意 `set_config` 操作。
   - 验证 `atomic_commit_push_outbox` 触发 `SEC-03 Fail-Closed` 拒绝写入。
4. **`test_sec03_atomic_commit_push_outbox_positive_allowlist_negative_tests` (PASS)**
   - 负向测试非对象操作载荷 (`"string"`) -> Fail-Closed 拒斥。
   - 负向测试未知操作类型 (`"malicious_eval"`) -> Fail-Closed 拒斥。
   - 负向测试 `set_config` 未知键 (`"arbitrary_custom_url"`) -> Fail-Closed 拒斥。
   - 负向测试 `set_config` 传递嵌套字典 (`"value": {"nested_token": "secret"}`) -> Fail-Closed 拒斥。
   - 负向测试 `set_config` 传递嵌套数组 (`"value": ["token"]`) -> Fail-Closed 拒斥。
   - 负向测试非白名单操作 (`create_item`, `add`, `delete`) -> Fail-Closed 拒斥。
   - 负向测试 `set_config` 夹带未知敏感字段 (`telegram_token`) -> Fail-Closed 拒斥。
5. **`test_sec03_malformed_config_fail_closed_guarantees` (PASS)**
   - 负向测试 `read_config_checked_at` 遇字符串、数组、数字、布尔非对象配置 -> Fail-Closed 拒斥。
   - 负向测试 `merge_synced_config` 遇本地非对象配置 -> 严格返回本地副本，绝不返回远端配置。
   - 负向测试 `merge_synced_config` 遇远端非对象配置 -> 严格返回本地配置。
   - 验证配置合并仅漫游白名单标量键，完全忽略未知键和嵌套字典。
6. **`test_sec02_lan_outbox_push_failure_retention_and_retry` (PASS)**
   - 搭建真实 Axum 临时 HTTP 服务模拟局域网接收端。
   - 场景 A (HTTP 500) -> 请求报错，本地 outbox 文件**严格保留**在磁盘。
   - 场景 B (HTTP 401) -> 请求报错，本地 outbox 文件**严格保留**在磁盘。
   - 场景 C (HTTP 200 但缺回执) -> 请求报错，本地 outbox 文件**严格保留**在磁盘。
   - 场景 D (重试成功 HTTP 200 带 `status: ok` 回执) -> 请求成功，本地 outbox 文件**被安全删除**。
7. **`test_sec02_lan_push_db_failure_and_receipt_verification` (PASS)**
   - 验证 `execute_lan_push_db` 在 HTTP 500、401、HTTP 200 缺回执时准确返回 Err，仅在明确回执时返回 Ok。
8. **`test_sec03_push_outbox_delivery_e2e_reconciliation` (PASS)**
   - 契约统一与端到端真实投递：白名单 `set_config` 经 `atomic_commit_push_outbox` 校验入队，通过 `drain_staged_outbox` 真正调谐写入 `config.json` 磁盘文件，并将状态由 pending 转为 delivered。
   - 入口阻断验证：非白名单操作 `create_item` 在入口被直接阻断拒绝。
   - 纵深防御验证：若暂存队列存在异常操作，`drain_staged_outbox` 拒绝标记为 delivered。
9. **`test_sec02_lan_rest_sync_endpoints_security_boundary` (PASS)**
   - 测试真实 Axum Handler：合法签名 200 OK、身份伪造 403、目标越权 403、动作挪用 403、缺信封 401、载荷篡改 401、注入攻击 Fail-Closed。
10. **`test_sec02_ws_sync_request_connection_subject_pinning` (PASS)**
    - 验证已认证 WebSocket 连接强验后续信封主体，拒绝冒名同步帧。
11. **`test_sec03_push_unrecognized_payload_rejection_and_outbox_retention` (PASS)**
    - 模拟接收端收到无法解析为数组且不含 `ops`/`data` 的合法 JSON（如 `{"foo": "bar"}`）。
    - 验证接收端严格 Fail-Closed 拒斥并返回 HTTP 400 Bad Request。
    - 验证发送端在收到 HTTP 400 后准确识别投递失败，本地 outbox 文件完整保留在磁盘等待修复与重试。
12. **`test_sec03_push_commit_receipt_delivery_status_contract` (PASS)**
    - 验证正常投递落盘后，`atomic_commit_push_outbox` 返回 `PushCommitReceipt` 满足 `stage: "applied"` 且 `applied_count == 1`, `pending_count == 0`。
    - 验证发送端在收到 `applied` 状态后安全删除本地 outbox 文件并向界面报告就绪。
13. **`test_sec03_disk_failure_pending_status_and_retry_convergence` (PASS)**
    - 模拟目标文件被写保护/目录不可写导致 `drain_staged_outbox` 写入磁盘失败。
    - 验证 `atomic_commit_push_outbox` 事务提交成功但检测到投递失败，回执返回 `stage: "pending_apply"`、`pending_count == 1` 及具体磁盘错误。
    - 验证发送端消费回执时拒绝清除本地 outbox，阻止发出完成状态。
    - 模拟后续写保护解除并重新调用 `drain_staged_outbox`，验证操作成功收敛落地为 `delivered`。
14. **`test_sec03_push_db_config_delivery_status_and_idempotency_receipt` (PASS)**
    - 验证 `handle_sync_push_db` 对包含 remote config 的同步载荷进行原子导入并投递配置。
    - 验证正常写入时返回 `stage: "applied"`，幂等重放时完整保留回执中的投递状态。
15. **`test_sec03_delivery_status_query_failure_returns_pending_apply` (PASS - Round 5 新增)**
    - 使用临时触发器模拟事务提交成功但后续查询 `staged_outbox` / `config` 抛出 `QueryReturnedNoRows` 异常。
    - 验证 `atomic_commit_push_outbox` 与 `handle_sync_push_db` 严格 Fail-Closed 降级为 `stage: "pending_apply"`，绝不在未核实磁盘状态时声明 `applied`。
16. **`test_sec03_idempotency_cache_update_failure_and_replay_stage` (PASS - Round 5 新增)**
    - 验证当幂等缓存更新受影响行数为 0 时，`atomic_commit_push_outbox` 立即报错阻止虚报成功（杜绝静默吞错）。
    - 验证正常更新缓存后，后续重放请求返回的缓存 JSON 包含完整且正确的 `stage: "applied"` 契约字段。
17. **`test_sec03_relay_production_push_receiver_failure_pending_and_recovery` (PASS - Round 6 新增)**
    - 穿透调用真实生产中继分发函数 `dispatch_inbound_relay_message_core` 处理 Relay push 请求。
    - 验证恶意/越权操作（如 `set_api_key`）触发 Fail-Closed，幂等状态标记为 `failed`，通过通道返回签名 error 响应帧，绝不发送 `commit_ack`。
    - 验证在注入磁盘写入故障时，接收端回传包含 `stage: "pending_apply"` 的 `commit_ack`，发送端消费后保留本地 outbox 绝不提前删除。
    - 验证故障恢复后，暂存任务经后台收敛至 `delivered`，后续 push 请求成功落地并返回 `stage: "applied"`，发送端唯有此时才安全删除本地 outbox。
18. **`test_sec03_relay_production_push_db_receiver_failure_pending_and_recovery` (PASS - Round 6 新增)**
    - 穿透调用真实生产中继分发函数 `dispatch_inbound_relay_message_core` 处理 Relay push_db 请求。
    - 验证畸形配置载荷（非 JSON Object）被严格 Fail-Closed 拒斥，幂等状态标记为 `failed`，回传签名 error 帧，绝不发送 `commit_ack`。
    - 验证在注入磁盘故障时，配置差异虽然入队但无法写入磁盘，接收端真实透传 `stage: "pending_apply"` 与 `delivery_error`，发送端解析验证契约。
    - 验证故障恢复后后台收敛至 `delivered`，后续 push_db 请求成功写入并返回 `stage: "applied"`。
19. **`test_sec03_active_sync_outcome_and_caller_diagnostics_closure` (PASS - Round 7 & 8 深度加固，切入生产函数)**
    - 验证 `ActiveSyncOutcome` 强类型枚举在 `Applied` 与 `PendingApply` 下的契约序列化。
    - **真实生产函数调用**：直接调用 `record_trigger_sync_outcome` 处理 `PendingApply`，验证诊断事件与活动运行的 `status` 严格为 `DiagnosticStatus::Pending`，`summary` 明确包含待应用原因，断言 `!= Success` 且 `!= "同步完成"`，同时直接验证内部 `sync_history::record_run` 落盘数据。
    - **生产全场景覆盖**：调用 `record_trigger_sync_outcome` 分别处理 `Applied`（返回 `Success` 与“同步完成”）和 `Err(...)`（返回 `Failed`、`None` 传输方式与“同步未完成”）。
    - **本地导入历史生成函数验证**：直接调用生产函数 `build_sync_import_history_entry`，验证在 `stage: "pending_apply"` 时历史日志生成 `status: "pending_apply"`，明细包含暂存待应用原因，杜绝提前宣称“同步更新已应用”或“成功合并云端数据”；在 `stage: "applied"` 时正常生成 `status: "applied"` 与应用说明。
20. **`pairing-flow.test.js` (PASS - Round 9 前端配对与启动同步全套解耦测试)**
    - 18 项测试全部通过，覆盖：
      - 正向建信与完整邀请串透传；
      - 首次同步发生部分同步失败时，步骤更新为 `error`，工作流返回 `{ success: false, paired: true, applied: false, stage: 'failed' }`，严禁误报 `done`；
      - 首次同步返回 `pending_apply` 时，步骤更新为 `pending_apply`，返回 `{ success: true, paired: true, applied: false, pending_apply: true, stage: 'pending_apply' }`；
      - 缺失 `triggerMobileSync` 接口时标记 `stage: 'skipped'`，返回 `success: false`，严禁误报 `applied`；
      - 首次同步返回非预期响应时，步骤置为 `error`，返回 `stage: 'unexpected'`，严禁走 `done`；
      - 迟到事件防护：步骤处于 `error` 或 `pending_apply` 时，迟到的 `done`、`running`、`pending` 无法覆盖现有状态（直接执行生产 `applyStepTransition` 状态机）；
      - 启动自动同步回执契约 (`handleStartupSyncOutcome`)：
        - 仅明确 `status === 'applied'` 视为成功、标记 `success` 并更新 `bob-last-sync-time` 时间戳；
        - `status === 'pending_apply'` 标记为 `pending`，阻断时间戳更新；
        - 空值、非对象、未知状态或非 applied 均判定为 `error`，阻断时间戳更新并触发静默监听 fallback。

---

## 5. 全面回归验证记录

- `cargo check --all-targets`: **EXIT 0** (0 编译错误)
- `cargo test --lib http_api -- --test-threads=1`: **36 passed, 0 failed**
- `cargo test --lib device_trust -- --test-threads=1`: **48 passed, 0 failed**
- `cargo test --lib sync_engine -- --test-threads=1`: **61 passed, 0 failed**
- `pnpm test`: **10 passed (10 files), 62 passed (62 tests), 0 failed, EXIT 0**

---


## 6. 下一步指引与验收状态

- **A3 本地复核状态**：**通过 (PASSED)**。A3 的全部边界审计与复核指出的生产缺口（包括启动自动同步回执判定契约、迟到事件防御直连生产函数、沙箱环境测试退出码修复）已全部经本地代码审查与全套测试验证通过。
- **A4 真机验收状态**：**尚未执行 (UNEXECUTED, 保持 LOCKED)**。
  - 明确界定：自动化测试与本地单元/集成验证**绝对不代替物理手机—PC 真实硬件表现**；
  - 绝不宣称公网 Relay 或物理手机已通过验证；
  - 未经用户明确授权，绝不执行 release/安装包构建，绝不安装到设备，绝不连接或改动线上服务，绝不执行 Git push，绝不擅自推送云端看板；
  - 待用户固定候选构建版本并协同操作物理设备后，方可启动 A4 真机验收流程。

