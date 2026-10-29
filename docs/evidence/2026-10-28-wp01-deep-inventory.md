# WP-01 — 前身项目深度只读盘点

日期：2026-10-28。统一项目任务入口：[CURRENT_STAGE](../CURRENT_STAGE.md)。本文是一次性静态审计证据；长期设计归入 [kernel-schema](../wiki/kernel-schema.md)，复用位置归入 [COMPONENT_MAP](../COMPONENT_MAP.md)。

## 1 基线与范围

| 对象 | 基线与现场 |
|---|---|
| 统一项目 | `1.0.1` / `main` 同指 `d842924`，接手时工作区干净；22个tracked文件，只有文档/配置，没有Rust或Vue业务代码，也没有CI工作流 |
| 旧 App | `<workspace_root>/bob-agent`，`a4ff5fd1cb57549b10b2be2223833da982b18e78`，main，工作区干净 |
| 旧 Online | `<workspace_root>/cloud-agent`，`d14a52786715d898dbcc9c255a30c721794fc7e4`，main；13个tracked修改、111个untracked项。不是旧盘点中的97e3480；本次核心审计源码未有tracked修改 |

只读源码、Git元数据与CI定义；未打开生产SQLite/.env/配置密钥，未连接节点、未调用模型、未运行旧测试/构建、未修改旧仓库或worktree。旧仓库状态属于采样，不宣称没有其他Agent活动。完整Git历史secret扫描与远端CI运行状态未检查；凭据结论只指所查源码的存储路径。

### 可重复统计口径

- 旧App：`git ls-files`；后缀rs/js/mjs/ts/py/html，basename命中test_*、test-*、test(s).rs、*_test.rs、*_tests.rs、.test.或.spec.：27个命名测试文件。tracked `src-tauri/src/**/*.rs` 含 `#[test]` 或 `#[tokio::test`：44个承载文件；并集68个文件。
- 旧Online：tracked源码在test/tests/__tests__目录，或JS/TS文件名含.test/.spec，后缀ts/tsx/js/jsx/mjs/cjs/py/rs；并集118个（server77、ui33、deploy/vps3 6、scripts/lib2），其中.test/.spec命名83个。
- 两个项目口径不同，分别披露，不能把文件数当测试用例数/通过数。没有执行这些测试。
- App两份workflow（android.yml/windows.yml）在main/mobile-pc-rebuild push或手工触发，未设paths过滤，包含npm安装及Tauri打包，无显式单测步骤。Online只有wp1-06-fixture.yml，在chore/wp1-06-fixture分支跑集成fixture。统一项目当前未带入这些workflow；不能推断新主线已有CI或CI全绿。

## 2 旧 App：同步、表与凭据

以下路径相对旧App仓库；行号对应上方commit。

### 2.1 静态表清单

tracked Rust源码生产前缀（首个`#[cfg(test)]`前）匹配CREATE [VIRTUAL] TABLE，共48个唯一名称，含3个显式FTS虚表及独立vault，不含SQLite自动shadow表；不是运行数据库现场表数。

| 创建位置 | 表名 |
|---|---|
| src-tauri/src/db.rs:147–350 | conversations、messages、wiki_fts、notes_fts、messages_fts、session_observations、evolution_log、kg_nodes、kg_edges、kg_source_batches、execution_errors |
| calendar.rs:14/32 | events、sync_tombstones |
| capture.rs:102/128；capture_router.rs:62 | capture_journal、capture_events、capture_enrichment |
| daily_brief/service.rs:13/22 | daily_brief_cache、daily_brief_seen |
| device_trust.rs:452–545 | trusted_devices、pairing_invitations、authenticated_sessions、rpc_anti_replay、rpc_idempotency_cache、rpc_staged_outbox、rpc_processed_events、peer_revocation_outbox、identity_reset_journal |
| discord.rs:18 / telegram.rs:17 / scheduler.rs:256 | settings（同一表名） |
| scheduler.rs:26 | cron_jobs |
| goal_runtime/repository.rs:31–150 | goal_runs、goal_attempts、goal_evidence、goal_checkpoints、goal_approvals、goal_events、goal_runtime_receipts |
| remote_agent/store.rs:46 | vault（独立数据库） |
| result_receipt.rs:152 | action_receipts |
| sync_engine.rs:286/467 | staged_changes、staged_write_recovery |
| work_core/repository.rs:20–95 | work_projects、work_objects、work_relations、work_events、work_idempotency |
| work_core/decision_change.rs:84；project_links.rs:193/216 | work_change_reviews、project_link_candidates、work_external_links |

`work_objects` 以kind/data_json/parent_id/revision承载Responsibility、Goal、Milestone、Task、Decision、Artifact、Evidence、Risk、Change、Commitment（models.rs:93）。不能误报每种对象都有独立SQL表；Memory也未发现独立表。统一Schema仍须保留来源与语义映射，不能自动把goal_attempts当Execution、work_objects中artifact当已校验快照。

### 2.2 同步实际模型

`sync_engine.rs:3338` SyncData，`:3535–3613`导出：settings白名单、conversations、messages、events、capture_journal（排除local_only）、cron_jobs、kg_nodes/edges、sync_tombstones、笔记/资料文件及脱敏配置。settings仅last_sync_ts/last_routine_date/theme/language/weather_city；导入保护本地last_sync_ts。Relay不传消息图片及kg_nodes/edges；wiki_fts导出为空。

**work_*、goal_*、设备信任、认证会话、幂等表、vault均不在SyncData。** 当前主要是双向合并，不是Home单权威：

- conversations：按更新时间LWW，无冲突副本。
- messages：按sync_id追加去重，缺sync_id不导入（4046–4075）。
- events：双方在last_sync_ts后修改则生成冲突副本，写sync_conflicts（3929–4025）。
- capture：内容派生幂等键并合并新处理状态（4098）。
- cron/kg/settings：INSERT OR REPLACE（3887、4105–4144）。
- 笔记：按文件mtime覆盖（4146–4192）；tombstone仅支持若干实体并比较时间（3848）。

静态缺口：源码引用sync_conflicts与messages.sync_id，但本轮在初始化/ALTER未找到相应建表/加列定义；没读真实库，不能据此断言现场库必然损坏。迁移前需用获准的脱敏样本核对。

### 2.3 不是同一种 Outbox

| 名称 | 实际职责、存储与幂等 |
|---|---|
| rpc_staged_outbox | device_trust.rs:510，接收端配置操作暂存；UNIQUE(session_id,request_id)，与RPC回执同事务登记，随后调谐文件；不是发送端用户消息队列 |
| rpc_idempotency_cache | :493；session_id、request_id、subject_device_id、target_device_id、action、payload_hash、status、lease_generation、execution_token、response_json、error_message、created_at、updated_at；PK(session_id,request_id)，会话轮换改变幂等命名空间 |
| rpc_processed_events | 记录消费event_id，但文件副作用与此记录并非一个事务 |
| peer_revocation_outbox | :531；签名设备吊销通知，pending→sent→delivered/failed；条件认领、ACK、有界重试可参考，event_id索引非UNIQUE；不能当用户会话Outbox |
| write_mobile_outbox | **不是SQL表**；sync_engine.rs:2930命令写mobile_outbox.json（2576路径）；读数组追加、临时rename；没有逐条业务ID/租约/回执，坏JSON读取会变空数组，存在并发读改写风险 |
| outbox.rs::write_outbox | :82写bob_outbox.json，:196调谐config.json；支持配置/Key/目录操作，是单文件配置批次，不是Durable Intent Outbox |

上述两张实际队列表字段也须保留：

- rpc_staged_outbox：id INTEGER PRIMARY KEY AUTOINCREMENT、event_id、session_id、request_id、operations_json、status(default pending)、attempts(default 0)、last_error、created_at、delivered_at；UNIQUE(session_id,request_id)。
- peer_revocation_outbox：id INTEGER PRIMARY KEY AUTOINCREMENT、event_id、target_peer_id、revoked_device_id、revocation_payload、status(default pending)、attempts(default 0)、last_error、created_at、sent_at、delivered_at。
- rpc_processed_events：event_id TEXT PRIMARY KEY、session_id、request_id、applied_at。以上字符串字段为TEXT、时间/次数为INTEGER；具体NULL/default定义以device_trust.rs:510–542为准。

`device_trust.rs:1612`先写config文件，`:1624`才写processed_event，崩溃可能重复应用。会话级幂等、文件rename、注释“绝不重复”均不足以证明任意工具exactly-once。

`db.rs:134–142`磁盘打开失败回退内存库。新设计禁止把这种路径当持久化成功；磁盘接纳失败应保留输入草稿，不清空输入框。

### 2.4 凭据分三套，不笼统称安全

1. `llm.rs:911–964/1013`与`lib.rs:577–610`：API Key/自定义模型Key进入明文config.json。pending/backup/fsync是写入可靠性，不是加密。
2. `crypto.rs:22–35/47/63/113`：设备身份文件用Argon2+AES-GCM，但源码有默认PIN/常见PIN自动解锁逻辑，不能等同OS安全存储。没有读取或输出实际私钥/PIN。
3. `remote_agent/store.rs:1–115`、`system_protection.rs:5–24`：随机AES key由Windows DPAPI / Android DeviceVault包裹；其他平台fail-closed unsupported。可以提取抽象，不能声称Linux bob-node安全存储已实现。

## 3 旧 Online：实体、迁移、调用边界

路径相对旧Online。完整业务DDL入口`packages/server/src/db/migrations.ts`，不是旧`schema.ts`子集。v1–17共20表；迁移按版本事务执行并更新user_version（571–604）。以下字段为静态重建，JSON也以TEXT存储；主键通常TEXT id，时间/版本为INTEGER，细节以迁移源码为准。

| 表 | 最终列（含后来ALTER） |
|---|---|
| runtime_hosts | id、name、kind、status |
| projects | id、name、description、status、created_at、updated_at |
| workspaces | id、project_id、runtime_host_id、path、repo_url、branch、is_primary、created_at、canonical_path、updated_at；UNIQUE(runtime_host_id,canonical_path) |
| tasks | id、project_id、title、description、status、created_at、updated_at、instruction、max_iterations、runtime_handoff_in_progress |
| executions | id、task_id、workspace_id、agent_type、status、started_at、ended_at、runtime_session_ref、updated_at、parent_execution_id、triggered_by_review_id、iteration、baseline_snapshot、kind、model_id、handoff_reason、handoff_package、runtime_id、ended_reason、checkpoint_id、runtime_process_pid、runtime_process_token、runtime_process_start_identity、runtime_process_group_id、provider_permission_mode、provider_timeout_ms、execution_profile_id、execution_profile_version、requested_config_snapshot、resolved_config_snapshot、observed_config_snapshot |
| sparks | id、project_id、raw_text、source、status、created_at、metadata、updated_at、promoted_to_task_id |
| verification_runs | id、execution_id、task_id、status、summary、created_at、completed_at |
| verification_checks | id、run_id、name、command、status、exit_code、output、duration_ms、created_at |
| reviews | id、task_id、execution_id、status、note、created_at、resolved_at |
| artifacts | id、execution_id、type、name、uri_or_content、metadata、created_at |
| steering_messages | id、task_id、execution_id、sequence、content、status、created_at、delivered_at、acknowledged_at；sequence无UNIQUE |
| checkpoints | id、task_id、execution_id、source_runtime_id、source_workspace_id、repo_url、git_sha、git_ref、base_sha、workspace_tree_hash、reason、created_at |
| runtime_handoffs | id、request_id(UNIQUE)、task_id、source_execution_id、source_runtime_id、target_runtime_id、checkpoint_id、target_execution_id、status、reason、error_code、created_at、updated_at、completed_at |
| task_creation_requests | request_id(PK)、payload_hash、task_id、created_at |
| acceptance_contracts | id、task_id、version、target_iteration、status、source_requirement、execution_id、requirements_json、technical_checks_json、deferred_items_json、allowed_effects_json、escalation_rules_json、final_decision_mode、created_at、frozen_at；UNIQUE(task_id,version) |
| fact_integrity_runs | id、task_id、execution_id、contract_id、contract_version、passed、checks_json、created_at |
| acceptance_reviews | id、contract_id、contract_version、task_id、execution_id、reviewer_execution_id、reviewer_identity_json、requirement_checks_json、blocking_findings_json、advisory_findings_json、recommendation、created_at |
| project_messages | sequence(INTEGER PK AUTOINCREMENT)、id(UNIQUE)、project_id、task_id、request_id、role(user/assistant)、content、references_json、created_at；UNIQUE(project_id,request_id) |
| discussion_budget | id(PK且=1)、committed_micros、reserved_micros |
| discussion_model_calls | id、project_id、user_message_id、status(reserved/completed/uncertain)、reserved_micros、charged_micros、input_tokens、output_tokens、created_at、completed_at |

迁移目录：v1基线；v2规范工作区；v3instruction/Spark晋升；v4检查审阅；v5父执行迭代；v6steering；v7手工patch；v8model/handoff；v9runtime/checkpoint；v10handoff；v11进程归属；v12provider策略；v13配置事实；v14请求幂等；v15验收合同；v16对话；v17讨论预算。

另有独立认证库（`client-auth.ts:34`，`client-runtime.ts:48`）：client_auth_meta(key,value)、client_pairings(id,user_code_hash,client_instance_id,client_name,challenge,expires_at,failures,consumed_at,principal_id,grants_json)、client_credentials(credential_id,token_hash,principal_id,client_instance_id,client_name,grants_json,expires_at,revoked_at,pairing_id)。user_code_hash/token_hash/pairing_id有唯一约束。当前ClientGrant只有work.read（client-auth.ts:7），不能由配对成功推断允许执行。

### 3.1 状态与治理

- Project：类型是string，常见active/archived/test_archived；缺库层枚举。Task：inbox/ready/running/verifying/review_required/changes_requested/completed/blocked/cancelled（models.ts:51）；旧DDL default=open、createTask默认ready，迁移须兼容历史值。
- Execution：pending/running/paused/completed/failed/cancelled/lost/ended。VerificationRun：pending/running/passed/failed；Check：passed/failed/skipped/timed_out；Review：pending/approved/changes_requested/rejected（models.ts:63–163）。
- `convergeExecutionAndTaskTransaction`（1061–1180）保护已完成Task及新执行；completed→review_required、failed→changes_requested、cancelled→cancelled、lost→blocked。执行完成不是用户接受。
- approve路由（api.ts:1420–1508）核对归属、非空验证检查、冻结合同、事实完整性、验收审阅，再事务更新Review与Task。通用updateTask/updateReview仍可改状态，新内核应关闭绕过领域命令的写入口。
- requested/resolved/observed（shared/types.ts:468–509）分层；dispatch_observed不等于provider_reported。执行配置有保护，但observed更新仍可能覆盖；新内核需追加观测来源。
- Artifact没有单独版本SQL列：版本依Execution、快照/hash在metadata。不能原样复制成“新内核已有不可变版本表”。

### 3.2 幂等、恢复与受控文件

- `models.ts:448–478` Task创建是request_id+规范化payload hash事务幂等；Handoff和项目消息也有局部幂等。不存在完整通用发送Outbox/接收Inbox。
- `adapters/agent-adapter.ts:9`事件仅type/payload/timestamp，`events.ts:19`进程内EventEmitter；不是持久全局event_id/sequence。runtime输出有stdout/stderr cursor；tool-records.ts仅对Pi/Codex做有限投影（最多30条、2MB输入）。
- `db/migration.ts:74–191`启动扫描running/paused，核验exit/cancel账本及PID/token/startticks；不覆盖所有pending/终态后验证。未知归属不冒充恢复成功。
- `routes/node-work.ts:5–62` 原生DSH连接/回执是内存Map，重启会丢；prompt异常统一unknown。可复用协议知识与UI，不可当Durable Outbox。
- `verification.ts:1093–1224`受控快照、`execution-file-inventory.ts:20–77`大小/hash/符号链接检查；读失败不回退liveworkspace。文件快照与SQL跨介质，需专门设计收尾一致性。
- `db/index.ts:123–169`部分非事务SQL错误吞为0changes/null。新本地接纳不得沿用这种错误处理。

### 3.3 Harness 接口与不对称能力

| 接口/路径 | 当前事实与复用限制 |
|---|---|
| AgentAdapter | start/sendMessage(delivered或queued)/cancel/state/events/dispose；governed CLI并非全都实现同一个接口 |
| RuntimeAdapter | shared/types.ts:376–385；inspect/prepareWorkspace/startProcess/inspect/read/cancel/checkpoint；process handle带runtime/execution/PID/PGID/token/startticks/spec hash |
| AGY | agy-agent-adapter.ts:154–215，一次性print+text；当前governed路径不支持conversation/continue；不是stream-json常驻会话 |
| Codex | codex-agent-adapter.ts:73继承共用runner；exec --json；工具投影有支持；当前仍有冻结model/version校验；不得推断任意新版本兼容 |
| DSH CLI | dsh-agent-adapter.ts:306–350；headless、临时patch/HOME、本机凭据引用；只LocalRuntime路径；环境变量清理不等于系统隔离 |
| DSH Web | dsh-web-session.ts:84，以原sessionId+requestId续聊，与CLI Execution路径不同 |
| Pi | pi-agent-adapter.ts / agy-agent-adapter.ts:156–166，mode json/provider/model/print；工具记录投影有支持 |
| Generic API | pty-manager.ts:99–105 raw_llm→raw-llm-cli.js，旧PTY路径，不具备与四条governed路径同等级的验证/恢复证据 |

实际四条governed入口是AGY/Pi/DSH/Codex（api.ts:1019/1041/1066/1091），不能把Generic API与它们混为同一能力。旧PTY按projectPath复用且切会话停旧session，safe_steering/approval_handshake为false，不应复制为共享目录跨工具接续的默认实现。同步冲突文件如api-Bobbik-X1C.ts、en-Bobbik-X1C.ts不是正式入口。

## 4 复用裁决建议（不是代码迁入授权）

- 保留不变量与场景：签名信封、payload绑定、fencing、回执去重、版本CAS、吊销ACK、受控快照、Task/Execution/验证/人审分离。
- 改造：OS vault与Connection接口、Runtime/Harness IPC、UI展示与有来源的事件投影；不要整文件搬入巨型sync_engine/device_trust。
- 重写：Home权威事件复制、统一业务request_id、磁盘Outbox+持久Inbox、跨重启unknown对账、事务化领域命令。
- 延后/未验证：运行库缺列核验、实际CLI对账能力、Linux密钥保护、全部旧功能迁移、许可证与Git历史secret扫描、设备/网络/真机验证。

成本判断：不是一次改表即可完成。首先打通本地接纳→Home事务回执→单一runner→版本验证，再逐域搬迁；不以文档完成宣称BP2或1.0.1可用。

## 5 WP-02 设计核查与结果

本轮设计并入稳定页kernel-schema：31张核心草案表、两段持久投递、单飞预热、Home/Replica权威、原生会话绑定、工作区租约、不可变父版、检查/人工审阅分离。

执行的只是 Python标准库SQLite内存DDL/关系约束检查：25项通过、0失败。覆盖建表、外键、跨项目/版本拒绝、冻结配置/快照不可改、未知执行占位、Outbox ACK元数据、事务回滚与重复键约束。可复现脚本与结果：[DDL检查脚本](wp02-schema-check.py)、[检查回执](2026-10-28-wp02-schema-check.json)。这不是Rust状态机实现测试，不证明真实磁盘持久、网络重连、CLI exactly-once或真机行为。BP2的O01–O15仍全部待实施验证。

下一步仍为原WP-03连接与威胁模型，BOB-D-006/007保留proposed；Step4/5未开始，未安装依赖、未构建、未发布。

## 6 源码校验摘要

下表由本次读取文件计算SHA-256；旧仓库只读，原文件未复制进统一项目。

| 来源 | 文件 | SHA-256 |
|---|---|---|
| legacy-app | `src-tauri/src/db.rs` | `764629e5a79060e6e30425200bf9eb01bfba702d9f4b929b7c756d5e802eddb6` |
| legacy-app | `src-tauri/src/sync_engine.rs` | `3f6fe2113c880a0e0b04fac8e266a8249815b442e4ce33ba7e82232ff80905be` |
| legacy-app | `src-tauri/src/device_trust.rs` | `290861d7b663f0b6f6c88db439d14c46c848a2788c8044c90cf122401cf67d88` |
| legacy-app | `src-tauri/src/outbox.rs` | `062ee81d863056768454977c3ced1a9eb89fe219e358a9e7812c0f139d244499` |
| legacy-app | `src-tauri/src/llm.rs` | `1d3f1e38152493cb7a434531dc0ec3a8c93201ee24f3ed011ff3ae631d525dc9` |
| legacy-app | `src-tauri/src/crypto.rs` | `06ff4061cc5ef832c52760e1e5fae71dabf8728dbca89f1a29145f33702e3769` |
| legacy-app | `src-tauri/src/remote_agent/store.rs` | `327e85fa1bc509582bd4e9b2d80ad2ccf40846d14db28ac62e6fd03454282039` |
| legacy-app | `src-tauri/src/remote_agent/system_protection.rs` | `fd6d3563e18336869071c3aea0d843e0392f725069a558640b06762432d085bb` |
| legacy-app | `src-tauri/src/work_core/repository.rs` | `9a1837ae5586c55d811c031a4662ace2995989eeacf98be56265df28841195ac` |
| legacy-app | `src-tauri/src/goal_runtime/repository.rs` | `e83dd4d0b05458195cf2ae37e6f7b2ed9df23ca7c113b9c7eff495f6b3c4c4a5` |
| legacy-online | `packages/server/src/db/migrations.ts` | `c88d37b2a37241b4ff6f726e7171e5b712ef09dd755136b211e05b08ffe803a7` |
| legacy-online | `packages/server/src/db/models.ts` | `b898631e8f2fb615d10105cbc767c8ad06783c7fb6ebab780641abe5f62cd5ae` |
| legacy-online | `packages/shared/src/types.ts` | `7ba3f05d4e2ea9df0b17124276416cdf67918f334cd526fc80940f12fefc9575` |
| legacy-online | `packages/server/src/routes/node-work.ts` | `9385658c0a3d8e0cb25bcc3c9c6122c8e07c7aa068c249b322047986d6ae7790` |
| legacy-online | `packages/server/src/verification.ts` | `83578a4ce610b18073710c3e22bbbd3135faec13310ebebd8893d86ab9b147f1` |

## 7 交接与回退

本轮只改统一项目文档/证据，回退使用本轮Git差异；不回滚旧仓库。旧App与Online继续按自身合同运行，不用本设计关闭任何旧验收。完整业务实现、构建、迁移、现场验证与用户接受均未发生。
