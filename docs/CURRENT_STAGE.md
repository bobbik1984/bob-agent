# Bob — Current Stage（1.0.1 施工与交接合同）

> 当前事实与详细执行顺序的唯一入口；长期阶段见 [DEVELOPMENT_ROUTE](DEVELOPMENT_ROUTE.md)，需求与原始验收条款见 [todo](../todo.md)。沿用 BP、S、M、RV、OL、XP 编号，不另起路线。

## 发送与自动保存（本地增量）

普通对话统一“发送”，灵感速记“记下”。沿用草稿自动保存和Home Outbox；无执行服务且无Home配置时发送前阻止提交，明确提示并保留草稿，不再伪装为发送成功。已配置Home的离线消息沿既有本地Outbox等待连接。本轮未验证真实跨端离线重连，不上线、不打包。

## 统一会话阅读（本地审阅中，未发布）

2026-10-31：普通助手回复及 Work 的 Online 历史改用 MessageContent；新增无 HTML 执行的 Markdown 子集渲染、代码/表格横向滚动、会话纵向滚动约束和键盘聚焦。源文与历史数据不重写。节点相对附件仍待受控读取，未伪装成可用图片；DSH 真实传输仍按下方 F2/F5 继续。

轻量 JS 回归49/49通过，包含新增6项渲染/不安全链接/长文末尾保留检查。现有统一本地入口增加“长对话与 Markdown · 本地阅读检查”隔离样例，直接使用本仓库实际组件；390像素浏览器已验证从开头到末尾及返回、无页面横向溢出、末尾不被输入框遮挡，日程往返保留阅读位置与草稿。证据：[检查回执](evidence/ui-capture-20261031/message-reader-review.json)、[窄屏效果](evidence/ui-capture-20261031/message-reader-mobile.png)。仅前端阅读验收，不代表真实DSH、SQLite后端或手机真机验收；未编译Rust、未上线、未打包。

## 日历事件弹窗（本地审阅中，未发布）

真实后端续接：已补模型保存失败显式提示、日历编辑/版本冲突源码与重开回归测试；发现HTTP远端连接保存接口只读name/url而前端发送label/baseUrl，已补兼容接收。连接页改为列表、添加地址/令牌、编辑、删除确认；未验证连接不能标在线。DSH检查与原会话传输仍未完成。真实bob-node离线编译被自动审批拒绝（沿用此前暂不编译门槛），未运行Rust测试、未切换模拟预览。下一步需明确批准本地Rust编译和定向测试；无发布/APK授权。

本地入口已统一为现有5194根页面的完整App，日历独立链接仅跳转。统一预览使用隔离浏览器记录，不调用模型、不保存真实凭据；已实际检查聊天输入/本地记录、模型选择在无Key时刷新保留、日历新建及390像素完整导航。旧5188模拟层缺少模型角色保存处理；源码此前吞掉角色保存错误，现改为收到保存回执后更新，失败明确提示。真实Runtime已有独立模型配置落盘机制，本轮未编译验证。日历图标高度32px，本周改为空心圆。

工具栏增量：日期、左右切周箭头、回到本周定位图标、日历/列表切换合为一行，保留中英可访问名称。390像素实际浏览器五个按钮顶边相同、工具栏高44px、无横向溢出；下一周、本周及列表切换通过。仍仅本地审阅，未发布。

用户要求先在本地测试网页确认，全部确认后才能发布网页和生成 APK。日历事件改为统一细边框卡片；已有事件打开详情编辑，空白时段打开新建弹窗，关闭不推动日历布局。新增编辑命令源码保留版本检查，后端编译、持久化及冲突验证尚未运行。本地视觉审阅使用独立浏览器测试记录，不读取或修改生产日历，不计作 SQLite / 手机验收。实际浏览器已验证事件点击、空白时段预填、新建、编辑、刷新留存、下拉菜单、关闭和 Escape；390像素无横向溢出。截图见 [本地弹窗](evidence/ui-capture-20261031/calendar-modal-review.png)。

## 连接页整理（2026-10-31，网页已发布）

用户批准更新本地测试版与线上UI。前端 ui-connections-20261031：连接我的电脑 / 连接远程Agent两个用途；网页不显示原生扫码、角色选择或伪配对状态。原生扫码/二维码沿既有能力开放，管理操作与手动排障收起；普通同步不宣称传输密钥。DSH不可用状态明确展示，无假表单。

外置测试源码和前端构建已更新；43/43现有前端回归通过。线上实际资源回读通过，桌面和390像素页面经私有SSH通道检查。未编译Rust、未重新打包手机、未调用模型；DSH未接通。后端/数据库不变、不重启，原网页已备份。证据：[回执](evidence/ui-capture-20261031/connections-release.json)、[截图](evidence/ui-capture-20261031/connections-live.png)。

## F2/F5 原生DSH接续（2026-10-31，源码施工中）

用户批准按顺序接入现有DSH真实会话、区分历史导入；随后明确限定先写源码、暂不编译。没有新增发布授权。本轮X1T SSH只读探针成功，3080返回401，证明服务可达且需要认证，不证明列表或续聊通过；没有读取令牌或发送模型消息。

已改源码：OnlineWorkState记录导入项目ID并投影source_kind；Work默认折叠历史导入区，设备会话组排除历史项目。目录刷新不再创建推测的Execution/Verification/Artifact；旧持久快照未删除，后续须识别旧合成记录并以缺证据方式投影，不能宣称已清理线上历史。DSH原生地址为connection_id+session_id；共用Runtime解析append/replace、去重、原目录和不完整历史标记，拒绝错误会话身份。

未完成：受控认证及网络传输、服务配置/凭据存储、网页/手机共用真实列表与会话视图、持久发送意图与未知回执对账、指定旧会话只询问进度、刷新/断线恢复及真机验收。不能用旧Bob的既有成功代替新版验收。

验证：现有前端43/43通过、diff检查通过；新增Rust测试未编译/未运行，未启动候选、未部署、未打包。X1T/B4模型与Blender路径不恢复。

现有S2完成范围仅Bob Online兼容协议，不能解释为DSH原生会话适配完成。沿F2/F5和现有OL/XP队列继续，不新建平行路线。

## 界面追补（2026-10-31，网页已更新，APK未重打包）

- 捕获入口改为灵感速记。强调色改为统一明暗主题色，不依赖手机系统Highlight；强制高对比模式仍使用系统色。
- 日历时间栏完整铺底、缩窄并居中；去掉分钟，午夜显示24，其余1–23，不改变真实时间索引。
- 连接入口显示目标角色和状态点，Home附身份短码。当前接口没有可靠硬件类型/友好名，不能猜测PC或VPS；后续沿F2补设备元数据、最后观测时间和执行能力区别。
- 390×844浏览器明暗模式检查通过，时间轴与日期内容高度一致；[截图](evidence/ui-capture-20261031/calendar-refined.png)。前端轻量测试43/43通过。本地预览不代表真实远端连接或真机验收。
- 用户要求同步线上UI后，已发布独立前端 ui-calendar-20261031。服务端保持be04b73，不重启、不改数据库；DSH和历史分区的未完成增量未纳入。线上JS/CSS回读一致，经临时SSH通道在390像素浏览实际服务确认小时整数居中、完整底色、强调色及灵感速记。不是手机真机或公网登录验收。APK未重打包；X1T/B4暂停。证据：[发布回执](evidence/ui-capture-20261031/calendar-release.json)、[线上截图](evidence/ui-capture-20261031/calendar-live.png)。备份web_dist保留在本次回执所列目录；恢复备份网页即可回退，无需恢复数据库。

## 当前增量：首屏恢复与灵感捕获（2026-10-31，网页已部署、Android 开发包已构建）

沿 F1/F2/F3、BP3 与 XP-10 修复用户本轮反馈，保留全部历史项目。

- 旧库导入改用不选择会话的内核入口；重复导入及重开不会抢占当前选择。
- 网页从当前浏览器恢复明确选择；首次/失效选择显示空白，首次保存时才创建会话。浏览器 A 的选择不成为浏览器 B 的默认页。客户端选择/未保存闪念存于该端浏览器存储；业务记录仍由内核 SQLite 保存。跨端同会话草稿冲突不在本次证明范围。
- 已发布版本连接入口为单圆点；本轮本地候选增加连接目标角色/身份与状态点。Home凭据本身不表示在线。
- 悬浮输入后显示“不关联/已有项目/新项目”，取消或关闭不会建项目；保存通过同一 SQLite 保存点写入 capture、可选项目、项目笔记及同步意图；失败回滚，原请求重试不重复。草稿和选择刷新后保留；悬浮工具仅保留拍照/选图，扫码配对只在设备连接中。
- 无装饰 emoji、主题/i18n 与焦点要求固化于 DESIGN_PRINCIPLES，AGENTS 强制读取，新增源模板/翻译扫描回归。

验证：bob-kernel 全套离线测试通过（1 个子进程夹具显式 ignored）；bob-node 2/2；原生命令 34/34（7 项专门执行测试 ignored）；JS 17/17；实际本机 bob-node + Vue + SQLite 浏览器 8 项通过，涵盖失败注入、刷新、独立客户端、桌面/390 像素及英文暗色。没有调用模型。

发布：源码 `be04b7315a97e1b7fb4f51cc5489fe691dac4cbf` 已同步 GitHub `1.0.1` / `main`。统一网页节点已完成备份、数据副本迁移验证与重启切换，运行稳定（NRestarts=0）。Android 固定签名开发 APK 已由 GitHub Run `41174093233` 构建并通过签名、16KB ELF 与 ZIP 对齐检查。实际线上页面经临时私有验证通道确认新版悬浮窗和空白首屏；内置浏览器没有公网登录态，未声称公网登录闭环通过。证据见 [发布回执](evidence/ui-capture-20261031/release.json)。

边界：手机系统拍照/选图/权限拒绝、外部视觉模型质量、跨 Home 图片同步仍未验或未实现；不恢复 X1T/B4。下一步用本次固定签名 APK 做真机拍照和选图验收，再验证用户配置的视觉服务；不能把自动构建通过视为手机体验已通过。

## 1 当前基线与结论（2026-10-31）

核对日期：2026-10-31。分支 `1.0.1`。已完成 **S0 → S1 → S2 → S3/S4** 核心闭环与 **`https://bob-online.bobbik.org`（Hostinger VPS2 `187.124.144.87`）** 常开节点及统一网页端部署验证。详细实测证据见 [2026-10-31 S0–S4 与 bob-online.bobbik.org 验证报告](evidence/2026-10-31-s0-s4-bob-online-verification.md) 与 [Online 端点探测记录](evidence/2026-10-31-online-client-api-probe.md)。

| 能力域 | 已完成并验证范围 | 后续待真机/跨网补充验证项 |
|---|---|---|
| **S0 可信边界与诚实状态** (`RV-05/06/13`, `OL-02/04`) | `direct_llm.rs` 导出/合并共享配置已彻底剔除明文凭据；`home_host.rs` `PeerSync` 先通过 `host_receive_peer_sync` 校验再合并配置并重算 `payload_sha256`；Online Token 存入系统安全存储且快照不回传明文；`personal_snapshot.json` 改为严格相等断言 | 跨不同局域网的手机与桌面真机配对异常注入复验 |
| **S1 日常个人助理闭环** (`RV-16/17/18/20`) | `LocalApp::promote_conversation_to_project` 原位关联 Project/Task 并保留原 `conversation_id`/消息/草稿/材料（`RV-17`）；空闲 15s 退避、隐藏暂停、同步失败指数退避 `3s→60s`（`RV-18`）；结构化决定表单（理由/备选/重审条件，`RV-16`）；本地北极星指标展示与清空（`RV-20`） | `RV-11` Pixel 8 + Windows 真实商业 Provider Key 两轮对话与 `adb logcat` 审计；`RV-19` 系统级定时通知 |
| **S2 Online 接入与受控续聊** (`OL-01`..`OL-07`) | 原生命令 `app_online_begin_pairing` / `app_online_exchange_pairing` / `app_online_sync` / `app_online_submit_operation` / `app_online_revoke`；`WorkView.vue` 支持任务续聊、指定父版创建 V2 修订、人工审阅通过/驳回、浏览器打开跳转 | 手机蜂窝网络下的断线重连与离线缓存切换真机录制 |
| **S3/S4 单一内核三端同源与常开节点** (`XP-01`..`XP-10`) | 新增 `crates/bob-runtime` 与 `crates/bob-node`；前端新增 `http-bridge.mjs` 自动适配 Tauri 与浏览器；Hostinger VPS2 部署 `bob-online.service`（`127.0.0.1:53020`），只读幂等导入旧 Online `nexus_control_plane.db`（69 个项目），并通过 Caddy + Cloudflare Tunnel 在 **`https://bob-online.bobbik.org`** 正式上线 | 旧 `agent.bobbik.org` 并行观察期满后的 BP5 切换与退役确认 |

## 2 已完成施工阶段摘要（S0 → S4）

### S0：收敛事实、凭据与同步边界（已完成代码与自动化验证）
1. **凭据不随共享配置泄露**：`export_shared_config_from_dir`、`export_shared_config_for_sync`、`merge_shared_config_with_app` 已移除所有 API Key / Token 明文导出与覆盖；`remoteNodes` 仅暴露 `hasToken: bool`。
2. **先校验后副作用**：`HomeRequest::PeerSync` 在 `host_receive_peer_sync` 验签与哈希校验通过后才合并共享配置，并调用 `out_bundle.compute_sha256()` 更新返回包摘要。
3. **固定契约快照断言**：`export_personal_snapshot_fixture_contract` 默认严格对比 `apps/bob-app/tests/fixtures/personal_snapshot.json`，不再在常规测试中静默覆写。

### S1：可日常使用的个人助理（已完成内核/UI 闭环，待补 RV-11 真机商业 Key 验收）
1. **原位升级为工程项目 (`RV-17`)**：`catalog.rs::promote_conversation_to_project` 原位更新会话 `project_id` 并幂等创建 `task-{conversation_id}`，保持 `conversation_id`、消息、草稿与引用不变。
2. **轮询与同步退避 (`RV-18`)**：活跃执行 1.5s，空闲退避 15s，`visibilitychange` 隐藏暂停，`syncHome` 失败按 `3s → 60s` 指数退避。
3. **决定记录与本地指标 (`RV-16`, `RV-20`)**：`KnowledgeView.vue` 支持录入理由、备选方案与重审条件；设置「工作间」面板展示并支持清空本地北极星指标。

### S2：Online 接入与受控任务续聊（已完成并经 `bob-online.bobbik.org` 公网验证）
1. **认证与缓存 (`OL-01`–`OL-04`)**：支持设备码配对、轮询兑换 Bearer Token、系统安全存储保存、增量拉取项目/任务/消息/快照及主动吊销。
2. **受控写入与审阅 (`OL-05`–`OL-07`)**：`/api/client/v1/operations` 支持携带幂等 `request_id` 的任务续聊（`continue_task`）、指定父版 V1→V2 修订（`create_revision`）、人工审阅（`review`）与取消（`cancel`）。

### S3 / S4：`bob-runtime`、`bob-node` 与 `https://bob-online.bobbik.org` 常开节点（已部署上线）
1. **`crates/bob-runtime`**：提供跨平台 `SecretStore`（含 Linux 服务器 0600 权限 `FileVaultSecretStore`）、`RuntimeCapabilityManifest` 与 `ModelHubRuntime`。
2. **`crates/bob-node`**：无界面常开节点，托管统一 Vue 3 Web 端（注入 `window.__BOB_NODE_RUNTIME__=true`），提供 `/api/rpc/:command`、`/api/capabilities`、`/api/events` 与完整 `/api/client/v1/*` 接口，并只读幂等导入旧 Online SQLite 数据库。
3. **Hostinger VPS2 生产部署**：
   - systemd 服务：`/etc/systemd/system/bob-online.service`（运行于 `/opt/bob-online/target/release/bob-node`，监听 `127.0.0.1:53020`）。
   - 公网入口：`https://bob-online.bobbik.org`（浏览器路径受 Caddy `common_auth` 保护，未认证返回 `401`；`/api/client/v1/*` 走 Bearer Token 鉴权）。

## 3 下一阶段待推进事项（后续迭代）

1. **RV-11 真机商业 Key 端到端验收**：在 Pixel 8 与 Windows 填入低价商业模型 Key 完成两轮真实问答及断网恢复记录。
2. **RV-14 / RV-15 视图模块化演进**：按需继续拆分 `App.vue` 子面板与桌面横屏侧边导航增强。
3. **RV-19 / RV-21 提醒通知与全文检索**：接入系统级到期通知与 SQLite FTS5 消息/知识检索。
4. **BP5 旧系统并行观察与切换**：保持 `agent.bobbik.org` 与 `bob-online.bobbik.org` 并行运行，待日常使用稳定后按用户指令执行最终切换。

## 原始反馈的当前处理

“贪食蛇”旧首屏、单圆点、文字归属的本地修正见上方当前增量；本次图片与 UI 续做见下方。旧“未实施”描述已被这些代码与证据取代，但不改写既有线上验收。

## BP3 / F1–F3 图片捕获续做（2026-10-31，已部署与构建，真机验收待补）

- 文字 + 系统拍照/选图入口 + PNG/JPEG/WebP（最大 8 MB）原件保存 + 视觉角色识别 + 编辑采纳 + 可选项目/新项目 + 知识页原图回看，已接入同一 Rust 内核和网页/Tauri 命令；不是只在 mock 里替换按钮。
- 原图按 SHA-256 独立保存；capture、项目笔记和原图关联在同一保存点提交，重试不重复。未关联草稿图片保留在当前节点，不静默删除。识别不创建决定或提醒，用户正文与建议不会互相覆盖。
- 移除悬浮窗内部 Logo、模型/扫码/日程快捷按钮；保留拍照与选图。输入无嵌套蓝圈，图片展开后长面板可滚动，触控按钮至少 44 px。统一规范仍为 DESIGN_PRINCIPLES。
- 修正网页节点的 modelEntryId/modelId 接收别名。视觉请求在释放节点状态锁后执行；请求绑定用户已看到的视觉角色，角色变更则拒绝，凭据不返回前端。请求有 60 秒时限；无自动重试。

验证：内核全套离线通过（1 个子进程夹具 ignored），新增原图/关联回归 2/2；bob-node 2/2、bob-runtime 3/3；原生命令 34/34、7 项专门测试 ignored；JS 18/18；实际 Vue + bob-node + SQLite 文字浏览器回归 8 项、图片流程 10 项通过。图片识别只使用本机假服务，验证真实 HTTP 图像载荷与编辑保存链路，不是外部模型质量验证。截图和结果见 [审计增量](evidence/2026-10-31-development-route-audit.md#图片捕获闭环实现增量)。

未运行：Android 相机/系统选图及权限拒绝真机操作、商业视觉服务识别、跨 Home 图片同步。当前 UI 明示原图仅存在当前节点；内核仅校验大小/格式签名，前端额外解码检查，不能宣称完整恶意图片扫描。新迁移 0008 为增量表，本次发布已备份数据库；回退保留图片表/原件，不删数据，旧客户端不会显示新图片。X1T/B4继续暂停。
