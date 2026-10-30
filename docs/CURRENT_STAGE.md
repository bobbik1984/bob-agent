# Bob — Current Stage（1.0.1 施工与交接合同）

## 当前施工入口：2026-10-30 五阶段推进计划执行状态（Stage S0 → S4）

> **唯一任务池与详细验收路径**：见根目录 [`todo.md`](../todo.md) 中的 `RV-01 ~ RV-21`、`OL-01 ~ OL-07` 与 `XP-01 ~ XP-10`。
> **历史演进归档**：2026-10-28 至 2026-10-30 初版的详细验证流水已迁入 [`docs/evidence/2026-10-29-historical-stage-log.md`](evidence/2026-10-29-historical-stage-log.md)（`RV-10`）。

### 1. 当前已真实跑通并有自动化/真机证据的能力

1. **Stage S0（P0 止血与真相对齐，`RV-01` ~ `RV-10`）— 已完成**：
   - **`RV-01` 日程状态值对齐**：`ScheduleView.vue` 与 IPC 统一使用 `'completed'`（对齐内核 `scheduled | completed | cancelled`），内核负向测试拒绝 `'done'`。
   - **`RV-02` 日程/待办 `kind` 字段**：新增 `crates/bob-kernel/migrations/0007.sql`，为 `schedule_items` 增加 `kind TEXT NOT NULL DEFAULT 'event' CHECK(kind IN ('event','todo'))`，打通 `create_schedule_item_with_kind`、`app_schedule_create`、`kernel-bridge.mjs` 与 `ScheduleView.vue` 分栏。
   - **`RV-03` 时间字段名与单位统一**：统一使用 13 位 UTC 毫秒（`start_at`, `end_at`, `reminder_at`），`kernel-bridge.mjs` 默认回退 `Date.now()`。
   - **`RV-04` 记忆字段与状态值对齐**：`KnowledgeView.vue` 与设置“工作间”记忆列表统一读取 `mem.body` 与 `mem.state`，状态切换统一使用 `'active' | 'superseded' | 'forgotten'`。
   - **`RV-05` 去除静默吞错**：移除个人事务写操作上的 `.catch(() => null)`，统一通过 `personalFault` 展示 `role="alert"` 告警横幅。
   - **`RV-06` Rust ↔ UI 真实序列化快照契约**：由 `crates/bob-kernel/tests/direct_provider_and_personal_ops.rs` 直接序列化生成 `apps/bob-app/tests/fixtures/personal_snapshot.json`，Node 测试与 `ui-browser.py` 5 视口回归直接加载该快照。
   - **`RV-07` 设置“工作间”真实统计**：会话数显示 `(catalog?.conversations || []).length`，独立显示已保存消息数 `saved_messages`，未配置目录/节点显示本地化 `t('unconfiguredValue')`。
   - **`RV-08` ~ `RV-10` 文档对齐与瘦身**：清理不存在的函数名引用，更新 `FEATURES.md`、`ARCHITECTURE.md`、`kernel-schema.md`，并将 `CURRENT_STAGE.md` 历史流水归档至 `docs/evidence/2026-10-29-historical-stage-log.md`。
2. **Stage S1（个人助理闭环与个人数据同步，`RV-12` + `RV-13`）— 核心完成**：
   - **`RV-12` 禁用未实现占位按钮并补齐 i18n**：本地模型下载/目录按钮显式 `disabled` 并标注“即将支持”，模型筛选框占位符接入 `t('modelFilterPlaceholder')`。
   - **`RV-13` 个人数据 Replica → Home 同步**：`0007.sql` 新增 `personal_ops_sync_outbox`；`personal_ops.rs` 实现 `pending_personal_op_syncs`、`ack_replica_personal_op`、`Store::receive_replica_personal_op_with_receipt`（含 `inbox_receipts` 幂等重放与 `expected_version` 版本冲突拒绝）；`bob-transport` 新增 `HomeRequest::ReplicaPersonalOp` 并接入 `home_host.rs` 与 `home_link.rs`。
   - **待用户填入真实外部商业 Key 验收项**：`RV-11`（真实商业 API Key 真机问答记录 Evidence）。
3. **Stage S2 / M0（接入云端节点 `agent.bobbik.org`，`OL-02` ~ `OL-04`）— 客户端只读与 PKCE 配对适配器完成**：
   - 新增 `apps/bob-app/src/online-client.mjs`（实现 `bob-core-client/1.0` PKCE S256 配对 `beginPairing`/`exchangePairing` 及 `/api/client/v1` 的 `fetchCapabilities`、`fetchProjects`、`fetchProjectTasks`、`fetchProjectMessages`、`fetchTaskSnapshot`、`revokeSelf`）。
   - 在 `WorkView.vue` 新增第 3 张执行节点卡片“☁ 云端 Online Work 节点”与交互面板，并通过 Loopback 测试与 5 视口 Playwright 验证。
   - **待推进项**：`OL-01`（获批后对生产 `agent.bobbik.org` 发起只读探测）及 `OL-05` ~ `OL-07`（在 `cloud-agent` 仓库补齐 `/operations` 写入接口）。
4. **Stage S3 / S4 关键交互能力先行落地（`RV-16`, `RV-17`, `RV-18`）**：
   - **`RV-16` 决定记录 IPC 与 UI**：新增 `app_decision_record`、`app_decision_status` IPC 与 `KnowledgeView.vue` 第 4 子页“⚖ 决定记录 (decisions)”。
   - **`RV-17` 对话升级为 Work 项目入口**：在对话顶栏新增“💼 升级为工作项目 (`promoteConversationToWork`)”快捷入口。
   - **`RV-18` 轮询自适应退避**：页面隐藏（`document.hidden`）暂停轮询，非对话空闲视图自动降低轮询频率。

---

### 2. 下一阶段待执行队列（Stage S3 `XP-01`~`05` & Stage S4 `XP-06`~`10`）

| 阶段 | 任务 ID | 核心工作 | 验证门禁 |
|---|---|---|---|
| **S1 收尾** | `RV-11` | 在 Pixel 8 与 Windows 上填入真实外部商业 API Key 完成两轮问答并归档 Evidence | `docs/evidence/YYYY-MM-DD-f1-device-acceptance.md` |
| **S2 扩展** | `OL-01`, `OL-05`~`07` | 获批后探测线上 `agent.bobbik.org/api/client/v1/capabilities`；在 `cloud-agent` 实现 `/operations` 幂等写入 | 专用测试项目两轮续聊与断网对账通过 |
| **S3 (M1)** | `XP-01`~`03`, `XP-05`, `RV-14`~`15` | 抽离 `crates/bob-runtime` 平台 Trait，下沉 `src-tauri` 业务逻辑，拆分 `App.vue` | `cargo test --workspace` 全绿；`App.vue` < 600 行 |
| **S4 (M2–M4)** | `XP-04`, `XP-06`~`10`, `RV-19`~`21` | 交付无界面 `crates/bob-node` 服务端与 `http-bridge.mjs` Web 适配层，收敛旧 Online | Linux 容器启动 `bob-node`，手机/电脑/浏览器三端同源验证通过 |
