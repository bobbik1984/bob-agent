# VPS3 原生适配器首轮验证

日期：2026-11-04。基线：`62cffe2` + 工作树现有设计改动。用户授权必要源码修改并指定 VPS3 验证；本轮未发布、未安装/升级工具、未创建公网监听、未测试手机。

## 实现与验证层次

- `runtime_rpc` 已有五项 Harness 动作现在进入真实、受 `device_harnesses.json` 宿主/工具授权限制的探测。前端输入不能指定任意程序、SSH 目标或凭据；profile 未适配时保持未知并返回 `HARNESS_PROFILE_NOT_SUPPORTED`。
- 不再把配置的 installed/auth_cached 或工具名称推断成已认证、tmux 监管、流式和续聊成功。官方文档支持与现场观测、Bob 接通程度分别记录。
- 公共 JSONL 子进程组件支持同进程多请求、ID 关联、提前通知缓冲、UTF-8 分帧、1 MiB 单帧/4 MiB 总输出/64 项通知上限、超时和自有进程清理。该上限适用于探测；长任务服务的持久事件与窗口仍待实现。
- 当前官方接口依据仍见 `wiki/harness-auth.md §6`。AGY 官方文档的 ACP 支持不能推断到旧安装版本。

## VPS3 真实结果

通过 cluster-ops 登记的 SSH 连接，以当前 OS 用户在临时目录执行同一候选探测源码。SSH 只传 Python 源码与固定探测参数；没有复制工具凭据。以下是协议层实测，不是部署后 Bob 页面实测：

| 工具 | 实际版本 | 结果 | 未验边界 |
|---|---|---|---|
| Codex | 0.156.1 | app-server initialize/account-read/thread-list 成功，账户已登录，同进程复用 | 列表返回 0；未验证原会话恢复、发送、流式、审批、取消 |
| Pi | 0.85.1 | 同进程 get_state/get_messages/get_available_models 成功 | 隔离空历史；可用模型 0，所选模型不可用；实际回复与流式未通过 |
| AGY | 1.2.16 | 程序存在；ACP 初始化时退出 PROCESS_EXITED | 不推断为未登录或永久不支持；未自动升级 |
| Claude Code | 2.1.74 | auth status 返回 loggedIn=true | 未发送模型请求，未测历史/续接/流式 |
| DSH | 未确定 | 观察到既有 3080 监听；当前 PATH 无 dsh | 端口不能证明服务身份，本轮未测认证或会话 |

Pi 实际短文本探针：临时目录、`--no-session --no-tools --no-extensions --no-skills --no-prompt-templates --no-themes`，只请求固定连通性文本。返回 `REQUEST_REJECTED`，未收到接受回执、文本增量或完成；自有进程已退出。安装源码确认该错误分支发生于 prompt preflight，之后只读核对 `get_available_models` 返回 0，未再次发送。不能把这次拒绝写成完成或假流式，也不能从泛化错误断定具体密钥为何不可用。未读取密钥、未修改模型配置。

## 回归

- `python -B crates/bob-runtime/tests/test_harness_probe.py`：最终 11/11。
- `cargo test --offline -p bob-runtime --lib`：62/62，外置 Cargo 缓存。
- 最终嵌入源码更新后 `cargo test --offline -p bob-runtime harness_auth::tests`：6/6。
- 工作区 `project_aligner.py` 对已登记外置仓库的精确补丁兼容修复：9/9；保留相对越界/目标越界/基线哈希校验与备份，不触发看板发布。
- 浏览器、Android、Bob 节点重启恢复与完整端到端发送未运行；没有用协议验证替代这些验收。

## 后续门槛与回退

继续现有 BP4-06.a–d：先修节点配对审查阻断项；接持久事件日志/客户端游标、一个可用工具的发送—真实增量—终态路径，再验原 ID 恢复、取消、断线不重放。Pi 需确认其自身可用模型认证；AGY 需核对安装版本的官方协议，升级另行授权。代码审查还发现旧 `agy_index_probe.py` worker 的续接失败回退新会话行为需要单独封堵，不能借此冒充原会话恢复。

回退仅撤回本次探测源码/调用改动，保留既有 DSH 凭据、会话、回执和设计文档；不 reset 其他工作树改动。VPS3 未安装候选服务，无远端服务回退操作。

## 正式源码 SHA-256

- `crates/bob-runtime/src/harness_auth.rs`: `7600f3cb3970c071a75403b93452cdacf00d4df7c57ba6d1a92660a397315b89`
- `crates/bob-runtime/src/agy_directory.rs`: `9169f3e9818dd71a20a5507048676f97da1d8b84014b94b7a0a7b61bfe591937`
- `crates/bob-runtime/src/harness_probe.py`: `17c0a8a15e37bcc7e38e132cccff7d43094097975dda540946eba5e63efc061d`
- `crates/bob-runtime/tests/test_harness_probe.py`: `59f2a8daa63f5485309dbfd9f590f3c8b6a17bf8de7fddbdb3b601f5fc2cb74f`

## 脱敏原生观测

```json
[
  {
    "schema_version": 1,
    "tool": "codex",
    "installed": true,
    "auth_state": "authenticated",
    "observed_at": 1793782226895,
    "version": "0.156.1",
    "preferred_transport": "app_server_stdio",
    "observed_transport": "app_server_stdio",
    "probe_kind": "read_only_no_model",
    "capabilities": {
      "list_sessions": "supported",
      "read_history": "unknown",
      "resume_chat": "unknown",
      "stream_events": "unknown",
      "cancel_execution": "unknown",
      "read_artifacts": "unknown",
      "approvals": "unknown"
    },
    "protocol_verified": true,
    "process_reused": true,
    "checks": [
      "account/read",
      "thread/list"
    ],
    "sample_count": 0,
    "owned_process_exited": true
  },
  {
    "schema_version": 1,
    "tool": "agy",
    "installed": true,
    "auth_state": "unknown",
    "observed_at": 1793782235103,
    "version": "1.2.16",
    "preferred_transport": "acp_stdio",
    "observed_transport": null,
    "probe_kind": "read_only_no_model",
    "capabilities": {
      "list_sessions": "unknown",
      "read_history": "unknown",
      "resume_chat": "unknown",
      "stream_events": "unknown",
      "cancel_execution": "unknown",
      "read_artifacts": "unknown",
      "approvals": "unknown"
    },
    "protocol_verified": false,
    "process_reused": false,
    "checks": [],
    "advertised_flags": [],
    "reason": "PROCESS_EXITED",
    "owned_process_exited": true
  },
  {
    "schema_version": 1,
    "tool": "claude",
    "installed": true,
    "auth_state": "authenticated",
    "observed_at": 1793782238029,
    "version": "2.1.74",
    "preferred_transport": "structured_cli",
    "observed_transport": null,
    "probe_kind": "read_only_no_model",
    "capabilities": {
      "list_sessions": "unknown",
      "read_history": "unknown",
      "resume_chat": "unknown",
      "stream_events": "unknown",
      "cancel_execution": "unknown",
      "read_artifacts": "unknown",
      "approvals": "unknown"
    },
    "protocol_verified": false,
    "process_reused": false,
    "checks": [
      "auth/status"
    ]
  },
  {
    "schema_version": 1,
    "tool": "pi",
    "installed": true,
    "auth_state": "unknown",
    "observed_at": 1793782703401,
    "version": "0.85.1",
    "preferred_transport": "stdio_rpc",
    "observed_transport": "stdio_rpc",
    "probe_kind": "read_only_no_model",
    "capabilities": {
      "list_sessions": "unknown",
      "read_history": "supported",
      "resume_chat": "unknown",
      "stream_events": "unknown",
      "cancel_execution": "unknown",
      "read_artifacts": "unknown",
      "approvals": "unknown"
    },
    "protocol_verified": true,
    "process_reused": true,
    "checks": [
      "get_state",
      "get_messages",
      "get_available_models"
    ],
    "history_scope": "isolated_probe_session",
    "sample_count": 0,
    "available_model_count": 0,
    "configured_model_available": false,
    "reason": "NO_AVAILABLE_MODEL",
    "owned_process_exited": true
  }
]
```


## 指定 DeepSeek 凭据后的实际流式复验

用户授权使用本地统一凭据中 alias `DS for Remote`。按模型注册表选取 `deepseek-flash`，使用 Pi 官方 models.json 兼容端点机制；密钥仅经 SSH stdin 传入远端内存，再以进程环境变量提供，临时模型文件仅保存环境变量引用。隔离 agent 目录、无工具、无扩展、无持久会话；结束后临时目录自动清理，未改变 VPS3 Pi 默认配置。

实测 prompt 已接受，首段 972 ms，23 次 text_delta，收到预期标记，agent_settled 终态及 idle 状态均核对成功，总计 1063 ms，98 个字符，自有进程已退出。因此本轮已证明 VPS3 Pi 在指定有效凭据下可真实流式收发；先前默认环境可用模型为 0 的观察仍成立。尚不证明 Bob 页面事件转发、原会话续聊或长期配置完成。

配置依据：[Pi 官方模型配置](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md)。无密钥写入本记录。
