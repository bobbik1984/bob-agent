# Bob — Architecture

> 本文记录**已接受的边界与不变量**和**待决的架构候选**，两者分节标明。代码位置见 [COMPONENT_MAP](COMPONENT_MAP.md)，决策理由见 [DECISIONS](DECISIONS.md)。截至 2026-10-28，已开始新写内核与App控制器；完整功能仍未接通。WP-02详细合同见 [wiki/kernel-schema.md](wiki/kernel-schema.md)，当前验证范围以CURRENT_STAGE和Evidence为准。

## 1 已接受的不变量

来源：用户在本项目中的确认（BOB-D-001 至 BOB-D-005、BOB-D-008），以及两个前身项目中**都已接受、并且与本项目定位一致**的原则（ONL 宪法第 1–12 条、APP-D-008 / D-009 / D-016 / D-018）。

1. **One truth per item**：每个对象任一时刻只有一个权威写入者；交接必须显式，并保留来源（ONL 宪法 11）。
2. **Project ≠ Workspace ≠ Host**：项目是逻辑主体；工作区是某台主机上的目录；只同步工作，不同步全部硬盘（ONL 宪法 1）。
3. **Task > Execution**：换模型、换 Harness、换主机都创建新的 Execution，保留同一个事项身份；不在原 Execution 内“换脑”（ONL 宪法 2、8）。
4. **Execution completed ≠ Task completed**：生成、登记、验证、审阅、接受分开记录（ONL 宪法 4，ONL-D-WEB-01）。
5. **Transcript ≠ Memory**：会话记录只是证据；长期记忆有来源、可检查、可纠正、可遗忘（ONL 宪法 5，APP-D-019）。
6. **Capability is real only when available**：能力只有在当前环境真实可用时才出现；降级必须诚实，不静默回退（APP-D-016，ONL 宪法 9、12）。
7. **Requested / Resolved / Observed**：模型和运行配置分开记录；没有观测证据就是 `unknown`。
8. **三个信任域分离**：设备配对、操作授权、Provider 认证相互独立。配对成功不等于可以写文件，有模型 Key 不等于批准执行。
9. **凭据留在边缘**：凭据存放在设备的系统安全存储（Windows DPAPI、Android Keystore）或执行主机本地；不同步、不进日志、不进成果、不进上下文包。
10. **懒式响应、静默预热与本地接纳（BOB-D-008）**：输入框focus/input（15秒冷却）静默预热；与后台投递共享Rust内核中的单飞唤醒和新watch。UI发送先将消息与intent_outbox在同一个磁盘事务中提交，提交成功立即上屏/清空本次草稿，不等网络。已证明零请求字节的失败是not_dispatched，可自动重连；已经或可能出网但无ACK才是unknown_send，先对账，不能盲目重派。磁盘失败保留草稿，禁止内存库降级后假称保存成功。Outbox的peer_accepted只表示对方持久接纳；running、检查及人审状态来自各自领域对象，不混成一条发送状态。完整状态图、崩溃边界与O01–O15验收仅在[kernel-schema](wiki/kernel-schema.md)维护。
11. **原始证据不被覆盖**：摘要、检索、纠正都不能覆盖原始捕获和证据。
12. **个人基础设施不进代码**：域名、IP、节点只能经配置或 Provider 注入（BOB-D-004）。

## 2 目标逻辑分层（BOB-D-005 已接受：单一 Rust 内核）

```text
┌──────────────── Shells ────────────────┐
│ Tauri Desktop │ Tauri Android │ (iOS later) │ bob-node (headless)
└───────────────┬─────────────────────────┘
                │  Vue 3 UI（沿用旧 App 设计语言，Online 的治理视图并入同一界面）
┌───────────────▼─────────────────────────┐
│ bob-kernel（Rust，单一实现）              │
│  · 个人连续性：Conversation、Capture、Memory、Decision、材料引用 │
│  · 工作治理：Project、Task、Execution、Artifact（版本）、Verification、Review │
│  · 个人事务域：日程、待办、笔记（领域模块，各有一个权威位置） │
│  · 授权与策略：Policy、批准、审计回执       │
│  · 存储：SQLite + 追加型事件日志            │
└───────┬───────────────┬─────────────────┘
        │               │
┌───────▼──────┐ ┌──────▼──────────────────────────┐
│ Connection   │ │ Capability Providers               │
│ Providers    │ │ 模型（BYOK）· 文件 · Harness runner（进程边界）· 通知 │
│ L1–L5        │ └──────────────────────────────────┘
└──────────────┘
```

## 3 数据所有权模型（BOB-D-004 / 005：Home 加 Replica）

- 每个用户有且只有一个 **Home** 内核作为主权威（**作者个人环境：X1C**；白领默认：自己的 PC）。
- 其他设备是 **Replica**：只读缓存加带 `expected_version` 的意图队列。
- 追加型对象（捕获、消息、证据）可以在任意一端用全局唯一 ID 创建，之后汇入 Home；同ID同摘要幂等，同ID不同内容明确冲突。首次离线会话/项目保留local标记，经Home接纳后才成为权威。
- 修改已有对象必须由 Home 接受；版本过期时拒绝，并向用户解释原因。不做多主合并，CRDT 不是首轮前提。
- Home 不可达时，Replica 可以独立完成基本对话（BYOK 直连模型）和捕获；涉及 Home 的操作进入队列，界面明确提示。
- 活动数据库**不通过文件同步**共享。
- 有副作用的事项/原生会话续聊先Replica→Home持久接纳，再Home→执行节点；只读历史可独立读取。Home离线不绕过主权威直接派执行。
- 同一工作区的写入由持久租约与fencing保护，未知执行不因超时释放；手工原生IDE不受Bob租约约束，需核对工作树。旧App双向LWW及旧Online内存回执不能直接复用为此模型。

## 4 待决问题

| 问题 | 决策 | 前置工作 |
|---|---|---|
| 内核语义对齐（同名实体、迁移边界） | BOB-D-005已接受；[WP-02草案](wiki/kernel-schema.md)已形成并经DDL检查，尚非实现 | WP-03审阅与BP2契约测试 |
| 连接分层与默认方案 | BOB-D-006设计已接受 | 实现与跨网安全验证待做 |
| 旧App同步去留 | WP-01已证明为双向合并；提取协议/测试，Home权威同步需重写，禁止整文件照搬 | BP1去留审阅及迁入批准 |
| UI 收敛：直接演进旧 App UI，还是另起 UI 包 | 待开题（审查 AD-4） | WP-02 |
| 旧 App 微信网关等模块是否进入开源核心 | 待开题 | BP6 前 |

## 5 安全边界（必过门）

手机远程读取 PC 文件（F4）、Harness 在个人电脑上执行，是攻击面最大的两条路径。上线前必须完成 WP-03 威胁模型：设备配对与吊销、操作授权范围、默认拒绝的执行环境、重放防护、日志脱敏，以及丢失手机时的处置。

## 实现边界补充（2026-10-28）

连接/安全设计见已批准的[connectivity](wiki/connectivity.md)与[threat-model](wiki/threat-model.md)。本地Outbox、App发送控制器及共享唤醒/Home接收/调度内核已实现；UI不自建网络重试状态机。预热是独立后台动作，不成为本地保存的前置条件。持久Inbox消费与异步dispatcher已通过本地集成检查，真实Home认证/传输尚未接通，UI IPC已连接LocalApp并通过原生编译及进程启动，窗口交互仍待验，不能把本地保存提升为对端已采用或模型已运行。


### BP2回复证据边界

本机官方CLI登录与远端Home设备认证分开。早期`external_cli_probe`只记录独立CLI验证结果，回复使用现有messages/evidence/evidence_links，不产生虚假的Task/Execution或Home回执；这段历史不能作为Bob按钮派发证据。原生历史接口输出role/reply_to；普通前端不得调用回复导入。

当前Windows本机只读Runner已绑定既有Task/Execution：消息先持久接纳，再按request_id登记唯一执行、工作区租约与预定Job归属；本机接管意图不得同时进入通用远端派发。官方CLI由专用worker启动，worker先加入设置了关闭即终止的Windows Job，持久启动登记成功后才释放输入。未配置可执行文件、启动登记失败或Job绑定失败不得降级裸跑；此路径只允许空白临时工作区的只读文本回复，不代表一般工具或真实仓库隔离已获验收。

取消请求与停止确认分开记录；超时、取消和退出均检查整个Job的子进程清理结果，未确认时保持unknown与占用。回复只有在有效成功事件、正常退出及清理确认后，才与执行终态和证据同事务落盘。重启不自动重放模型：以预定Job身份核对旧进程是否消失，再从持久完成回执恢复；没有结果证据则记录中断失败，不虚构回复。完成回执临时文件恢复与编码后大小上限已补齐专项回归，详见当前施工合同与Evidence。

UI发送、执行状态、取消与历史读取均经原生命令调用同一内核；成功回复来源标为正式codex_execution，与external_cli_probe区分。实际原生命令模型往返和GUI点击属于不同验收层；当前进展由CURRENT_STAGE记录。Home授权与TLS传输现已实现独立模块；App首次配对、外部监听和Android仍未接通。

### Home接收边界

可信传输完成标准双向TLS身份验证后，按本地Home身份/epoch及实际证书指纹获取内核授权上下文；请求体不能自行声明可信身份。授权绑定设备、密钥引用、精确会话、版本与有效期，每次业务操作重新核对，撤销使旧上下文失效。接收/回执查询与执行授权分离。

传输使用TLS 1.3正常证书链及域名验证，并额外绑定已配对证书指纹；禁止明文降级、首次使用即信任及早期数据。网络I/O有绝对期限与帧上限，但不声称可强制中断任意数据库回调。可能已发送却未得到回执必须查询原请求，缺失回执仍不构成安全重派证明。

Windows原生vault仅接受可信app-data目录，使用当前用户DPAPI固定槽位、拒绝覆盖/重解析路径/明文回退。不对前端暴露任意文件路径；DPAPI不能抵御同用户恶意进程。Android Keystore、身份签发与首次配对尚未实现，不以测试证书填补该边界。
