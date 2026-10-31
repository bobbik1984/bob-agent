# OL-01：Online `/api/client/v1` 实机探测与契约对齐记录

**探测日期**：2026-10-31  
**探测目标**：
1. Hostinger VPS2 (`187.124.144.87`) 现网 `bob-core.service` (`http://127.0.0.1:53019`)
2. 仓库源码 `D:\OneDrive\Learning\Code\Gemini\cloud-agent\packages\server\src\routes\client-api.ts` 与 `client-runtime.ts`
3. 新常开节点 `bob-online.bobbik.org` (`bob-node` 统一服务端)

---

## 1. 实机探测结果 (`http://127.0.0.1:53019`)

通过 SSH (`plink -batch -load "Hostinger"`) 对 `127.0.0.1:53019` 执行探测：

```bash
curl -s http://127.0.0.1:53019/api/client/v1/capabilities
# 返回：{"error":"Not Found"}
```

### 根因核查
- 现网 `bob-core.service` 运行 `/home/ubuntu/projects/cloud-agent/packages/server/dist/index.js`（编译时间 `2026-10-28 00:05`）。
- 该 `dist/` 目录尚未包含后续在本地 `cloud-agent` 仓库新增的 `client-api.js`、`client-runtime.js`、`client-auth.js`、`client-read-service.js`，且 `candidate.env` 未设置 `BOB_CLIENT_API_ENABLED=1` 与 `BOB_CLIENT_AUTH_CONFIG`。
- 同时，源码 `cloud-agent/packages/server/src/routes/client-api.ts` 第 28 行与第 112 行定义的 `CAPS` 为：
  ```json
  {
    "catalog": true,
    "history": true,
    "pagination": true,
    "submit": false,
    "query": false,
    "cancel": false,
    "approvals": false,
    "artifacts": false,
    "parent_revision": false,
    "review": false,
    "persistent_execution": false
  }
  ```
  当请求 `/api/client/v1/operations` 时，`client-api.ts` 抛出 `501 CLIENT_OPERATION_UNSUPPORTED`。

---

## 2. 架构与实现对齐决策（消除双轨假设）

1. **`bob-online.bobbik.org` (`bob-node`) 原生提供完整 `/api/client/v1/*` 与 `/api/rpc/*`**：
   - `bob-node` 不仅托管统一网页端（通过 `/api/rpc/:command` 提供完整 `bob-kernel` + `bob-runtime` 能力），同时原生实现 `/api/client/v1` 完整协议族：
     - `POST /api/client/v1/pairings`
     - `POST /api/client/v1/pairings/:id/exchange`
     - `GET /api/client/v1/capabilities`（开启 `catalog`, `history`, `pagination`, `submit`, `query`, `cancel`, `artifacts`, `parent_revision`, `review`, `persistent_execution`）
     - `GET /api/client/v1/projects`
     - `GET /api/client/v1/projects/:id/tasks`
     - `GET /api/client/v1/projects/:id/messages?after=<seq>`
     - `GET /api/client/v1/tasks/:id/snapshot`
     - `POST /api/client/v1/operations` 与 `GET /api/client/v1/operations/:id`（支持幂等 `request_id`、`continue_task`、`submit_message`、`create_task_iteration`、`cancel_execution`、`submit_review`）
     - `POST /api/client/v1/credentials/self/revoke`
2. **客户端 Native/Backend 适配层 (`OL-02` ~ `OL-07`)**：
   - Online Bearer Token 仅保存在后端 Vault (`device_vault` / `android_vault` / `bob-runtime` Linux `0600` vault)，前端渲染层只读取 `hasToken`、`status`、`capabilities` 与脱敏元数据。
   - 当远端 `capabilities.submit === false`（如旧版只读 `client-api.ts`）时，客户端诚实展示只读浏览状态并禁用远端任务写入；当连接 `bob-online.bobbik.org`（`capabilities.submit === true`）时，完整开放指定任务续聊、V1→V2 派生、产物预览与验收。
