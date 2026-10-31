# 2026-10-31 S0–S4 统一内核、常开节点与 `bob-online.bobbik.org` 部署验证报告

## 1. 概述与执行范围

本次施工按 `docs/CURRENT_STAGE.md` 与 `docs/DEVELOPMENT_ROUTE.md` 的阶段顺序（`S0` → `S1` → `S2` → `S3/S4`），完成了：
1. **S0（可信边界与诚实状态）**：凭据同步脱敏、PeerSync 先校验后合并并重算 SHA-256、Online 凭据存入系统 Vault 且前端仅读布尔/元数据状态、固定契约快照断言（`RV-05`、`RV-06`、`RV-13`、`OL-02`、`OL-04`）。
2. **S1（日常个人助理闭环）**：原位会话升级为工程项目（`RV-17`，保留原 `conversation_id`、消息、草稿、笔记与决定引用）、空闲 15s 轮询退避与同步失败指数退避（`RV-18`）、结构化决定记录表单（`RV-16`）与本地北极星诊断指标（`RV-20`）。
3. **S2（Online 接入与受控续聊）**：生产端点只读探测（`OL-01`）、原生 Online 配对/轮询/增量缓存/受控续聊与 V1→V2 修订/人工审阅/吊销（`OL-02`～`OL-07`）。
4. **S3 / S4（`bob-runtime`、`bob-node` 常开节点、统一 Web 端及 `bob-online.bobbik.org` 部署）**：
   - 新增 `crates/bob-runtime`（平台抽象 `SecretStore`、`FileVaultSecretStore` 0600 权限、`RuntimeCapabilityManifest`、`ModelHubRuntime`）。
   - 新增 `crates/bob-node`（常开无界面 Rust 节点，监听 `127.0.0.1:53020`，提供统一 Vue 3 Web 端静态托管、`POST /api/rpc/:command`、`GET /api/capabilities`、`GET /api/events`、完整 `/api/client/v1/*` 协议，以及对旧 Online `nexus_control_plane.db` 的只读幂等导入 `XP-10`）。
   - 前端新增 `apps/bob-app/src/bridge/http-bridge.mjs` 并接入 `native-bridge.mjs`，实现手机（Tauri Android）、桌面（Tauri Windows）、浏览器（`bob-node` Web）三端共用同一套 Vue 3 前端与同一 Rust 内核。
   - 在 **Hostinger VPS2 (`187.124.144.87`)** 完成 `/opt/bob-online` 构建与 `bob-online.service` 部署，通过 Caddy (`http://bob-online.bobbik.org`) 与 Cloudflare Tunnel 暴露公网域名 **`https://bob-online.bobbik.org`**。

---

## 2. 阶段逐项实现与验证证据

### 2.1 S0：凭据边界、PeerSync 顺序与契约快照
- **`apps/bob-app/src-tauri/src/direct_llm.rs`**：
  - `export_shared_config_from_dir` 与 `export_shared_config_for_sync` 彻底移除明文 `apiKey`、`token`、`tavilyApiKey`、`tinyfishApiKey` 导出，仅同步非敏感元数据。
  - `merge_shared_config_with_app` 忽略远端同步包中的任何密钥字段，防止跨设备普通数据同步覆盖或注入凭据。
  - `build_model_hub_snapshot_and_sync` 的 `remoteNodes` 列表去除明文 `"token"`，仅返回 `"hasToken": bool`。
- **`apps/bob-app/src-tauri/src/home_host.rs`**：
  - `HomeRequest::PeerSync` 先执行 `host_receive_peer_sync` 校验设备身份、配对状态、方向与 `payload_sha256`，校验通过后才合并非敏感共享配置，并在更新 `out_bundle.shared_config_json` 后调用 `out_bundle.compute_sha256()` 重算摘要。
- **`RV-06` 契约快照测试**：
  - `export_personal_snapshot_fixture_contract` 改为读取 `apps/bob-app/tests/fixtures/personal_snapshot.json` 并严格 `assert_eq!`，仅当显式设置 `BOB_UPDATE_SNAPSHOTS=1` 时更新快照。

### 2.2 S1：原位升级 Work、退避轮询、结构化决定与本地指标
- **`RV-17` 原位会话升级 (`crates/bob-kernel/src/catalog.rs`)**：
  - 实现 `LocalApp::promote_conversation_to_project(conversation_id, project_id, project_title, at)`：原位更新现有会话的 `project_id`，保持 `conversation_id`、历史消息、草稿和关联材料不变，同时幂等创建 `tasks` 表记录（`task-{conversation_id}`）。
- **`RV-18` 轮询与同步退避 (`apps/bob-app/src/App.vue`)**：
  - 前台有活跃执行时保持 `1.5s` 刷新；无活跃执行时退避至 `15s`；页面隐藏（`document.hidden`）时暂停轮询；`syncHome` 失败按指数退避（`3s → 6s → 12s → 24s → 60s`）。
- **`RV-16` 决定记录表单 (`apps/bob-app/src/views/KnowledgeView.vue`)**：
  - 支持录入并展示决定的标题、理由（Rationale）、备选方案（Alternatives）与重审条件（Review Condition），状态覆盖 `proposed | accepted | superseded | rejected`。
- **`RV-20` 本地北极星指标 (`apps/bob-app/src/App.vue`)**：
  - 在设置「工作间」面板展示本地诊断指标（续接成功次数、首次对话配置步数、`unknown` 状态停留时长、成果找回次数）并提供清空按钮。

### 2.3 S2：Online 接入与受控任务续聊 (`OL-01`～`OL-07`)
- **生产探测 (`OL-01`)**：见 `docs/evidence/2026-10-31-online-client-api-probe.md`。
- **原生命令与 UI 闭环 (`OL-02`～`OL-07`)**：
  - 实现 `app_online_begin_pairing`、`app_online_exchange_pairing`、`app_online_sync`、`app_online_submit_operation`（支持 `continue_task`、`create_revision` V1→V2、`review_task`、`cancel_task` 幂等 `request_id`）与 `app_online_revoke`。
  - `WorkView.vue` 新增指定父任务续聊、创建 V2 修订、成果审阅通过/驳回及「在浏览器中打开」诚实跳转入口。

### 2.4 S3 / S4：`bob-runtime`、`bob-node` 与 `https://bob-online.bobbik.org` 公网验证
- **本地全量自动化测试结果**：
  - `cargo test --manifest-path Cargo.toml`：`bob-kernel`（89 passed）、`bob-transport`（24 passed）、`bob-runtime`（4 passed）、`bob-node`（2 passed，含 `bob_node_rpc_and_client_v1_end_to_end_closed_loop` 与 `xp10_import_legacy_nexus_db_is_idempotent_and_read_only`）全部通过。
  - `cargo test --manifest-path apps/bob-app/src-tauri/Cargo.toml --lib --features native-tests`：34 passed, 0 failed。
  - `node --test`：38 passed, 0 failed。
  - `rebuild_and_verify.py`（5 视口 Playwright + Vite 生产构建）：`REBUILD_AND_VERIFY_SUCCESS`。
  - `verify_bob_node_web_e2e.py`（真实 `bob-node` 二进制托管统一 Web 前端 + Playwright Chromium 端到端操作）：`BOB_NODE_WEB_E2E_VERIFIED`。

- **Hostinger VPS2 (`187.124.144.87`) 部署与公网 `https://bob-online.bobbik.org` 实测结果**：
  - 服务进程：`/etc/systemd/system/bob-online.service`（`active (running)`，监听 `127.0.0.1:53020`），只读幂等导入 `/home/ubuntu/.bob/state/isolated_p3_02c_9f2ae20/nexus_control_plane.db` 共 **69 个历史项目**。
  - 反向代理与认证边界：Caddy `http://bob-online.bobbik.org` 对浏览器访问启用 `import common_auth`，未登录访问 `GET https://bob-online.bobbik.org/` 返回 **`401 Unauthorized`**；`/api/client/v1/*` 走设备 Bearer Token 鉴权。
  - 公网端到端实测回执（2026-10-31 02:03 CST）：
    - `GET https://bob-online.bobbik.org/`（无凭据）→ `HTTP 401`
    - `POST https://bob-online.bobbik.org/api/client/v1/pairings` → `HTTP 201` (`pairing_id: "pair_1793383405038_5"`)
    - `POST https://bob-online.bobbik.org/api/client/v1/pairings/exchange` → `HTTP 200` (`credential_id: "cred_1793383405848"`)
    - `GET https://bob-online.bobbik.org/api/client/v1/capabilities` → `HTTP 200`（`catalog/history/pagination/submit/query/cancel/approvals/artifacts/parent_revision/review/persistent_execution` 全部为 `true`）
    - `GET https://bob-online.bobbik.org/api/client/v1/projects` → `HTTP 200`（返回 `69` 个项目）
    - `GET https://bob-online.bobbik.org/api/client/v1/projects/proj_91131130ed7cc55a8e9e87c5/tasks` → `HTTP 200`（返回关联任务 `task-conv-task_1792501503481_mlkdu9p`）
    - `GET https://bob-online.bobbik.org/api/client/v1/tasks/task-conv-task_1792501503481_mlkdu9p/snapshot` → `HTTP 200`
    - `POST https://bob-online.bobbik.org/api/client/v1/operations` (`type: "create_revision"`, `request_id: "pub-verify-rev-v2-001"`) → `HTTP 200` (`operation_id: "op-1793383411603"`, `iteration: 2`, `executions_count: 2`)
    - `POST https://bob-online.bobbik.org/api/client/v1/operations` (`type: "review"`, `verdict: "approved"`, `request_id: "pub-verify-review-001"`) → `HTTP 200` (`operation_id: "op-1793383412622"`, `task_status: "completed"`)
    - `POST https://bob-online.bobbik.org/api/client/v1/devices/current/revoke` → `HTTP 200` (`{"status": "revoked"}`)
    - 吊销后再次请求 `GET https://bob-online.bobbik.org/api/client/v1/projects` → `HTTP 401` (`{"code": "CLIENT_TOKEN_REVOKED", "retryable": false}`)
