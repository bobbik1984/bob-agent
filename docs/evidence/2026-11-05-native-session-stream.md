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


## 2026-11-06 已落盘正文刷新修复（服务已发布，生产浏览器只读验证通过）

用户批准定向编译、候选验证与网页/相关后端发布；不打包 APK，不向原会话发送新请求。

- 原因：原生历史是有界可变快照（最多60条），不是只追加日志。旧逻辑仅比较消息数游标，将末条正文增长、满窗口滚动误判成 not_modified。
- 修复：原生读取显式返回 history_snapshot，不再用条数证明内容未变化；前端整体替换快照，比较正文变化，移除过期尾项；当前原生会话空闲轮询由60秒缩短到5秒。缓存、读取时间及网络延迟仍会影响实际可见时间，不能承诺5秒端到端刷新。
- 前端定向回归9/9、Rust agy_directory定向测试3/3、Linux release及Vite构建通过；现有大包警告保留。
- X1T真实Codex原会话的本地候选读回：连续两次60条、同游标仍返回快照正文，通过；未发送消息或修改项目文件。
- Hostinger网页与后端已发布，运行程序及HTTP文件哈希读回通过。来源为47e5599aa08654c8d02792a4ce7bc0656ccd35ab加本轮工作树修复，并非干净提交发布。
- 程序SHA256：911fe94e272a5e0020d0454480af1675b1c8c0f385a1fe18cdaac5091e9effe5；网页index SHA256：9f7f9c09f1b054fe2ed4181ec6b5ec94fc0c3a43a95ced707819792939729bda；HTTP SHA256：57946c52a29312bd6af61215355cf0fd834443ca3182036705f240c36a90736f。
- 回退备份位于服务根的 operator-backups/quota-device-20261106-134100-87176ead；只恢复程序及静态资源，不以旧数据库覆盖运行数据。
- X1T/VPS3节点已更新，二进制与Hostinger相同。VPS3发现35条会话、本机同游标读回13条；X1T发现21条原生会话、本机同游标读回31条。两节点服务active、运行文件SHA256一致。X1T跨洋全量/压缩/增量传输超时，最终使用既有本机内网SSH完成；未改网络配置或认证。
- 节点回退：各自程序目录内的 node-access-20261104-r2.before-history-1793973147（VPS3）与 node-access-20261104-r2.before-history-1793973339（X1T），恢复后重启用户级bob-node-access。
- 生产浏览器验证通过：经既有SSH私有通道访问实际Hostinger服务，从侧栏打开X1T原Codex会话，读到用户截图之后的真实新回复；DOM为60条，初始底部间距0，输入框可见，无横向溢出。向上翻阅后输入框仍可见。未发送消息。公网独立登录与手机真机不在此次验收内。
- 生产RPC经现有节点认证连续读取同一Codex原会话，两次均为history_snapshot=true、not_modified=false、60条，耗时6.89秒/10.75秒。真实网络延迟仍存在，本次不宣称网络已优化。
- 未覆盖：外部终端进程尚未落盘的实时token、真正的服务端每10条分页、手机真机、APK。不能将本次快照刷新修复宣称为所有工具实时流式完成。

源码SHA256：

- `crates/bob-runtime/src/agy_directory.rs`: `3041a485b98e4044ec8a69a594d37a4e9b09cba8a07834bae8180f5841325ae3`
- `apps/bob-app/src/components/DshSessions.vue`: `a1bff1a5c381ee10171549cff47087bee4d931ef2ced2b428483cbcc5de8c74d`
- `apps/bob-app/tests/phase-a-ux.test.mjs`: `fa0ef5f4a40ded795d45471ad20b7697d564bde780ec89b08d697b286d0553c5`

- 补充Claude Sea2原会话生产RPC核验：两次同游标读取均为history_snapshot=true、not_modified=false、31条，耗时2.84秒/3.37秒；没有发送消息。


## 2026-11-06 Codex 本轮身份隔离与最终正文恢复

- 实际 `_native_worker` 发送链路使用 `turn/start` 确认的 turn ID，与原 thread ID 一起过滤通知；旧轮次、其他会话和缺身份通知不会成为本轮正文或终态。原生轮次 ID 写入持久回执。
- 以 item ID 聚合正文；`item/completed` 的权威正文修复遗漏或重复增量，多段助手消息保留段落。重复最终快照不重复展示；已完成 item 的迟到 delta 忽略。部分版本在 `turn/completed` 返回的最终 items 也可补齐正文。
- 确认回执之前到达的通知有界暂存（1024 帧 / 1 MiB），确认后按顺序处理；无法确认 turn、缓存溢出或传输中断保留 unknown，不重发。正文超过既有 262144 字符上限显式失败。空最终正文清除只针对 Codex，Claude 工具消息保持原行为。
- Python 实际 worker 专项 20/20（含已有 AGY/Pi/Claude 保护）通过；模拟 Rust compact_python 传输压缩后的同一专项再次 20/20 通过。Rust agy_directory 3/3 与最终 Linux release 编译通过；无依赖升级。用 X1T 实际安装的 Codex 0.167.0 生成官方 JSON Schema，核对 AgentMessageDelta / ItemCompleted / TurnCompleted / TurnStartResponse 必需身份字段通过。没有调用模型或向项目会话追加消息。
- 网页后端与 X1T/VPS3 节点已更新；候选先读真实目录和同游标历史，正式服务重启后运行文件哈希与 active 状态核验。X1T 目录21条、原生历史31条；VPS3目录35条、原生历史13条。网页资源未变；不是新的浏览器视觉验收。
- 生产已认证连接只读核对 Claude 原会话：连续两次31条，history_snapshot=true / not_modified=false，分别3.86秒、3.20秒。传输曾超时或停滞，X1T/VPS3最终均用现有直连通道更新，没有改网络或凭据配置。
- 发布程序 SHA256：`e45407af45ba1a8c183c9276292a7e468844424f8d77987034e68588817d9b7e`；网页 index SHA256：`9f7f9c09f1b054fe2ed4181ec6b5ec94fc0c3a43a95ced707819792939729bda`。源码为 `47e5599` 加工作树修复，不是已推送的干净提交。
- 回退：网页服务备份 `quota-device-20261106-143221-f2436cb0`；节点程序目录内分别为 x1t `node-access-20261104-r2.before-codex-turn-1793975659`, vps3 `node-access-20261104-r2.before-codex-turn-1793976277`。只恢复程序，不回滚会话数据库或重发未知请求。
- 边界：此次没有真实 Codex 模型收发终验，不能关闭旧 unknown 请求；没有实现外部 CLI 进程未落盘 token 的旁观订阅、跨轮常驻监管、取消确认或 DSH assistantStream 持久订阅。手机仍暂停，无 APK。
- `crates/bob-runtime/src/agy_index_probe.py` SHA256：`ed5f68d4b727c6058f3f4d14423f5dd566870cb99ec173993dd9127c9b694bdd`。
- `crates/bob-runtime/tests/test_native_stream.py` SHA256：`c4d9381773cbbb74da7f7d5efd157c09365513580b1c7b502c239f3f8c677bb7`。

- 最终生产 Codex 只读回归：使用本轮生产目录返回的原会话 ID，连续两次60条、history_snapshot=true / not_modified=false，分别5.34秒、4.5秒。未向该会话发送请求。
- 稳定回退优先使用本次修改之前的 `911fe94e…` 程序：网页 `operator-backups/quota-device-20261106-142452-4cab5367/bob-node`；X1T `node-access-20261104-r2.before-codex-turn-1793975659`；VPS3 `node-access-20261104-r2.before-codex-turn-1793975277`。上文最新备份是发布链记录，网页/VPS3最后一次备份包含中间候选，不作为稳定回退首选。


## 2026-11-06 DSH 持续订阅与原生未知回执核对

### 实际接线与边界

- `dsh_live.rs` 持有官方 session/follow WebSocket，maxMessages=1、assistantStream=true；原 session/attempt/index/revision 校验，缺帧重连，基线替换、不拼接重复临时正文。提交的助手正文短暂保留至历史cursor覆盖，abandoned不假装完成。每连接按凭据范围隔离；最多8条活跃lease，45秒无人读取回收；每条正文256KiB、最多8条提交桥接消息。
- `app_dsh_live` 已接 bob-node 解锁分派、节点认证白名单/连接身份映射、共用 kernel-bridge、Tauri命令和 DshSessions。浏览器每次请求结束后约1秒轮询（重连约3秒），不是浏览器SSE/WebSocket推送。临时流不落第二份永久聊天库；节点重启后依靠上游基线恢复，尚非统一持久事件协议完整实现。
- 浏览器只在已靠近底部时跟随新正文，翻阅历史保持位置。现场发现投影asOfSeq比实时cursor慢1条，增加每个实时cursor仅触发一次历史补读，既有周期刷新继续负责索引追赶。
- `inspect_receipt` 对本地unknown保持unknown，并核对原输入；Codex只有已记录的native_session_id/native_turn_id才允许官方thread/read(includeTurns)只读对账，匹配completed/failed/interrupted终态后同事务保存snapshot/终态。无resume/prompt/自动重发，未确认、缺ID、空完成正文或网络错误保留unknown。

### 验证结果

- Rust DSH 定向27/27，包含真实TCP/WebSocket测试：服务未关闭前即读到增量，断线后新基线替换旧文本；正常、身份错配、乱序/重复、abandoned和容量边界覆盖。
- Python原生worker24/24；新增确切轮次恢复、缺轮次、错误thread和输入冲突案例。前端12/12，包括阅读位置、重连替换、跨会话隔离和投影滞后不重复拉取。
- Linux release及Vite构建通过；既有前端大包告警保留。Tauri源码已接线，本轮未编译Tauri/Android，未做手机验收。
- X1T私有候选复用既有单连接认证引用，未新建令牌、未复制整套Vault。实际DSH目录20条；Winter Shelter_M0最近10条；持续订阅从connecting到live，连续8次读取只有1条上游连接、cursor5111。会话空闲，revision0，无新模型请求；这证明连接/基线，不能称实际token生成终验。
- 统一本地5194浏览器实际打开Winter Shelter_M0，正文、时间戳、Markdown可见，初始定位最新；向上滚动仍保留底部输入框，无脚本error。测试使用私有SSH转发，不等于公网独立登录或手机测试。
- 生产已认证RPC：X1T目录41条，Winter最近10条cursor5111；live读取1.50/1.08/1.01/1.00秒，连接身份映射保持，connecting后live、connections=1。未向原会话发消息、未修改真实工作区。
- X1T原有1条Codex unknown缺native_turn_id，仍保留未知；不能从相似文字猜测或自动重发。实际旧回执恢复未通过，不虚标完成。

### 发布与回退

- 基线47e5599加现有工作树修复，非干净提交发布，未推送GitHub或生成APK。网页及后端发布通过兼容探测、现有数据备份和HTTP哈希读回。
- 最终程序SHA256：`659da618fe02c68d40479c69876ef61b97e9d577bd957c57d4039b7915c41ac7`；网页index：`38f4a31048eaf39e9dc98ea20b2920bab93f35259378955a29c41c5dd924ec9f`；HTTP：`323fd9f63e14a5b27e303b77c8848fc415650b1b3f3fe368b90b6280c2304373`。
- 本轮改动前回退备份：网页operator-backups/quota-device-20261106-152217-b794187a；X1T程序目录node-access-20261104-r2.before-dsh-live-1793978401；VPS3程序目录node-access-20261104-r2.before-dsh-live-1793978545。只恢复程序/静态资源，不回滚会话数据库或重发未知请求。节点最终摘要对齐结果见后续读回记录。
- 下一步仍沿CURRENT_STAGE §3.6：真实DSH生成/断线终验、外部CLI官方事件旁观、缺身份未知回执人工核对、持久监管/取消、全局容量/过期cursor；手机等待用户通知。

本轮源文件SHA256：

- `crates/bob-runtime/src/dsh_live.rs`: `89837e325d0c5f3c5b24e1ccdc9960a8c57f7fd7f6be983d9bb94989dd387cf9`
- `crates/bob-runtime/src/dsh_transport.rs`: `11f8f0d261c3f18b34574bb30b33b4e0755665268e6318efd89b6715aadad278`
- `crates/bob-runtime/src/agy_index_probe.py`: `29fe7ad066e31a4cc0a331eadc4d526f32ac982ee039f657237507074d1d86a8`
- `crates/bob-node/src/lib.rs`: `c6dfcb7be0c6348b75b1f6450381bfb829a047d636f0e7f3ecfa836a027f1fa4`
- `apps/bob-app/src/components/DshSessions.vue`: `0061db8cd234d56bb158ef1a5b6c164b84fe774e44c14f6969cd818d66df6c70`
- `apps/bob-app/src/kernel-bridge.mjs`: `ca7d2bbbd314fc149dc52be54b315537c43e9651c0f8b71663d42aefed4f6902`
- `apps/bob-app/tests/phase-a-ux.test.mjs`: `dd66c8d568982ec1611c0fc62b1584737872a7f066610dc7224f17f1b65563be`


- 最终节点对齐读回：X1T/VPS3均active且运行摘要为上文659da618…；替换前真实原生目录21/35条，同游标正文31/13条通过。最新备份1793978679/1793978810为本轮中间候选，稳定回退仍采用上文发布前1793978401/1793978545。程序测试代码追加后构建摘要变化，已重新统一三台运行文件，未重启DSH或其他原生CLI。

- 最终统一构建后的生产认证复验仍通过：41条目录、Winter最近10条、cursor5111；connecting转live，后续connections均为1，读取1.53/0.94/0.94/2.21秒。公网匿名HTTP从本机返回403，未绕过认证；本次公网独立登录浏览器验收仍未完成，不能将回环HTTP摘要读回等同公网用户端完整验收。

## 2026-11-07 真实生成恢复、官方外部旁观与取消确认

用户沿现有授权要求继续实施、验证并上线，不打包APK；手机测试仍暂停。来源47e5599加现有工作树，未新增干净提交或推送GitHub。

- 聊天统一滚动控制器：初始/重入最新，ResizeObserver处理图片和容器增高；上翻记录首条可见消息偏移，向下箭头恢复跟随。5194真实Winter浏览器：底距约0；PageUp后箭头出现；点击后底距约0；输入框y=773.125保持。预览返回保持旧锚点。
- DSH实际请求 `bob-recovery-b0b32389-e880-4e16-a569-73d7fb2b9028`，原会话 `session-771043d7-4fae-4d27-b0eb-b6cd714992f9`：仅一次禁工具/禁文件进度询问。生成125字符时重启私有读取候选，重连基线cursor5124，最终548字符；匹配用户消息1、助手回复1，Git HEAD/status/diff指纹前后相同。仅停止读取服务，未重启DSH或外部CLI。证据在该节点私有状态目录 `bob-dsh-generation-recovery.json`；这不是物理网络断线或手机验收。
- Pi原会话 `pi:01a099bb-74bc-757c-b29b-d0bf540f05b7`，请求 `bob-stop-baf4279b-3aed-4d61-9a90-6e17422c3feb`：既有DS for Remote profile，禁工具。57字符时请求取消，pending后官方abort发送，最终111字符及aborted/agent_settled确认，回执cancelled。节点 `bob-pi-cancel-verification.json`。实际RPC读取终态通过，未做浏览器停止按钮真发送或Codex实际取消终验。
- 取消只针对新建Bob托管Codex/Pi请求的exact session/request/turn；旧worker不宣称支持；重复取消幂等；进程丢失保留unknown；完成竞争保留completed；外部旁观无取消权。
- X1T Codex0.167.0 已存在的 app-server：同用户进程/fd发现官方Unix WebSocket，校验握手、掩码/分片/ping、8MiB帧上限；仅在thread/loaded/list找到目标后thread/resume附着，绝不turn/start或恢复磁盘旧会话。官方机制参考 https://learn.chatgpt.com/docs/app-server 。原会话 `codex:01a1a242-5eae-7df1-b0e6-4ccf81051b4e` 从官方服务读到463字符快照，当时返回的最近文件历史页未包含其完整拼接文本。该比较不是对全部原始文件的取证，不能据此断言所有字符均未落盘。断开旁观连接重接得到基线；sent_turn=false。证据 `bob-codex-observer-verification.json`。
- 旁观事件入既有SQLite序列，request ID绑定thread+turn；断连保留正文但不虚报仍在生成。最多8个旁观租约、60秒闲置关闭自身连接，不终止外部服务。缺官方接口的独立Codex TUI、Claude/AGY仍未接旁观；全局事件保留/过期cursor机制仍未完成。
- 浏览器发现一个真实合并缺陷：官方整轮快照由多个助手消息组成，旧去重只比较单条，导致历史/实时双显。改为同一用户轮次内按顺序匹配已显示段落；新片段未入历史时仍保留实时内容，不能跨用户轮次合并。新增保护回归。
- Rust DSH27/27、Python native worker34/34、前端滚动/Phase A21/21通过；Linux release和Vite通过，大包告警保留。Tauri native_events/native_cancel仅源码接线，未编译Android或生成APK。
- 最终生产认证DSH读回：41条目录，Winter最近10条cursor5301，连续4次live、connections=1，耗时0.99/0.97/1.00/0.95秒。真实用户会话期间继续变化，不能把测试早期cursor当永久最新。
- 三节点运行程序SHA256 `43f2ed1b877a57bc7077aa8caa8e0e18b537adc60998810cd902f6cdb02e2e0b`。兼容探测仅复制一致性kernel.sqlite，不复制Vault；未轮换凭据、未改项目文件或网络配置。
- 稳定回退：网页服务根 `operator-backups/quota-device-20261106-161042-4090a20f`；X1T程序目录 `node-access-20261104-r2.before-dsh-live-1793981409`；VPS3对应 `node-access-20261104-r2.before-dsh-live-1793981532`。恢复程序/静态资源并重启Bob服务，不回滚会话数据库或重发unknown。后续网页重复发布备份只是发布链记录。

本轮最终源文件SHA256：

- `apps/bob-app/src/chat-scroll.mjs`: `f532873a3fda638845c784e2743defe81a6915948dbf651f0e16f520ee9e1b77`
- `apps/bob-app/src/components/DshSessions.vue`: `5f27842479c78979ad0ae1f86303158d9327f63bd2c632322fb29425871fa406`
- `apps/bob-app/src/components/PrimaryChatPane.vue`: `3c9e863e2432c3410896047abbd8097e5af6e76f95b13126e30359fe51669edd`
- `crates/bob-runtime/src/agy_index_probe.py`: `130e1c5125091d6c6beb2cbb1bfb41222df3f4eb1367e1bbb3080946b22facb8`
- `crates/bob-runtime/src/agy_directory.rs`: `3f290d865580532be865f6abb3cd9b2e9b14507b1c6e69e33222d7c23cab84a8`
- `crates/bob-node/src/lib.rs`: `92c5b2491e0dd965d5723c4907116ee92f67446808c953fac6e81a5a63f6d92d`
- `crates/bob-runtime/tests/test_native_stream.py`: `1ec36e2378bc97b0d6060ffc090e56411ff35d4970e972055b9cd4c0915e6790`
- `apps/bob-app/tests/phase-a-ux.test.mjs`: `c4d770a90361980dd84581e3639dd3ae6db6022097000cc13fdba70f21e110ca`


最终网页读回：index SHA256 `46b7e57b9f4fa21c9e3267fd01dee01891553f36a63eeefa2fd3dc5b8c5fda35`；注入运行标记后的HTTP SHA256 `d79f9282e40920e6a6912b3e94edba7aa16753de07e848453f8a47f9fcc973cc`；发布备份 `quota-device-20261106-163336-fdea62b6`。发布脚本本机HTTP哈希/运行程序校验通过；公网独立登录仍未验，不与私有转发浏览器混同。

最终5194真实Codex浏览器：历史/实时重复卡已消失，初始底距约0；PageUp后底距605px并显示下箭头，点击后底距约0。修复快照缩短触发浏览器滚动夹取时误开启跟随；该分支加入回归，总计21项前端检查通过。旁观期间用户在原工具继续产生新回复，观察未发送新指令。


## 2026-11-07 增量：有界事件与失效游标恢复

- 优先事件恢复，Claude额度本轮未改。源码仍基于47e5599工作树，未创建提交；APK/手机均未执行。
- Python48、前端24、Rust27项定向检查通过；网页及Linux后端构建通过。补测过期/领先游标、epoch变化、UTF-8容量、100条分页补齐、旧worker事件投影、unknown保留、双向终态崩溃窗口、迟到帧、外部旧轮次核对与旧回复不冒充新回复。
- X1T和VPS3真实SQLite一致性只读备份至隔离临时目录；分别649/410条事件，强制缩减副本至2条后恢复6/6个检查点、5347/1580字符；5/7条发送回执保留。重载模块模拟读取端进程重建，原epoch游标无重复，旧epoch恢复替换快照。源库和项目文件未被测试修改；没有新增模型请求。
- 统一5194实际RPC读取41条真实目录，首次epoch空值得到reset，后续只读更新游标，旧epoch得到替换快照。浏览器发现并修复旧终态/旧外部running追加到最新回复、旧订阅升级接力与回执已结束但终态事件缺失问题。最终浏览器及生产读回结果随后追加。
- 成果核对：DSH预览允许PNG/JPG/JPEG/WebP、MD、TXT/JSON/YAML/YML/GD/TSCN/JS/CSS/HTML；只有图片和MD渲染，HTML为源码。真实Codex Cartagena图片绝对路径点击返回DSH_FILE_PATH_REJECTED，尚未打通原生工具统一工作区文件解析。未因测试放宽文件边界。
- 仍未完成：外部Claude/AGY/独立Codex TUI未落盘订阅、所有工具审批/工具步骤/成果等全事件类型、未知原轮次补齐、Codex真实取消、跨轮监管及手机验收。部分正文和终态恢复通过，不宣称完整BP4退出。

### 本增量最终发布与真实页面复验

- 本地5194真实Codex最终文章为当前London/Lisbon进展，早期验证回执按原时间插回，不再追加到末尾；底距约0.7px，无遗留正在回复状态。外部官方内存订阅本次返回unavailable，诚实降级为文件历史；此前live证据不代表此时仍可附着。没有新发模型请求。
- 三节点程序SHA256 `b0fcdb648a772f1c793a11629cd4e49b893e438499bd0cb22fedea3ab17fbb4f`；网页index `7f10652fc0d0739bd734d73bb8cbc164455767dbc7d2bd0e179a01ede8b905df`；HTTP运行标记版本 `332f5eaab6be8977ce02b09b17ba5bbd071283349fa9d0ded5b19727c5d2e79e`。
- 主服务回退备份 `quota-device-20261106-172935-05627185`；X1T `node-access-20261104-r2.before-dsh-live-1793985971`；VPS3 `node-access-20261104-r2.before-dsh-live-1793986070`。恢复程序/网页资源后重启Bob服务，保留SQLite与回执，不重发unknown。
- 生产主服务经既有认证设备链路读回：[{"node": "x1", "directory_count": 41, "history_messages": 60, "epoch_baseline": true, "incremental": true, "snapshots": 6, "external_live": "unavailable"}, {"node": "vps3", "directory_count": 35, "history_messages": 13, "epoch_baseline": true, "incremental": true, "snapshots": 6, "external_live": "history_only"}]。这是实际生产RPC和私有通道浏览器证据，不宣称独立公网登录或手机终验。
- 最终源码 `crates/bob-runtime/src/agy_index_probe.py` SHA256 `baa2b2a696a3b1b99e5c6c143b4409e7b637f5c0f22ea6987f89fc711adb3b87`。
- 最终源码 `apps/bob-app/src/components/DshSessions.vue` SHA256 `91d282d994ce7bed0a47b8bbca8eff1ac9a7659a3f52635fa03ed9571b077891`。
- 最终源码 `crates/bob-runtime/tests/test_native_stream.py` SHA256 `0795bac46f1b07b1ec095b5ecf230c82f21441238a859a76d0e9379a99b1fbed`。


### 2026-11-07 Codex长会话实时附着上限修复（源码与远端只读验证，未发布）

真实X1T官方服务列出2个已加载会话，原验证目标仍在其中。旧接线在thread/resume返回完整历史时触发HARNESS_OUTPUT_LIMIT（8MiB帧限制），不能误诊为认证失效。现使用experimentalApi能力协商、excludeTurns元数据附着，再以thread/turns/list limit=1、desc、full读取最新轮次；保持帧上限，不回退整段历史。当前源码直接在目标运行时附着成功，身份匹配、最新1轮为inProgress；没有turn/start、没有修改项目文件。50项Python定向回归通过，包括仅附着已加载会话、最新轮次替换及不支持有界协议时不退回全历史。

部署边界：未编译/发布本次修复，未验证浏览器连续断线及手机。审批/工具/成果完整恢复和跨轮监管仍未完成。读取初始化期间通知丢失及长单轮大小边界需要继续检查。

渲染环境探测：X1T和网页宿主有pdftoppm，VPS3无；三个节点均未发现LibreOffice/soffice。此结果只是依赖盘点，不是PDF或Office渲染完成证据。用户要求的HTML网页、PDF、Office实际渲染仍须实施和逐格式验收。


### 2026-11-07 接续响应身份与HTML渲染源码增量（未发布）

- 官方双向RPC可能出现服务端审批请求ID与Bob读取请求ID相同；读取响应现要求没有method且明确包含result/error，防止把审批请求当作读取成功。52项Python回归通过；不自动批准，也不宣称审批恢复完成。建立连接期间的通知缓冲及快照去重仍未实现。
- 现有DshFilePreview弹窗接入HTML srcdoc iframe，sandbox仅allow-scripts、无allow-same-origin；CSP禁网络连接、表单、子框架、插件及base覆盖，允许页面内CSS/脚本和data/blob图片。下载仍保留原始文件。2项渲染策略检查通过，Vite构建通过（保留既有大包警告）。
- 尚未做本增量浏览器渲染/隔离实测，不能把字符串检查当浏览器安全验收。外部CSS/JS及同目录资源包、原生Harness文件读取、PDF和Office仍待完成；HTML当前仅自包含页面路径，不能标记完整网页成果完成。统一5194仍是上个候选，本轮只读检查未更新它；生产未发布、无APK。


### 2026-11-07 原生会话成果读取与本地HTML浏览器验证（候选，未发布）

节点两个RPC执行入口均新增native preview路由，按已有宿主grant校验harness；源端仅允许原会话助手明确引用的文件，绝对路径必须位于真实会话工作区，逐目录FD+O_NOFOLLOW拒绝符号链接并核对读取前后文件状态。读取限2MiB，base64响应有界4MiB；版本为内容SHA256，可返回not_modified。当前授权引用来自最近60条有界历史，旧引用不在窗口时诚实拒绝，不能冒称全部历史成果已可读。

X1T临时目录6项Linux安全回归通过（相对/绝对、缓存、用户文字不授权、越界、符号链接、FIFO、超限）；原生流52项Python及Rust27项通过，Linux release编译成功。真实现有Codex会话只读取得PNG68920字节、HTML1347字节、Markdown4137字节；另一个大PNG被2MiB预算拒绝。没有新模型请求或项目文件修改。

候选binary SHA256 dcef0ed0d55a50778df7b25af2550bc5a5821a6d34a538dc98bb5ae10dbcd64b。已更新既有5194私有候选（没有另开预览入口）。浏览器打开真实Side-by-side review page，Detail comparison/London quarter/Clipper标题和原CSS版式渲染；原生PNG缩略图也已显示。HTML内相对图片仍损坏，因页面资源包未接线，不判完整网页预览通过。PDF/Office、HTML资源包、图片超限/缩略图及全事件恢复仍未完成；生产未发布、无APK、手机暂停。

### HTML resource bundle browser acceptance — 2026-11-07

- Private unified localhost candidate only; no production or APK change. Candidate SHA256 `2d191cc3c0084f79183bb7251251a15b955f3962ad0fa8d8f9a64752e102cd93`, web `index-BT5T1Rhd.js`.
- Real X1T Codex HTML comparison artifact rendered with all seven embedded images. Browser image natural dimensions: 1280x720, 1280x800, four 1152x648, 1280x800; each complete with nonzero dimensions. Visual screenshot checked layout and images.
- Found and fixed persisted preview bypassing revalidation: dialog shows cached copy immediately, checks latest resource version, then replaces displayed content; failed refresh retains prior verified copy. No cache clearing used to achieve acceptance.
- Resource bundle confines normalized relative paths to workspace, rejects symlinks, limits count and total bytes, and creates bounded image previews. HTML uses isolated iframe and restrictive resource policy.
- Scope limits: not arbitrary website packaging (nested CSS/module/font dependencies remain); PDF/Office rendering and complete event recovery remain open. This browser evidence does not prove phone behavior or adversarial HTML isolation.

### Document page rendering implementation — 2026-11-07

- Added native artifact PDF/Office format dispatch, local PDF page rasterization, page count and original-byte download in the shared preview component. Explicit limits: 2 MiB original/read envelope budget, 30 pages maximum, bounded converter time; excessive documents fail explicitly, not silently truncate.
- X1T temporary two-page PDF exercised real pdfinfo/pdftoppm: both JPEG pages produced, original bytes unchanged; 11 POSIX preview tests passed including malformed PDF, missing renderer, byte cap, workspace boundaries and unchanged-version conversion avoidance. This is a generated fixture through the actual native preview function, NOT yet a real-session browser PDF acceptance.
- Office conversion requires LibreOffice plus bubblewrap; private profile, network namespace and restricted filesystem mounts are wired. No package installed. Installation authorization requested; actual Office conversion/security acceptance remains pending.
- Frontend build passed. 27 Rust targeted tests and candidate build passed before final unchanged-document optimization; latest Python source tested separately. Private browser still runs prior HTML candidate; no production change, no APK.
- Remaining: candidate build with latest cache optimization, real session PDF/browser verification, DSH document path parity, Office install/real formats, larger document paging, renderer resource hardening and full event recovery matrix. Whole BP4-06 remains in progress.

### Codex control-wait source event preservation — 2026-11-07

- `_control_result` now invokes a source-event sink for notifications received while waiting for an RPC reply. Approval requests cannot be mistaken for replies even when numeric IDs coincide. Sink failure propagates rather than silently discarding events.
- Both attach-time and steady-state Codex observer paths persist bounded `source_event` payloads under exact thread/turn identity. Missing turn identity is not guessed; other-thread events are excluded. Snapshot-overlapping text deltas are not appended a second time by this capture path. Read-only observer never answers approval requests.
- 55 Python stream tests passed, including new interleaved notification/approval persistence, identity/size rejection and sink-failure propagation. A test SQLite handle cleanup failure on Windows was corrected and the full suite rerun successfully.
- Real X1T already-running Codex original session: read-only attach succeeded, latest_count=1. During a 12-second observation, 108 notifications arrived and 104 exact-turn notifications were persisted in a temporary verification journal. Methods included agentMessage delta, item start/completed, autoApprovalReview start and moderation metadata; session-less account/config/remoteControl notifications were not attributed to a guessed turn. No new turn or model prompt was started; no project files changed.
- Remaining: `source_event` is preserved in bounded journal, but rich UI event projection and retention/reset snapshots for tool/approval/artifact state are NOT yet complete. No claim of lossless all-event recovery, approval handling or physical-network fault acceptance. Source increment not deployed; phone/APK unchanged.

### Tool/approval recovery projection — 2026-11-07

- Native source events now additionally project bounded activity snapshots (64 tool records, 32 approval observations, explicit partial flag). Event pruning preserves these snapshots. Item identity replaces updates, completed/error tools cannot reopen from a delayed start; unknown/terminal execution marks still-running tools and pending approvals unconfirmed rather than fabricating completion or approval.
- Codex latest-turn bootstrap restores confirmed tool statuses and preserves approvals received while attaching. Shared conversation component consumes both live activity_snapshot and reset activity; compact expandable execution records are bilingual and theme-aware. Approval is observation only and directs the user to the source tool; no Bob approval action implemented.
- 58 stream tests pass including pruning/reset, bounds, delayed starts and bootstrap/approval merge. Frontend build passes. Real X1T loaded original session restored 5 completed tools from its authoritative latest turn; 12-second observation had no new turn-scoped tool activity, so this does not prove fresh live UI transitions. No new prompt or project changes.
- User chose not to install Office converter/isolation dependencies; no installation. Continue independent work, retain Office rendering acceptance as open.
- Browser acceptance of new activity component pending at this checkpoint; no production publication or APK. Full cross-tool approval/cancellation/retention matrix and PDF/Office/large-document completion remain open.

### Activity component real browser acceptance — 2026-11-07

- Latest private candidate SHA256 `f2d536484940f74ff160d5e991814346e3be52c9d82af89abb3eecbf7415fbae`, frontend `index-D1eQN96k.js`; 27 Rust targeted tests and release candidate build passed. Updated the existing localhost5194 tunnel only.
- Initial browser observation showed no new activity; checked candidate executable hash, local grant route, observer source capability flags and SQLite event kinds rather than assuming success. Subsequent real original-session activity produced 59 source_event and 2 activity_snapshot records at the inspected checkpoint.
- Browser displayed an expandable execution record for a real X1T shell command with completed status. Expanded text and screenshot confirmed the command/status and fixed bottom composer. No simulated frontend data or model request was used. This proves one real tool event reaching the shared UI, not approval interaction or all harness event parity.
- Bootstrap with pre-existing terminal snapshots still needs broader restart verification; source event retention and approval decision APIs remain partial. Office dependencies remain uninstalled by user instruction. No production publication, no APK.

### Terminal activity backfill and approval closure — 2026-11-07

- Fixed terminal execution snapshots rejecting activity backfill. Activity can be restored without reopening a completed/cancelled/failed execution; pending approvals and running tools in terminal/backfill state remain unconfirmed unless separately confirmed.
- Frontend transport failure and external observer loss now invalidate running tool/pending approval observations while preserving completed/error records; cached objects are not mutated. Shared helper has direct regression coverage.
- Reviewed local official Codex app-server README and protocol reference: `serverRequest/resolved` has threadId/requestId and means resolved OR lifecycle-cleared, not approved. Added exact typed request ID plus per-observer identity matching; ambiguity or a different socket never resolves a guessed latest turn. UI labels closure as Request closed, not approved.
- 61 Python stream tests and 1 frontend activity-state test passed; frontend candidate built. New tests cover terminal backfill, disconnect semantics, numeric/string request distinction, other socket, duplicate closure and ambiguous matches.
- This increment is source/test verified; current localhost5194 still runs prior tool-display candidate. No live approval was induced or answered, and no claim of actual approval-action acceptance. Office installation remains deferred by user, production/APK unchanged. Cross-harness events, cancellation/full restart matrix, and document end-to-end acceptance remain open.

### Bounded older-turn reconciliation — 2026-11-07

- Fixed frontend settled-reply dedup deleting separate activity evidence. Settled text is removed from the draft, while tool/approval evidence remains; late activity snapshots can still arrive without reopening a completed execution. Two frontend activity tests pass.
- Official local Codex protocol `ThreadTurnsListParams/Response` documents summary item view and nextCursor. Observer now checks outstanding exact old turn IDs using at most three 20-turn summary pages. It never falls back to full history, guesses the latest turn, or resends a prompt. Missing records stay unknown. Summary-only confirmation preserves previously saved reply text.
- 63 stream regressions pass, covering pagination, cursor use, request bounds, no-result unknown and body preservation.
- X1T real loaded original session provided an actual older completed turn through official summary API. In a separate temporary verification journal, its unknown checkpoint recovered to completed with sentinel body preserved. Current full turn count remained one; no new turn started and no production receipt/project changed. This is real source-state reconciliation against an isolated checkpoint, not proof of an actual interrupted send.
- Source increment not yet installed into localhost candidate. Full old sends lacking any native turn identity still cannot be guessed; external Claude/AGY, approval actions, real cancellation/physical-disconnect matrix, and document preview acceptance remain open. No Office installation, production publish or APK.

### Private candidate recovery browser verification — 2026-11-07

- Candidate SHA256: `9d5145c05bfdd9e5a24d2a8c1ccf453195f2aae5407613da7755580f95d6313d`; frontend `index-DtbGyfQo.js`. Rust 27 targeted tests and release completed; frontend build completed. Installed only into the existing private candidate and unified localhost5194 tunnel.
- Browser reload of the real original Codex session recovered two activity summaries (1 and 12 real tool records). Switching to Schedule and back retained both summaries and current replying status. No prompt, approval or project-file mutation was performed.
- This proves browser reload/re-entry projection, not physical network interruption, confirmed cancellation or all-harness acceptance. No production publish, APK or Office dependency installation.

### DSH document transport parity — 2026-11-07

- DSH preview now accepts PDF and Office document extensions after the existing server-history reference authorization and workspace stat checks. Read-before/after version checks and original-byte checksum remain intact. Document bytes go to the existing bounded renderer through stdin, not command-line arguments; no new dependency installed.
- Converted document versions are namespaced separately; unchanged version/size returns not_modified before conversion. Shared original+render budget remains 2MiB; max30pages and renderer/sandbox missing errors remain explicit. Rendering runs on the Bob host serving this transport; a host lacking the converter cannot claim preview support.
- 30 Rust targeted tests passed on the private build host. Both pdfinfo and pdftoppm paths were separately verified present, so the real PDF bridge test ran rather than skipping: valid PDF produced JPEG page bytes, malformed input returned render failure; unsupported type and excessive bytes were rejected before launching the worker. Candidate release build passed before test-only additions.
- Read-only inspection of 60 recent assistant/user messages in the existing X1T Codex session found no PDF/Office links. Original-session browser document acceptance remains open. Office dependencies remain explicitly deferred by user. No prompt, project mutation, production publication or APK.

- Verified candidate binary SHA256 5d204a899a18b7a176fdc67fdbb70c322ffb6950160d53e6c89c4779d0e650d installed into existing private localhost5194 candidate; production unchanged.

### Native cancellation races and real socket reattach — 2026-11-07

- Fixed native Codex worker dispatch: server requests with IDs 1/2/3 no longer impersonate initialize/resume/start responses. Read-only verification still rejects tool approval requests; no approval authority broadened.
- Pi now rechecks cancellation after identity response and before prompt dispatch. Confirmed cancellation in this window records not_dispatched; no model prompt is sent. Added regression cases; 65 Python stream tests pass. Existing tests still require exact interrupted terminal event for Codex cancellation; interrupt acknowledgement alone remains unknown.
- 30 Rust targeted checks and candidate release build passed.
- X1T original loaded Codex turn was actually inProgress. Closed the verification socket, reattached through the official API and confirmed the same turn was still inProgress. In an isolated temporary journal, applied the production recovery projection: unknown retained prior text; restored running snapshot equalled official current text, one snapshot only, two tool records. No prompt/interrupt sent and no production journal/project modified.
- Scope: real connection reattach plus isolated checkpoint projection, not full observer-process restart, physical network interruption, actual cancellation or all-harness acceptance. Office installation remains deferred; no production/APK.

### Real Codex cancellation and fresh-reader terminal recovery — 2026-11-07

- Used a dedicated verification session in an empty private temporary workspace, never the user's running project session. Existing Bob Codex session contained unresolved receipts, so it was excluded. Native worker used read-only policy with approval denied; prompt explicitly prohibited tool/file access.
- Failed setup evidence: an empty new thread was not resumable after closing its creating server; worker returned HARNESS_RESUME_REJECTED before turn/start. A bootstrap probe then used an incorrectly short blocking receive timeout; its pipe closed and log showed turn_aborted. The next test reused that now-persisted dedicated session only after official thread/read confirmed a terminal turn. No unknown request was retried.
- Actual cancellation: worker accepted/resumed the dedicated session, acquired exact native turn ID, emitted 47 text characters; cancel API returned cancel_requested. Pending event and dispatch timestamp observed. Terminal event cancelled, backed by official interrupted. In-flight output reached 526 characters before terminal.
- A separate fresh reader process checked exact official thread/turn and persisted journal: official status interrupted; receipt cancelled; reset snapshot cancelled; cancel_pending false; partial body retained and matched receipt. Verification work directory contained 0 files. Node-local sanitized proof: bob-codex-cancel-verification.json.
- This closes real Codex backend cancellation plus fresh-reader recovery, not browser Stop-button acceptance, physical network outage, full node reboot or all-harness cancellation. User project tasks were not interrupted. No production change, APK or Office installation.

### Browser Stop and cancelled reply history fix — 2026-11-07

- On unified localhost5194, selected the dedicated cancel verification workspace/session. Sent one explicit read-only no-tools prompt through the actual composer. Stop reply appeared; clicked it, button disabled during request; page then showed 已停止回复 and returned to Send. No user project task was selected.
- Browser exposed a real defect: an older cancelled partial reply disappeared on the subsequent turn/reload, because receipt-to-history merging admitted completed only while frontend correctly removed settled drafts. Fixed backend to merge cancelled partial text at its original completion time with existing history dedup. Full native turn_aborted/codex_internal_context envelopes are hidden as control metadata; ordinary text mentioning tags stays visible.
- 66 stream regressions and 30 Rust targeted checks passed, release built. Candidate SHA256 a3fe835ec991693364eec1d747165e8f8cd0737b8c4abd3c6607f589e7efd870 installed privately.
- Actual browser reload showed the original partial assistant reply between its user prompt and the later verification prompt. No internal interruption messages remained. Schedule → Chat returned with exactly one terminal partial paragraph and zero Stop buttons. No new send was needed for this recovery check.
- Pending: full physical-network/node-reboot matrix, other harness live sources/approval actions, and complete document acceptance. No production/APK/Office installation.

### PDF real conversation-to-preview end-to-end — 2026-11-07

- Created a deliberate two-page PDF fixture only in the dedicated cancel verification workspace. Original user project files were untouched. `preview-check.pdf`: 1118 bytes, SHA256 `5c44d60748e53185bc9f02a9cf2338e038e63c650508bc223ed1a8da25e828c1`. It contains distinct page titles and blue/green rectangles.
- Sent one no-tools/no-file-access prompt in the actual existing verification session through the unified web composer, asking for the exact Markdown reference. Waited for the same request; node receipt confirmed completed. Assistant reply supplied the reference, so existing server-history authorization remained intact. No fake grant or fabricated history entry was used.
- Clicked the actual reply link. Existing file-preview dialog rendered figures 1/2 and 2/2; both images complete with natural dimensions 906×1280. Visual screenshot inspection showed the first page's title, blue shape and footer in correct PDF layout. Returned through Back to conversation, reopened the same link; both page figures present again and original download link remained available.
- Acceptance scope: basic PDF via real native conversation, actual node filesystem, converter, authenticated transport and existing browser dialog. Fixture is explicit verification data, not an existing user-project document. Does not establish complex/encrypted/large PDFs, DSH end-to-end parity or Office rendering. No new dependencies, publication or APK.

### Private node service restart during verification request — 2026-11-07

- Through unified localhost5194 composer, sent one read-only/no-tools verification prompt in the dedicated temporary workspace session. Before restart, authoritative receipt was running with an acknowledged native turn; worker PID command line contained the exact request ID. Exactly one matching request existed.
- Restarted only the existing private bob-node candidate using its executable/PID guard. Production service and original user Harness processes were not restarted. After restart, candidate PID changed; recorded request ID, native turn ID and worker PID identity remained unchanged, request completed with 950 characters, one matching request. The worker had exited normally by the after-check.
- This observes running-before/completed-after across candidate restart; it does not assert the exact millisecond the turn completed relative to candidate termination. Node-local sanitized proof: bob-candidate-restart-verification.json.
- Browser reload recovered the complete reply. Exact final paragraph 重启恢复验证结束 occurred once, Stop buttons count zero. No prompt retransmission or duplicated request.
- Scope is private node HTTP service restart and browser reattachment. Full machine reboot/power loss, worker crash, other harnesses and full event matrix remain separate unverified gates. No production publish/APK/Office installation.

### Dedicated Codex worker crash recovery — 2026-11-07

- Dedicated read-only verification session only: after native turn acknowledgement, checked the worker command line for exact request/session/source identity, then terminated that worker alone. No production service or user project process was targeted.
- Receipt and replacement snapshot became unknown after process-loss detection. Same-ID retry returned idempotently with Popen guarded and never invoked; a different request in the unresolved session was rejected with HARNESS_SESSION_BUSY.
- Official exact-turn reconciliation recovered cancelled with the same native turn. Node-local proof bob-worker-crash-verification.json records stage=reconciled and all three guards true. A later read independently confirmed this proof.
- Initial test wrongly expected idempotent repeat to throw and reported unexpected replay allowed. Inspection confirmed the existing-ID branch returns without spawn; the corrected guarded check passed. This was a test assertion error, not observed duplicate execution. Re-running reconciliation after success hit its expected unknown precondition; no new prompt was sent.
- This validates dedicated Codex worker loss and authoritative terminal recovery, not machine power loss, all Harnesses, or complete event recovery. Office dependency installation remains deferred by the user. No production publish/APK/phone testing.

### Confirmed-turn supervision outlives startup timeout — 2026-11-07

- Found the native worker unconditionally ended its loop after 180 seconds and closed the owned process, even after a native turn had been acknowledged. Replaced this with a startup-only deadline; Codex requires an exact acknowledged turn, other adapters require the existing session identity check. Explicit native terminal/cancel/process failure remains authoritative.
- A bounded reader queue could lose its EOF sentinel when full. Added a reader completion signal checked after queued frames drain; a dead reader is no longer mistaken for endless quiet output.
- 69 Python regressions pass, including late completion with elapsed-clock injection, missing turn ACK retaining unknown, and drained EOF without sentinel. 30 Rust targeted tests and private release build pass.
- X1T isolated protocol fixture used a real subprocess and pipe, delayed its result by 185 seconds after the native-turn-shaped acknowledgement, and completed. No model/API was invoked and no user session or project was touched. This is a real process/pipe duration check, not proof that every official Harness survives an equally long generation.
- Installed the verified build in the existing private candidate behind localhost5194. No production deployment, APK, mobile test, or deferred Office installation. Task-specific budgets and full long-running Harness/fault acceptance remain open.

### Bounded official receipt reconciliation — 2026-11-07

- Replaced includeTurns=true recovery with metadata-only thread/read, up to three 20-turn summary pages and exact-turn thread/items/list (20 items/page, up to 16 pages, 1 MiB item aggregate, 20-second traversal budget checked before requests; each control response retains its separate 8-second bound). No prompt/resume is sent.
- Initial real check exposed installed/reference version mismatch: the local reference offered thread/turns/items/list, while X1T rejected it. Generated the installed binary's official JSON schema in a disposable temporary directory. Its thread/items/list returns ThreadItemEntry with mandatory turnId and nested item. Updated adaptation and rejected any item with a different turnId. No unsupported full-history fallback.
- Real read-only checks used isolated temporary receipt databases and existing dedicated verification turns. Cancelled turn recovered cancelled; completed turn recovered completed with exactly 950 characters matching the persisted reply. Both retained original native turn IDs. Trace: initialize, initialized, thread/read, thread/turns/list, thread/items/list. No original receipt changes or new messages.
- 73 Python regressions pass, covering exact identity, wrong-turn entries, cursor loops, exhausted page/byte bounds and single terminal recovery. 30 Rust targeted tests and private release build pass; updated existing localhost5194 candidate only.
- Older turns beyond the summary budget, oversized item sets, unsupported versions, and live external tools remain explicit incomplete gates. Office installation deferred. No production/APK/mobile acceptance claim.

### Original request recovery includes source activity — 2026-11-07

- The original-request reconciliation transaction previously restored only text and final status. It now projects source-confirmed tool activity from the exact returned turn into the same request snapshot before writing terminal state. Existing approval observations are retained and become unconfirmed, never implicitly approved.
- Shared activity projection also serves external observer bootstrap. An explicit inProgress item stays unconfirmed when a turn is terminal; turn completion no longer overrides this explicit item status. Confirmed failed/completed items retain their separate outcomes.
- X1T existing original Codex session: read official bounded summaries/items without any prompt/resume/execution. In a disposable isolated receipt DB, an unknown receipt referencing that existing exact turn recovered completed, 1349 text characters, 11 activity records of commandExecution/imageView, with completed/error states. No original request or project files were modified. This is actual official-source data with an isolated recovery receipt, not a claim that an original user request failed during this check.
- 74 Python event regressions and 30 Rust targeted checks pass; private release built and installed behind the existing localhost5194 endpoint. No frontend changes in this increment; broader browser event-type, approval-action and failure-matrix acceptance remains open.
- No production deployment, APK, mobile test, or deferred Office component installation.

### Document preview deadline and forwarded errors — 2026-11-07

- Native preview's 35-second host deadline was shorter than Office conversion (45s), PDF metadata (10s) and rasterization (40s). Document paths now get a 110-second host budget; ordinary image/text remain 35s. Document RPC relay gets 240s to also cover DSH history/stat/download/re-stat; connection establishment is bounded at 10s. This does not override reverse-proxy limits and is not an asynchronous render job implementation.
- Preview forwarding previously discarded all non-success HTTP reasons. It now retains a fixed whitelist of document/path/file error codes on HTTP errors or ok=false, maps request timeout to DSH_FILE_TIMEOUT, and keeps unrecognized upstream details private. Other RPC behavior remains unchanged.
- 31 Rust targeted checks passed, including real local HTTP 400 forwarding for missing renderer, conversion timeout and unknown private details. Existing real PDF bridge check passed. Private release SHA256 024714de71f96f17b42abd2a158f6a5bf8f17fb2cbbee1472672245ac264abb4 installed behind localhost5194.
- Browser opened the existing dedicated-session PDF link: both rendered pages complete, 906x1280 pixels; return-to-conversation restored the composer. This reopen may reuse preview cache; it is not evidence of a slow uncached conversion or public-network deadline acceptance.
- Office installation deferred; Word/Excel/PowerPoint rendering, slow public proxy paths and asynchronous conversion remain pending. No production/APK/phone changes.

### Installed external CLI interfaces and Claude ownership guard — 2026-11-07

- Read-only X1T CLI help/version check: Claude 2.1.319 exposes agents --json listing active interactive/background sessions, logs for terminal text, attach for interactive terminal attachment. AGY 1.4.12 exposes stream-json input/output, --conversation, --remote-control and daemon start/status/stop. No daemon or session was started by these checks.
- Real Claude agents JSON returned one session with pid/cwd/kind/startedAt/sessionId/name/status fields, status idle. No names, conversation contents or credentials were exported into this evidence.
- Implemented pre-resume official active inventory check. Exact session identity blocks another process even if idle; command failure, timeout or unrecognized schema fails closed as activity unconfirmed. In an isolated temporary receipt database, the actual X1T inventory caused HARNESS_SESSION_BUSY and zero NativePipe execution launches. Original session/project untouched; no prompt sent.
- 76 Python regressions and 31 Rust targeted tests passed; private candidate updated. This is duplicate-owner prevention, not external live subscription, approval support, cross-process atomic locking, or official remote-control protocol integration. Remaining work retains those gates.
- No production publish, APK, phone test, or deferred Office dependency installation.

### Owned Codex event capture and failed real tool gate — 2026-11-07

- Source now uses the original request and exact acknowledged turn for owned Codex source-event capture. Notifications preceding the acknowledgement are bounded and replayed after identity binding; unrelated turns are excluded. Protocol regression suite: 78 passed.
- Read back the existing dedicated X1T verification receipt rather than sending the prompt again. It completed with 47 response characters and unchanged workspace file hashes. The event journal contains turn start/end, user and agent item start/end, 32 agent message deltas and token-usage notifications.
- The response reported a terminal environment permission failure. No commandExecution item was emitted or captured (tool_count=0). Consequently the real tool-event gate FAILED; this is not proof of live tool recovery. The failure must be investigated against the official source event stream before another acceptance attempt. Permissions were not broadened and the request was not replayed.
- Prior candidate build completion output was unavailable after handoff; no active cargo build/test process was found on the builder. Neither fact proves build success. This increment is not claimed installed in the private browser candidate or production.
- Office converter installation remains explicitly deferred by the user. No APK or phone test.

### Pre-ack approval observation and exact-turn history merge — 2026-11-07

- Read the dedicated original turn through official bounded items API: userMessage and two agentMessage entries only. The absence of commandExecution matches the stream journal, while rollout-derived UI history contains a tool invocation marker. Invocation is not proof of successful execution.
- Fixed server approval requests arriving before turn/start acknowledgement: bounded observation buffering, exact-turn binding and one response only. A replayed observation never sends the server response again. Cross-turn approval observations are rejected; permission policy is unchanged. This is observation preservation, not an approval action implementation.
- Candidate browser exposed duplicate text when native rollout had two assistant items and the durable receipt held their combined text. Added exact native turn text aggregation using turn_context/task_started and terminal boundaries. Suppress a receipt only when the exact-turn complete aggregate equals it; other-turn combined text is retained. No fuzzy text matching added.
- 81 Python regressions and 31 Rust targeted checks passed. No production publication, APK, phone test or Office dependency installation.

- Installed private candidate SHA256 4c0d34a4fbe0145696f54e59b1ca58b0251c62f637fb962044d97d505698fe03. Unified 5194 browser reload restored the dedicated real session: both assistant paragraphs appear once, the tool invocation marker remains, and the composer is present. No new prompt was sent during this verification.

### Tool invocation versus returned evidence — 2026-11-07

- Read only the dedicated original turn's rollout shape/output. A custom_tool_call_output returned an input_text list containing bwrap loopback RTM_NEWADDR Operation not permitted. Official bounded turn items omit this tool wrapper, so their lack of commandExecution does not imply there was no invocation or returned failure evidence.
- Codex history now joins output by native turn and call_id, consumes each matched call once, clears associations at turn boundaries, limits pending calls to 256 and displayed output to 600 characters. Returned evidence is kept as a process step, not treated as proof of success. Backticks in returned output cannot terminate its enclosing process fence.
- Process summary uses bilingual translation keys and neutral call counts. Returned-output rows do not increment invocation count. No new hardcoded UI colors or emoji.
- 82 Python tests, 31 Rust targeted checks and the relevant frontend process parsing/count test passed. Vite and private node builds passed. Candidate binary SHA256: 64dfd17558cc76e198327a9095667f3ef4cc53f04883c880f2f56c80385c834e; web bundle index-BuJ0qgFo.js.
- Unified 5194 browser reload of the real dedicated session displays one command call; expanding it shows the original invocation plus the actual permission-error output. Assistant body remains unduplicated and composer is present. No prompt was sent, permissions loosened or project files modified for this readback.
- This does not complete live external output subscription, structured tool status across all harnesses, approval actions or the full fault matrix. Office installation remains deferred. No production deployment/APK/phone test.

### AGY ACP installed capability and real history handshake — 2026-11-07

- Official documentation checked: https://antigravity.google/docs/cli/headless/ ; https://antigravity.google/docs/ide/extensions/acp ; https://agentclientprotocol.com/protocol/v1/session-setup . Local root help omits ACP but `agy acp --help` succeeds.
- X1T actual `agy acp --stdio` initialize returns protocolVersion 1, agentInfo Antigravity ACP Server version 1.4.12 and session list/load/resume/close capabilities. Authentication methods are advertised; none was invoked or changed. The child process was closed after each read-only probe.
- Actual session/list returned 14 entries with sessionId/cwd/title/updatedAt fields. Exact previously verified session load returned sessionId/models/configOptions/_meta and 148 session/update notifications. Observed categories: user_message_chunk, agent_message_chunk, agent_thought_chunk, tool_call. No prompt or project-file write was requested.
- A subsequent independent resume probe returned the same requested sessionId and again 148 updates. This contradicts the documented no-history-replay expectation for this installed version. Adapter work must account for replay; do not use resume on every browser poll or treat initialization updates as fresh turn output.
- This is not evidence of live subscription to a separate running CLI, safe concurrent ownership, cancellation, approval, sustained session process reuse, or completed AGY integration. Existing private and production sending paths were not switched. No daemon installation/start, APK, phone test or Office installation.

### AGY actual ACP prompt wiring and same-process two-turn check — 2026-11-07

- Source worker now selects official `agy acp --stdio`, negotiates loadSession, consumes initialization replay before creating prompt events, and binds updates to the requested session. It persists raw source notifications and assistant text chunks; user echoes and thought chunks do not become assistant text. Server request IDs are not mistaken for prompt responses.
- Permissions are denied, not implicitly granted. `session/cancel` only records dispatch intent; cancelled requires the prompt result's stopReason. Lost process retains partial output as unknown; no automatic prompt replay. Unsupported stop reasons do not become success. The obsolete headless AGY parser branch was removed.
- `python -B crates/bob-runtime/tests/test_native_stream.py`: 85 tests passed in 12.617s, including replay exclusion, foreign-session exclusion, lost prompt no-replay, permission/cancel ID collision, terminal cancellation, and two prompts sharing one initialized connection. These cancellation and disconnect cases are protocol fixtures, not actual AGY fault acceptance.
- X1T existing AGY session `0a4ac253-9ab5-49cf-861e-90b3d39debc8`: same ACP PID 99033 completed requests `bob-acp-readonly-4cdea518-1a43-4c86-99b8-faa53dd0ee48` and `bob-acp-readonly-fd02e956-5080-4b33-9046-0782cbdcf48b`. Exact replies were `ACP connection verified 1` and `ACP connection verified 2`; each emitted 2 text deltas. Observed updates: user_message_chunk, agent_message_chunk; first turn also session_info_update/available_commands_update. No tool call updates observed. Prompts explicitly prohibited tools, file access and commands. No claim of full workspace fingerprint validation. Connection closed cleanly after the second turn.
- These real prompts invoke the connection class used by the source worker; the installed web binary has not been rebuilt. The request launcher still creates one worker per request, so cross-request process persistence is NOT complete. Next: session-scoped supervision/queue, worker identity checks, owner exclusion, real cancellation/disconnect/restart matrix. External independently running CLI subscription remains unproved. No production publication, APK, phone testing or deferred Office dependency installation.

### AGY persistent queue on actual request entry — 2026-11-07

- `_native_start` now routes AGY through an SQLite queue and per-session supervisor. A random supervisor identity is stored with the receipt and verified against its process command line. One live version owns the session; a different code version cannot silently compete. Process launch occurs before transaction commit so a launch failure does not leave a false accepted receipt.
- The supervisor serially consumes accepted requests, reuses one `AgyAcpSession`, and removes delivered queue records. Lost accepted/running requests become unknown and block a different request. The same request ID does not cause a new dispatch. Idle shutdown after 300 seconds closes the connection before releasing the registry under one transaction. Confirmed terminal receipts survive later persistence exceptions.
- 91 native stream tests passed in 13.098s. New checks cover owner reuse, version mismatch, spawn rollback, dead-owner unknown barrier, queue consumption/idle shutdown, and post-terminal write error. Existing replay/cancel/identity tests remain green.
- Real X1T original session verification: requests `bob-acp-queue-d683ead5-5c62-4ac3-943d-d04ce810bb85` and `bob-acp-queue-fd1d0c5a-be3a-485c-95f6-47de312eff3d` were submitted independently through `_native_start`. Both completed with `Queue verified 1` / `Queue verified 2`. Both receipts identify supervisor PID99866; ACP request IDs are 3 and 4. No tool call updates observed. Prompts prohibit tools, file access and commands. This verifies the actual source request entry; it is not browser acceptance or a full workspace hash audit.
- The real queue run preceded the final post-terminal-write-error guard; that guard is covered by the new regression. Production remains unchanged. External independent CLI writer exclusion, real cancellation/transport interruption and machine restart, general activity UI projection, and Office rendering acceptance remain open.

- Subsequent private candidate build: 31 targeted Rust DSH tests passed, offline locked release build succeeded. Downloaded binary SHA256 `d92f9ee01d71be0c42c4419d04a136cac15f0d3187d0e86f42cad3f3a64c5214` matched the build host. Replaced only the X1T private 9531 candidate, reused unified localhost5194 tunnel and unchanged web bundle. Candidate startup script confirmed the new process stays running. Browser interaction acceptance is still pending; this is not a production release.

### AGY browser cancellation and explicit negative receipt — 2026-11-07

- Actual localhost5194 browser exposed an upgrade race: an older live supervisor refused the new source before queueing, but the generic transport error led to an unknown UI state. Fix: persist a same-ID/content `not_accepted` record for this refusal, return it from start/receipt, and recognize only identity-bound negative receipts in the UI. Retain the draft and allow manual retry, never auto-send. Absence from the ledger/history still means unknown. Existing pre-fix pending UI was manually unlocked for a distinct verification prompt; no failed prompt was automatically replayed.
- Real browser also exposed two old gates: Rust native_cancel and the frontend cancellable selector only allowed Codex/Pi. Both now include AGY while still requiring source-reported cancel_supported and a Bob-owned request.
- First browser-origin request `571f535f-11cb-45ac-a7ad-8d02c136617b` was accepted through the installed backend, produced partial text, then cancelled through the exact host cancellation function while the UI gate was being repaired. Official `acp_stop_reason` confirmed cancelled; partial text and terminal survived private backend restart.
- Second browser-origin request `35817582-2bec-4a57-a631-0a2edf38524d`: visible “停止回复” clicked in localhost5194; host receipt cancelled with cancel_confirmed_by=acp_stop_reason. Browser then showed status “已停止回复”, removed the generation/stop state and restored the composer. Both runs used supervisor PID103144, ACP request IDs3/4. Prompts prohibited all tools/file access/modification. No full workspace hash audit claimed.
- 91 Python regressions passed (12.920s), negative-receipt JS test passed, 31 targeted Rust tests and offline release/frontend builds passed. Final private binary SHA256 `a1c0959e38243ca4a3bd83fbe4770f0c283f39f135bab6665331f5784ee3539c`; web bundle `index-CznnPGtU.js`. Only private9531 and unified5194 updated. Negative-receipt rendering has targeted automated coverage; the exact version-conflict UI branch was not re-injected after the fix.
- Not complete: transport-loss/reconnect and machine-restart matrix, safe ownership of externally running CLI sessions, general tool/approval activity projection, Claude external live subscription and deferred Office rendering. No production deployment/APK/phone test/Office package installation.

### AGY generation survives private ingress restart and browser reload — 2026-11-07

- Preflight attempt `bob-agy-restart-2c8976eb-c02d-41b5-8104-1cc5c9d4d01c` returned a durable not_accepted / HARNESS_SUPERVISOR_VERSION_BUSY receipt and was never queued. Cause: direct Python test supplied uncompressed source, while installed Rust uses compact_python, producing a different supervision version. Verification script was corrected to use exactly the bridge's compression; the version guard was not weakened. This supplies real host-side negative-receipt evidence, not UI branch acceptance.
- Actual request `bob-agy-restart-50e1da76-1c5a-4390-8ec6-29d5c8ca77ba`, original AGY session `0a4ac253-9ab5-49cf-861e-90b3d39debc8`, explicitly prohibited tools and all project-file operations.
- After 6 partial characters, the private backend executable identity was verified, PID104193 stopped and new private backend PID106007 started on the same9531 data/static directory. The AGY supervisor identity remained alive and unchanged. Production service was never restarted.
- Completion: 2170 characters, expected ending “AGY重启恢复验证结束”, same supervisor, one accepted event and one running event. Resume from pre-restart cursor returned64 events with reset=false. No duplicate prompt dispatch observed. Sanitized host proof saved at the node's Bob runtime evidence location.
- Real localhost5194 browser was reloaded after backend restart; it restored the selected original AGY session and displayed the complete response and end marker, with composer visible and generation no longer active. It did not issue a replacement prompt.
- Scope: private ingress service restart plus browser reload. This does NOT prove recovery after AGY child death, supervisor death, full-machine outage, or external independent CLI writer/stream attachment. No new build/source change was needed this turn; current private candidate remains the prior a1c0959... binary and index-CznnPGtU.js. No production deployment/APK/phone or Office installation.

### AGY owned child loss and unknown receipt — 2026-11-07

- Dedicated session a4b713c2-7cd0-4c07-ad26-7143a2c81859; request bob-agy-child-fault-15813328-3956-45ff-b597-b629faa4094e. Read-only prompt in an empty verification workspace, not the user's working session.
- After22 partial characters, verified supervisor ownership, child PPID/argv and process-group identity before killing only the ACP child group. Result unknown / HARNESS_PROCESS_EXITED, partial retained, one request and one accepted event, new request HARNESS_SESSION_BUSY, no workspace files created.
- Same-ID response originally misleadingly said accepted although no replay occurred. Source now returns unknown for an unknown stored execution. Fresh host readback: ack unknown, receipt unknown, one request, zero supervisors,22 partial characters. No new prompt was issued by readback.
-91 Python tests passed in12.878s. This proves fail-closed no-replay behavior, not exact native terminal-state recovery. Existing unknown barrier intentionally retained. No candidate rebuild, production release, APK, phone testing or Office installation in this increment.

### AGY activity projection — 2026-11-07

- Actual prompt receive path now atomically persists source frames and projected tool activity. Partial updates retain prior title/status; late running cannot reopen terminal tool state. Exact session filtering and load-replay exclusion remain. Limits64 items/512 title characters with partial marker.
-93 Python tests passed, including protocol-level actual worker-path snapshot persistence, wrong-session rejection, patch merging, terminal monotonicity and bounds. These are simulated transport tests, not real AGY tool/browser acceptance.
- Pending: real tool event verification and browser rendering, private candidate rebuild, approval interaction, external writer subscriptions, exact unknown reconciliation. No production/APK/Office installation.

### AGY real failed tool projected in browser — 2026-11-07

- Dedicated session a9a52777-1964-41dd-b3cf-0037e34e44ac, request bob-agy-tool-2249cfcc-2662-491c-918a-51fd2ab6f5aa. Read-only current-directory request, shell/filesystem mutation explicitly forbidden. Source emitted tool_call + tool_call_update with failed status: read_file on a directory.353-character final response accurately explained the inability to list; directory remained empty. This is failed-tool evidence, not successful directory access.
- Persisted activity: one read item call_2641178737632121245, label File ., status error. Real localhost5194 sidebar discovered Read-Only Integration Verification; opening it restored the final response and activity. Expanding execution record visibly showed File . and 失败, composer retained.
-31 Rust targeted tests passed; release compiled. Binary SHA2564298d4cfd01423d83e066bb9a5a225b7b13ac512421fc82db3f72c6bc52481fe downloaded and verified, installed only into private9531 candidate, existing5194 tunnel reused. No production service change/APK/phone/Office installation.
- Still open: successful real tool transition, permission interaction, independent external CLI live subscription, exact unknown reconciliation and full fault matrix.

### AGY successful read and chronological activity — 2026-11-07

- Same dedicated session a9a52777-1964-41dd-b3cf-0037e34e44ac, request bob-agy-read-2b41525f-b69a-475d-8bb8-11d167048984. A nonsensitive readonly-check.txt fixture was prepared only in Bob runtime verification workspace; the agent read it, returned correct marker/count, and did not change its SHA256. Source call_84440440584706699 had read/completed state.
- Browser reload restored original session, correct reply, and completed tool activity. It exposed reversed activity ordering from request-ID enumeration; orderedNativeActivities now sorts by observed timestamp, preserving unknown timestamps last. Three targeted JS tests passed; frontend build index-cNIIPK5J.js installed into existing private5194 candidate. Browser verified earlier File . / 失败 precedes File readonly-check.txt / 已完成. Composer retained.
- Separate unresolved observation: the fixture's artifact button says 点击重试. This runtime-directory file preview has not been accepted; investigate reference scope/authorization without broadening filesystem access. Tool success is not file-preview success.
- No production/APK/Office installation. Approval decisions, independent external writer subscription and exact unknown reconciliation remain open.

### Native local-file URI preview repaired end-to-end — 2026-11-07

- Same AGY test session a9a52777-1964-41dd-b3cf-0037e34e44ac produced a file:/// link. Direct host preview originally failed NOT_REFERENCED; browser revealed unnormalized request also failed PATH_REJECTED. No missing file or network outage was inferred.
- Frontend artifactPath normalizes node-local URI for shared preview/cache address. Backend normalizes only explicit assistant references; host authorities, query/fragment, user-only references and outside-workspace references do not authorize a read. Existing no-follow descriptor traversal and size protections retained.
-13 POSIX preview regressions passed on X1T (including URI, space escaping, remote/user-only rejection);31 Rust targeted tests passed. Frontend path test and build passed. Candidate binary SHA2565755770802fb4fc75975dffcfb1a9485f5acfce318a63fbe4f27ab8151832225; frontend index-CNqKBC5-.js installed only into private5194/9531 environment.
- Browser reload showed readonly-check.txt / 预览已准备; clicking displayed the real marker and Expected count:7 with original download link, then 返回会话 restored composer and original reply. Host returned48 bytes; source SHA256f0cc234df59ad820709dc288f31c4a0ac56d0eeabe0db327ac6caa3d83298247. No model request was repeated during diagnosis.
- This closes the prior runtime fixture preview issue. It does not prove Office rendering, approval interaction, all external streams or complete event recovery. Production/APK/phone/Office installation unchanged.

### AGY permission observation and local denial persistence — 2026-11-07

- Owned ACP prompt receive path now persists permission requests as pending activity. A successful deny response write emits permission_decision/local_decision=denied and matching activity. This describes the client's decision, not remote acknowledgement or confirmed cancellation.
- Transport failure while writing the reply leaves unknown execution and unconfirmed approval; no permission_decision event is emitted. JSON-typed IDs prevent numeric/string request collisions within the request. Bounded32 approval records; terminal local denial cannot reopen to pending from a delayed duplicate.
-95 Python tests passed, including worker-path mock transport failure during denial and existing real receive/deny protocol wiring. Frontend bilingual build passed (index-Cm9kb9ZE.js). No real remote permission request/browser acceptance in this increment; no private installation of this increment, production/APK/phone/Office unchanged.
- Interactive user allow/deny transport remains unimplemented. This increment improves honest durable evidence only; it does not close approval integration or full event recovery.

### Native control harness-grant enforcement — 2026-11-07

- native_cancel and native_events previously checked connection presence but not the granted harness. Both now require exact tool membership before contacting host; claude-jsonl aliases claude, unknown tool fails closed. This is scoped to these two control/event entry points, not a complete authorization audit.
-33 Rust targeted tests passed, including disallowed Codex on AGY-only grant rejected before any host contact and alias checks. Release built and verified SHA256e5dbdcbb402ddc2991eff8fee3f1ee9ab1d220d93adc4e28c23a9515d6d6a789. Private9531/5194 candidate updated, including prior95-test permission-record source and index-Cm9kb9ZE.js frontend.
- Browser reload of the real authorized AGY verification session restored reply, prepared artifact and File readonly-check.txt / 已完成 activity. No extra prompt sent. Production/APK/phone/Office installation unchanged.
- User approval action RPC/UI still absent. The new entry must preserve connection/tool authorization and bind execution request, source permission ID, supervisor ownership and expiry before applying a single decision. Default denial observation is not approval-feature acceptance.

### Single-use permission queue, RPC and web controls source — 2026-11-07

- Added native_permissions with request/session/source request ID, owning supervisor token/PID,5-minute deadline and source-provided once-only options.32 records per execution; duplicate choices are idempotent, conflicting choices rejected, dead owner/terminal execution rejected. Claim precedes pipe write; dispatching state is never replayed after write failure.
- Actual ACP prompt loop registers/flushes only when explicit approval_mode=interactive; existing default denial remains. Node app_native_permission uses connection and Harness grant and paired forwarding; web bridge and NativeActivity one-time choices/description wired. Tool detail over8192 characters removes allow_once options rather than asking approval of truncated context.
-98 Python tests passed after final detail changes;3 existing activity JS tests passed.33 Rust targeted tests and release build passed before the final Python detail serialization change. Final frontend index-BbmY9qxg.js build passed. No installation this increment, no real interactive permission/browser acceptance.
- Open: native App command bridge, permission table global retention, enabling prompt-side mode only after live tests, actual user choice/timeout/cancel/transport-loss browser matrix. Current installed candidate remains e5dbdcbb... and prior frontend. Production/APK/phone/Office unchanged.

### Permission cancellation race and real denial terminal — 2026-11-07

- Fixed queued allow losing cancellation/expiry priority: dispatch claim reads execution cancellation in the same DB transaction and overrides pending or queued choice to rejection. Added1024-row ceiling, pruning expired inactive metadata only; running permissions retained. App command registration and native_dsh permission forwarding added, not native-compiled or device-verified.
- First real request bob-permission-readonly-4b1e091d-9e88-43f6-8f3a-071b0f2b6fec in dedicated session a9a52777-1964-41dd-b3cf-0037e34e44ac received permission960bc682-895c-42c4-a74e-ae2abce6c0a9. Submitted decline only; tool pwd failed, local denial persisted. Official end_turn without assistant text was incorrectly classified HARNESS_EMPTY_REPLY/unknown. Old request remains untouched, never replayed.
- Actual prompt path now accepts authoritative end_turn even when text is empty and persists session/prompt_result with request serial and stop reason. This does not infer terminal status from absent text or history.
- New dedicated session2047f91f-ac7a-4d24-9e9e-03c7bc77a902, request bob-permission-readonly-733a4886-45e6-4a65-87ab-7002e29a5d92, permission153c95d6-6187-4514-900b-5064fa8dacda: real permission received, decline queued then sent, execution completed. Workspace file list remained readonly-check.txt. No allow/elevated permission was granted. Host proof: agy-permission-fixed-proof.json in Bob runtime evidence directory.
-101 Python tests passed, including queue cancellation/expiry competition, metadata bounds and authoritative empty end_turn. No fresh candidate installation, production/APK/Office installation or phone test. Browser choice roundtrip and native App compilation remain unverified; interactive mode remains off in normal prompt route.

### Browser permission rejection roundtrip — 2026-11-07

网页审批真实往返（2026-11-07）：5194私有候选已更新，X1T专用AGY会话显示真实pwd权限详情，点击“拒绝”后节点回执completed/空正文，permission denied且不可重复操作，刷新恢复原会话。工具未收到完成状态仍诚实显示待核对，不以本端拒绝伪装工具取消。发现无正文终态/历史先结算时“正在回复”和“正在提交选择”残留，已修复为按全部执行状态重算及按具体审批决定清除提示；4项前端定向通过、构建并更新候选，修复后的新一次生成终态浏览器复验尚待。节点33项通过，候选SHA77811477f4d996afb57ad61dc6f646ba3348de2a36e295785e487816e670482c。正式发送仍默认deny，主动审批仅本次专用验证显式开启；允许分支、App编译、正式入口启用、外部CLI完整订阅和Office真实渲染仍待，生产/APK不变。

Request bob-permission-readonly-74a6f395-0320-4bf9-9ef3-a8b672d474e5; session agy:2047f91f-ac7a-4d24-9e9e-03c7bc77a902; permission 1e345315-149e-4cdc-9cfc-010ebfdf9512. Observed real pending detail, Reject button, local denial, completed readback, refresh recovery. No approval granted. New frontend index-DJxFDucF.js; no claim of fresh-generation visual acceptance after the indicator fix.

### Waiting approval and empty terminal visual acceptance — 2026-11-07

审批等待及空正文终态页面复验（2026-11-07）：actionable审批要求源permission_id、pending、未过期且执行仍active；断线/已终态不可点击，待审批卡自动展开，输入区优先显示等待审批。5项前端回归通过，私有前端index-DgEXjDF2.js已更新。X1T同一专用AGY会话新请求实测显示等待审批→网页拒绝→原回执completed空正文→输入区无回复中/无停止按钮，确认上轮残留修复。拒绝分支已验；允许分支/正式发送入口交互启用/App编译/外部CLI完整订阅仍待，Office组件暂不安装，未上线或打包。

Request bob-permission-readonly-e98fb4ec-fb18-40ed-9284-c81d218610ea; session agy:2047f91f-ac7a-4d24-9e9e-03c7bc77a902; permission609610e9-6b44-4222-aad1-9766341df0b5. Live browser showed automatically expanded pending approval and waiting indicator, then rejection removed action buttons and busy indicator without reload. Authoritative same-request readback completed, empty text, denied approval. Tool remains unconfirmed because source did not provide a tool terminal; not falsely called cancelled. No allow or project file modification.

### Normal web send and single-use AGY approval — 2026-11-07

AGY实际发送审批闭环（2026-11-07）：prompt_session联系宿主前核对精确工具授权，AGY正式请求携带interactive，其余工具保持deny。33项定向通过；私有候选e30f618afb67611e1b42f5b303e3b35def470153c64963245b685eb755e73cab更新。5194浏览器实际输入并发送只读pwd请求→等待审批→仅允许本次→工具completed→原会话返回exit0及目录→输入区恢复；本端approved不替代工具终态，两者分别验证。原fixture SHA保持不变。至此本地候选普通发送已启用AGY单次审批，允许及拒绝分别有真实证据；不代表生产或原生App已验。审批中的断线/服务重启与其他Harness完整事件仍待，Office组件暂缓，未上线/打包。

Request551a4d8c-809a-41c9-a1ec-da269b25f00c in agy:2047f91f-ac7a-4d24-9e9e-03c7bc77a902, permissionfd1e7edd-74be-448d-ba83-cb7ca3b546a2, source toolcall_6122354307460704060. Input submitted using normal textarea/send button, not direct probe injection. Clicked allow_once for exactly pwd after inspecting command; no permanent grant. Authoritative request completed, tool completed, permission approved/actionable false. Fixture SHA256 f0cc234df59ad820709dc288f31c4a0ac56d0eeabe0db327ac6caa3d83298247 unchanged, sole file readonly-check.txt. Added ungranted prompt rejection to pre-host regression. Full cross-Harness event recovery not complete.

### Approval waiting across private node restart — 2026-11-07

审批中私有服务重启恢复实测（2026-11-07）：正常网页发送的AGY只读请求停在pending后，验证候选进程身份并停止12秒再启动；server PID122060→124265，监管PID118332不变，请求及permission身份不变。页面刷新恢复同一审批→拒绝→原请求completed，输入区正常结束。持久事件accepted/running/permission_decision/completed各1，无重复派发。只验证私有HTTP服务重启及页面刷新，不等于整机/执行监管进程崩溃；短暂断线按钮隐藏未直接捕获，保持未验。未动生产/项目文件/Office组件，无APK。

Request17fd519e-601e-4ef7-beb2-4a12e58c0897 in agy:2047f91f-ac7a-4d24-9e9e-03c7bc77a902; permission64e206e5-d512-4092-9df7-31edc3250131. Before/after readback same pending identity, owner PID118332. Reloaded browser displayed pending approval; clicked reject. Final statuscompleted, permission sent/denied, tool source error; no permanent approval. Event counts: accepted1, running1, permission_decision1, completed1, source_event7, activity_snapshot8. Proof runtime file approval-restart-proof.json. Test driver initially failed a field lookup before stopping anything; corrected database-column lookup then resumed the same request, no resend.

### Claude owned stream identity and source event persistence — 2026-11-07

Claude受管流事件身份与持久化（2026-11-07）：实际worker在原会话身份确认前拒绝正文/终态，不再允许未绑定流污染当前回执；确认后保存stream_event/assistant/result源事件及原有正文增量。102项Python回归通过。X1T独立只读Claude会话初始化后原session续聊真实完成，786字符、46条text_delta、53条source_event（含assistant/result/stream_event），accepted/running/completed各1。未占用既有工作会话，工具关闭。此项不等于外部独立Claude实时旁观，后者仍待；当前增量未重新编入5194节点候选，浏览器验收待，Office/生产/APK不变。

Actual sessionclaude-jsonl:25349bbd-4d4d-40dd-9377-2207c7c8b3c4, requestbob-claude-stream-39f209b9-0a26-4c84-8cd1-ef4991227aa3. Installed CLI help verified session-id/partial-message options; created dedicated no-tools session then resumed using actual worker. Runtime proof claude-stream-proof.json. Statecompleted, errornull; no project commands or modifications. Initial new unit fixture had incorrect SQL column count, corrected against actual schema; final102 tests pass13.576s. External independent Claude subscription, tool activity projection, cancellation and browser validation remain open.

### Claude normal web send, live partial display and reload — 2026-11-07

Claude网页实时输出及生成中刷新（2026-11-07）：33项Rust定向通过，私有节点候选f7290b0475059edfe764f52c07f7a2f1e11ea636df8551686ec05c6ce7c104af已装入5194。侧栏真实Claude验证会话可恢复历史；正常输入框发送20句只读问答，生成中页面实见前5句及第6句未完、“正在流式输出”，随即刷新，恢复单条完整20句及正常终态，无重复正文/请求。原请求回执1655字符、105条text_delta、112条source_event，accepted/running/completed各1。受管Claude网页流与刷新已验，不等于外部独立CLI旁观/Claude取消或整机恢复；Office暂缓，生产/APK/手机不变。

Sessionclaude-jsonl:25349bbd-4d4d-40dd-9377-2207c7c8b3c4, requestd0003ead-5338-48ef-a2c2-0c257b18f068. Native browser snapshot captured first five complete sentences plus partial sixth while status streaming; page reloaded during generation. Final browser snapshot shows one user prompt and one complete twenty-item assistant reply, no busy indicator. Readback statuscompleted and exactly one accepted/running/completed. No tools or file modifications requested; no takeover of an existing external Claude process.

### Claude confirmed interrupt, preserving partial output — 2026-11-07

Claude官方取消实际worker闭环（2026-11-07）：按官方SDK双向stream-json控制协议先initialize、再发送原session用户输入；停止请求使用独立request_id的interrupt，必须同时得到精确success控制回执和aborted_streaming结果才置cancelled，支持终态/回执乱序，正常完成竞争仍completed。部分正文保留。Python/Rust/网页取消能力门已补源码，103项回归通过。X1T原专用会话实际worker取消成功，204字符保留，accepted/running/cancel_requested/cancelled各1，19源事件/11正文增量。当前新增未编入5194候选，网页停止按钮及App仍未验收；外部独立CLI/整机恢复/Office仍未完成，未上线打包。

Protocol source: https://raw.githubusercontent.com/anthropics/claude-agent-sdk-python/main/src/claude_agent_sdk/_internal/query.py (interrupt and control envelopes), plus official subprocess_cli.py streaming input transport. Installed CLI real probe confirmed success interrupt control response and result terminal_reason=aborted_streaming (is_error=true); not generic process death. Runtime protocol proof claude-interrupt-proof.json. Actual worker requestbob-claude-cancel-bec0d0e4-5c61-43e2-a27f-bc3d3a1260c0, sessionclaude-jsonl:25349bbd-4d4d-40dd-9377-2207c7c8b3c4; finalcancelled/errornull/204chars. Runtime proof claude-cancel-proof.json. No tools/file changes, no source session takeover. Frontend/HTTP capability source wired but not rebuilt or browser accepted yet.

### Claude unconfirmed cancellation negative cases — 2026-11-07

Claude取消未确认异常补齐（2026-11-07）：缺少确认、控制回执编号不匹配、拒绝停止和传输中断均保留已有正文，不生成cancelled/completed。HARNESS_CANCEL_UNCONFIRMED修正为unknown，避免普通error误解为上一轮已经确定结束。104项Python回归通过（16.768秒），新增四种负向场景。App转发入口只读核对未发现Claude前缀阻断，但仍未编译/真机验收。本增量未更新5194候选或生产；网页停止按钮实测仍待。用户再次明确暂不安装Office转换及隔离组件，Office真实渲染保持未完成，先推进其他功能。

Check: python -B crates/bob-runtime/tests/test_native_stream.py; 104 passed. No remote request, dependency installation, production change or APK in this increment.

### Claude browser stop and reload verification — 2026-11-07

Claude网页停止与刷新恢复实测（2026-11-07）：33项Rust定向检查及前端构建通过，私有候选28faa2617f0016473450a1517ab3568a0a86479382eb642b3fc302e30a2d05a3、前端index-C9eB1qen.js已更新统一5194。在原专用只读会话通过正常输入发送80句验证请求，看到流式正文后点击停止；确认期间显示正在确认停止，刷新后仍恢复同一请求，终态确认后恢复正常输入。节点最终cancelled/error null，4020字符保留，accepted/running/cancel_requested/cancelled各1，281源事件与267正文增量。停止并非瞬时完成，未将点击或历史中断标记当作成功证据。受管Claude网页取消闭环已验，不等于外部独立CLI实时订阅、执行监管进程或整机崩溃恢复；App/手机未验，Office安装仍暂缓，无生产发布和APK。

Exact request: `3ea66d2a-1f8a-4e9c-9065-79b6d5306e38`; native session: `claude-jsonl:25349bbd-4d4d-40dd-9377-2207c7c8b3c4`. No tools or project file operations requested. Browser observed streamed partial output, stop pending, reload pending, then restored send composer with retained partial answer. Read-only SQLite request/event check confirmed cancelled, not merely process exit.

### Worker loss and repeated delivery — 2026-11-07

进程丢失后重复投递状态修复（2026-11-07）：非AGY启动入口先核对原worker，再按持久状态返回unknown/not_accepted/终态，不再对未知原请求一律回答accepted；不重复启动，新请求仍被unknown屏障拒绝。进程存在不等于所有权，无法读回原请求身份不得推断alive。106项本地回归通过（16.089秒）；X1T临时隔离目录3项检查通过（0.597秒），包括真实Python子进程退出后原请求重入、部分正文保留及无第二次spawn。此为无模型的实际OS进程故障验证，不冒称运行中Harness或整机崩溃恢复；本增量尚未重编入5194，外部CLI未落盘订阅/Office/App门禁仍待。未安装组件、未上线/打包。

Actual child script intentionally exits without terminal receipt. Its PID is waited/reaped, then the original request is submitted again. It returns unknown, preserves partial text, starts no replacement process and rejects a different request in that session. Terminal replay reports cancelled without spawning. No active user CLI was interrupted. Initial Windows real-process test failed because existence-only probing claimed alive; identity-required correction passed locally and on Linux.

### Official Claude terminal evidence recovery — 2026-11-07

Claude官方终态持久化后的恢复窗口（2026-11-07）：source_event将精确session的success结果及精确request的interrupt确认/aborted_streaming分别折入快照；只对unknown执行使用双证据恢复，不抢占活跃worker。缺确认/错会话/错确认保持unknown，乱序可恢复，部分正文保留，不启动新执行。107项Python通过，随后unknown限定守卫定向复验通过；33项Rust通过。X1T取上一轮真实请求3ea66d2a-1f8a-4e9c-9065-79b6d5306e38的两条官方回执，在临时隔离数据库反序恢复cancelled/4020字符/单终态，原回执逐字不变。不是实际杀死运行中CLI或整机测试。最新私有候选19791f6a12af444352ee81d4295c7d874ef8f1c1189bdf2b200b37fe418266ad已更新5194，包含上一轮进程丢失重复投递修复。外部未落盘订阅、运行中监管崩溃、App/Office仍待；未上线/打包，Office安装暂缓。

No success inference from prose or process death. Official terminal evidence survives event projection in a bounded snapshot; only unknown receipts are reconciled. Six scenarios cover success, cancellation, reverse ordering, missing acknowledgement, wrong session and wrong acknowledgement. Exact real source receipt remains unchanged; no new execution.

### Live Claude worker interruption and unknown UI — 2026-11-07

运行中Claude监管中断与网页未知状态实测（2026-11-07）：专用只读会话生成期间核对/proc中的native_worker、原session和request后仅中断该worker；147字符已持久化，原请求unknown/HARNESS_PROCESS_LOST，重复原请求仍unknown，新请求被BUSY屏障拒绝，accepted/running/unknown各1，无completed/cancelled，fixture哈希未变。原CLI随后仍完成并落盘80句历史，暴露网页错误只显示恢复成功、遗漏未知回执提示。现独立跟踪执行状态，未知状态持续提示、暂停发送但保留草稿；5194真实浏览器输入测试草稿→发送禁用→刷新草稿仍在，已清空测试草稿。前端index-GrMd80lM.js已更新。此验证证明监管进程死亡不等于取消，尚未自动权威核对这类Claude未知轮次；完整外部订阅/整机恢复/Office仍未完成，未上线/打包。

Request `bob-claude-worker-crash-e8258ae0-99a4-4630-81df-e36af20794ff`, dedicated session `claude-jsonl:25349bbd-4d4d-40dd-9377-2207c7c8b3c4`. Runtime proof `claude-worker-crash-proof.json`. Source events16/text deltas9. Only the verified Bob worker was killed; no user project CLI or production service was interrupted. Underlying CLI outliving the worker was directly observed through later persisted history. Unknown remains intentionally unresolved, not rewritten from response prose. Browser did not send the draft.

### Claude input message identity — 2026-11-07

Claude原生输入身份补齐（2026-11-07）：发送前将由session/request确定的UUID持久化为native_user_message_id，再随stream-json用户消息传入CLI；同文案不再作为未来恢复的唯一定位依据。107项Python回归通过（17.066秒）。X1T新专用只读会话96161113-6ad0-4eca-9bc2-2e14f45234e0返回READY/官方success，原生user记录精确保存传入UUID26bd36f3-86cc-493f-bfd6-70dd9d27ef6b。只读审计上轮故障记录发现thinking与text均可能携带end_turn，故不据此或完整正文把旧unknown改completed；旧请求缺发送前锚点仍保留unknown。界面保留活动卡时同步终态status，避免已确认后仍残留unknown。源码未重编入候选，官方终态长期独立收集/旧未知权威核对仍待；无安装/生产/APK。

Official source: https://raw.githubusercontent.com/anthropics/claude-agent-sdk-python/main/src/claude_agent_sdk/types.py (resume_drops_turn documents UUID supplied on streamed user messages). Installed CLI actual probe preserved the supplied UUID in exactly one native user record. Runtime proof: claude-input-uuid-proof.json. No retroactive fabricated binding for previous unknown requests.

### Independent Claude receipt capture survives worker loss — 2026-11-07

Claude独立回执收集与生成中监管崩溃恢复（2026-11-07）：受管CLI外增加独立结构化输出收集进程，先保存再转发，读取端消失后继续保存；原请求输入UUID/session/request写入收集头，恢复仅使用精确身份、完整行、官方success或interrupt双证据。单文件8MiB、总目录64MiB/128文件准入上限，满则拒绝新增、不删活跃回执；并发配额锁/保留策略仍需加固。108项回归通过（17.810秒），含真实收集进程输出读端关闭、错UUID与截断拒绝。X1T专用会话96161113-6ad0-4eca-9bc2-2e14f45234e0收到236字符时核对身份后中断worker，独立收集最终恢复completed/4501字符/error null，accepted/completed各1，原请求重入不重启，fixture哈希未变。实际完成监管崩溃后的正文/终态恢复，但不是整机掉电或全工具事件恢复。当前源码尚未编入5194；外部独立CLI完整订阅、收集端自身故障与容量并发、Office门禁仍待，无生产/APK。

Exact request `bob-claude-wire-crash-d62e8511-0b56-4af0-a04f-46c4083ac4df`; runtime proof `claude-wire-crash-proof.json`. Events: accepted1/running1/unknown1/message_snapshot1/completed1/source_event20/text_delta18. Original reader died during generation; recovery used the independently saved official result, not transcript prose. No replay, no project mutation. This capture currently retains raw structured frames locally but only reconstructs text/terminal after this failure; full activity projection remains a separate gate.

### Capture quota concurrency and recovered browser — 2026-11-07

独立回执并发容量保护及候选网页复验（2026-11-07）：收集进程在SQLite跨进程写锁内检查文件数/总字节并创建或追加，单文件8MiB、目录64MiB/128文件；满额不启动新CLI或停止继续捕获，不清理活跃记录。109项本地通过（17.621秒），X1T两项真实子进程检查通过（0.939秒），含127已有记录时并发两次准入仅一次成功、总数128。33项Rust与前端构建通过；节点候选fb2676b2d663a5ca2165c0e36b5ddf95bb3a5154055792329e1c125330399d5d、前端index-FRShEV45.js已更新5194。浏览器打开96161113专用会话显示恢复后的完整80句，输入未发送测试草稿时发送按钮启用；旧25349会话仍未知并禁止发送。测试草稿已清空，无新增模型请求。自动过期清理/容量释放、完整活动事件恢复及外部CLI/Office门禁仍待；无生产/APK。

Quota checks and append occur under the same SQLite immediate transaction, measuring actual spool bytes. The receipt storage cannot silently remove active captures. Tests cover concurrent admission count but not simulated disk-full/power-loss or automatic retention; those remain open. Browser only edited and cleared a draft, never submitted it.

### Settled receipt retention lifecycle — 2026-11-07

独立回执缓存保留期与安全释放（2026-11-07）：收集进程关闭子进程后原子写入精确身份.done标记；新请求准入前在同一容量锁内，仅释放超过7天、completed/cancelled、已关闭、官方回执与持久正文/状态完全一致的缓存。请求账本和正文不删，unknown/运行中/无关闭标记/身份或正文冲突保留；最多检查128个缓存头并按request查账本，不全扫会话历史。110项回归通过（18.374秒）；扫描有界化后3项本地定向及X1T隔离临时目录3项（1.303秒）通过。未清理任何真实历史，当前源码未重编入5194，旧无done缓存保守保留。完整活动恢复、外部CLI订阅、整机故障和Office仍待，无生产/APK。

Six retention cases: expired confirmed cache removed; recent/unknown/no-closed-marker/different-text/different-marker retained. All six request rows remain. Second cleanup is a no-op. Real subprocess reader-loss and concurrent-admission tests also pass with completion markers. No production or actual user cache deletion was performed.


Claude工具活动恢复（2026-11-07，源码验证）：结构化assistant tool_use及user tool_result按工具ID投影，精确session隔离；独立wire恢复在确认原request/input UUID及官方终态后，将工具快照与正文一起持久化，不重新执行。未收到工具结果保持unconfirmed，不以最终正文推断成功；64项有界，截断标partial，快照不带工具输出正文。113项Python回归通过（17.052秒），包含恢复入口无Popen、重复读回仅一份activity_snapshot、错会话及迟到tool_use不覆盖结果。当前尚未重编私有候选或完成真实工具/浏览器验证；审批、成果引用完整恢复及外部CLI订阅仍待，不视为1–8整体完成。Office组件继续暂不安装，未上线/未打包。


Claude工具结果接收与候选更新（2026-11-07）：实际worker过滤器原先未保存user类型的tool_result，现已纳入source_event，避免只有工具开始而丢完成状态。114项Python回归通过（18.183秒），含实际worker接收循环；33项Rust定向通过。私有候选SHA256 a4c295a884cabdcc9048573ef2afe8b61548b7d1cbe1b42737481a6d07a2c3da已安装，统一5194刷新后恢复原Claude专用会话完整正文及底部输入框。此次浏览器只验证候选恢复可用，不等同真实工具事件验收；正式Claude命令仍关闭工具，需在既有授权边界下建立只读工具验证路径，不能把投影测试当成实际工具可用。生产/手机/APK未变，Office暂不安装。


Claude真实工具增量读取（2026-11-07）：发现仅更新event_snapshots不能让正常cursor轮询得到工具活动，现同事务在活动变化时追加activity_snapshot，重复原始事件不重复推送。115项Python回归通过（18.558秒），33项Rust定向通过。X1T已有真实会话eeddd53b-e6a8-434f-ae8a-e20ee1fee104中2条tool_use/2条匹配tool_result，在隔离临时账本通过实际source_event→增量读取接口得到4次活动更新、最终2项completed；源文件SHA未变，无模型调用或工具重执行。该证据是原始真实记录回放，不冒称外部CLI未落盘实时订阅或新工具调用。候选已更新统一5194，节点SHA256 c6a76c04c6c22dc5e758f784992144e7a01a07abfbd3dbd5d38a38748e9f36c3。下一步仍须完成真实运行时工具授权/审批/成果恢复与外部订阅。Office安装继续暂缓，未生产部署/打包。


Claude监管失联期间持续正文恢复（2026-11-07，真实故障）：独立wire读取在尚无官方终态时也可恢复已核对input UUID/session的完整帧前缀正文和工具观察，保持unknown及禁止重发，不把进度当完成；末尾半帧忽略、错session不输出。116项Python回归通过（18.265秒），33项Rust通过。X1T专用会话96161113-6ad0-4eca-9bc2-2e14f45234e0请求bob-claude-wire-progress-c5757d63-f401-49f8-ae4e-1bc8a7dce230在124字符时仅SIGKILL精确Bob worker，独立CLI/采集器继续；202次unknown观察期间正文124→29197字符，最终completed/error null，accepted/running/unknown/completed各一次，fixture SHA未变，同请求无第二次执行。私有候选29ca1e48232d4e052882de61f6320ffac071dbe0fbd13f4356dfb6880ca251be已更新5194，浏览器reload实见240句完整回复及原输入框。这是Bob受管任务worker故障，不等同外部独立CLI附着或整机故障。当前恢复使用有界message_snapshot，增量传输效率仍可改进。Office组件暂不安装，生产/手机/APK未动。

官方接口核对（2026-11-07）：X1T现装agy帮助确认--remote-control及remote-control start/status/stop；Claude帮助包含后台会话/attach/logs，但logs描述为terminal output，不能作为结构化事件解析替代。官方Remote Control文档描述各自网页接入，并未在本轮核查中建立Bob第三方只读事件订阅证据，不因此宣称可直接接管。来源：https://antigravity.google/docs/remote-control?tab=cli 与 https://code.claude.com/docs/en/remote-control 。外部Claude/AGY/独立Codex TUI仍按未完成处理，禁止抓取终端文字或复用订阅凭据伪造接口。


Claude单次工具审批真实闭环（2026-11-07，私有验证）：复用native_permissions队列，支持官方can_use_tool控制请求及精确request_id的allow/deny回复；仅单次选择、取消/过期转拒绝、写出前claim不重发、验证worker PID+session+request归属。未绑定会话及超限输入拒绝。wire恢复保留已有审批决定，不用工具快照覆盖。118项Python通过（18.478秒）。X1T专用会话96161113-6ad0-4eca-9bc2-2e14f45234e0请求bob-claude-permission-b049cb50-faca-4b03-a7d3-dc389535f349，私有验证源码只启用Read并强制ask；只批准精确无敏感fixture一次。实得1次permission_decision、Read completed、审批approved、253字符回复、fixture SHA不变；5194浏览器reload→展开执行记录实见已完成与本端已批准本次操作。审批由验证程序提交，尚非网页按钮交互；普通发送命令仍关闭Claude工具，Rust入口交互启用及拒绝/断线审批实测待完成。私有服务仍为上轮候选，仅本次执行使用新源码；未生产部署/打包，Office继续不安装。协议依据官方claude-agent-sdk-python query.py与subprocess_cli.py，不使用订阅Token逆向接口。


Claude网页拒绝真实验收（2026-11-07）：Rust native_permission允许Claude前缀并继续精确Harness授权校验；33项Rust通过，候选13202857201c31382ebb4195ba73aa4ca9c1ed0e85fe3ac15b26d3899178cafd已更新5194。X1T原专用会话请求bob-claude-browser-permission-fb6d68a1-25f5-4b54-954f-f8d435cca29b：网页实见等待审批→点击拒绝→CLI返回未获批准→工具error/审批denied→completed，刷新仍保留回复且恢复输入框，fixture未变。验证脚本原断言错误地要求reject_once，实际UI明确提交null，随后只读核对原队列sent/choice null及唯一permission_decision denied，未重发请求；已补null拒绝回归，118项Python通过（17.994秒）。普通入口工具仍关闭；网页允许分支、审批中服务重启及正式发送启用待完成。另发现两次同文本用户消息在浏览器疑似合并，回复挤入上一条；需按原生消息身份核查，不能按文本去重。Office暂不安装，生产/手机/APK未变。


Claude同文本不同轮次身份修复（2026-11-07）：历史解析器曾以最近user文字相等删除合法新输入，导致后续assistant并入上一轮。现仅按原生UUID去重；回执带native_user_message_id时按精确UUID核对是否已有输入，缺锚点旧回执仍保守保持既有逻辑，不猜测身份。119项Python通过（17.389秒），覆盖不同UUID同文本、重复UUID及相同文本的新回执。X1T专用真实会话两次相同Read请求只读核对通过：2个原生输入UUID→2条用户消息→各自独立回复，源JSONL SHA不变，未重新调用模型。33项Rust及候选构建通过，节点SHA256 2b9b66143d44beee91c451c7898446e32fc4eabbdacbbdeedbc7ef8177383f3b 已更新5194。5194浏览器读回确认07:20与08:38两条相同用户输入分别对应成功读取和拒绝读取的独立回复，底部原会话输入框可见。生产/手机/APK未变，Office仍暂不安装；Claude正常入口工具启用、网页允许及断线审批、外部订阅仍待。


Claude审批等待期间服务重启与网页允许验收（2026-11-07）：X1T专用只读会话请求bob-claude-restart-permission-17ecbf3a-638e-4cef-ba11-c026fc9fad6c，在Read待审批时核对私有9531服务PID/exe后停止并重启该读取服务，原worker仍存活、原审批ID保留。5194刷新显示同一Read请求及精确fixture范围，网页点击仅允许本次后completed/error null，237字符回复，Read completed/审批approved；accepted/running/permission_decision/completed各1，activity_snapshot 5/source_event 36/text_delta 13，fixture SHA不变。再次刷新仍显示独立用户轮次与完整回复、正常输入框。本次只读验证源码强制Read ask；正常产品发送入口仍关闭Claude工具，尚不能把该证明当成默认入口已启用、worker审批中崩溃或整机故障证明。候选二进制未变；生产/手机/APK未动，Office组件继续暂不安装。


Claude正常网页发送工具审批接通（2026-11-07）：HostGrant新增默认空claudeTools，仅从宿主配置传入原生发送；明确非空时进入interactive，工具参数仅列出获准工具并逐个ask，拒绝通配/default/参数注入，接收control_request再次核对工具名。121项Python通过（18.676秒），33项Rust通过，候选SHA256 164cbf338d586102be989382f93a7ee9734a2b6193c263ee36e8990bf9fb1765已更新5194。私有候选临时仅配Read，由网页正常输入发送请求2ab1af5f-05bd-46de-8002-6eb8e8395b08，原96161113会话出现待审批，网页仅允许本次后Read completed/审批approved/completed/error null，238字符回复，accepted/running/permission_decision/completed各1；fixture哈希不变。此次未替换worker启动参数，实走正式Rust→Python发送路径；完成后精确恢复临时宿主配置。只证明Read真实路径，非所有工具/写入或生产启用；执行回执尚未保存工具授权快照需补齐。外部CLI订阅、整机故障与Office仍未完成，未上线/打包。用户本轮明确同意向既有Hostinger私有目录发送源码进行构建。


Claude授权快照与审批期间撤销实测（2026-11-07）：执行前持久化tool_authorization版本、审批模式和工具清单；批准时与宿主当前claudeTools取交集，旧无快照请求不允许新增批准但可拒绝。123项Python（18.456秒）及33项Rust通过。私有候选064638d440f8b070f7e1e8404645399a2ea094a9fa7a180f845650f8583baf8c已更新5194。网页正常发送a50ca39d-556e-4e55-b1f1-d3c6ffd9c285，在Read待审批后移除私有授权；点击仅允许本次未产生permission_decision，仍pending且无文件内容。随后网页拒绝，CLI返回拒绝说明，completed/error null，工具error、审批denied、唯一permission_decision，319字符，fixture哈希未变；临时配置已恢复。撤销阻止的是尚未提交批准的请求，不承诺撤回已执行副作用或批准后的原子撤销。发现UI未持续显示撤销原因，后续需补；外部CLI订阅与完整事件恢复仍未完成。未上线/打包，Office暂不安装。


撤销提示与外部Codex故障订阅验证（2026-11-07）：审批错误改为按permission_id独立保存在对应卡片，不被轮询notice覆盖；HARNESS_TOOL_NOT_GRANTED显示中英文撤销提示、禁用允许但保留拒绝。5194真实请求c933df74-a763-49dd-98fc-b46dcfaa609e验证提示跨轮询保留，随后拒绝completed/工具error/审批denied/319字符、fixture未变、临时授权已恢复；前端index-r-xQyThj.js。独立官方app-server客户端创建专用会话01a1b392-fcf4-73f1-84a1-b727afa11922，仅生成100句无工具文本；Bob observer在152字符时核对PID身份后中断，重新订阅恢复4384字符与completed，同turn 01a1b392-fd12-7540-b523-98d7dcb10791，新观察1484事件，未通过Bob native_worker发任务、未重发、fixture未变。这证明app-server外部执行者通道，不证明任意TUI或AGY/Claude外部进程附着。此前尚未开始任何轮次的空会话附着超时，未发送模型请求，保留该边界。另修复完整旧轮次回执恢复时遗漏工具活动，先恢复activity再终态，124项Python（18.504秒）及33项Rust通过；此工具修复为回归证明，未冒称真实工具执行故障验收。候选节点SHA 424924e62703aa7c1202e2cd9d59aef209bff6b1b4af597a8dd5d82f32975397 已更新5194。仅摘要的旧回执仍无法重建完整工具/成果，外部各Harness全覆盖仍待。未上线/打包，Office组件未安装。


原会话实时接入与恢复续查（2026-11-07）：现场读取 X1T 官方帮助及 `claude agents --json --all`，确认一个 interactive/busy 会话；`claude attach` 是终端接入，`claude logs` 是最近终端输出，不能宣称结构化正文/审批事件订阅，未抢占或重发。AGY 官方 ACP 为客户端启动的 stdin/stdout 子进程；Remote Control 的 start/status/stop 不等于开放的第三方实时订阅接口。官方 Hooks 可报告生命周期，Sidecars 的 agentapi 文档只列发送/新建，不据此冒称可订阅所有运行中会话。外部 Claude/AGY 正文与审批实时接入仍未验收。

本轮实际修复：旧 Codex 未确认轮次先按摘要定位，命中后仅补读相同分页的 full items，最多三页/各二次请求；失败仍按已确认状态恢复并保留已有正文。125 项 Python 回归通过（19.245 秒），包含完整工具结果回填与读取失败降级。X1T 原验证会话 01a1b392-fcf4-73f1-84a1-b727afa11922 的官方只读历史接口读回原 turn、completed、4384 字符/2 items；使用隔离检查点，无提示发送、无项目修改。原常驻 app-server 已不可附着，本次验证用新只读 app-server 查询历史，不能充作正在生成的外部进程续接验收；真实工具/审批/成果故障补读仍待。摘要降级后再次补全机制仍待，不标记完整事件恢复完成。此次源码尚未更新运行候选，未上线/打包/安装 Office。


### 2026-11-07 AGY官方结果落盘后监管退出与本地目录图标

- 真实原会话：agy:2047f91f-ac7a-4d24-9e9e-03c7bc77a902（专用只读验证工作区）。请求bob-agy-terminal-fault-2c9232b2-0912-42ff-b4d4-5cfc2c9b1208，仅一次read_file读取readonly-check.txt，无项目文件写入。
- 在官方session/prompt_result源事件持久化后、普通终态投影之前注入SystemExit(91)。重读先unknown后completed，207字符回复、marker和count=7保留，read工具completed。accepted/running/completed各1，source_event10，text_delta4，activity_snapshot2，unknown1，message_snapshot1；没有重发。
- 验证文件SHA256前后一致：f0cc234df59ad820709dc288f31c4a0ac56d0eeabe0db327ac6caa3d83298247。节点私有agy-terminal-fault-clean-proof.json为verified，不提交原始会话/凭据。
- 之前另一专用会话的尝试被HARNESS_SESSION_BUSY拒绝，未投递；保留其unknown屏障，没有人工改成completed。
- 统一5194浏览器确认“已恢复最新回复状态”、207字符回复、执行记录File readonly-check.txt已完成、文件实际预览marker/count及返回会话。目录Claude/AGY/DSH图标可见，Codex可访问图片标识可见；Pi官方SVG已随包提供，尚无真实Pi目录行视觉验收。
- 新增精确AGY请求身份/源正文/终态恢复及摘要后完整条目补读回归。127项Python、33项Rust通过，前端151模块构建通过。前端index-C1O6IkrL.js / index-BbPySbzH.css；候选bob-node SHA256 01bdb6f8d5e1408aa213635277be4cdbc4eec217bd261c61b7993fa2d1ffb3a1。
- 仅更新私有候选及统一本地页面；无生产发布/APK/Office安装。未证明任意外部Claude/AGY运行进程热附着、整机恢复或全部工具/审批/不可变成果事件恢复。
