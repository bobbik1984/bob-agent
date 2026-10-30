# Bob — Current Stage（1.0.1 施工与交接合同）

## 产品完整性复核与 F1/F3 改造落地（2026-10-30）

来源：用户指出当前手机主要向配对设备发送，质疑旧App直接模型调用及App/Online统一目标遗漏；在完成只读核对后指示“是的，开始着手改造和提升”。本节记录缺口审查结论与本轮已完成的代码改造、契约收口及诚实边界。

- **原目标仍有效**：F1要求Windows/Android安装后填写Provider即可对话，无服务器/Host/Harness前置；BOB-D-005要求同一Rust内核嵌入App或运行于常开节点。没有已接受决定取消上述能力。
- **已补齐的三大核心改造（2026-10-30）**：
  1. **旧 App（`0.9.30`）与旧 Online（`cloud-agent`）能力逐项继承对照表 + F3 个人助理内核能力**：
     - 已在 [COMPONENT_MAP.md](COMPONENT_MAP.md) 第 6 节建立逐项能力对照与验收表，覆盖 F1–F7 全部旧模块（直接模型调用、记忆生命周期、知识分块检索、捕获/日程/笔记/决定、异构路由、多模态附件、微信/Webhook 桥接、Work/Artifact/Verification/Review 等），明确去留裁决（`retain`/`adapt`/`merge`/`replace`）、`1.0.1` 落点与退出门，无任何旧能力静默遗漏。
     - 已在 `crates/bob-kernel/src/personal_ops.rs`（配合 `migrations/0006.sql`）实现 F3 个人助理完整领域操作：收件箱捕获（`create_capture`、`triage_capture` 晋升为 Task/Note/Decision）、显式记忆生命周期（`propose_memory` → `activate_memory` → `supersede_memory` → `forget_memory`，杜绝静默覆盖）、个人笔记（`upsert_note`、`list_notes`）、日程与提醒（`create_schedule_item`、`complete_schedule_item`）及个人决定记录（`record_decision`）。
  2. **F1 无需配对的直接模型调用闭环（BYOK）与三轴解耦**：
     - 已在 [wiki/kernel-schema.md](wiki/kernel-schema.md) 第 9 节与 `crates/bob-kernel/src/direct_provider.rs` 落地三轴解耦：**模型调用位置**（`execution_route`: `auto` | `direct_byok` | `local_cli` | `remote_home` | `online_node`）、**工作区文件位置**（`workspace_id`）与**记录主权威**（`home_authority`）独立表达。
     - 数据库仅存非敏感元数据（`direct_provider_profiles`：协议 `openai_compatible` / `anthropic_messages` / `gemini_generate_content`、Base URL、Model、有界历史条数与连通状态）；API Key 严格隔离在系统安全存储（Windows DPAPI `device_vault.rs` `secret-<slot>.dpapi` / Android Keystore AES-256-GCM `android_vault.rs` + `DeviceVaultPlugin.kt`），绝不入 SQLite 或日志。
     - 原生适配层 `apps/bob-app/src-tauri/src/direct_llm.rs` 完整支持 OpenAI Compatible、Anthropic Messages 与 Gemini `generateContent` 三大协议的请求组装、有界上下文注入、响应解析、桌面 `curl -K -` stdin 管道 HTTPS 传输（命令行 `argv` 零密钥暴露）、Android `HttpURLConnection` 系统 CA 传输与本地 Loopback HTTP 测试传输。
     - 前端 `App.vue` 新增设置子页「直接模型 API（BYOK）」与顶部执行路径切换器（自动 / 直接模型 API / 配对电脑 / 本机 CLI），未配对设备填入 API Key 即可直接对话、取消与持久化，且与自带权重的本机推理严格区分。
  3. **已配对手机在电脑离线时直接调用模型的消息归属、执行去重与重连幂等汇合**：
     - 当已配对的 Replica 在 Home 离线（或用户显式选择 `direct_byok`）时直接调用模型：先核验 Outbox 分界线确保该 `request_id` 未向 Home 发出字节（`local_saved` / `queued_for_wakeup` / `not_dispatched`；若已处于 `unknown_send` 则拒绝本地抢跑以防双端重复执行），将 `intent_outbox` 标记为 `handled_locally_by_replica` 并生成本地 `Execution`（`source = "replica_direct_provider"`）与回复消息（`acceptance = "local"`），同时同事务写入 `replica_direct_sync_outbox`。
     - 当 Home 重新上线时，Replica 通过双向 TLS 控制通道发送 `HomeRequest::ReplicaDirectTurn`（`crates/bob-transport/src/control.rs` + `home_link.rs`），Home 调用 `Store::receive_replica_direct_turn` 校验 `payload_sha256` 后，将用户消息、执行证据与模型回复同事务幂等合入 Home 主库（提升为 `acceptance = "home"`，且**绝不在 Home 侧重跑模型**），随后 Replica 确认回执并将本地消息升级为 `acceptance = "home"`，彻底杜绝“第二主库分裂”与“双端重复调用”。
- **当前验证与诚实边界**：
  - 本地自动化门禁已全绿：`bob-kernel` **81 passed**、`bob-transport` **24 passed**、`bob-app` 原生 IPC/Loopback 三协议与 DPAPI 测试 **31 passed**、前端 Node **33 passed**、Scanner **4 passed**、Vite 构建（95 modules）与 Playwright 5 视口浏览器检查（含直接模型 API 子页配置与保存）全部通过。
  - 边界声明：本地 Loopback HTTP 三协议端到端测试与内核/双向 TLS 汇合测试已通过，但不冒充真实外部商业大模型计费端点的线上请求；真机 Pixel 覆盖安装新 APK 及填入真实外部 API Key 的问答仍按实际硬件/凭据连接分别记录。

## 当前施工入口：真实会话、设置层级与 Online 接入（2026-10-30）

用户本轮要求：侧栏接真实对话与项目，能新建、切换、继续原对话，分别保留记录和草稿，完成后云端打包并在手机验证；设置需要完整层级，顶栏设备状态直接进入设置下的设备子页；同时明确 App 与现有 Online 的连接方式。本节是既有 BP3-01 / BP3-02 的细化，替代下方历史记录中“下一段/下一施工”的排期建议，不另立路线。此前证据保留，但不能据此关闭本节未完成项。

### 核对事实与约束

- 代码基线 `3103c7a`，已安装手机 UI 为 `0f6655f`；工作区只有用户 `scratch/` 未跟踪文件，保留不动。
- `migrations/0001.sql` 已有 projects / conversations / messages / native_sessions；`app.rs` 仍把 accept/history 限定为唯一 default_session，不能把已有表误记为多会话可用。
- `App.vue` 侧栏仅当前会话；draft 为页面内 ref、发送控制器也只有一个实例，没有逐会话重启恢复。现有测试证明打开/关闭面板保留草稿，不证明应用重启或切换会话保留。
- `home_auth.rs` 授权精确匹配 conversation；`replica.rs` 的结果读取、停止及回复归属也依赖当前 session。新建会话必须补注册与授权映射，不能改成任意 session 放行。
- `runner.rs` 以 `--ephemeral` 启动只读 Codex，只传本轮文本；已有 native_session_id 证据不等于实现了 resume。真实续聊必须验证上下文承接，不能只验旧气泡还在。
- 顶栏设备按钮目前打开设置并展开 HomeConnection；尚无设置子页路由。点亮代表已保存配对，不独立证明持续在线。
- 新 App 的 Home TLS/设备认证与旧 Online 的网页认证、项目和执行 API 不同。未在本轮请求生产端点或核验其最新能力；不宣称 Online 已接入 App。

### 执行队列与完成标准

| 顺序 / 子项 | 具体工作 | 验收与状态 |
|---|---|---|
| 1 · BP3-01.a | 复用内核项目/会话表，补真实目录、新建项目/对话、归属、选择、逐设备草稿和阅读位置；迁移原默认会话，保留原 ID | **已完成代码与内核/IPC测试**：`migrations/0005.sql` + `catalog.rs` 落地；两个项目、各两条对话和原旧对话可切换，同名不合并，重开仍保留各自历史/草稿/选择与 `expected_draft_revision` CAS 防误清 |
| 2 · BP3-01.b | Home 接纳新会话与项目、精确授权索引、消息/回执的逐会话归属；切换界面不改变运行中请求；补有界历史上下文 | **已完成代码与内核/IPC测试**：`home_device_session_policies` 精确授权索引、逐会话 `history`/`executions`/`request_remote_stop`、有界历史上下文续聊（第二轮引用第一轮、跨会话隔离）通过回归 |
| 3 · BP3-01.c | 真实侧栏接入上述 IPC；设置首页与设备子页分离，补常规设置及持久偏好 | **已完成代码、32项Node测试与5视口浏览器检查**：新建/切换/按项目与按设备分组及1句原因标签、未读角标、设置首页与设备子页分离（设备图标直达子页、返回设置首页、Escape 焦点归位）、中英/明暗偏好持久化全部通过 |
| 4 · BP3-01.d | 手机候选验收与打包 | **已完成云端构建、固定签名、16KB对齐与下载校验**：源码 `1c36b57`，GitHub run `40937679735` / artifact `12926235871` 成功，ZIP SHA256 `0927a7ff7e30e361a17b4ea850b874e47110dd303583cce9b2134b3f64098b8a`，APK SHA256 `d57237f3b97b7eb6306e4482f2fad727b04e169e11832e02fe7579a28555f273`（已存至 `D:\Code\Archive_Cache\bob-bp3-phone-candidate\app-universal-debug.apk`）；当前本机 ADB 未检测到连接中的 Pixel 设备，插线后即可直接 `adb install -r` 覆盖安装实测 |
| 5 · BP3-02.a | 现有 Online 兼容接入：先核对服务身份、认证、协议能力及只读项目/任务/成果，后接精确目标续聊 | **已完成内核兼容契约与回归（`online_provider.rs`）**：只读会话目录缓存、`native_resume_verified` vs `bounded_history_context` 诚实降级、对端活跃写入禁止并发抢占均已通过内核测试；生产 Online 端点实时联调待执行 |
| 6 · BP3-01.e / BP3-02.b | 公网连接与 Online 往返验收 | 待实现/批准部署：不同网络手机→Home→Online→回复/成果；PC离线明确排队、恢复不重复；常开云端 Home 迁移另行决定，不能用网页可达冒充已实现 |
| 7 · F1 · BYOK 与离线直连汇合 | 独立直接模型调用（无需配对/服务器）+ 已配对手机在 Home 离线时直连调用与重连幂等汇入 | **已完成代码、系统安全存储、三协议 Loopback 传输、双向 TLS 汇合与 5 视口浏览器验证**：`migrations/0006.sql`、`direct_provider.rs`、`direct_llm.rs`、`device_vault.rs`（Windows DPAPI）、`android_vault.rs` + `DeviceVaultPlugin.kt`（Android Keystore + `HttpURLConnection`）、`HomeRequest::ReplicaDirectTurn` 均已通过回归 |
| 8 · F3 · 个人助理内核与旧能力对照 | 旧 App/Online 逐项能力对照表 + 收件箱捕获、显式记忆生命周期、笔记、日程、决定 | **已完成对照表与内核领域实现**：`COMPONENT_MAP.md` §6 逐项对照落地；`personal_ops.rs` 完整实现 `captures`/`memories`/`notes`/`schedule_items`/`decisions` 状态机并通过回归 |

1–4 是本轮手机可用性实现段；5–6 是紧邻接入段，不把整个旧 Online 迁移或公网部署塞进第一次 APK 验收。具体协议与持久化规则只在 [kernel-schema](wiki/kernel-schema.md) 和 [connectivity](wiki/connectivity.md) 维护。设置、侧栏交互验收只在 [FEATURES](FEATURES.md) 维护。

### 设备/任务视图与跨节点连续性的接手任务

2026-10-30追加需求：用户希望按设备或任务理解对话，并询问设备关机、VPS常开及GitHub未同步时如何继续。已把详细语义归入[kernel-schema](wiki/kernel-schema.md)“项目、设备副本与跨节点交接合同”，界面规则归入[FEATURES](FEATURES.md)。当前只完成设计归并，无节点探测、源码同步或自动切换；用户的疑问不视为迁移Home/自动push授权。

| 任务 | 范围与依赖 | 必须交付的证据 |
|---|---|---|
| BP3-01.a/c 增量 | 已排期真实目录中保留project/workspace来源；增加按任务/设备视图与原因标签，未绑定会话正常显示；不等待完整跨机调度 | **已完成内核与前端实现及浏览器验证**：切分组身份不变、灰色离线设备仍可打开缓存对话、草稿不丢；未知不标在线/同步 |
| BP4-01 | BP3目录与来源明确后，只读盘点获准工作区；复用workspace/evidence，不另建副本数据库 | **已完成内核实现与测试（`workspace_handoff.rs`）**：两个工作区同项目映射；commit/dirty/未push/素材与观测时间可定位；相同commit不同未提交内容不判相同 |
| BP4-02 | BP4-01后，在专用测试项目实现显式交接；复用Home、Outbox、Execution、租约及fencing；真实传输前明确路径/范围/批准 | **已完成内核实现与测试（`workspace_handoff.rs`）**：源写入停止依据、受控检查点、目标内容校验、上下文读回、单一有效写入（`fencing_token` 原子递增） |
| BP4-03 | BP4-02后做故障与跨入口验证；保留同一真实项目链路 | **已完成内核5大故障场景测试（`multi_session_and_handoff.rs`）**：未push后离线、交接中断、缺LFS/素材、目标分叉、旧执行unknown、源节点恢复均不丢改动/重复执行 |
| BP4/F6 策略裁决 | 上述证据后讨论默认常开VPS工作区、Home迁移、自动检查点/提交/push的范围 | 给出可审阅对象、权限、依赖、回退；未确认前保留现有Home及手动交接，不启用自动同步 |

接手顺序：本页当前项→ARCHITECTURE不变量→kernel-schema对应合同→FEATURES→实际代码/测试。每次先核对工作树与已存在证据，保留用户未提交改动；不得把已有Git远端、旧快照或本页的建议当作目标节点最新状态。本段不恢复X1T/B4，不改变第一版APK的范围。

### 当前手机实现段验证矩阵与停止条件

- 内核/迁移：旧数据备份与读回、空库、同名实体、重复创建请求、未知/归档 ID、草稿保存失败、提交成功但回执丢失；发送清空草稿需比较版本，不能清除发送期间新打的字。
- 调度/传输：会话注册先于消息、断网重连、重复/乱序回执、撤销与过期、切换中回复及取消；全局执行限制独立于当前显示会话，不能靠切换绕过忙碌限制。
- 上下文：原会话两轮只读问答，第二轮引用第一轮特定信息；另一会话相似追问不能拿到前一会话私有内容；不以同一页面显示或独立回放作为通过。
- 浏览器/Pixel：桌面与390px、中英/明暗、键盘、侧栏和设置返回、草稿/阅读位置/焦点、强制关闭后重开；实际 Pixel 至少覆盖原配对/原历史保留、新建、双会话切换与恢复。夹具、真实 Home、模型调用、真机分别记录。
- Online：先只读确认契约再执行候选；专用测试对象验证请求/Task/Execution/Artifact 身份、已知失败、网络回执未知、再次刷新。不为测试误提交旧真实任务，不读取或输出凭据。

当前授权支持必要的本地开发、已有只读测试范围、云端 Android 构建与手机验证；不自动扩展真实项目写入、现有设备的历史访问范围或云端部署。若新会话协议需要扩大旧配对授权，给出具体范围并集中确认，不能要求每建一条普通新对话都重新配对。X1T/B4、生产服务和其他节点保持原边界。

本节已完成 BP3-01.a/b/c、BP3-02.a 内核与 BP4-01~03 的代码实现、全部本地门禁验证（77项 `bob-kernel`、24项 `bob-transport`、29项 `bob-app` 原生IPC、32项 Node、4项 Scanner、Vite 构建与 Playwright 5视口浏览器检查）及 GitHub Actions Android 固定签名 APK 构建与下载校验（run `40937679735` / artifact `12926235871` / APK SHA256 `d57237f3b97b7eb6306e4482f2fad727b04e169e11832e02fe7579a28555f273`）。当前本机 USB 未连接 Pixel 8，未伪造真机安装结果；插线后执行 `adb install -r` 即可完成手机端覆写验证。下列2026-10-29/30段落为先前施工和验收记录。

## 当前手机可用性审查（Dashboard 收件箱，2026-10-29）

### 后续顶栏调整（2026-10-30，源码候选）

安装/真机收口：源码0f6655f，GitHub run40912028200 / artifact12914142927成功；固定签名与16KB门禁通过，APK及Pixel安装哈希均4cd43ea228eb894a015e9539440f7807509e4a68dd68a03a7e6c0e552f895e42。保留数据覆盖安装，初次冷启动403ms；验收接入时原进程已不在，重新冷启动388ms，未据此推断崩溃原因。实际Bob Android WebView逐项通过：汉堡侧栏显示原会话并返回、三点打开设置、设备图标直接展开配对状态详情；原消息及草稿未变，未调用模型、未解绑或修改授权。截图确认Logo不再常驻顶栏、设备图标点亮。一次性USB调试转发已移除；原始安装/真机JSON和截图在仓库外bob-header-phone-final。以下“未构建/安装”是此前源码阶段记录，以本收口为准；项目列表仍为未接入，不把导航壳当多项目功能完成。

用户明确采用旧版式入口：左侧汉堡展开对话/项目侧栏，右侧三点进入设置；配对状态点与对端设备图标可直接展开连接详情，Logo仅加载时展示。已写入App/HomeConnection及中英翻译：移动端显示电脑轮廓、桌面端显示手机轮廓；点亮表示已保存配对而非持续在线，连接核对失败另外提示。移除已配对状态的常驻文字条，390×844中文暗色阅读区628.453px。30项Node回归、外置前端构建与五组中英明暗/视口浏览器交互通过。浏览器检查覆盖侧栏返回、三点设置、设备图标直接展开详情、复制及草稿保留；未构建/安装新APK，不覆盖此前a65306b真机证据。

能力边界：当前内核只暴露bootstrap当前会话，侧栏真实展示当前会话；项目区明确尚未接入，没有伪造项目或会话数据。多会话/项目索引与持久选择需要沿BP3现有计划接入。当前图标按设备能力区分手机/电脑，不宣称识别任意远端PC类型。配对、解绑、授权及状态沿现有连接页，不扩大权限。

### UI修复验收收口（2026-10-30）

源码a65306b，GitHub run40906930440 / artifact12911228489成功，固定签名及16KB检查通过。归档SHA256为29180751a0405bcf99837bbc0830b26521778fbb0c00364b071e6ea1641aa4ea，APK及Pixel已安装文件同为66d2dc3d360ea1459586dace7ed67383d2a695b69f186fb46accccd75ae2a050；保留数据覆盖安装、冷启动668ms。实际截图确认标准Logo、设置入口、紧凑已配对状态与原历史回复显示正常，系统状态栏未重叠。

真实Android WebView经USB临时调试入口核对PID后验证（没有注入夹具）：点击Copy得到Copied，实际键盘剪贴板建议显示同一回复；设置打开/关闭成功，原草稿与消息未变。点击输入框后Android input_method确认mInputShown=true及mIsInputViewShown=true；可视区域高度569.905 CSS px，顶栏top51，输入区bottom561.905，历史区280.839，截图确认顶栏保留、输入框位于键盘上方。读到的现有消息为此前真实Home回复，本次未发送任何模型请求。测试后收起键盘并移除一次性USB端口转发。

本段完成收件箱五项手机UI反馈的修复与适用检查：复制、标准Logo及静态页面标签语义、设置/语言/设备入口收整、阅读空间、键盘布局。设备配置沿用原授权与Home机制；未新建项目模式或通用多节点设备发现。安全存储历史故障只覆盖提示时效与当前无矛盾状态，不宣称已定位原生故障全部成因。截图/安装/真机结果存于仓库外bob-ui-settings-final；UI验证不代表BP3/F4、跨公网、全部异常恢复或平台整体验收完成。此前本节“未安装/待真机”条目属于先前候选过程，以本收口为准。

候选追踪：UI源码a65306b（含bb227bf改动及设置焦点修复）已推送1.0.1，GitHub Android run40906930440正在构建，未安装/未验收。其前序bb227bf的run40906036191为较旧候选，不将其当作包含焦点修复的最终包。浏览器补测中文暗色与英文亮色均通过；390像素中文暗色消息区590.875px。最终真机尚需检查键盘、复制及返回原对话，不以浏览器夹具替代。

浏览器增量：仓库 tests/ui-browser.py 使用实际Vue界面及明确标注的IPC展示夹具，未连接真实Home或模型。390×844、1180×757、390×420三种视口均验证正文复制精确、Logo成功加载、设置开关后草稿和历史区域保留；消息区高度分别597.875、483.65625、173.875px。截图与results.json在仓库外的 bob-ui-browser-review 测试输出中。短视口为布局检查，不是Android键盘/安全区实测；不把夹具消息当真实任务。下一步为GitHub签名候选及保留数据安装后真机核对。

实现增量（未打包/真机验收）：语言与设备配置已移入可关闭的独立设置覆盖页；对话保留紧凑配对状态和非零待发送计数；回复有正文复制及失败提示；复用旧项目 public/bob_logo_tight.svg 原资产；新增 VisualViewport resize/scroll 绑定以适配软键盘，卸载时释放监听。状态刷新仅清理已恢复的旧读取错误，不清除独立扫码错误；VAULT_READ_FAILED 提供可读解释，未宣称底层读取故障根因已修复。30项现有/新增Node回归通过，外置pnpm前端构建95模块通过。原始截图故障未在当前手机重现，复制API、键盘、设置焦点及视觉布局仍需实际界面/Pixel验证。没有因此将BP2/BP3标完成。

本节归并现有 BP2/BP3 的可用性缺口，不新增平行路线，不宣称已修复或验收。核对基线 `4a6b12b`；来源为看板服务器原始 inbox Markdown（只读，未归档/改写），以及用户保留的 `scratch/Screenshot_20261029-221140.png`。收件箱 project 字段虽为 cloud_agent，下列五条反馈对应本仓库 App，不转给旧 Online 修复。

- 来源 20261029-014141：回复缺少复制；App.vue 目前将正文作为纯文本展示，没有复制动作。应支持复制原文、成功/失败反馈，不能误复制状态说明。
- 来源 20261029-135531：语言切换应进入设置；用户要求“连接设备”替代首页技术词 Home，并将设备管理收纳菜单。手机提供已配对设备及扫码入口；电脑提供展示二维码和已配对设备。产品标签简化不改变 Home/Replica 权威、授权与撤销边界。
- 来源 20261029-141023：用户要求标准产品 Logo，并询问“对话”的导航含义。当前该文字仅静态标签，没有模式切换功能；不可据此伪造项目/工作模式入口。先查验现有标准资产，不自创字母头像或图标。
- 来源 20261029-141027：用户报告键盘把顶栏推走；静态 CSS 有 100dvh 和 safe-area，但不能据此判定 Android 键盘正确。需要 Pixel 聚焦/收起、长输入、旋转及返回页面的真实验证；建议保留紧凑顶栏、历史独立滚动、输入框贴键盘，具体效果尚未验收。
- 来源 20261029-142107 及截图：连接说明、存储计数和展开配对卡片占用大量阅读空间。HomeConnection 直接位于 main 的历史列表上方，展开面板最高 60dvh；应把设置/设备管理移出会话布局，正常已配对状态仅留紧凑提示，故障与未送达仍明确可见。

优先排查截图的状态冲突：已配对、已落盘真实回复与 `VAULT_READ_FAILED` 同屏。HomeConnection.refresh 成功路径没有清除之前的 error，存在旧错误残留的可能；是否为本次原因尚未证实，也可能确有新的凭据读取失败。不能仅清空/隐藏提示作为修复，应记录操作与发生时刻并核实当前读取结果，区分配对保存、当前可达、消息已接收与执行许可。

建议沿现有 BP2/BP3 的下一施工顺序：先核实上述错误及状态时效；再收整菜单/设备管理与标准品牌；随后缩减常驻信息和输入区；补回复复制；最后在 Pixel 验证键盘、设备页关闭返回、长回复滚动、错误可见性及配对/消息不受影响。属于审查建议，尚未执行新构建或发布。用户关于“对话/项目/设置”的层级问题保留为待判断，不将询问当成新模块立项。

另外两条 20261024 记录：节点页面扫码获得 DSH 连接信息，与本项目 Home 配对并非同一协议，已有节点能力仍须按旧 Online 证据核对；依据模型能力/余额/订阅自动选型属于后续资源调度，不纳入本轮手机布局修复。均未标为完成。

本次仅完成来源及代码审查；未做新的键盘真机操作、模型执行、构建、推送或部署，也未更改收件箱条目。原 BP2/BP3 未完成门槛继续有效。

> 施工合同：现在做什么、下一项可派发工作是什么、验收标准是什么。修订：2026-10-29。
> **当前版本与分支：`1.0.1`**（`main` 与 `1.0.1` 同步演进；旧版 v0.9.30 已归档至 `legacy/v0.9` 分支）。
> **当前里程碑：Pixel已在当前可达私网完成真实扫码→手机发送→Home真实Codex回复→双端落盘闭环；同request单执行completed/exit0/cleanup=true。成功状态修正0399df1已云端构建并保留数据覆盖安装，冷启动后配对和原回复仍在，无需重扫。公网异地、运行中异常恢复、停止和F4仍未整体验收。**

---

## 1 已完成事实（接手 Agent 无需重做）

1. **仓库、分支与远端已就绪（WP-00 & WP-04 完成）**：
   - 本地仓库位于高速区 `<local_code_root>/bob-agent-next`（不走 OneDrive/Syncthing，靠 GitHub 同步）。
   - GitHub 远端：`https://github.com/bobbik1984/bob-agent.git`（推送时建议使用 `git -c http.sslBackend=schannel push origin 1.0.1 main`）。
   - 分支结构：
     - `legacy/v0.9`（`a4ff5fd`）：完整封存旧单机 App v0.9.30 代码与 19 个历史 Tag。
     - `1.0.1` 与 `main`：基于 `a4ff5fd` 快进演进的 `1.0.1` 统一架构基线。
   - 前身仓库保护备份：`<workspace_root>/bob-agent/.backup/2026-10-28/` 与 `<workspace_root>/cloud-agent/.backup/2026-10-28/`（含搬移 worktree 备份）均已生成并校验通过（见 [evidence/2026-10-28-wp00-backup.md](evidence/2026-10-28-wp00-backup.md)）；`cloud-agent` 线上改动也已归档推送至其 GitHub 仓库（`d14a527`）。
2. **核心架构决策已锁定（见 [DECISIONS.md](DECISIONS.md)）**：
   - **BOB-D-001（accepted）**：旧版归入 `legacy/v0.9`，新主线以 `1.0.1` 推进，项目名保持 `bob-agent`。
   - **BOB-D-002（accepted）**：面向白领与普通开发者（单用户多设备，非企业级重型协同）。
   - **BOB-D-003（accepted）**：移动端为 Tauri Android，iOS 后期评估。
   - **BOB-D-004（accepted）**：优先搭建作者完整个人工作环境，**X1C 作为个人环境的 Home 主权威库**，手机作为 Replica。
   - **BOB-D-005（accepted）**：**单一 Rust 内核（`bob-kernel`）**，同时嵌入 Tauri Desktop、Tauri Android 与可选无头 `bob-node`；Harness 适配器走进程边界。
   - **BOB-D-008（accepted）**：**懒式响应（Lazy Dormant Sync）+ 输入框聚焦/打字静默预热（Speculative Pre-Warm）+ 单飞唤醒复用（`inFlightWakeupPromise`）+ 本地持久化发件箱（Local-First Durable Outbox）**，保证网络状态不阻塞本地持久接纳；远端接收/执行须各自有证据，永久离线或权限失效不承诺远端必达（审计报告见 [evidence/2026-10-28-lazy-send-audit.md](evidence/2026-10-28-lazy-send-audit.md)）。

---

## 2 下一步接手 Agent 的详细任务队列（按顺序执行）

| 顺序 | 工作包 ID | 任务名称 | 当前状态 | 交付物位置 |
|---|---|---|---|---|
| **Step 1** | **WP-01** | 前身项目深度只读盘点补全 | **静态盘点完成**（不含旧测试执行/全历史secret扫描） | `docs/evidence/2026-10-28-wp01-deep-inventory.md`、`docs/COMPONENT_MAP.md` |
| **Step 2** | **WP-02** | `bob-kernel` 单一内核 Schema 与懒式预热 Outbox 状态机定案 | **设计与25项DDL约束通过**；当前内核54项离线回归通过 | `docs/ARCHITECTURE.md`、`docs/wiki/kernel-schema.md` |
| **Step 3** | **WP-03** | X1C Home ↔ Android 连接配置与安全威胁模型 | 设计已获批；私网Pixel配对与真实回复已验，公网/离线/停止待验 | `docs/wiki/connectivity.md`、`docs/wiki/threat-model.md`、`DECISIONS.md` |
| **Step 4** | **BP2-01** | 建立 `1.0.1` 代码工程骨架（Rust Workspace + Tauri v2 + Vue 3） | 内核已编译；Vue/Tauri壳源码已接，原生构建与启动通过，窗口交互待验 | `Cargo.toml`、`crates/bob-kernel/`、`apps/bob-app/` |
| **Step 5** | **BP2-02** | 实现 `BOB-D-008` 懒式响应、打字静默预热与本地 Durable Outbox 闭环 | 本机及Pixel→Home真实只读回复通过；取消手势、公网与异常恢复待验 | `crates/bob-kernel/src/outbox.rs`、`src/wake.rs`、`src/inbox.rs`、`src/dispatch.rs`（后三项相对crate） |

---

### Step 1 — WP-01：前身项目深度只读盘点补全（只读，不改旧仓库）

1. **旧 App 同步与信任引擎审计**：
   - 读取 `<workspace_root>/bob-agent/src-tauri/src/sync_engine.rs`、`<workspace_root>/bob-agent/src-tauri/src/device_trust.rs`、`<workspace_root>/bob-agent/src-tauri/src/outbox.rs`。
   - 梳理清楚：现有 SQLite 表清单、哪些表参与同步、`rpc_staged_outbox` / `peer_revocation_outbox` 表结构及 `write_mobile_outbox` JSON命令的幂等差异（后者并非表）、哪些可直接复用到 `1.0.1` 的“X1C Home + 手机 Replica”架构中。
2. **旧 Online（Bob Core）实体与治理状态机盘点**：
   - 读取 `<workspace_root>/cloud-agent/packages/shared/src/types.ts`、`<workspace_root>/cloud-agent/packages/server/src/db/` 及路由层。
   - 列出完整的表结构、状态流转（`Project`、`Task`、`Execution`、`Artifact`、`Verification`、`Review`）与 Harness Runner（DSH / AGY / Codex / Generic API）调用接口。
3. **凭据存储与安全扫描**：
   - 检查旧 App `<workspace_root>/bob-agent/src-tauri/src/llm.rs` 与 `crypto.rs` 中 API Key 和设备私钥的存储方式（区分系统安全存储 vs 明文 `config.json`）。
   - 统计两个旧仓库的测试文件数量与CI工作流定义（只统计不运行；不把定义存在写成远端CI通过）。
4. **验收输出**：
   - 写入 `docs/evidence/2026-10-28-wp01-deep-inventory.md`，并更新 [COMPONENT_MAP.md](COMPONENT_MAP.md) 中的模块复用映射表。

---

### Step 2 — WP-02：`bob-kernel` 单一内核 Schema 与懒式预热 Outbox 状态机设计

1. **同名实体统一 Schema（SQLite DDL 草案）**：
   - 2026-10-28交付见 [kernel-schema](wiki/kernel-schema.md)，来源与验证见 [WP-01/02 Evidence](evidence/2026-10-28-wp01-deep-inventory.md)；31张草案表、25项内存DDL约束检查通过，不代表运行状态机/真机验证。
   - 将旧 App 的 `Conversation / Message / Capture / Memory / Goal / Milestone / Task / Decision / Artifact / Evidence` 与旧 Online 的 `Project / Task / Execution / Artifact(带版本) / Verification / Review` 合并为同一套无歧义的 SQLite 表结构：
     - 明确 `Goal/Milestone`（目标分组层）→ `Task`（用户事项层，唯一权威）→ `Execution`（单次模型/Harness 执行尝试，含 `requested / resolved / observed`）的三层关系。
2. **`BOB-D-008` 懒式响应 + 打字预热 + 本地 Outbox 协议规范**：
   - 完整定义 `intent_outbox` 表结构（`request_id`、`session_id`、`target_device_id`、`kind`、`payload_json`、`expected_version`、`send_state`、`attempts`、`last_error`、`created_at`、`dispatched_at`、`acked_at`）。
   - 画出状态机流转图并固化边界：
     - **触发端（UI 输入框）**：`@focus` / `@input`（15s 冷却）触发静默预热 `prewarm()`，与 `send()` 共享单飞 `inFlightWakeupPromise`。
     - **落盘即成功**：`send()` 写入 `intent_outbox`（状态 `local_saved` → `queued_for_wakeup`）即返回 `true`，UI 立即清空输入框并上屏显示消息。
     - **出网分界线**：字节写入传输层前失败（如 `!alive`、唤醒超时、旧 `watchId`）一律记为 `not_dispatched`，自动重连并刷新 `watchId` 后重发，**绝不打入 `REMOTE_OUTCOME_UNKNOWN`**；字节已写入传输层但未收到 ACK 记为 `unknown_send`，重连后先按 `request_id` 对账再决定补发或拉取结果。
     - **空会话休眠**：移除 `messages.length > 0` 限制，空会话在静止阶段同样允许进入 `linkDormant`。
3. **`bob-kernel` Crate 模块划分与进程边界契约**：
   - 明确 `crates/bob-kernel` 内部子模块：`storage`、`domain`（continuity + governance + personal_ops）、`outbox`、`sync`（Home/Replica）、`policy`、`providers`（Connection / Capability / Harness Runner IPC）。
4. **验收输出**：
   - 新增 `docs/wiki/kernel-schema.md` 并更新 [ARCHITECTURE.md](ARCHITECTURE.md)。

---

### Step 3 — WP-03：连接配置与安全威胁模型

1. **个人环境与开源默认连接配置**：
   - 基于 [wiki/connectivity.md](wiki/connectivity.md)，定义 Connection Provider 接口（L1 局域网 mDNS/HTTP → L2 QUIC/iroh P2P → L3 可配置 E2EE Relay → L4 云盘异步信箱 → L5 常开节点）。
   - 作者个人环境（X1C Home ↔ Android）通过外部配置文件注入现有 Relay 与 Headscale 地址（读取工作区 `cluster-ops` 的 `service_map.md`，**严禁硬编码进源码**）。
2. **安全威胁模型（必过门）**：
   - 针对“手机远程读取 X1C 文件（F4）”与“触发 X1C Harness 执行”编写威胁模型：设备配对（QR + PoP）、一键吊销出件箱、操作级授权（R0–R3）、工作区路径沙箱、重放防护（nonce + timestamp + request_id）及手机丢失处置。
3. **验收输出**：
   - 新增 `docs/wiki/threat-model.md`，推动 `BOB-D-006` 与 `BOB-D-007` 状态更新。

---

### Step 4 & Step 5 — BP2：工程骨架搭建与核心链路落地（需注意重型操作防火墙）

> ⚠️ **注意**：根据全局 `AGENTS.md` 重型操作防火墙规则，执行 `cargo build`、`npm/pnpm install`、`tauri build` 前需向用户确认授权；且所有缓存/编译产物必须留在本地 `D:\Code\` 下，绝不污染 OneDrive。

1. **BP2-01 工程骨架与旧代码按模块迁入**：
   - 在本仓库建立 Cargo Workspace（`crates/bob-kernel`、`apps/bob-app/src-tauri`）与前端工程（基于 `legacy/v0.9` 的 Vue 3 设计语言演进，合入 Online 的任务治理视图）。
2. **BP2-02 落地 `BOB-D-008` 懒式预热与发送必达闭环**：
   - 修复 `legacy/v0.9` 审计出的 4 处非即时发送漏洞（`dsh.rs` 未出网误报 `REMOTE_OUTCOME_UNKNOWN`、`remote-agent-service.js` 的 `connecting` 跳过唤醒与旧 `watchId` 残留、空会话不进 `linkDormant`、`ChatView.vue` `@pc` 离线拒发）。
   - 实现输入框 `@focus` / `@input` 静默预热（`prewarm`）+ 单飞 `inFlightWakeupPromise` + SQLite `intent_outbox` 本地先落盘状态机，并编写覆盖“休眠秒发、预热中途点发送、未出网断连重发、已出网断连对账、空会话休眠”5 大场景的自动化测试。

---

## 3 允许与禁止（安全护栏）

- **允许**：只读查阅 `<workspace_root>/bob-agent`、`<workspace_root>/cloud-agent` 与 Google Drive 文档；在本仓库编写文档、设计 Schema，并在获得对应阶段授权后编写 `1.0.1` 代码与测试；向 `origin` 的 `1.0.1` 与 `main` 分支提交并推送。
- **禁止**：修改前身仓库的业务代码（除非用户专门要求给 `legacy/v0.9` 打热补丁）；在代码中硬编码 `bobbik.org`、IP 或任何密钥；未经用户确认执行耗时的全量打包编译。

## 4 历史交接（2026-10-28，WP-01 / WP-02；当前状态以第5节为准）

- 基线：1.0.1/main=`d842924`，接手干净；旧App=`a4ff5fd`，旧Online=`d14a527`，保留所有既有改动。
- WP-01：48个旧App静态表、20个Online业务表+3个认证表，含同步范围、状态门、Runner差异、凭据风险与测试/CI口径；检查并归并审计中的四处发送漏洞。
- WP-02：Schema与实体映射、Home两跳、首次离线接纳、workspace租约、预热/投递单飞、已证明未发与未知分界、持久接收回执、父版本/验证/人工审阅合同。文档设计自检完成，未将细节变更写成用户新决策。
- 已通过：SQLite内存DDL及关系约束25项；旧系统测试、Rust状态机、磁盘崩溃恢复、手机真机、节点/模型/生产检查均未运行。具体范围及可复现脚本见Evidence。
- 下一项：原WP-03。BOB-D-006/007仍proposed，许可证/文件级代码迁入清单、连接威胁模型与构建授权未满足，Step4/5不启动。新项目文档按BOB-D-004通过GitHub同步；未擅自建立Drive新镜像。
- 进度投影经project_aligner的reviewed-patch生成：当前工具限定工作区内部路径，故在临时目录校验同一投影后按原文件hash比对写回本项目；不修改旧项目投影或全局注册表。
- 回退：仅恢复本轮统一项目文档差异；没有旧仓库、数据库、凭据或生产变更要撤销。

## 5 BP2执行证据沿革（2026-10-29；当前收口见本节末）

授权来源：用户批准共享唤醒、Home持久接收、发送恢复调度前三项，并明确无需逐步确认；沿用已批准的离线编译测试范围。BOB-D-006设计已接受；BOB-D-007 Drive只读镜像仍未决，不阻塞本段。没有联网安装、真实设备/模型任务、Tauri打包或生产发布。

| 本轮小目标 | 代码事实 | 验证边界 |
|---|---|---|
| 共享唤醒 | WakeCoordinator返回共享Future；后台认证/订阅adapter、旧代失效、自动闲置关闭、异步清理及容量保护 | 10项运行测试；adapter使用夹具，未验证真实认证网络 |
| Home可靠接收 | 消息/领域事件/Inbox回执同事务；重复请求幂等；Home本机local消息提升；取消只记录stop_requested | 8项磁盘测试；错误身份/epoch/会话/撤销及事务回滚覆盖 |
| 发送恢复调度 | claim→异步wake/transport→complete；每目标/lane单活动领取、控制优先、退避/预算、未知先查回执、迟到回调拒绝 | 7项调度测试含两份真实SQLite经fixture transport的丢ACK→重启→查回执；没有网络设备 |

完整回归34项通过，1项ignored为由父测试主动调用的子进程夹具；Unix专属链接检查未运行。详情与源码hash见[运行证据](evidence/2026-10-29-bp2-runtime.md)。原6项Node控制器测试前轮通过，本轮未改其源码。

### 当时的下一执行项（后续进展见本节末）

1. BP2-01/02继续：Vue/Tauri壳和IPC源码已写，现有输入框控制器已连接同一内核命令门面；原生编译通过，窗口发送与恢复待验。UI必须区分本地保存、等待Home、Home收到、停止请求与停止确认。不能因前三项内核通过就标BP2整体完成。
2. 为真实Provider实现认证、watch generation写入防护、合作取消和有界I/O；注入HomeMessageContext前验证身份与会话授权。模拟公钥引用不是配对凭据。
3. 获相应依赖安装/打包及现场接线授权后，验证PC→Android休眠、断网、重连、重启及重复点击。未运行Linux/Android、断电/磁盘故障或公网验收。

### 当时的保留限制（后续本机Runner进展见本节末）

- 查询不到回执不证明未执行：保持unknown_send，不自动重派；没有发明“安全重发”证明。
- 控制lane本段落地cancel_execution；answer_permission及其他业务意图仍待各业务合同实现，未知kind不会被通用调度伪造处理。
- Rust不能强杀不合作阻塞线程：未退出adapter与待清理watch继续占容量，清理失败拒绝新唤醒；不宣称底层已清理。
- 配对、真实Home选址、实际传输协议、runner停止确认都不在本次已验证能力中。

回退：恢复本轮代码/文档提交；未改变Schema 0001、生产数据、凭据或legacy仓库。依赖和Cargo.lock未增加。GitHub为本项目同步入口，未创建新的Drive平行计划。

### BP2界面接线增量（2026-10-29，基线f2bb0d5）

新增LocalApp命令门面及0002迁移（本机随机身份/默认会话持久化）；旧0001保持不变。Tauri使用独立开发标识与app_data_dir/kernel.sqlite，不读取旧Bob数据库。保存成功后立即上屏，历史刷新失败仍保留已确认消息；普通浏览器没有原生IPC时禁止伪造保存。Home未接时显示provider_unconfigured，不自动配对或发模型任务。

已验证：Windows离线Rust36项（新增命令门面2项）、Node9项；Tauri源码rustfmt/JS语法与中英键检查；外置pnpm安装、前端构建、原生检查与开发进程启动。未验证：窄屏、完整视觉、手机或网络；用户手动保存后实际桌面重开读取已核实。证据见[shell检查](evidence/2026-10-29-bp2-shell.md)。

用户已同意沿现有pnpm并与开发目录脱开的方式继续：依赖、前端dist、Vite与Rust缓存均放外置暂存/缓存目录，源码仅保留源码和锁文件。pnpm安装、前端构建、原生cargo check及Tauri开发版编译/进程启动已通过。桌面工具先应用授权超时，随后窗口捕获超时；未完成实际发送、重开恢复与窄屏操作验收。下一项是恢复窗口验证能力后完成这些交互检查，再接真实Provider；不生成安装包或发布。

### 原生命令门验证（2026-10-29）

生产与测试现在共用configure命令注册。通过Tauri测试Runtime调用真实命令和磁盘LocalApp：3项通过，覆盖camelCase参数、保存/重复请求、释放Store后重开、错误参数/会话拒绝，以及无存储时不得返回接纳。测试Runtime不是实际窗口，不能替代点击验收。Windows测试启动曾报STATUS_ENTRYPOINT_NOT_FOUND，补native-tests特性的Common Controls v6声明后3项实际执行通过；未修改系统。正常开发构建未启用此测试特性，也再次编译/启动通过。

实际窗口捕获仍超时、点击缺少geometry。已向用户请求在Bob开发窗口手动保存测试文字；早先检查为0条；后续用户在App保存后只读核对为1条，并重开实际窗口读到同一记录。聊天回复本身不是保存证据。保留开发窗口供本地验证，未连接Home/模型。

### 历史收口：独立CLI回复回放（已由下节继续推进）

2026-10-29（代码基线 f2bb0d5 加工作区差异）：用户实际保存与桌面重开读取已核实。另获批准执行一次真实 Codex CLI 只读回复测试，0.165.0-alpha.1.2 使用既有 ChatGPT 登录，返回 `BOB_REPLY_OK 2+3=5`，退出0；没有工具调用事件。未独立观测供应商模型身份。

本轮已完成：真实事件归并、同请求幂等回复落盘、来源与原消息关联、重开读取，以及原生命令到Vue的assistant消息接线。为验证界面，把本次已完成的CLI回执明确作为 **external_cli_probe** 回放到独立开发库；窗口可访问树已读到回复和来源提示。没有伪造Task/Execution、Home ACK或用户接受，也不将回放写成Bob按钮派发成功。

最新验证：内核43项、原生命令4项、Node9项通过；外置前端构建通过。真实窗口观察覆盖文本与来源提示，不覆盖像素视觉、390像素或手机。证据与校验码见 [shell检查](evidence/2026-10-29-bp2-shell.md)。真实CLI stderr有PowerShell快照不支持和一个外部skill元数据告警；未影响本次回复，但ignore-user-config不能被解释为完整环境隔离。

当时待推进的BP2/BP3事项（当前状态见下节）：
1. 正式本机Runner将发送意图绑定既有Task/Execution，先持久登记再启动官方CLI；不把本次回放接口当执行器。
2. 实现进程所有权、退出/取消/超时清理与重启后unknown对账；未经确认不自动重放模型。
3. 接入正式回复事务与UI运行/失败状态，做Bob按钮发起的端到端验收；当前按钮仍只保存本地消息。
4. 按既有threat-model完成远端Home认证/传输及Android验证。本地官方CLI登录不等于远端设备配对。

已授权的代码及外置离线验证继续自主推进；本次只有一次真实模型调用，无生产、安装包、真实项目修改或额外模型调用。BP2与BP3均未整体完成。

### 当前收口：受管只读Runner与正式发送链路（2026-10-29）

用户已批准Bob点击发送至真实Codex回复的实现、必要的真实只读测试及既有订阅消耗；不修改真实项目、不发布。沿BP2/BP3原队列实施，不新增平行计划。

- **代码已接通**：UI先持久保存消息，再通过原生命令登记Task/Execution与独立临时工作区；同request_id只允许一次启动，本机接管后阻止通用Outbox重复派发。配置可执行文件时可进入真实Codex只读Runner；未配置不假称已执行。
- **进程与结果治理已实现**：Windows Job先绑定worker，持久启动登记后释放输入；取消/超时终止并核对整棵进程树。清理不明保持unknown和容量占用。成功事件、退出及清理证据共同决定回复落盘；消息、执行终态与证据同事务提交。完成回执外置持久保存以供数据库写入失败后的恢复，不重跑模型。
- **重启恢复已实现**：准备事务即固定Job归属，避免启动前崩溃留下无归属执行；恢复先核对相同Job已不存在或为空，再恢复完成证据或将无结果的旧执行记为中断失败，绝不自动重派。完成回执.pending崩溃窗口与编码后大小限制两处审查问题已修正，新增3项原生回归通过。
- **已验证**：内核54 passed，1 ignored为父测试调用的子进程辅助入口；前端Node17 passed；原生8 passed。原生4项ignored为显式选择入口，其中两个进程专项和一个真实模型专项已单独通过，崩溃辅助入口由父测试调用。Windows真实进程夹具覆盖正常完成、超时、取消、输出超限、非零退出，以及杀死父监督进程后子进程退出且不重放。最终Vite35模块与原生开发构建通过。夹具不是模型，也不是GUI点击。
- **真实模型原生命令验证通过**：一次28.28秒真实Codex调用，经app_accept→app_execute→codex_execution回复→释放并重开读取，结果为`BOB_NATIVE_REAL 11+7=18`；运行中及重开后重复同request_id均不重跑，Job已不存在。此验证使用原生命令测试Runtime，不是鼠标点击，也不独立证明供应商模型身份。
- **实际窗口链路通过**：现有唯一草稿`BOB_UI_LIVE_20261029`已由Vue提交，真实开发库恰好1条执行，completed、exit0、cleanup=true；窗口显示`Bob · Codex`及`BOB_UI_REPLY_OK 7+8=15`和已保存提示。最终源码重新构建并重启后，新窗口仍显示相同消息/回复，执行数保持1。未导入或回放该回复。自动化工具未直接观测物理点击者，不据此声称自动点击成功；实际提交、执行、持久化和界面显示有数据库与可访问树证据。本工作段共两次真实模型调用（原生命令与实际窗口各一次）。
- **未验**：取消按钮具体点击手势、像素视觉、390像素、Android、跨设备Home认证与传输；不独立证明供应商模型身份。

下一步沿原BP2/BP3工作段：补齐取消按钮交互与窄屏视觉检查，再按现有threat-model推进远端Home认证与Android。不得把本机官方CLI登录当作设备配对，也不以进程夹具替代手机验收。无安装包、生产发布或真实项目修改。具体证据沿用[shell检查](evidence/2026-10-29-bp2-shell.md)。

### 当前推进：Home认证与停止操作（2026-10-29）

沿BP2/BP3实现，没有发布或修改真实项目。新增Home设备与精确会话授权表，身份必须来自可信传输观测；每次接收/查询/取消重新检查权限、版本、有效期与Home epoch。接收权限不等于远程启动模型的批准。

标准TLS 1.3传输已实现双向证书验证与绑定指纹，有界消息和网络超时，并接到现有内核持久接收事务。Windows设备凭据存储使用当前用户DPAPI；本轮仅假数据测试，不生成正式设备凭据。详细结果及源文件校验值归入[shell证据](evidence/2026-10-29-bp2-shell.md)。本地套接字测试不是手机连接或首次配对验收。

停止操作：彩虹请求正常完成且没有stop_requested；第二次长文本请求以timeout终止、cleanup_confirmed=true，同样没有停止请求。因此手动停止仍未验收，不自动再发送模型任务。固定输入框旁Stop已实现并通过前端回归/构建，保留真实点击验收项。

下一段按原顺序：完成短时配对邀请、双方身份确认及vault接线；将传输接入App的Outbox投递/查询；补Android Keystore和手机端接收显示，再用真实手机验证离线、恢复、重复发送与停止。当前未发现可用Android SDK/JDK/adb工具链；安装与测试包/USB操作另作具体接线步骤，不用模拟配对冒充手机可用。没有开放外部监听、改防火墙或部署服务。

### Android构建方式调整

用户于2026-10-29要求使用GitHub自动构建：停止本机JDK/SDK/NDK下载，本机只保留现有ADB用于Pixel 8安装与验证。新增手动触发的`.github/workflows/android-dev.yml`，产出独立`org.bobagent.nextdev` ARM64 debug APK、源码提交与SHA256；不创建Release。工作流已上传并实际运行成功（run 40809505137，提交49546d0）；独立APK已安装到Pixel 8并冷启动，数据库完整性通过。具体证据见现有shell检查文档。手机点击保存/重开交互与Home连接仍未验收。首次配对、Android安全存储和Home接线仍按原BP3继续。

### Android 16 KB兼容修正（2026-10-29）

用户真机截图证明首包存在LOAD段4KB对齐缺陷；启动和数据库成功不代表16KB兼容。e62d55f在Android原生链接加入max/common-page-size=16384，打包前检查全部ELF LOAD/RELRO与zipalign。GitHub run 40810731635全部通过，修复包尚待真机更新与弹窗复验。现有Pixel PAGE_SIZE=4096，不能声称16KB系统实测通过。手机顶栏与状态栏重叠也已由截图确认，沿BP2手机适配待修；Home接线仍未完成。

## 独立仓库迁移（2026-10-29，用户批准）

旧GitHub仓库改名为bob-agent-archive，保留全部历史、标签和构建证据，不启用只读归档。新bob-agent从旧仓库08f9991源码快照开始独立历史，并包含当时未提交的APK签名失败证据；不继承旧提交图。前文中的历史提交号均归属archive。新库main与1.0.1从同一个初始提交开始，后续不能假定二者始终同步。

原版App与新org.bobagent.nextdev继续独立存储和安装，允许并行比较；仓库拆分不解决开发APK签名不稳定，签名固定与手机更新仍待完成。旧工作区不删除，新工作区为同级bob-agent-next。

### 固定Android开发签名（2026-10-29）

用户批准复用原长期签名。已定位工作区credentials/bobbik-release.p12及旧流水线三个Secret名称；已由用户安全输入密码并解锁私钥，自动读取别名，三个Secrets已加密写入新仓库；证书SHA256为dafb9fbdc736f2c70864a7005cb199b70ab5816a6833e8ec3e5ba743fee338f5。GitHub run 40815120561已全部通过，APK证书指纹与该长期密钥一致，ELF及ZIP 16KB门禁通过。手机现装临时签名包需要经用户批准重装后才能升级到此签名，尚未执行卸载。新工作流缺少持久签名配置即停止；签名后保留apksigner及16KB检查。GitHub Secrets不随仓库改名后的新建仓库自动迁移，也不能读回旧值。配置完成后再执行实际构建、证书核对与升级验证；手机现装临时签名包仍不得擅自卸载。

### 固定签名真机重装（2026-10-29）

用户明确批准清除独立Bob Dev测试记录后，已仅卸载org.bobagent.nextdev并重装run 40815120561固定签名APK。Pixel安装成功、冷启动789ms，手机base.apk SHA256与下载包一致：a18b2f971575bf744b7c2484b6c87d8946e2b5e2fd160a597dace006e2ef37b3。其他包移除集合为空，旧Bob未卸载。新kernel.sqlite已建立，应用PID日志无匹配的崩溃/对齐错误。未观测手机像素画面，因此弹窗消失、点击保存与Home连接仍未验收。

### 手机安全区域修正（2026-10-29）

用户确认固定签名版可打开且无16KB兼容弹窗，补齐该设备实际观察；仍不能推定16KB内核运行验收。用户截图确认顶栏压住系统状态栏。本轮在app-shell统一消化四向safe-area，启用viewport-fit=cover与键盘内容视口缩放，移除main重复底部安全区；顶栏禁止flex收缩。修正提交0b095fb已通过GitHub run 40817356929构建、固定签名和ELF/ZIP 16KB检查；已通过ADB覆盖安装到Pixel 8，冷启动584ms，设备base.apk校验与下载包一致。未卸载或清除数据。真机顶部/底部像素布局与键盘交互仍待观测，不以启动成功代替视觉验收；不写死状态栏像素高度。

### Home 实际接线候选（2026-10-29）

基线7fb52fc加本轮源码差异。实现短时邀请、证书私钥持有证明、双方确认、Android Keystore/Windows DPAPI身份、精确会话授权与独立只读执行许可；消息接收和执行准备原子提交。旧本地记录不自动上传；未知发送先查询回执。Home忙碌/未批准返回持久拒绝；结果分页轮询、历史停止请求不能遮蔽最终状态。

最新原生21通过/7显式专项入口未批量运行；已单独通过真实TLS监听→模拟CLI回复与重复请求检查，以及一次真实Codex只读13+8回复、落盘、重开不重派检查。后者不经过手机或实际网络。当前完整workspace离线回归通过，传输17项通过。用户确认状态栏不再重叠。

剩余：Android新接线云端编译/安装、真实双方配对、Pixel点击发送/停止、离线恢复及手机重开。精确拒绝原因当前保存在Home，手机只显示拒绝；有限重试预算耗尽后的用户恢复体验仍待补齐。不得将这些标为通过。X1T/B4和真实项目修改不在本轮范围。

### Android Home候选已安装（2026-10-29）

源码5a1b9eb，GitHub run40830182960构建成功，artifact12878965262。固定签名/16KB检查通过；Pixel以install -r覆盖安装成功，启动Status ok，未卸载/清数据。下载与设备base.apk SHA256均为0ab7a37f4f8867ce8d7c2d11e861b10c3cc33c333041e6f607a8aa348f381c80。首次手机配对、点击发送与停止及断线恢复仍未验收。


### 当前施工：二维码首次配对（2026-10-29）

用户批准持续实现至完成。默认手工JSON交换已被明确否定；按connectivity第7节替换为Home生成QR、手机相机扫码、Home首次确认、自动保存与重连。沿原BP2/BP3，不新增平行计划。5a1b9eb安装和测试证据保留，但不能代表二维码链路已实现/验收。当前改造涉及首次引导传输、原生相机、UI与云端Android构建；真实手机扫码通过前保持未完成。

二维码恢复加固已通过原生回归：保存失败不授权、pending与confirmed两段重启恢复、迟到回复隔离和撤销保持均有检查。第一轮Android相机编译成功；最终恢复候选待重新云端构建和Pixel扫码验收，不以软件解码代替相机。


异地使用是既有移动个人环境的硬验收项：允许用户远控Home生成QR并在不同网络配对、重连。当前2c42a6d仍是可达局域网/私网的QR候选，公网中继尚未实现，不能将当前扫码候选或同Wi-Fi验证记为异地可用。后续沿WP-03/BP3与connectivity L3接线，不新建平行路线；需要公网部署时提供具体候选、影响、批准和回退。

### 当前施工收口：扫码即确认（2026-10-29，BOB-D-009）

用户明确否定扫码后的多重选项，现行默认改为扫码自动确认、保存与开启已配置的有界只读回复。二维码服务端完成授权，不依赖Home面板继续显示或用户点击；手机默认界面不再呈现高级手工角色/JSON选项。保留系统相机授权、365天设备撤销及1小时只读限制，不开放真实项目修改。

当前代码检查：原生27 passed/7 ignored，Node20 passed，Vite92模块构建通过。新增覆盖自动确认、未配置执行器的真实错误、自动路径共用的监听启动且不调用模型、重启后完成手机连接、重复确认不延期，以及持久化失败保持pending不授权。前版2c42a6d APK已以install -r安装且设备hash一致；本次代码尚需新的云端构建/安装。未以回归测试替代Pixel相机或异地连接验收。源码无模型调用、生产变更或项目写入。

扫码恢复补充：QR的自动同意标记和原始只读许可截止时间随原恢复元数据保存（v3，兼容v1/v2手工记录）。Home重启恢复不要求再次点击，也不把1小时许可延期；手工旧记录不会自动获得QR授权。回归仍为原生27通过/7 ignored、Node20通过、前端92模块构建通过。

旧版配对兼容：电脑已保存v2且已confirmed/consumed的设备，在用户主动打开连接入口（含已批准--pair-phone测试入口）时直接启用本次有界只读监听，无需再点击第二个按钮。状态轮询本身不续期；未确认旧记录不静默授权。现场只读核对：现有v2记录已consumed，旧邀请与原1小时窗口已过期，没有活动执行；没有删除配对或清空数据。

### 撤销后重扫码收口（2026-10-29）

实际电脑窗口读取显示旧设备已revoked，而非仅邀请过期；此前只读SQL中的consumed不足以判断当前授权，现以授权/界面核对修正。没有复活旧授权或删除测试库。

修正：新扫码入口将已撤销Home公开恢复文件原位移入校验后的历史子目录，保留数据库历史；同密钥/同Home/同epoch的手机凭全新邀请与PoP重新授权，revision递增。手机只在同Home/同会话且认证revision更高时归档旧连接配置并保存新配置；跨Home、换密钥、旧证明与重复消费仍拒绝。手工配对的原拒绝规则保持。部分归档或保存失败保持明确失败，不裸覆盖。

新增真实TLS生命周期回归串联首次扫码、重启、撤销、保留历史、新QR、同手机重扫码与再次保存成功。中间曾由回归捕获内核拒绝已撤销设备、配置只写一次两个真实缺口，已修复；最终原生27通过/7专项ignored、内核82通过/1子进程辅助ignored、Node20通过，Vite92模块通过。没有在本段调用真实模型。

5c5307c手机包已覆盖安装Pixel并冷启动（842ms），下载/设备hash同为5d9eeb2d0b7f4599f0c363008614cd5b267246c5babbb35f62a6a79f14575f6f；该包不含后来发现的撤销重扫码修正，因此该阶段随后由下节最终安装证据收口。真实手机相机/消息往返仍未验收。

### 最终候选安装与实际窗口核对（2026-10-29）

- 代码232abbda9450b6eaf8b39689e3ce0bbc7de97853，GitHub run40874574390 / artifact12896858920；固定签名和16KB门禁成功。APK与Pixel安装文件SHA256均为5a6a4e510d1e30904d879f4e897dc1744d8459d6965859ab0efc671daef1cac5。
- Pixel install -r成功，未卸载/清数据，冷启动421ms（Wait422ms）。下载末段曾断连；断点续传后按GitHub整个ZIP的SHA256重新校验，未安装不完整包。
- 电脑同候选独立测试二进制SHA256为d476fe4efeecc35474c279f4722e618a74ee3706098ff2bf6daa1688699eec8a。读取实际窗口可访问树，看到Phone pairing QR code、有效倒计时和Waiting for phone to scan；没有在Windows更改防火墙或绕过安全提示。
- 旧已撤销的公开恢复文件由新入口保留在历史目录，未删除原消息或复活旧授权。设备需扫描全新邀请才会生成新授权。
- 已请求用户用Pixel真实扫码并发送只读连接测试；尚无完成回执，不记为手机端到端通过。跨网络公网通道仍未实现，没有修改X1T/B4或生产服务。

### 当前施工：扫码识别与结果反馈（2026-10-29）

用户现场反馈232abbd二维码难扫，相机返回后手机/电脑没有明确结果。只读检查：Home最近邀请仍open、未见pending/confirmed，初查只在files子目录未见配置，后续发现真实位置在应用根目录，因此初查不能用于判定配置缺失；也没有足够证据倒推此前相机是否成功读码。现有扫描器配置120秒超时，却把超时的空结果当取消，UI又静默吞掉取消，这是已确认的反馈缺陷；不把它擅自写成该次失败的唯一根因。

已修：QR v2紧凑二进制和字母数字编码，仍携带同一证书、挑战与地址并走原TLS/PoP，不绕过认证；v1仍可读。同一已撤销夹具测得1074字符→788字符，QR version27→18，125×125→89×89小格，显示320→420px；独立ZXing解码420px图完全一致，但不是相机实测。手机按真实回执显示scanning/connecting/verifying/connected；扫描超时和用户取消分别提示。Android仅记录结果枚举/时间，绝不记录二维码内容，辅助区分读码与WebView返回故障。

通过：原生28 passed/7专项ignored（含v1/v2身份范围一致与真实TLS配对生命周期）、Node26 passed、Android暂存4 passed、前端93模块构建。内核82项为前一代码基线结果，本段未改内核。尚需新APK构建/覆盖安装与真实扫码；公网异地通道仍未实现，无模型/Blender/生产操作。

二维码展示补充：旧qr-panel的max-height:60dvh会让320/420px二维码与标题/操作挤在同一滚动面板，默认760px窗口可能不能完整显示。改为独立、按视口缩放的QR展示层，展示时隐藏高级选项；当前仍需实际窗口和手机复验。

### 首次真实Pixel回复闭环（2026-10-29）

- 运行版本16e34c24eb4e2f82a0064c47c8b752420eed4c42，GitHub run40881180074 / artifact12900213155。固定签名/16KB检查成功；Pixel保留数据覆盖安装，冷启动410ms，APK与设备hash均为57fc5fdb63b77c7951328302b3d95f78ada90882697af5d0a3130c5f2e6abe33。
- 用户调整网络后，手机与电脑一个局域网地址同网段，路由改走wlan0，限定TCP连接成功。此前三个候选经手机ipsec路径连接失败；不因此断定VPN是唯一丢包点，也没有改防火墙/VPN。原有Bob WebSocket中继在cluster-ops有登记，旧协议仅转发notify/ack/commit_ack/proxy，不能把已有域名当成当前Rust TCP/TLS Provider已接好。本段没有迁入旧代码或改中继。
- 用户扫码后Home邀请consumed，Android扫描结果ok。手机公开配置、内核Home绑定、会话及授权revision与Home一致；真实配置在应用数据根目录，不是files子目录。此前只查files的“未见配置”不能作为缺失结论，现已纠正。
- 用户在Pixel看到Bob·Codex回复“连接正常，已收到消息”。Home执行local-execution:879ce506d5c0146f022568b42d493e925dd10e1f3a00f7d60902d719b27d37a6，Task local-task:879ce506d5c0146f022568b42d493e925dd10e1f3a00f7d60902d719b27d37a6；同request恰好1条执行，completed、exit_code0、cleanup_confirmed=true，耗时19.325秒。用户消息来源精确匹配手机授权会话。
- 一致性检查过的手机SQLite副本中，恰有同execution回复，source=home_codex_execution，文字与用户报告一致；没有把回放记录当派发。该段为用户在手机实际触发的一次真实模型执行，Agent未自行追加模型测试；提供方实际模型身份未独立核验。
- 仍有UI缺陷：成功后仍像未连接。已改为认证保存成功后自动返回对话/聚焦输入框，主界面常驻“Home已配对，可以发送消息”，入口同步显示已配对。该标识代表保存的配对，不夸大为持续在线；每条消息的送达/执行仍独立。Node28项与前端93模块通过，新的UI包待构建/安装；现有已连通Home暂不重启。
- 本次不覆盖异地公网、离线重连、手机重启、停止手势或真实项目修改；不能把BP3/F4或全部个人环境标完成。X1T/B4与生产服务未动。

### 成功状态界面覆盖更新（2026-10-29）

源码0399df17f00766771acaaa0cc619eb705d0137ac，GitHub run40888854563 / artifact12904790666成功；固定签名/16KB门禁通过。APK与设备SHA256同为053df37a3ba8b49d6213b1eb7373183026389b2eeb021cbc9ddf567e2afd707e，install -r保留数据，冷启动926ms。该版只改变成功提示/返回对话交互，未改变16e34c2已真机验证的原生协议，不要求用户再次扫码。

覆盖更新后重新只读核对：Home授权有效、手机内核Home/会话绑定与授权revision一致，配置校验码与更新前相同；手机仍有同一次home_codex_execution回复且正文一致。完成的是App覆盖更新/冷启动后的配对与已终态回复保留，不是手机系统重启、执行中断或公网验收。电脑Home未重启、未自动重跑模型；用户无需再为本次界面修正重复发测试消息。
