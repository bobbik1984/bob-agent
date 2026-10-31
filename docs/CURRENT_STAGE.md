# Bob — Current Stage（1.0.1 施工与交接合同）

> 当前事实与详细执行顺序的唯一入口；长期阶段见 [DEVELOPMENT_ROUTE](DEVELOPMENT_ROUTE.md)，需求与原始验收条款见 [todo](../todo.md)。沿用 BP、S、M、RV、OL、XP 编号，不另起路线。

## F2/F5 Winter Shelter 长历史读取修复（2026-10-31，已上线）

版本 `dsh-history-20261031`，源码 `fe018b0`。真实历史响应4,684,655字节，超过原WebSocket 4MiB限制；原实现把容量错误折叠为DSH_HISTORY_UNAVAILABLE，误导用户检查地址。认证与会话身份正常。历史帧、消息与累计预算独立提高至16MiB，普通RPC仍为4MiB；容量、超时、其他读取错误分别提示，不无限放宽。

Linux定向Rust6/6、release与前端构建通过。候选第一次验证HTTP400未保留响应体，原因未证实；补充错误记录后第二次独立数据副本启动、迁移、读取及真实Winter历史通过。备份后切换生产成功，未用回退；线上RPC读到16条文字，随后公网浏览器读到17条（源会话仍可变化），40个Markdown标题，历史容器可滚动尺寸828/18255px，错误消失。仅最近文字窗口，不代表全部历史或附件。未发送消息、未新建会话、未改X1T项目；APK未重打，B4继续暂停。

见[发布与验证回执](evidence/ui-capture-20261031/dsh-history-release.json)和[公网正文](evidence/ui-capture-20261031/winter-history-public.png)。恢复备份程序与网页可回退；不能以旧数据库覆盖发布后记录。后续沿F2补历史分页与附件，不另建计划。

## F2/F5 侧栏层级修正（2026-10-31，已上线）

网页 `conversation-hierarchy-20261031`，源码 `7d8a06e`。项目标题缩短为“连接设备名 / 项目名”，通用设备SVG、完整路径悬停提示；项目子会话缩进18px，独立会话保持根级。分组仍以连接ID与完整目录区分，不合并同名项目，不猜测系统类型。

路径缩写/同名隔离/分组回归3/3、前端构建通过；统一5194与公网真实X1T目录已核对。公网390px下目录可见宽度与滚动宽度均302px，Winter Shelter子会话缩进18px，无横向溢出。本次仅替换前端并保留旧版备份，无服务重启、数据修改或模型请求，APK未重打。回退恢复回执所列备份web_dist即可，不恢复数据库。见[发布回执](evidence/ui-capture-20261031/hierarchy-release.json)与[截图](evidence/ui-capture-20261031/hierarchy-public.png)。

## F2/F5 统一对话导航（2026-10-31，已上线）

本轮用户批准直接上线测试。已发布 `unified-chat-20261031`，源码 `db3569906a761aa841d545fb8f2251988b984a8c`。移除主导航Work标签，本地与真实DSH会话共用对话侧栏、搜索、设备标签与最近活动排序；历史导入单独折叠。点击直接打开正文，连接配置在设置。原生会话仍以连接ID+Session ID寻址，不导入/复制原记录。选择和草稿刷新恢复；普通对话切换清除远端选择。

定向JS17/17、前端构建通过；统一5194真实DSH列表、搜索、打开原DOS会话、Markdown、草稿刷新恢复及设置连接入口通过。此次未发送新模型请求，发送复用上一版本已实测链路。未重打APK。公网四项导航、侧栏真实DOS搜索及原历史已回读，390像素侧栏未横向溢出。发布过程首页校验曾因服务端注入和CRLF换行误判；第一次自动回退，第二次回退重名失败后已明确核对当前新版与全部静态资源一致，旧版备份仍完整。校验与回退暂存脚本已修正，不将这两次误判计为正常发布检查通过。发布证据与回退见 [回执](evidence/ui-capture-20261031/unified-chat-release.json)；不变更数据库、Rust服务或节点工具权限。

## F2/F5 原生 DSH 接续（2026-10-31，真实验证通过，已发布）

用户本轮授权真实验证通过后上线，取代早期“仅源码、暂不编译”限制。复用Rust Runtime与现有节点凭据管理，新增固定DSH list/history/prompt协议，未复制legacy代码。网页“工作”和侧栏入口直接读取原会话；工作目录按DSH报告分组，不将同名或历史导入项目自动合并。

- 已核对X1T返回22条原始记录，排除子Agent后16条用户会话。列表及工作目录组按最近真实活动排序，支持搜索与折叠。旧Online69个项目/81条会话仍是历史导入，单独收起。
- 原DOS会话ID未变；浏览器发送一次只读进度请求，用户消息sequence9453、回复9458，各一条；刷新后可重新读取。同一connection_id/session_id绑定历史与发送，不创建新会话。仅请求三句进度、不要求文件修改；未对节点全盘文件做独立哈希审计。
- 安全Markdown阅读、长历史滚动、390像素无页面横向溢出已实测。窗口只取最近200条原始记录并诚实提示不完整历史；附件、图片文件回传未接通。
- 发送前写持久传输回执；同一请求重复不重发，未知回执阻止自动重派。回执仅为外部DSH传输证据，不冒充Bob Task/Execution/Artifact生命周期。未知回执的用户自助核对解除仍待完善。
- 空白对话顶部显示“对话”，不再借用旧导入的DOT标题；有记录时仍显示所选会话标题。
- 定向Rust4/4分别在Windows和Linux通过；前端发送7/7、阅读/桥接/启动选择24/24通过。候选Linux release和前端构建通过。真实续聊证据见 [dsh-native-live.json](evidence/ui-capture-20261031/dsh-native-live.json)。APK未重打，Tauri原生DSH命令和手机真机尚未验收；本次证明的是新版网页。
- 候选只读引用原服务中指定连接的凭据，不复制生产vault；正式服务使用原有凭据库。X1T/B4与Blender未恢复。

发布：源码 `148a88d85c5f16d212d545b095d08ed44b8d7389`，版本 `dsh-native-20261031`。生产数据副本启动/迁移和读取通过，备份后重启切换成功，未使用回退。生产RPC与公网浏览器均读到原DOS会话和测试回复；公网空白标题为“对话”，390px视窗内容宽385px。发布核对未再发送模型请求。备份位置及二进制校验见[发布回执](evidence/ui-capture-20261031/dsh-native-release.json)，[公网窄屏截图](evidence/ui-capture-20261031/dsh-public-390.png)。回退先核对发布后新增数据，再恢复已备份程序与网页，不能直接覆写当前数据库。

下一步：补Tauri宿主接线、附件及未知回执恢复。沿F2/F5推进，不另建路线。

## 侧栏与真实来源核对（2026-10-31 已上线）

源码 `d8ee00b`，网页 `ui-sidebar-20261031`。实际平台目录69个项目全部为legacy_import，共81条会话；不是X1T DSH实时目录。历史导入默认收起，搜索可找回，项目可展开/收起；长标题省略，项目新建按钮固定为加号，侧栏不横向滚动。按最近消息/新建时间倒序，阅读位置和草稿保存不再提升活动时间。

统一5194已接实际平台后端，非模拟目录；浏览器搜索/折叠通过，抽屉可见和内容宽度均263px。JS49/49、Linux目录回归7/7、生产数据副本启动检查通过，备份后上线。证据：[发布与来源](evidence/ui-capture-20261031/sidebar-release.json)。未发送模型消息，未重打APK。

本节为侧栏发布时的范围。X1T DSH接线已由上方当前F2/F5增量取代；旧合成记录核查、设备视图进一步收纳仍待完成。

## 发送、阅读与日历编辑（2026-10-31 已发布）

用户本轮明确批准上线与 GitHub APK 构建，替代此前本地审阅/暂不编译门槛。源码基线 `45d0cbf8e3614a7e21b91fdf2996f21417bbd716` 已推送 `1.0.1` 与 `main`，网页发布 `ui-reader-send-20261031`。

- 普通对话统一“发送”，灵感速记“记下”。草稿自动保存；无执行服务且无Home配置时阻止提交、明确提示并保留草稿。Home离线沿现有Outbox等待，真实跨端离线重连尚未验证。
- 助手回复和Online历史使用同一安全Markdown子集组件，支持标题、列表、代码、表格；长会话纵向滚动，宽表格/代码局部横向滚动。相对附件未接通时明确不可用，不加载不可信HTML或自动远程图片。
- 日历新建/编辑使用弹窗，支持关闭与版本冲突检查；连接管理改为列表及添加/编辑/删除确认，模型保存失败明确提示。本节发布时DSH真实接续尚未完成；现已由上方F2/F5发布记录取代，APK范围仍未验证。
- JS49/49、Linux kernel/node/runtime回归与release构建通过（kernel一个既有测试ignored）。独立生产数据副本启动/迁移、bootstrap和个人数据读取通过；备份后重启切换成功，NRestarts=0，未触发回退。
- 390/1180浏览器阅读、草稿恢复及未配置发送保护已验证；发布后通过临时私有通道检查实际服务的发送/记下文案，未写生产测试消息。不是公网登录或手机真机验收。
- GitHub Android Run `41184361439` / Artifact `13036301637` 成功，固定签名及16KB ELF/ZIP对齐检查通过。成品位于 `dist-release/reader-send-45d0cbf/`，未安装到手机。

证据：[发布回执](evidence/ui-capture-20261031/reader-send-release.json)、[实际发布页面](evidence/ui-capture-20261031/reader-send-live.png)、[阅读检查](evidence/ui-capture-20261031/message-reader-review.json)。回退使用发布回执中的旧二进制、web和数据备份；数据库恢复须先核对发布后新增数据，不能直接覆盖。本节之后的F2/F5网页接线已完成，见顶部；手机验收仍待完成，不恢复X1T/B4。

统一5194当前已指向正式bob-node与真实DSH连接；早期隔离浏览器预览仅是历史证据。

## 连接页整理（2026-10-31，网页已发布）

用户批准更新本地测试版与线上UI。前端 ui-connections-20261031：连接我的电脑 / 连接远程Agent两个用途；网页不显示原生扫码、角色选择或伪配对状态。原生扫码/二维码沿既有能力开放，管理操作与手动排障收起；普通同步不宣称传输密钥。DSH不可用状态明确展示，无假表单。

外置测试源码和前端构建已更新；43/43现有前端回归通过。线上实际资源回读通过，桌面和390像素页面经私有SSH通道检查。未编译Rust、未重新打包手机、未调用模型；DSH未接通。后端/数据库不变、不重启，原网页已备份。证据：[回执](evidence/ui-capture-20261031/connections-release.json)、[截图](evidence/ui-capture-20261031/connections-live.png)。


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
2. **BP4-04 ～ BP4-07 多节点统和、项目画像胶囊与快慢双轨子代理落地（[BOB-D-010](DECISIONS.md#bob-d-010--多节点统和工作模式快慢双轨调度异构能力互借原生子代理与项目画像胶囊)）**：
   - `BP4-04`：在 `bob-kernel` / `bob-runtime` 实现 3～8 KB 项目画像胶囊（`ProjectContextCapsule`）自动提取与快轨规划注入，防止无源码操作端写《任务卡》跑偏；
   - `BP4-05`：在 `bob-runtime` 内生实现基于正规商业 API 的 `invoke_subagent` 并发子代理池；
   - `BP4-06`：实现两段式《任务卡》与受控进程边界持久会话 CLI（`agy`/`codex`/`dsh`）低频调度，严守防高频冷启动与反逆向合规红线；
   - `BP4-07`：扩展 `RuntimeCapabilityManifest`，接入跨节点海外商业 API 代理（`overseas_model_egress`）与 `bobbik-pc` 独显/显示器实机渲染验证（`gpu_display_render`）回传 V1/V2 快照。
3. **RV-14 / RV-15 视图模块化演进**：按需继续拆分 `App.vue` 子面板与桌面横屏侧边导航增强。
4. **RV-19 / RV-21 提醒通知与全文检索**：接入系统级到期通知与 SQLite FTS5 消息/知识检索。
5. **BP5 旧系统并行观察与切换**：保持 `agent.bobbik.org` 与 `bob-online.bobbik.org` 并行运行，待日常使用稳定后按用户指令执行最终切换。

## 原始反馈的当前处理

“贪食蛇”旧首屏、单圆点、文字归属的本地修正见上方当前增量；本次图片与 UI 续做见下方。旧“未实施”描述已被这些代码与证据取代，但不改写既有线上验收。

## BP3 / F1–F3 图片捕获续做（2026-10-31，已部署与构建，真机验收待补）

- 文字 + 系统拍照/选图入口 + PNG/JPEG/WebP（最大 8 MB）原件保存 + 视觉角色识别 + 编辑采纳 + 可选项目/新项目 + 知识页原图回看，已接入同一 Rust 内核和网页/Tauri 命令；不是只在 mock 里替换按钮。
- 原图按 SHA-256 独立保存；capture、项目笔记和原图关联在同一保存点提交，重试不重复。未关联草稿图片保留在当前节点，不静默删除。识别不创建决定或提醒，用户正文与建议不会互相覆盖。
- 移除悬浮窗内部 Logo、模型/扫码/日程快捷按钮；保留拍照与选图。输入无嵌套蓝圈，图片展开后长面板可滚动，触控按钮至少 44 px。统一规范仍为 DESIGN_PRINCIPLES。
- 修正网页节点的 modelEntryId/modelId 接收别名。视觉请求在释放节点状态锁后执行；请求绑定用户已看到的视觉角色，角色变更则拒绝，凭据不返回前端。请求有 60 秒时限；无自动重试。

验证：内核全套离线通过（1 个子进程夹具 ignored），新增原图/关联回归 2/2；bob-node 2/2、bob-runtime 3/3；原生命令 34/34、7 项专门测试 ignored；JS 18/18；实际 Vue + bob-node + SQLite 文字浏览器回归 8 项、图片流程 10 项通过。图片识别只使用本机假服务，验证真实 HTTP 图像载荷与编辑保存链路，不是外部模型质量验证。截图和结果见 [审计增量](evidence/2026-10-31-development-route-audit.md#图片捕获闭环实现增量)。

未运行：Android 相机/系统选图及权限拒绝真机操作、商业视觉服务识别、跨 Home 图片同步。当前 UI 明示原图仅存在当前节点；内核仅校验大小/格式签名，前端额外解码检查，不能宣称完整恶意图片扫描。新迁移 0008 为增量表，本次发布已备份数据库；回退保留图片表/原件，不删数据，旧客户端不会显示新图片。X1T/B4继续暂停。
