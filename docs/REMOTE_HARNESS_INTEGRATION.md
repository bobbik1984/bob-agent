# Work 通用远程 Agent 接入规格

状态：用户已批准按此方向实施（2026-10-21）。RH-00/RH-01 的最小连接与浏览子集已实现并通过本地测试；目标 X1T 未连接验证，RH-02 指令闭环尚未实现。本文替代上一轮“以 DSH 专用接入为中心”的方案范围，DSH 为首个适配实现；不替代既有整体路线图或解除 A4/A5 门禁。

## 1. 需求与事实边界

用户已明确的需求：

- Bob 是手机端统一的远程工作入口，能够接入 DSH、Hermes Agent、OpenClaw 等执行端，而不是为每个执行端另建一套网页容器或 Work 页面。
- 在 Bob 的 Work 界面使用 X1T 上已运行的 DSH，而不是另开一个不适配手机的网页。
- 以聊天为中心，保留可展开的项目/会话侧栏；PC 与 Android 复用 Bob 现有组件、主题与交互。
- 沿用 DSH 的现有工作区、会话及工具环境，优先满足已使用的 Blender 工作流，后续承接 Godot 等工作。
- 看清文字回复、执行进度、待确认操作和真实产物；断线重连不应反复干扰使用或误报任务状态。

核验基线：上一轮 Bob `main`，HEAD `3876a46`，进度文件版本 `0.9.27`；DSH 上游源码固定为 `dsh-v0.2.1-rc.2`（预发布）。本轮另查询 OpenClaw 与 Hermes 官方公开文档，不代表用户安装版本已兼容。目标设备实际 commit、插件、反代和网络日志尚未核验；不能把公开文档等同于现场实况。旧待办中的重建版本文字不能作为当前安装包结论。

以下为已采纳的开发方向，不能据此把全部目标写成已实现功能。当前 `todo.md` 中 DSH-ACP 与 `progress.yaml` 中 A5 的依赖不自动解除。当前实现边界与下一步以第 8 节为准。

## 2. 推荐边界与方案比较

| 方案 | 优点 | 限制 | 本次建议 |
|---|---|---|---|
| 内嵌 DSH 网页 | 接入快、界面完整 | 保留手机布局问题；依赖网页 Cookie、嵌入与导航行为；不是 Bob 原生体验 | 仅作为显式“打开原网页”后备入口 |
| Bob 原生 Work + 通用连接契约 + 各 Agent Adapter | 用户使用同一套界面，适配器处理运行端差异 | 要实现认证、流协议、交互与恢复；需固定各端版本 | 主路径，DSH 首接，再用第二个执行端检验抽象 |
| ACP Adapter | 适合标准化自动化控制与将来多运行时支持 | 此版本是 stdio 控制面；不能直接视为现有网页会话的替代接口，缺少完整历史/UI 特性 | 保留后续评估，不作为此轮前置 |
| VPS3 新增统一网关 | 可供未来多客户端复用 | 增加部署、凭证托管、代理与故障层 | 本轮不需要；后续另行授权 |

调用链：`Work 共用聊天/项目界面 → tauri-bridge.js → Bob Remote Agent 契约 → DSH / OpenClaw / Hermes Adapter → 各端原生接口`。HTTP、SSE、WebSocket 是传输方式，Agent API 是业务协议，两者不可混淆。

- Bob 管理 Project、用户决策、Work 记录、外部引用与本地展示缓存；DSH 管理执行会话、历史、工具、运行目录和执行结果。
- DSH 工作区不自动等同于 Bob Project。首次绑定显式选择；用 `endpointId / workspaceId / sessionId` 联合标识外部会话，不用标题或 URL 单独作为身份。
- 初版只做映射与读取缓存，不双写两套聊天历史为权威来源，不复制 DSH 数据库，不把 DSH 执行完成自动等同于 Bob Project 完成。
- DSH 接入认证与 Bob-to-Bob 扫码持钥配对分开；“添加运行端”不能伪装成已有可信 Bob 设备。
- 未来 Online Agent 可复用适配协议和展示事件语义，但其认证、部署与执行授权独立；不以本轮需求授权更改 VPS3/X1T 服务。
- Bob 必须可独立安装、发布并直连执行端；cloud-agent 是用户个人的可选工作平台，不是 Bob 的必需后台。统一的是客户端契约与能力协商，不是部署、数据库或授权。X1T 是首个现场样本，不是协议设计与本地实现的前置条件。

### 2.1 通用接入契约（推荐设计，非现有代码）

不要求所有 Agent 实现同一网络协议；由 Bob 定义内部契约，适配器翻译各自协议。建议能力面为：连接/认证/断开、读取能力、列出远程范围及会话、读取历史、创建/续接会话、发送输入、订阅/恢复事件、回答问题/审批、请求取消/查询结果、读取产物。基础必需项与可选项分开，缺失能力显式为 unsupported/unknown，不能用假成功填满接口。

| 对象 | 归属与约束 |
|---|---|
| Connection | Bob 保存逻辑端点 ID、适配器类型、已核实版本与能力、地址配置、凭据引用；网络地址不是设备身份，凭据不随一般配置同步 |
| Project | Bob 管理目标、任务、决策与产物关联；可挂多个 Connection，不等于任意 Agent 的一个文件目录 |
| RemoteSessionRef | 关联 Connection、原生 Agent/profile、远程会话 ID 和可选 workspace；无 workspace API 时不虚构远程项目，只做 Bob 本地归类 |
| Execution | 记录提交 ID、远端 run ID（若支持）、执行状态、恢复游标与副作用未知态；远端状态保持证据来源 |
| ArtifactRef | 指向可验证的产物及访问方式；支持远程引用或明确的本地缓存，不自动把远程绝对路径当手机可访问地址 |

通用事件包含文本增量、工具状态、待输入/待审批、产物、执行终态和恢复信息。保留有版本的安全扩展，避免将不同协议全部挤成纯文字；未支持的上游特性清楚标记，不隐式丢弃。

能力发现必须结合已测试版本、端点声明和当前权限。模型声称“我能操作文件”不算能力证明。列历史、附件上传、远程文件读取、审批、取消确认、断线续接、服务端持久运行分别判定，不只有一个 connected 布尔值。

### 2.2 一致的添加入口，不一致的认证由适配器处理

用户流程：添加执行端 → 粘贴地址/导入二维码 → 无凭证识别或手动选择类型 → 按该端协议完成认证 → 展示可用能力 → 绑定 Bob 项目或进入已有会话。

- URL + token 是交互外形，不是统一认证标准。不得将用户凭据依次发给多种猜测接口来识别服务。
- 用户提供带 token 的 URL 时在本机解析，身份验证之后不在显示地址、分享或日志中保留秘密。二维码是接入信息载体，不天然提供建信或撤销保障。
- DSH 使用其 Cookie/Peer 生命周期；OpenClaw 使用其 Gateway 握手、角色/权限与设备认证；Hermes 依据实际启用的 API 或 Dashboard 认证机制接入。必须分别兼容，不转换成 Bob-to-Bob 的信任记录。
- Hermes 的 OpenAI 兼容执行 API 可以作为入门能力，但不能据此宣布已接入全部原生项目、历史、审批和文件；应按实际版本探测更完整接口。
- Agent 执行端与模型供应商分开：Agent 可以运行工具并保有状态，不得把执行端伪装成普通模型再由 Bob 运行第二层自主工具循环。
- 远端授权决定能执行什么，Bob 可进一步限制但不能扩大。没有远端审批能力时，不展示有强制保障的批准按钮；未验证可控性的任务保持禁用或显式在原端处理。

### 2.3 通用性和可靠性的验收边界

“通用”第一轮须由至少两个不同协议的适配器证明：第二个适配器接入后不复制 Work 页面、不加入品牌专属核心状态分支。适配器增加和升级可以需要 Bob 发版；首期不建设任意代码插件商店。

同一项目可以选择不同执行端，但运行中的会话不因切换下拉菜单迁移。交接必须显式创建新的执行记录，经过用户确认选择摘要/文件，并说明旧工具状态、进程、文件系统和权限不会自动继承。审批绑定原 Connection/Session/Execution，不得跨端复用。

手机切后台、断网或关闭 App 不等于远端停止；只有服务端明确支持持久执行时才能承诺后台继续。恢复后先查询/续接，不重新发送未决写操作；无幂等或查询能力时保留“执行结果未知”，由用户核实。首期不为保活增加后台常驻耗电机制或服务器推送依赖。

多设备使用同一 Bob 项目时，项目引用可按现有策略同步，但认证仍由每个客户端单独完成；不能同步 Cookie 或凭据来伪装已授权。

## 3. 原生界面规格

- Work 原有概览、目标、任务、决策与动态保留；项目内增加“执行会话”入口，进入聊天工作区。不将整个 Work 替换成 DSH 首页。
- 宽屏左侧显示项目与会话列表，可折叠；手机默认隐藏为抽屉。打开后可查看、搜索与切换已有会话，切换不自动新建会话。
- 顶部显示当前项目/会话、执行端和连接状态；底部复用 Bob 输入框、附件入口及停止操作。
- 主区复用消息、Markdown、代码块与文件/图片卡片；抽取适量展示组件，不复制整份 ChatView，不将 DSH 前端全量插件树塞入 Bob 启动链。
- 工具过程、可展示的推理摘要默认收起；最终回答和产物优先。仅显示服务实际返回且允许展示的内容，不承诺获得模型隐藏推理。
- 待回答问题/批准操作必须明显可见，不能折叠到用户看不见的位置。不得把 Bob 原有 Diff 审批保障直接套用为 DSH 保障。
- 连接恢复时保留草稿和已读历史，显示克制的恢复提示；持续不可用时提供“重连/打开原网页”。任务是否运行、是否完成由 DSH 事实决定，不由连接灯推断。
- 主题沿用 Bob 变量与 logo 品牌色，使用现有矢量图标及双语键；手机横竖屏、键盘弹起和长会话不遮挡会话列表与输入框。

## 4. 首个 DSH Adapter 的上游协议约束与实现验收

### 4.1 认证与连接生命周期

该版本浏览器通过根路径 query token 换取 Cookie；程序接口通过 `GET /connect?token=...&interactions=answer|none` 返回 302、Mux 位置和 API Cookie。不是普通 Bearer Token API，HTTP carrier 不接受 Authorization-header token。

适配器必须：

- 在 Rust 层管理端点专属 Cookie/session，不让 Vue、同步导出、日志、诊断包或截图包含 token/Cookie。推理供应商密钥继续留在 X1T。
- 外部入口使用 HTTPS/WSS；验证重定向目标，禁止自动携带凭证跨到未授权主机。不修改现有 Host/Origin 校验来绕过失败。
- 精确处理 302 与 Cookie，而不是盲目自动跟随为普通 GET；同一 API Peer 的 HTTP/WS 使用同一认证上下文。
- 每个 Peer 只建立一个拥有生命周期的 API socket。断开/Host 重启后重新 `/connect`；不能无限重用被销毁 Peer 的旧 Cookie。
- 需要问答/审批的交互模式实现 `interactions=answer` 与 `$events` ready 握手；WS open 不是 ready。使用单一重试调度器，取消上一代监听并隔离迟到事件。
- 区分“从 Bob 删除连接”和“在 DSH 撤销凭证”。上游此版本没有单会话 logout，不能将本地清除表述为远端撤销，也不能擅自撤销全部用户会话。

风险：launch token 建立的是 owner 权限 Peer，不是天然按项目限权。上线前须明确可信个人端点范围；若要多用户/强项目隔离，必须另行做服务端授权，不能用 UI 隐藏替代。

### 4.2 会话、流、审批与取消

- Unary 使用 `/api/<namespace>/<method>`，参数是命名 `args`；实时订阅通过 `/api/remote.mux`。按该版本描述符、帧和解码契约实现，不能按 OpenAI Chat Completions 或简单 JSON WebSocket 猜测。
- 先获取工作区和会话列表及分页历史；恢复连接重新获取基线，按稳定事件标识/序号归并，防止快照与增量重复显示。
- `prompt.requestId` 是幂等标识。请求受理不等于执行完成；网络超时后核实原请求，不生成新 ID 无条件重发。
- UI 分离 connecting、ready、reconnecting 与 submitted、running、needs_input、completed、cancel_requested、cancel_confirmed、failed；具体由哪些上游事件支持，须在协议试验中列出映射。
- 审批、用户问题与取消使用上游真实接口及结果。无响应时保持未知/等待，不能声明已批准、已停或已完成。断线后过期问题不可继续提交。
- 协议试验未确认安全交互/权限行为前，仅浏览历史，不以“提示模型只读”代替权限边界。

### 4.3 产物与 Work 记录

- 用会话限定的文件接口读取真实产物；手机不能使用 X1T 本地路径作为可打开链接。
- `.png`/`.jpg` 直接预览；`.blend`、Godot 项目文件等显示类型、大小、来源会话与下载入口。没有真实文件就不渲染“已生成”卡片。
- 按二进制/multipart 与分段读取契约处理大文件，限制大小/缓存、检测版本变化，不将大文件全塞进消息 JSON。
- 文件接口本身不应被假定为严格的工作区沙箱；预览和下载限定已授权来源，进一步的隔离必须有服务端保障。
- 任务完成后生成可追溯的 Work 外部执行记录；接收为正式 Artifact/Decision 与项目完成是独立动作。初期不自动批量导入历史记录。

### 4.4 Bob → DSH → Bob 的第一条直连数据契约

本节于 2026-10-22 按用户要求核对上游源码，固定 `dsh-v0.2.1-rc.2` 的 commit `687f282e8625c6b8d11ff58211a90524be2e4317`。上游事实与 Bob 待实现映射分开；不是已完成发送功能的报告。

#### A. Bob 输出：用户意图，不是第二套 Agent 循环

Bob 向自身原生适配器提交逻辑端点/连接句柄、远端会话 ID、稳定的输入 requestId、当前输入内容以及可选时区。Bob Project ID 只用于本地关联，不作为 DSH 的 sessionId。凭据走连接上下文，不混进 prompt。沿用 DSH 历史，不默认上传 Bob 的完整聊天、记忆、模型密钥或重新生成系统提示词。

DSH 的真实输入为 `SessionPromptRequest`；以下为 `POST /api/session/prompt` 的请求示意，ID 均为占位示例：

```json
{
  "type": "client-request",
  "rpcId": "rpc-example",
  "method": "session/prompt",
  "payload": {
    "args": {
      "request": {
        "requestId": "input-example",
        "sessionId": "remote-session-example",
        "mode": "queue",
        "content": [{ "type": "text", "text": "用 Blender 创建模型并渲染一张图片" }],
        "clientTimeZone": "Asia/Shanghai"
      }
    }
  }
}
```

- 外层 rpcId 关联一次通信；内层 requestId 关联一次用户输入，写入 DSH 用户消息 source.rpcId，用于队列/历史核实与去重，二者不混用。上游会对已排队或已记录的相同 requestId 返回原受理；Bob 仍须处理“受理未知”，不在超时后生成新 ID 重复派发。
- `queue` 对应普通续问/排队，`steer` 对应运行中引导下一步；首个文字闭环只开放 queue，不把 steer 宣称为立即终止。
- 选取已有会话可直接使用其 sessionId。新建时调用 session/create，传已选 workspaceId，并以响应返回的 sessionId 为准；不能自行拼接宿主 cwd。DSH 负责模型、工具、运行目录与上下文。
- 上游还支持 image（mediaType/data/name）及 file（同一 Session 上传所得 receiptId）。这些是后续能力，不把手机本地路径直接发给 DSH 冒充可读附件。

#### B. DSH 输出：受理回执、事件流、文件读取三条不同通路

| 上游事实 | Bob 的待实现投影 | 不得作出的推断 |
|---|---|---|
| prompt RPC 的业务结果 `{accepted:true}` | 已接收/排队 | 不能标为完成、文件已生成 |
| session/follow 的 snapshot 与随后 event（含 seq/time/data） | 恢复历史、递增更新及去重 | 不能重复追加快照与同一 seq；需遵循 surfaceOp 替换语义 |
| 开启 assistantStream 后的 start/chunk/end | 流式文字、可折叠的服务可展示过程 | chunk 未必是纯文本；assistant end 不等于整个任务完成 |
| assistant/message 与 tool/result 等持久事件 | 最终文字、可折叠工具结果/错误 | 不能将工具输出一律当最终答案或用户消息 |
| turn/end 的 reason.kind | completed→轮次结束；aborted→轮次中止；blocked/error/max-tokens 等分别呈现 | 不能靠 socket 关闭、running=false 或“已完成”文字判断成功；轮次结束不等于 Bob 项目通过验收 |
| `$events` 的 approval/request、user-questions/request | 独立的批准/拒绝、用户回答卡片及回传 | 不自行生成虚假审批；不在无人回答时自动允许 |
| workspaceFiles/stat、readBytes 或 session/attachment | 经核实的文件元数据、图片字节、下载流 | 远端路径不是本机路径；文字提及文件不是文件存在证明 |

DSH 的审批只授予一次请求，且请求本身未必带完整工具参数；Bob 展示实际可获得的工具名、原因和关联调用，缺失内容明确提示，不编造 Diff 或沿用 Bob-to-Bob 的写前暂存保障。`interactions=answer` 的 `$events` 必须 ready 后才开放可交互执行，答复需绑定当前连接代际、会话和未决交互。

取消调用 `session/cancel` 的 `{accepted:true}` 仅表示取消已请求；上游使用 `keepInbox:true`，并不清空后续排队输入。Bob 等待对应轮次终态，保留“取消待确认/状态未知”，不能承诺所有子进程和外部作业均已停止。

#### C. 传回 Bob：统一展示对象，不把 DSH 原始帧泄漏到页面

计划由原生适配器输出：连接状态、输入受理状态、消息更新、工具过程、交互请求、轮次终态、产物引用。各对象带 connection/session 标识；有来源事件的保留 seq，流式片段保留 attempt/index，输入关联保留 requestId。通用类型不得以 DSH 专属事件名作为 Vue 分支条件；适配器负责转换和明确 unsupported。

产物两阶段：先得到远端引用并查询 stat/版本，再按用户操作或受限预览策略读取 bytes；下载完整、大小与版本核对后才成为可用本地缓存。PNG/JPEG 可安全预览，blend/Godot 文件以文件卡片展示；未知/HTML/SVG 不自动执行。读取接口本身不是 workspace 沙箱，必须再限定已授权文件来源、大小及访问范围。生成日志里只有一个路径时标为“待核实”，不存在或无权读取时明确报错。

Bob 是客户端和 Work 记录拥有者，DSH 是执行与会话事实拥有者。不依赖 cloud-agent，不连接 X1T 也可完成契约测试；实际版本与访问权限只在现场验收时核实。

#### D. 第一条执行链路的固定准出标准（RH-02，待实施）

1. 从 Work 选择已有 DSH 会话，先建立事件/交互订阅，再提交一条文字；同一输入具有稳定 requestId。发送超时不伪报失败重发，也不伪报成功。
2. 对 accepted、运行中、待回答、轮次 completed/aborted/error 分别测试；文字增量与最终历史不重影，切会话/断线后的迟到事件不得污染新会话。
3. 测试真实协议形状的审批允许、拒绝、撤回、过期及连接丢失；任何未获准操作不得由 Bob 自动授权。取消受理不得直接显示已停止，队列保留语义明确。
4. 故障后通过 snapshot/page 和原 requestId 重建事实；缺口、未知事件或不可核实结果保持可见，不靠“连接在线”猜执行成功。
5. 同一会话连续追问成立；Blender 图片先有可核实的远端文件再预览，文件不存在、越权、超过大小限制或读取途中版本变化均不生成虚假可下载卡片。
6. 本地协议/状态/组件测试与真实端点验收分别记录。当前仍是浏览子集；本节核对不改变完成状态、不触发构建或发布。

源码依据：上游 session-controller/src/types.ts、index.ts、commands.ts；core/session/src/types.ts；api/remotes/src/remote-events.ts；client/connection、client/file-upload 与 api/workspace-files 的 README（均固定到上述 commit）。

## 5. 重连问题的独立诊断

现有症状不足以确认根因。上游会在 `$events` 未 ready、事件流结束或格式错误时重建连接；HTTP 可用并不证明事件流健康。因此“任务还在工作但连接灯反复变化”在架构上可能发生，仍需现场验证。

只在获准的诊断窗口对比：同版本 X1T 本机直连与现有外部入口，分别观测静置、普通对话、长任务和手机前后台切换。

采集脱敏证据：HTTP/WS 状态码、是否 Upgrade 成功、关闭码、重试间隔、ready 到达时间及代理日志相关错误；不采集 query token、Cookie、聊天正文或模型密钥。优先排查转发路径/Upgrade、Host/Origin/Cookie、一致的 API Peer 生命周期及代理空闲超时。未采样前不得宣称 Tunnel 或 DSH 某处是确定根因。

## 6. 分阶段任务与固定验收门槛

以下是阶段验收标准；实际已实现子集见第 8 节。阶段成果分为本地验证、目标端点验证、双端安装验收，不能互相替代。

| 节点 | 交付 | 准出标准 |
|---|---|---|
| RH-00 通用契约与能力矩阵 | Connection/RemoteSession/Execution/Artifact 最小契约，DSH/OpenClaw/Hermes 协议差异表，公共测试夹具 | 前端不依赖 DSH 私有字段；无项目/取消/审批能力能诚实降级；认证与执行状态分离；不改变当前 Bob 本机和已配对 PC 路径 |
| RH-01 协议与连接试验 | 固定 rc.2 合约、Rust 最小连接原型、重连诊断记录 | 能获取真实工作区/会话/历史；错误凭证与跨域重定向拒绝；socket 断开后重新认证恢复；报告已证实原因或明确未定位，不隐瞒重连 |
| RH-02 Work 原生聊天最小闭环 | 项目映射、侧栏/手机抽屉、消息渲染、发送与实时回复、问题/审批/取消 | 电脑和手机能进入同一 DSH 旧会话连续追问；不多建会话、不重复执行；问答/审批只提交一次；取消无确认不标已停；DSH 故障不阻塞 Bob 首屏 |
| RH-03 真实工具与产物 | Blender 场景、图片预览/下载、Work 引用 | 复用原有 X1T 配置完成一次建模渲染；手机打开实际图片并下载真实产物；核对文件字节/大小/来源；Godot 单独验证，不因 Blender 成功就宣称全部工具可用 |
| RH-04 恢复、双端与候选交付 | 故障矩阵、兼容测试、同版本 PC/APK 候选 | 断网、前后台、Host 重启、超时未知结果均不丢草稿/重复执行/假完成；旧浏览器与 Bob 切换无历史重复；PC/APK 同源同版，构建与安装验收分别留证 |
| RH-05 第二适配器验证通用性 | 推荐 OpenClaw（以用户可用实例与版本为准）；Hermes 后续接入 | 复用同一 Work UI 完成真实会话、发送/结果和故障恢复；不复制页面、不混用凭据；可选能力准确显示；完成前只称“DSH 首接已验证”，不称多 Agent 已交付 |

RH-00 只做最小公共契约，不建设通用调度平台。RH-02 是尽早可用节点，不等待其他 Agent 全部适配；但不能跳过认证和真实交互确认。RH-05 先做协议映射评审，尽早反证公共设计，完整联调在 DSH 最小闭环后推进。RH-04 的构建、部署、覆盖安装另按当次授权执行。版本不兼容时明确提示，不静默降级成另一套未经验证的协议。

首期范围外：VPS3 统一调度、任意第三方插件商店、多人共享与细粒度授权、Godot 编辑器嵌入/远程桌面、自动导入所有项目、修改各 Agent 原有 UI。其他 Agent 的适配属于本产品路线，但不应阻塞先让用户在 Bob 中使用 X1T 的闭环。

## 7. 可核对来源

- [固定版本发布页](https://github.com/deepseek-ai/deepseek-harness/releases/tag/dsh-v0.2.1-rc.2)
- [认证、Cookie、API Peer、ready 与重试](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.1-rc.2/packages/client/connection/README.md)
- [RPC 与 Mux 契约](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.1-rc.2/docs/api-gateway.md)
- [会话与历史](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.1-rc.2/packages/api/session-controller/README.md)
- [工作区](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.1-rc.2/packages/api/workspace-controller/README.md)
- [文件读取](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.1-rc.2/packages/api/workspace-files/README.md)
- [交互审批](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.1-rc.2/packages/interaction/user-approval/README.md)
- [ACP 范围](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.1-rc.2/packages/acp/acp/README.md)
- [OpenClaw Gateway 协议](https://docs.openclaw.ai/gateway/protocol)
- [OpenClaw 认证与权限](https://docs.openclaw.ai/gateway/protocol/auth)
- [Hermes 执行 API](https://hermes-agent.nousresearch.com/docs/user-guide/features/api-server)
- [Hermes Dashboard 与认证](https://hermes-agent.nousresearch.com/docs/user-guide/features/web-dashboard)
- Bob 内部依据：`src/views/WorkView.vue`、`src/views/ChatView.vue`、`src/tauri-bridge.js`、`docs/PRODUCT_VISION.md`、`docs/DECISIONS.md`、`docs/BOB_EVOLUTION_ROADMAP.md`。

## 8. 当前实施边界与下一个可执行任务

### 已实现：RH-00/RH-01 浏览子集（本地验证，不等于节点全量准出）

- 原生通用 `RemoteAgent` 接口、连接句柄、能力矩阵、项目/会话/文字记录契约；品牌字段仅出现在适配器注册及连接表单，不向 UI 暴露 DSH 原始帧。
- DSH rc.2 `/connect` Cookie 认证、同端点 Mux、工作区基线、会话列表、近期历史快照。持久在线 socket 处理 Ping；流快照后关闭订阅。断线显式报错，需重新输入令牌认证，不自动重发任何指令。
- Work 新增按需加载的“远程 Agent · 接入预览”；复用 Bob Bridge、主题、多语言与 Work 页面。宽屏可折叠项目列表，手机选择后收起；旧 Work/Chat/配对 PC 流程不替换，无新增启动网络任务。
- 令牌与 Cookie 仅驻留本次连接内存，连接关闭时释放；不写用户配置、数据库、同步数据、浏览器持久存储或日志。拒绝公网 HTTP、跨端点认证跳转。DSH launch token 本身仍可能有 owner 权限，“浏览预览”是 Bob 客户端功能限制，不是服务端只读授权。
- 历史仅投影直接用户文本与 `assistant/message.data.message.content` 文本，其他记录明确计为未展示。不会把远端 HTML、文件路径、图片 URL 交给本地富文本/文件打开器。当前不是完整的历史 surface 折叠、分页或实时对话实现。
- DSH `session/follow` 上游会在快照后尝试激活已有 Agent；本客户端不发送 prompt，但不能宣称读取在宿主内部绝无激活或持久化副作用。接入只用于用户明确授权的个人执行端。
- 不支持发送、创建会话、审批、取消、附件、产物、Bob Project 持久映射、自动重连；能力矩阵对应返回 false。也未实现 OpenClaw/Hermes 适配器。

代码入口：`src-tauri/src/remote_agent/`、`src/components/work/RemoteAgentPanel.vue`、`src/work/remote-agent-state.js`、`src/tauri-bridge.js`。证据：[本地接入子集](evidence/remote-agent/RH-01-native-read-preview.md)。

### 下一项：RH-02 原生执行闭环 → 目标端点验收

1. 按第 4.4 节先完成通用输入/输出与 DSH 映射的本地契约测试；无需用户先提供 X1T 令牌，不能把特定设备作为框架实现前置。
2. 建立重新认证与代际隔离并保持用户草稿。具体外部入口的重连原因只在现场阶段诊断，本地测试不推断其网络故障。
3. 实现 `interactions=answer`、`$events` ready 与实际问题/审批响应，冻结受理/运行/待确认/完成/取消状态映射；在这些行为本地负向测试通过前不开放发送。
4. 开放既有会话中的一次 prompt，固定 requestId，验证连续追问、未知受理不盲目重发、取消不假成功。最终才连接 Blender 产物与 Work 引用。此时才能称“第一条可远程干活链路可用”。
5. 本地闭环后再以用户安全输入的端点/令牌核验实际版本和现场行为；在适当节点按单独授权走同源同版本 PC/APK 候选构建。当前未构建、未发布、未进行真机验收。

### 与 Online Agent 的复用边界

已只读检查 `cloud-agent` 当前阶段指向的工作树：其 AgentAdapter / RuntimeAdapter 与 Core 工作事实分离可作为设计参照；该工作树的 Remote DSH 路径为 SSH 启动隔离 CLI 任务（代码声明旧版 DSH 0.1.5-rc.3），不是接入 X1T 现有 rc.2 Web 会话。只复用抽象与状态语义，不直接跨项目 import 或复制整套运行器，不让 VPS3 成为 Bob 直连的强制依赖。

`agent.bobbik.org` 公开页面本轮读取未成功，本地源码观察不代表该域名认证后实际功能已验证；没有操作线上服务、凭据、数据库或当前会话。
