# Bob 1.0.1 — Stage S0 ~ S2 真机与全栈契约验收证据（2026-10-30）

## 1. 构建与真机安装基线

- **源码提交**：`db9a9ded3d590c5e8954d7bc4ac789c2cf9afdda`（分支 `1.0.1` 与 `main` 同步）
- **GitHub Actions 构建**：Workflow `android-dev.yml` · Run `41064471009` · Artifact `12981392665`（`conclusion=success`）
- **签名 APK 产物**：
  - 固定路径：`dist-release/bob-latest.apk`（归档副本 `dist-release/bob-1.0.1-db9a9de-universal.apk`）
  - APK SHA256：`061d174008887976406c78d190f2f72b7ccf77b21130d0373fbe57f9395fbe62`
- **目标真机**：Google Pixel 8（序列号 `3A091FDJH0004F`），通过 `adb install -r` 覆盖安装并自动执行 SQLite `0007.sql` 迁移。

---

## 2. Automated Test Suite Results（全量自动化门禁）

| 测试套件 | 覆盖范围 | 结果 |
|---|---|---|
| `cargo test -p bob-kernel` | 76 项内核测试：含 `0007.sql` 迁移、`schedule_items.kind ('event'\|'todo')`、拒绝非法 `'done'` 状态、`RV-13` 已配对 Replica → Home 个人事务幂等汇入与 `expected_version` 冲突拒绝、`RV-16` 决定记录状态流转、`RV-06` 导出 `personal_snapshot.json` | **76 passed, 0 failed** |
| `cargo test -p bob-transport` | 24 项双向 TLS 传输与控制面测试（含 `HomeRequest::ReplicaPersonalOp`） | **24 passed, 0 failed** |
| `node --test tests/kernel-bridge.test.mjs` | 12 项前端桥接与协议测试：含个人事务 IPC 映射、`RV-06` 真实 Rust 快照结构断言、`OL-02..04` `bob-core-client/1.0` PKCE S256 配对与 `/api/client/v1` 只读拉取/自吊销 | **12 passed, 0 failed** |
| `tests/ui-browser.py` (Playwright) | 5 视口（`390×844` 竖屏 / `1180×757` 横屏 / `390×420` 紧凑屏 × 中英/明暗）加载真实 `personal_snapshot.json`，覆盖对话、日程、工作（含 `bob-core-client/1.0` 云端节点卡片）、知识（含长期记忆与决定记录子页）、设置（含 199 模型中心与工作间真实统计） | **5 viewports passed, 0 failed** |

---

## 3. Pixel 8 (`3A091FDJH0004F`) 真机 SQLite 持久化与杀进程重开证据

通过真机 `org.bobagent.nextdev` 原生 Tauri IPC 实测（进程 `PID=22283` → `am force-stop` 杀进程 → 重启 `PID=22584`）：

1. **状态校验与写入（杀进程前，`PID=22283`）**：
   - 调用 `app_schedule_status(id, 'done')` 被 Rust 内核明确拒绝（`doneRejected: true`）；
   - 调用 `app_schedule_status(id, 'completed')` 成功并将 `version` 递增至 `2`；
   - 分别创建 `kind='todo'` 与 `kind='event'` 的日程项（13 位毫秒时间戳 `1793367915007`），以及 `status='proposed' -> 'accepted'` 的决定记录。
2. **强制杀进程并冷启动重开后读回（`PID=22584`）**：
   ```json
   {
     "todo": {
       "id": "rv-pixel-todo-1793367915007",
       "title": "Pixel 8 待办真机验证",
       "kind": "todo",
       "start_at": 1793367915007,
       "status": "completed",
       "acceptance": "local",
       "version": 2
     },
     "ev": {
       "id": "rv-pixel-event-1793367915007",
       "title": "Pixel 8 日程真机验证",
       "kind": "event",
       "start_at": 1793371515007,
       "status": "scheduled",
       "acceptance": "local",
       "version": 1
     },
     "dec": {
       "id": "rv-pixel-dec-1793367915007",
       "project_id": "proj-default",
       "body": "Pixel 8 真机决定记录持久化验证",
       "status": "accepted",
       "version": 2
     }
   }
   ```
   所有字段、状态与版本号在进程重启后 100% 保持一致。
