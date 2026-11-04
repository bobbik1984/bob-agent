# BP4-06 原会话续聊与真实增量验证

范围：2026-11-04 夜间至 11-05，用户授权 X1T/VPS3 原会话只读消息与定向编译、本地候选验证。手机暂停；未发布公网、未打包。新增记录仅在原生会话与 Bob 传输回执，不修改测试项目文件。

## 原生适配器现场结果

| 节点/工具 | 原会话结果 | 增量与时间 | 保护核验 |
|---|---|---|---|
| X1T Claude | 原 ID 收发成功 | 6 段；首段 29.065s，终态 30.066s | 原生 ID 一致，Git status+diff 指纹前后相同；关闭工具 |
| X1T AGY / Winter Shelter | 原 ID 收发成功 | 7 段；首段 22.104s，终态 22.105s | 原生 ID 一致，Git 指纹相同；未使用 bypass 权限 |
| VPS3 Pi | 原 ID 收发成功 | 38 段；首段 11.224s，终态 15.979s | 原生 ID 一致，Git 指纹相同；关闭工具/扩展 |
| X1T Codex | 未通过回复验收 | 第二次请求超时；官方 read 返回 14 turns，末项 interrupted/空 items，未找到该请求标记 | 保留 unknown、不自动重发；第一次请求经官方历史和进程核对为 not_accepted 后才允许新测试 |
| VPS3 Claude | 未通过回复验收 | 120s 无增量、无确认回复 | 旧 loggedIn 不代表可发送；未知请求保持待核对 |
| VPS3 Codex | 未发消息 | 已发现历史都指向不存在的临时工作目录 | 不重建目录、不创建新会话伪装续聊 |
| DSH | 本轮流式尚未接通 | 现场安装协议支持 session/follow + assistantStream=true；旧实现只取 snapshot 后关闭 | 现有历史/发送路径保留，不能声称本轮实时订阅已验 |

对应请求：Claude `bob-native-verify-1793806373173261957`；AGY `bob-native-verify-1793807607157828432`；Pi `bob-native-verify-1793807190458462075`。Codex 第二次为 `bob-native-verify-1793808328189451438`。不保存凭据、整段私人历史或原始进程日志。

## 本地网页真实验证

统一 5194 入口映射私有候选回环服务，使用两个明确宿主授权与既有 SSH 通道；未复制生产 Vault 或认证 Cookie。浏览器侧栏读到 X1T 21 条、VPS3 35 条（仅此候选授权清单，不能与生产含 DSH 的目录数量直接比较），打开 Pi 原有正文与前次验证回复，无 JS 异常。

页面真实发送：原生文本增量经 SQLite 事件序号、app_native_events 和统一 DshSessions 到达浏览器；不是将完成文本切字冒充上游流。浏览器观察到内容在生成期间增加，输入框保持可见。SSH 私有候选单次事件读取约 3–10s，网页使用有序游标轮询，尚不是 SSE/WebSocket 推送，也不是跨轮长驻进程池。

现场发现并修复：Pi message_end 的 length 被误算成功；仅思考无正文也不能算成功；user message_end 被误映射为助手 snapshot；已提交历史与增量暂时双显。增加截断/空正文失败、助手 role 校验与正文去重。首次长回答验证失败保留，不计为正常通过；最终候选浏览器 request `8e8b7606-63cf-4ba4-9eac-2c565d8a5eb2` 收到 26 段文本增量、约20.2s观察到终态，页面仅保留一条用户消息和一条回复，输入框可见，无JS错误。重开后24s的12次采样均为标记出现2次（用户/助手各一次），未重放成重复消息。见 [浏览器脱敏结果](2026-11-05-native-browser.json)。

## 当前保护与边界

- 移除 resume 失败后新建会话的静默 fallback，以及默认绕过权限标志。
- 单会话串行；请求 ID + 输入 hash 幂等，冲突拒绝；发送前落接受回执。进程丢失/传输超时记 unknown，不能推断 completed。
- SQLite 持久回执/事件可按 cursor 补读；队列、帧、正文及单请求事件数量有界。全局磁盘保留/过期游标、完整原生 turn/item ID 信封、跨轮长驻监管、取消确认仍未实现，不宣称 §6 契约全部完成。
- 本轮只读验证策略不等于已授权文件开发：Claude/Pi 禁工具，Codex read-only，AGY 默认权限且提示禁止工具；后续开发执行应走既有 Task/Execution 授权和工作区写锁。
- Pi 复用用户指定 DS for Remote，节点私有 profile 引用其本地密钥，限制访问权限；无前端明文、仓库密钥或生产凭据副本。
- Rust/Linux 定向候选构建与 Vite 构建通过；Vite 仍有既存大包警告。Python 原生续聊专项 8/8、现有 Phase A 前端回归 7/7 通过，git diff --check 通过。不是全量回归或手机验收。

## 接续与回退

沿 CURRENT_STAGE §3.6：先补 DSH 实时订阅与事件归一化；Codex/Claude 未知请求只读对账，明确原因后才恢复发送；补持久进程、取消/断线恢复、全局事件容量与过期游标，再扩展跨端验证。不得把历史读取或认证探测等同真实收发。回退仅停止本地转发和私有候选，不删原会话、回执或生产数据；未决请求不得因换版本重发。

## 源文件 SHA256

- `crates/bob-runtime/src/agy_index_probe.py`：`dfdf0096dfcb984013ae8e5e05734c531bd9f319c60aa01ba97c10369be39040`
- `crates/bob-runtime/src/agy_directory.rs`：`d2d31caee08e2d9d0f2c54bd93f90ed7ce09af3dddf858ee0258a01793753dc1`
- `crates/bob-node/src/lib.rs`：`586f69322f0137c862b949f101eab1900bb742eea454d068349e61e54c0bc135`
- `apps/bob-app/src/components/DshSessions.vue`：`a76861abc8a573d93c2e3422282af4bf940d902d92a7b6e219e9882e2c28dfd8`
- `apps/bob-app/src/kernel-bridge.mjs`：`485d2b274539916c629dd8dbbfb79fe07b35c40d69d543fed548f49de4570aba`
- `crates/bob-runtime/tests/test_native_stream.py`：`9ec0ff66224846b92bf9f66e0a7465226afbfeeada11d81f85784621b9ea254d`
