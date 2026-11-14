# Bob — Current Stage（1.0.1 施工与交接合同）

> **最新交接入口（2026-11-14）**：`WP-J1`～`WP-J6` 核心底座与生产端升级已合入主干（当前代码基线 `431b63d`）；后续任何 Agent 接手系统性进化任务，请直接跳转本文件末尾 **「5. 后 WP-J6 系统性进化施工合同（BOB-D-016：WP-E1 → WP-E4）」** 按序领取工作包。
> - 恢复原密码直登体验并解除 `6` 位配对码开箱劫持（`431b63d`）：移除 `App.vue` 启动期对 `<OnboardingWizard>` / `fetchOnboardingStatus` 的挂载拦截（保持网关原生账号密码认证直入工作台）；`crates/bob-node/src/onboarding.rs` 增加 `BOB_DISABLE_ONBOARDING` 与 `is_established_external_auth_node` 保护并拒绝在已启用外部网关认证的节点调用 `/api/node/v1/onboarding/claim` 写入 `node_access.sqlite`；修复 `ModelHubRuntime::has_any_provider_key`（`crates/bob-runtime/src/lib.rs`）只读不落盘副作用。
> - 新建项目/新对话毛玻璃居中弹窗（`<Teleport to="body">` + `backdrop-filter: blur(14px)`），级联顺序改为「① 关联项目与名称 → ② 选择运行设备 → ③ 选择该设备实际可用工具 → ④ 工作目录与首条指令」。
> - 右上角额度追踪自动增量打点（`crates/bob-node/src/quota_sync.rs` 配合环境变量 `BOB_QUOTA_CHECKPOINT_URL` / `BOB_QUOTA_CHECKPOINT_TOKEN`，在 `app_model_hub_get` 与拓扑同步时自动去重推送周额度快照并支持周周期重置轮转）。
> - 终端断线 `agy` 会话识别与一键后台接续（`agy_index_probe.py` 只读比对 `receipts.sqlite` `agy_supervisors` 识别 `cliInterrupted` / `resumable`，解除 `DshSessions.vue` 对 `historyReadable === false` 的打开与发送封锁，支持通过独立守护进程 `agy acp --stdio` `session/load` 无缝恢复）。
> - 多工具双向平滑交接与打断确认统一收口（`Claude` / `Codex` / `AGY` / `Remote-IDE`）：`agy_index_probe.py` 与 `agy_directory.rs` 统一输出 `terminal_holder` 与 `bob_holder`，支持 `Codex` `app-server` 热中断（`turn/interrupt`）与终端 `SIGTERM` 接管及交互式审批（`approval_mode='interactive'`）、`AGY` 桌面 IDE / Remote 只读保护（`agy_ide` / `HARNESS_REMOTE_CONTROL`）与独立 CLI 接管（`agy_terminal`），以及 `__bob_release_holder__` 释放空闲后台托管守护进程；前端 `DshSessions.vue` 统一展示忙态徽章、只读保护横幅、接管打断确认卡片、「交回终端」与可一键复制的「终端续接」CLI 命令卡片。
> - DSH 交互式问答确认闭环（`ask_user_question`）：`dsh_session.rs` 保留问题 `id`/`header`/`detail`/`multi_select` 并在读取 inbox `user-question-reply` 时自动抑制已排队回复；`dsh_transport::answer_question` 同时打通 `continued` 状态 (`POST /api/userQuestions/answer`) 与实时 `open` 状态 (`$events` WebSocket waterfall + `POST /api/$events/result` 含状态切换重试)；`node_pairing`/`bob-node`/`src-tauri`/`kernelBridge` 贯通 `app_dsh_answer` 与远端节点转发；前端 `DshSessions.vue` 替换原只读提示为内联单选/多选卡片、自定义补充输入、乐观提交去重与过期刷新反馈。
> - 生产上线与节点同步回执（`431b63d`）：Hostinger `dashboard.service`（`:3080`）已上线 `/api/assets/quota-checkpoint` 自动额度打点（`POST {}` 与 `GET /api/quotas` 均为 `200`，完整保留线上 `ai_quotas.json`）；Hostinger `bob-online.service`（`:53020`）已通过 `deploy-bob-online.py --non-interactive` 升级至 `431b63d`（`PID 2519913`，`active`，备份 `/opt/bob-online/operator-backups/quota-device-20261114-093612-238d6e07`），实测 `/api/node/v1/onboarding/status` 返回 `needsOnboarding: false`、`pairingCodeRequired: false`，未生成 `node_access.sqlite`，`http://127.0.0.1:53020/` 与 `app_bootstrap` 为 `200`，公网 HTTPS `https://bob-online.bobbik.org/` 未认证保持 Caddy `common_auth` `401` 原始密码鉴权；Contabo VPS3 `bob-node-access.service`（`:9527`）此前已升级为 `749a133`（`PID 2008766`，HTTP `200`）；X1T（`100.64.0.5`）休眠离线，Hostinger `/tmp/bob-node-stripped` 已更新至 `431b63d` 待 X1T 唤醒后同步。

> 当前外部Bot待办验收以本文件末尾“语义日期与原待办修改”及对应Evidence为准；早期阶段记录不是当前限制。

> Chief of Staff 幕僚长核心三项落地（2026-11-08）：
> 1. 本机节点接入：Hostinger VPS2 自动挂载 `local` 节点，Rust 端重构 `device_harnesses.json` 解析逻辑，彻底支持驼峰与蛇形混合字段及重复键（camelCase/snake_case 并存）反序列化韧性，本地免 Token 凭证通道就绪，本地 Claude (`/home/ubuntu/.claude`) 与 Codex (`/home/ubuntu/.codex`) 会话无缝进入侧栏目录并支持多轮对话翻阅。
> 2. 跨节点同类任务归一：实现 `resolveCanonicalProject` 规范项目识别引擎，覆盖 sea2, quant_lab, winter-shelter, history_map 等主流项目，打破物理节点隔阂，在项目分组下统一归纳多节点同名任务，会话行清晰展示各自主机节点徽章（`node-tag-pill`），维持本地独立上下文避免不必要 Token 膨胀。
> 3. 实验与临时测试智能屏蔽：基于 `isTestOrScratchSession` 启发式规则智能识别包含 test, probe, eval, tmp, scratch 等标记的无状态/测试会话，统一收纳于侧栏底部的折叠区域（`实验与临时测试 · N 项`），常态下保持项目视图极致干净，在用户搜索时通过 `:open="!!search"` 自动穿透检索。
> 验证与发布：全工作区 159 项前端测试、82 项 bob-runtime 测试及全量 Cargo 单元测试全绿；前端构建在 2.45s 内完成；生产端已发布至 Hostinger VPS2（`bob-online.bobbik.org`，服务重启并经本地与内网 RPC 验证 200 及正文加载成功），并同步剥离后的 release 二进制至 X1T 节点（`bob-node-access.service` 重启成功）。

> 2026-11-07 发布准备：用户批准上线及GitHub APK。线上官方缓存确认Gemini五小时约96%、周约99%，旧界面选最低余额误显示短窗口；本次固定antigravity为gemini-weekly、其他两家为weekly，缺周数据不回退。144项前端检查通过。Android显示名改Bob，使用既有正式SVG生成资源并在CI初始化后覆盖启动图标；保留包名和固定签名兼容升级。发布提交f75f521已上线Hostinger网页/后端及X1T/VPS3节点；公网浏览器确认gemini-weekly 99%、6d22h，Claude仍无官方额度。APK Run 42327592100成功，基于edda3c7（05aa86e仅归并历史、树相同）；名称Bob、15项启动图标像素、固定签名及16KB对齐均通过。成品dist-release/Bob-1.0.1-edda3c7.apk，未安装手机。

> 原生历史刷新（2026-11-06）：同消息数不再作为未变化依据，前端替换有界快照。定向测试与真实候选读回通过；Hostinger网页/后端及X1T/VPS3节点已发布并通过本机读回，生产浏览器经私有通道读取原会话新正文、定位底部及翻阅时输入框固定均通过。公网独立登录与手机未验，不能宣称外部CLI全实时流式已完成。见 [原生会话Evidence](evidence/2026-11-05-native-session-stream.md) 最新节。

> 连接清理（2026-11-06）：用户授权的两条旧连接已删除；两条统一节点入口保留且读取正常（X1T 41、VPS3 35条）。27条精确匹配项目关联接入新路径，41条未匹配历史关联保留；全部68条旧关联未删除。备份及客户端未验边界见现有额度/设备路径Evidence最新节。

> 最新额度功能（2026-11-05）：7e24e2a 已发布网页与后端。Codex app-server、Antigravity 官方 quota JSON 持续采集已接通；菜单已改为Logo、双线、剩余百分比与时长；来源和更新时间收至悬停详情，底线#333333，读取失败保留过期标记。X1T Claude 官方 statusLine 接收已安装，但尚无 rate_limits 观测，仍待验收。实际生产 Edge/Chromium ×390/1180px 显示、选字及菜单交互通过；浏览器经私有通道访问同一服务，未独立验证公网登录。无 APK/手机测试。详见现有额度 Evidence 最新节。

> 前次发布基线与设备路径验收见现有额度 Evidence；本轮未迁移或删除设备路径，Actions SSH 通道与手机验收边界不变。

> 下方云端交接及“尚未部署”措辞为本次发布前记录，由上述最新回执更新。GitHub Actions 运维 Secret 尚未验证开通；本次使用既有本地 PuTTY 通道，不混同两个执行环境。

> 当前事实与详细执行顺序的唯一入口；长期阶段见 [DEVELOPMENT_ROUTE](DEVELOPMENT_ROUTE.md)，需求与原始验收条款见 [todo](../todo.md)。沿用 BP、S、M、RV、OL、XP 编号，不另起路线。

## 独立运维通道（2026-11-05，等待一次性开通）

用户使用ChatGPT Android内的Codex云端项目，要求一次配置后Agent独立操作，不接受逐次在PuTTY代执行部署脚本。控制面工作流bob-operations.yml已通过PR #3合入main，PR #4补充bootstrap路径检查。Agent已成功触发inspect Run 41967908780并读取结果：BOB_DEPLOY_SSH_CONFIG尚未绑定，未开始SSH认证。

下一步由所有者一次性执行scripts/bootstrap-bob-operations.py并完成GitHub官方设备登录，把新专用SSH密钥直接加密存入仓库Secret；限定委托Bob服务的start/stop/reset-failed，不授予全局免密sudo。随后Agent自行触发inspect校验网络、主机指纹、身份、目录写权限和服务控制权限；通过后执行已授权的生产部署，不再把逐次操作交给用户。当前云端仍没有直接SSH通道，不等于GitHub runner已经连通，任何接入成功须有真实任务回执。

应用修复源为d7a7818076da3c0a0873788080541999871aa3e6，Android Run 41956502630已构建校验；生产尚未部署，真实官方额度仍未接通。控制脚本支持非交互文件操作/限定sudo，12项本地运维测试及语法检查通过，不能替代生产实连验证。

## BP4-06/07 设备多路径归并（2026-11-05，待实施）

用户确认：设备是唯一记录，公网统一入口和Tailscale内网地址是该设备的不同路径；此前工具专用通道不再单独代表设备。规则已归入ARCHITECTURE“设备统一入口与跨设备项目组织”。实际运行地址应归入运维inventory/service_map，不在产品源码硬编码。用户本次确认的目标映射如下（是目标配置，不代表已验证或已迁移）：

| 设备 | 主要访问路径 | 备选路径 | 旧入口处理 |
|---|---|---|---|
| X1T | `https://x1t-tablet.bobbik.org`，目标为该设备所有Agent的统一入口 | Tailscale `100.64.0.5`，端口/协议待核对 | `https://x1t-dsh.bobbik.org` 是早期DSH专用通道，等统一入口能力验证与关联迁移后停用 |
| VPS3 | `https://vps3.bobbik.org` | 专用Tailscale地址待从已授权运维配置核对，不猜测 | 同设备重复连接归并为设备下的路径，先保留旧ID映射和关联 |

以上映射由用户明确提供用于项目交接；不把同地址的两条配置直接当作公网/内网两条路径，也不因目标规划认定它们当前认证相同。

实施顺序：获取现有配置与备份 → 逐路径认证探测及设备/工具实例身份核对 → 保留原连接ID映射、会话/项目关联/草稿/未知回执 → 统一设备目录与路径状态 → 验证后停用多余入口。内网备选只读探测与执行恢复分别处理，禁止换路径重发未知执行。

修复候选：配置增加明确的deviceId分组，路径仍保留原连接ID与各自凭据；设置页可添加路径、人工归组、逐路径验证会话读取。目录按设备归组，失败提示标出具体路径并说明保留缓存。尚无可靠工具实例身份的跨路径会话不自动合并，行中保留来源路径；未实现自动路径切换，未知发送不自动重发。旧调用保存连接时保留分组和附加配置，未指定ID重复添加同地址复用原路径ID。当前操作是分组元数据维护，不代表全部关联已迁移或多路径高可用验收通过。

额度修复候选：删除后端固定余额、按本机用量推算订阅余额和重置周期的逻辑；前端不再采信旧百分比，缺少官方数据时明确显示暂不可获取，同时移除其他用量中的示例数值。真实账户额度、周期与重置时刻仍待接入官方认证数据源，不宣称已完成真实额度核查。

验证与交接见docs/evidence/2026-11-05-quota-device-paths.md。用户后续已明确授权直接生产部署；Linux x86_64生产程序和网页成品包已准备，隔离真实RPC冒烟通过。后续无需重复询问部署许可，当前只缺生产服务器实际访问能力；取得访问后仍须核对现网版本、备份和数据兼容性。用户现已通过PuTTY以ubuntu取得sudo，回传现网程序/数据/网页/回环监听与x86_64架构；可通过scripts/deploy-bob-online.py人工执行已授权部署（固定源码版本、数据副本兼容测试、备份及程序/网页回退）。云端仍不能直接SSH，不能把脚本准备视为部署完成。用户随后明确纠正：目标是一次性接入后由Agent独立运维，不接受每次由用户复制脚本到PuTTY代执行；因此先解决持久凭据与可用执行通道，人工部署脚本仅保留为已测试的备用工具，不再作为当前主方案。尚未执行生产部署。没有生产登录或节点凭据，尚未归并/删除线上连接，未部署生产网页。本次代码已合入1.0.1（d7a7818），Android Run 41956502630已成功构建、签名和校验；112项前端测试、生产构建与bob-node编译检查通过，完整运行时66/67（另1项在未修改基线复现失败）。390px隔离浏览器分组/逐路径状态/归组保留ID与额度菜单通过，不是生产或手机真机验收。公网入口可达不代表该设备全部工具已接通；代理拒绝访问不代表节点离线。

## 菜单切换混屏修复（2026-11-01，网页已发布）

用户要求立即修复线上。版本tab-visibility-20261101-r1，以既有f6db9d9网页源码为底，仅增加远端多根组件外的原生可隐藏容器，并区分文件引用的主题色细框；未纳入在研胶囊与缓存后端。此前v-show作用于多根组件，日程/知识/设置下仍残留远端对话。已备份web_dist后切换，无数据库变更、服务重启或回退。实际生产服务经私有SSH通道，在390/1180px分别验证三个菜单的远端正文和输入框均隐藏，返回对话仍可显示。不是手机真机或独立公网登录验收；已打开的旧标签页需刷新。见[evidence](evidence/2026-11-01-tab-visibility/deployment.json)与[browser](evidence/2026-11-01-tab-visibility/browser.json)。

## F2/F5 按需读取与有界缓存（2026-11-01，真机验证通过）

沿现有F2/F5施工：侧栏先显示已缓存的真实目录、后台刷新，单节点失败保留旧列表；原会话缓存最近10条正文，先显示并定位底部，再以游标核对增量。上翻按固定历史游标缓存分页，保留阅读位置。阅读缓存统一限制64 MiB/256项/7天，LRU淘汰，不存令牌，连接修改或删除清理所属缓存。发送与回执账本不使用该可清理缓存。

文件先核对DSH版本和大小，未变时复用本地原文件；图片生成最长边640px的WebP缩略图，点击仍打开原图。X1T运行服务为0.2.1-rc.2；PATH旧安装不代表运行服务。尚未发现原生服务端缩略图接口，因此首次仍下载原图，不宣称首次流量已下降。历史原始记录批量64条、页面仍至多10条正文，保留16MiB与时间上限。

源码684cbac，固定签名APK Run41224848706 / Artifact13051242091已构建、16KB校验并覆盖安装，保留连接。前端83/83、Rust运行时22/22、节点2/2及前端构建通过。真机Winter：最近10条定位底部，重开正文358ms，缓存列表34ms且后台刷新期间不清空，缩略图640×360，点击380ms打开1920×1080，关闭阅读位置不变，上翻10→20条且输入框不移位。实际版本核对返回not_modified且不含文件字节。缓存样本约1.86MiB；64MiB是序列化载荷上限，不等于操作系统精确磁盘占用。详见原有[证据](evidence/2026-11-01-dsh-mobile-network/REPORT.md)。首次图片仍获取原图，源历史仍带工具载荷；网络冷读仍可能超时。网页未部署，本轮未再发送模型消息，B4暂停。

## F2/F5 手机真实网络修复（2026-11-01，核心真机操作通过）

源码b79ec6a，固定签名APK Run41220334607已构建、下载校验并覆盖安装，保留原连接。修复重复历史读取、原生Blob图片被CSP阻止、后台轮询吞掉上翻请求，以及图片展开阅读锚点和过期等待提示。真实Winter原会话只有一次只读询问，回复可见并跨重启保留。

真机：最新底部定位、图片预载14.4秒/点击0.24秒打开1920×1080、关闭位置恢复、自动上翻9→19条及固定输入框通过；阅读锚点偏移0.25px。Rust22/22、前端76/76通过。首次10条目标仍可能因20秒读取预算只返回较少条目，冷加载性能、服务端正文筛选/缩略图/有界持久缓存未完成；不宣称网络已完全解决。网页未发布本次修改，B4暂停。详见[本轮证据](evidence/2026-11-01-dsh-mobile-network/REPORT.md)。

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

## 3 连续开发施工合同与渐进开发主计划（2026-11-02 整合修订）

本节是当前与下一步执行的唯一权威队列（整合 F2/F5 交互优化、单设备多 Harness 有序接入与保活、DeepSeek Super-Agent 路由及三项智能体防退化护栏）。沿用既有 BP / S / RV / XP 编号，不另立平行总计划。

### 3.0 四阶段渐进开发总览（Phase A → Phase B → Phase C → Phase D）

| 阶段 | 核心目标 | 严格施工顺序与交付物 | 退出门（Exit Gate） |
| :--- | :--- | :--- | :--- |
| **Phase A**<br>日常阅读与交互闭环<br>*(当前第一顺位，见 §3.3)* | 消除成果预览无法返回、缓存同步状态割裂、缺时间/复制、假流式四大痛点 | **A1** 成果预览左上角返回+安全区+Android/浏览器逐层退栈+恢复阅读位置 $
ightarrow$<br>**A2** 旧会话瞬时缓存+角落“检查更新/正在同步/暂未连接”统一+上翻“有新消息”浮钮 $
ightarrow$<br>**A3** 消息真实时间（跨天带日期）+ 保留 Markdown 复制栏 $
ightarrow$<br>**A4** “发送中→已接收→处理中→回复中→完成”五阶段状态与真实流式/整包诚实区分底座 | 390px/1180px 浏览器与单测全绿；预览返回不退应用、不重载会话；翻看旧消息不强滚到底；无假打字动画 |
| **Phase B**<br>多节点 CLI 有序合流与常驻触达<br>*(见 §3.6 第 1–4 步)* | 侧边栏有序聚合各节点 DSH/Claude/Codex/AGY，解决关 SSH 窗口即断连问题 | **B1** 抽屉置顶「Agent」，下方按项目/按设备有序分组（Web + Tauri `app_dsh_list` 合流）+ 新建 CLI 会话入口 $
ightarrow$<br>**B2** 目标节点 `bob-node` + 官方服务/持久子进程优先 + 原生会话恢复（BOB-D-013；tmux 仅作兼容保活） $
ightarrow$<br>**B3** 节点统一认证维护（发现/查认证/确定性恢复/登录/验能力，见 `wiki/harness-auth.md`）与工具递进接入：**先补 DSH 自动恢复**（先复用有效 Cookie，确认失效再从受控启动输出/限定日志换 token，核查独立程序握手协议） $\rightarrow$ **再完成 AGY**（同用户 Headless 缓存认证检测+历史读取） $\rightarrow$ **接通 Codex / Claude Code**（原版工具托管认证+原生持久接口优先） $\rightarrow$ **最后扩展 Pi（`--mode rpc` 管道）、Hermes（按 profile 隔离 `API_SERVER_KEY` + SSE）、OpenClaw（设备身份+配对+权限范围）** | 一次配设备可见已授权工具与历史；关终端窗口任务不中断；取消证实目标进程终止；AGY/DSH/Claude 互不串写 |
| **Phase C**<br>主 Agent 受控自治与三道防退化护栏<br>*(见 §3.5 & §3.6 第 5 步)* | 吸收 Super-Agent 与三篇论文结论，让主 Agent 省注意力且不盲目改坏代码 | **C1** Resident Router（快慢两级过滤）+ 非白名单任务 Observe/Ticket 草案确认卡（过程静默、终态回流） $
ightarrow$<br>**C2** 回执前置冻结 `ExecutionAssertion`（事前锦囊，`<0.90` 展开），对无证据“不对/重做”先展示锚点并澄清症状 $
ightarrow$<br>**C3** `TaskCard` 内联减法约束 + 必选检查全绿且进程终止后收回工作区写锁 $
ightarrow$<br>**C4** 首轮注入 3～8KB `ProjectContextCapsule` + 失败/换工具时注入 `WarmRestartState`（已定位文件+失败诊断+diff） | 草案卡未批不派发；笼统质疑触发澄清而非盲目推翻测试；验证全绿后只读锁定；V1→V2 跨工具热重启保留诊断上下文 |
| **Phase D**<br>真机实测与跨端接力终验<br>*(待用户通知连手机后执行)* | 在真实 Pixel 8 与多节点间完成端到端闭环验收 | **D1** ADB 覆盖安装最新 APK，实测沉浸汉堡包、置顶 Agent、票据卡夹、成果返回、缓存同步与流式状态 $
ightarrow$<br>**D2** 同项目“手机主 Agent 立项 $
ightarrow$ 节点 DSH/AGY 开发 $
ightarrow$ 胶囊/热重启交接 Claude V1→V2 $
ightarrow$ 手机审阅接受”闭环 | 真机冷启动、数据库完整性与上述场景全部取得实测 Evidence |

本节是下一步执行的唯一队列，取代旧BP4-04→05→06→07编号顺序；编号不变。来源：用户采纳本轮架构审查并要求形成连续工作指引。设计批准不表示功能完成或新增运行权限；有效边界见BOB-D-011、wiki/kernel-schema §10。

### 3.1 基线、目标与当前指针

当前最新增量：BOB-D-012独立实例契约与网页默认助理最小闭环已实现并发布为 `assistant-instance-20261031`。两轮持久讨论、一次原DSH会话询问、精确结果回流、390px、原会话跳转和候选重启恢复通过；APK未打包，原生App自动关联、分页、V1/V2治理仍待完成。证据见 [默认助理](evidence/2026-10-31-assistant-instance.md)。下方DSH手机专项记录仍保留其原验证范围，不与本次网页发布混同。

- 基线：源码fe018b0，证据4186208；网页DSH目录、原DOS只读续聊、Winter Shelter长历史读取已验证。Tauri原生接线、当前APK真机、附件、未知回执恢复尚未完整验收。接手时重新核对HEAD、dirty和实际发布回执，不能把该快照当永远有效的版本。
- 已有ModelHubRuntime、基础能力清单、工作区交接内核；胶囊、原生子代理池、新跨节点专长能力尚属设计。网页原生DSH传输回执不能冒充Task/Execution/Artifact闭环。
- 目标：手机打开原项目→查看真实进展→继续原会话→得到有版本的成果和验证→指定父版修改→人工审阅；不强迫用户在Chat/Work或快慢轨之间切换身份。
- **当前指针：F2/F5手机原会话接续补齐（3.3）进行中。Tauri接线已编译，Runtime DSH8/8、原生34通过/7专项跳过。新增持久化失败禁止发送、回执身份核对和未知恢复；共用组件/桥接21/21通过。本地5194复用现有已认证服务，原DOS会话仅一次进度询问，用户9465/回复9468，未新建会话；390px刷新历史/草稿通过。回执恢复浏览器4/4为本地候选API+注入回执，无额外发送；不能当作真实断线或手机真机。没有获取或转存令牌，前段凭据阻塞由现有连接复用解决。未上线、未打包；下一步为Android/手机真机、DSH权威未知回执查询及附件，不能用历史文本解除未知发送。详见[evidence](evidence/2026-10-31-dsh-mobile-recovery.md)。**

### 3.2 每次执行的固定规则

1. 读取AGENTS、本节、对应wiki和最近Evidence；核对工作树及他人改动，不reset/stash覆盖。确定本次最小文件范围、任务ID、代码基线与可恢复点。
2. 先查已有授权的对象、操作和有效范围。源码/轻量检查按授权继续；安装、编译、APK、真实付费调用、远端任务、迁移、发布等依项目门禁集中核对。过去一次发布或B4授权不自动扩展到本计划。
3. 每步先实现用户可用路径，再验证正常、失败、恢复；不能只写fixture或只新增菜单。模拟检查、真实供应商、浏览器、手机真机分别记结果。
4. 任一入口身份错配、权限扩大、未知执行或数据完整性异常，停止相关副作用；继续独立的源码和诊断，不绕过、不盲重派。超过预算必须保留失败与下一步，不换Task逃避预算。
5. 每个工作包收尾写回现有Evidence/CURRENT_STAGE，经事实对齐更新todo；记录输入版本、运行环境、结果、未验范围、回退方法。不得把自动检查写成用户接受。
6. 每次中断前固定恢复锚点：当前步骤、已改文件/未提交归属、已跑命令与进程身份、检查结果、待执行的唯一下一动作、授权缺口；不记录凭据。下一Agent先读回再继续。

### 3.3 第一项：F2/F5原生会话在手机可用

依赖：保留已上线网页行为，复用bob-runtime DSH适配与安全存储；不复制另一套协议。

实施：① 盘点Tauri命令/桥接是否覆盖列表、历史、续聊、凭据引用和错误映射；② 补缺失宿主接线与持久选择/逐会话草稿；③ 补未知发送的核对入口，只有权威未接纳证据才允许重发；④ 为附件先实现受控只读下载/查看（支持类型逐项声明，不能伪造远程路径可访问）；⑤ 统一手机、网页真实来源与时间提示。

#### F2/F5 交互体验与真实流式四项优化（2026-11-02 用户确认，按此顺序优先推进）

手机真机实测仍按用户要求等待通知后再进行；本地代码、单测与多视口浏览器验证按以下固定顺序实施：

1. **第一顺位：成果预览返回机制与顶部安全区修复（功能缺陷优先）**
   - 在预览视图左上角补齐显式返回按钮并遵守 safe-area 避开手机状态栏（保留右上角关闭作为辅助）；
   - 接入浏览器 History `popstate` 与 Android 原生返回键/手势拦截，按“图片放大等嵌套层 → 成果预览层 → 原会话”逐层退栈，禁止直接退回桌面或离开站点；
   - 预览关闭后直接恢复原会话 DOM 与原滚动位置（`scrollTop`），不重新请求或重载整个会话。
2. **第二顺位：进入旧会话缓存恢复与后台检查/同步状态统一**
   - 进入旧会话立即渲染本地已缓存的正文与成果预览（显式进入会话定位最新消息；同次阅读及关闭成果预览保持阅读锚点）；
   - 统一角落状态指示器：`正在检查更新…` → 发现增量时 `正在同步…`（仅增量合并变化记录） → 完成后短暂提示并自动收起；离线或请求失败显示 `暂未连接，正在显示上次记录`，保留正文与草稿；
   - 用户上滑翻看历史时锁定视口位置，有新消息到达时展示 `有新消息` 胶囊按钮而不强行滚底。
3. **第三顺位：消息实际时间与 Markdown 复制操作区**
   - 为每条消息增加轻量操作栏：展示真实时间戳（跨天带日期，无可靠时间时不展示、不伪造）；
   - 提供复制按钮，默认复制原始 Markdown 文本；桌面端悬停显示、移动端常驻易点，维持克制的间距与底色区分。
4. **第四顺位：各 Harness 真实流式接线、五阶段状态与断线恢复**
   - 按各工具真实接口能力区分“原生增量流式（逐段追加 + 断线续补）”与“整包返回（显示真实处理中，严禁假打字机动画）”；
   - 严格区分并展示 `正在发送／等待接收 → 已接收 → 处理中 → 回复中 → 完成` 五阶段，刚点击发送未获确认前不得提前显示“正在思考”；断线与 `unknown_send` 独立呈现并绑定回执核对。

验收：同一connection_id/session_id在网页与手机找到原会话；长Markdown可滚动、窄屏无整体横向溢出；经授权只读询问一次、回复落在原会话且不新建；切换两会话、刷新/重开各自草稿不串；凭据失效、离线、超限、发送回执未知显示准确。真实续聊需已有适用授权，否则先完成源码和假服务验证。附件无协议支持时明确列缺口，不拦住纯文本验收。

交付：代码/定向测试、统一本地页面、明确版本APK候选（获构建授权后）、实际手机证据。不能以桌面390px代替真机。回退仅恢复本轮程序，保留原会话、凭据和Outbox。

### 3.4 第二项：BP4/F6实例独立与共享对象边界（BOB-D-012）

实施：① 保持每个实例独立BYOK对话、草稿与资料持久化；② 明确浏览器对应实例及远端原生会话owner，不把客户端当成额外主库；③ 共享对象按来源ID/版本定位，只有相关远端写入依赖owner可达；④ 复用Outbox与原生回执，不自动复制凭据、迁移数据库或夺取工作区写权。

验收：无PC/无VPS时单实例能工作；远端离线不阻断无关本地对话；同名远端对象不合并；旧owner恢复不能双写；所有权转移和未知执行须显式核对。当前用户已确认此设计，取代全局X1C Home前置，不代表已有数据完成联邦迁移。

### 3.5 第三项：BP4-04最小项目画像胶囊

2026-11-01增量（符号骨架与Tauri命令对齐）：已在 `crates/bob-runtime/src/project_context.rs` 完成来源确定性分类（`SourceCategory`: `SpecCharter` / `CodeEntryPoint` / `ConfigManifest` / `GeneralSource`）与轻量符号骨架提取（`SymbolSkeletonItem`: 覆盖 Rust `pub fn`/`struct`/`trait`/`enum`，TS `export function`/`class`/`interface` 与 Python `def`/`class`，上限 16 项且严格限制 80 字符以守住 8KB 胶囊预算）；并在 `apps/bob-app/src-tauri/src/runtime_commands.rs` 与 `src/lib.rs` 将 `app_context_*`、`app_claude_*`、`app_capability_*` 与 `app_subagent_*` 共 23 项运行时 RPC 全量挂载至桌面与 Android 原生 Tauri IPC。`bob-runtime` 42/42、`bob-node` 8/8、`bob-app` 原生 IPC 35/35（7 项真实/进程专项 ignored）、前端 87/87 全部通过。

2026-11-01实现增量：bob-node宿主授权、网页胶囊读回、绑定目标的直接/助理发送已装配bob-task-card-v1（原目标、固定胶囊/基线、来源hash/片段、约束和wire hash），复用原派发回执。宿主每5秒检查授权来源并更新active版本，固定请求仍核对旧胶囊，不静默换版；网页可查看固定任务卡。真实本项目来源变化5027ms自动刷新、旧请求拒绝、新请求新材料、历史卡不变、390/1180读回通过；node5/5、runtime27/27、前端83/83。统一5194候选保留。用户本轮另行明确要求GitHub生成APK，Run41266758148 / Artifact13067890750已成功；源码4bdda61，APK已下载并核对SHA256和16KB ELF，成品dist-release/taskcard-4bdda61，本轮未安装/真机验收；网页不上线。BP4-04仍需来源分类/符号骨架、完整工作树/Owner交接、存储故障与真实远端Task/Execution/Artifact闭环；指纹不授予写权。见evidence/2026-11-01-context-task-cards/REPORT.md。

实施：先选一个获准工作区，不扫描整机。按project+workspace生成版本化索引，包含commit、dirty指纹、来源hash、提取时间、有效决定和最新审阅锚点；区分原文、摘要和推断。先用确定性提取，按需读取原文，不为生成胶囊默认增加模型调用。3～8KB是预算目标。将胶囊作为有来源材料装配，不升格为系统权限。

验收：同commit但dirty不同会失效；决定变更能定位来源；新建文件可以声明计划路径并由执行端检查父目录/范围；恶意文档不能扩权；缺文件/超预算/提取中断不会覆盖上一有效版本或伪称最新。执行端核对后才能写。交付契约、迁移（若必要）、读回UI与定向测试，不能只保存摘要文本。

### 3.6 第四项：BP4-06一个执行者的真实成果闭环（含 `tmux` 保活 + 原生 `session_id` 续接）

2026-11-10 本机会话删除：网页发布 `7f57cff`，定向 Rust 回归及隔离网页后端验证了删除、主助理保护与选择回退。生产页面资源已读回，「Bob-Bot 助理」可删除，原条目保留。X1T 两条空白 DSH 验证会话用原生归档接口处理，节点桥接 `a7cd520` 按原生归档状态过滤；37 项 DSH Rust 回归通过，生产目录确认两条测试记录均消失，仍返回 21 条 DSH 会话。登录后浏览器点击及 Android 仍待验。

多 Harness 原生新建（2026-11-09）：`2d13d79` 已发布至网页、X1T、VPS3。网页与已配对 APK 共用侧栏和主助理的显式创建入口。Claude Code、Codex、AGY、Pi 用用户首条消息启动原生会话，首轮受限权限。节点持久化请求 ID，确认原生历史可读后才按原生 ID 关联并打开。X1T 的 Claude Code、Codex、AGY 与 VPS3 的 Pi 均完成隔离首轮、同请求去重、同 ID 第二轮及生产后端历史读回，工作区文件未变化。Android 构建 Run `42583945097` 成功（Artifact `13658721618`）；待已配对 Android 真机与登录后公网网页交互验收。主助理自由文本自动代建仍待。

侧栏指定节点创建（2026-11-09）：网页、X1T 与 VPS3 节点已发布 9a43f56，GitHub 的 Android 接线随后修正并保留同一网页源码。新建项目可只建分组或启动首个本机／DSH 会话；项目内新建会话共用设备及工作区表单。主助理支持显式建 DSH 会话指令打开同一确认表单。已配对 App 通过网页主节点写入，手机真机仍待验。169 项前端回归、Linux DSH 37 项、节点项目权限 3 项通过，Windows PDF 项失败已在 Linux 同项复核通过。X1T 私有候选在验证工作区创建空白会话，同原生 ID 重试、读取及关联均通过；生产经统一节点读回同一条会话和新版网页静态资源。浏览器工具未能确认地址，未计入登录后视觉验收。Claude Code、Codex、AGY、Pi 原生新建和主助理自然语言代建仍待。

Android 签名 APK 对应 deafca7，GitHub Run 42569596753／Artifact 13652752156，SHA-256 e6c8f0eba688462b69f7c94195d2df25ad319b3179013698e0f85fe8be5b7b38，固定证书及 16KB 对齐已核验。首次编译缺失 URL 依赖已修，Gradle 公网下载 504 已切到官方资产并固定校验值。

DSH 待回答与停止（源码候选）：最新有界历史显示当前问题及选项，不从正文缓存恢复待答状态；答案仍在 DSH 原界面提交。停止走原会话的 `session/cancel`，保留队列和草稿；收到回执后重新核对会话是否空闲，未确认时不自动重发。前端 167 项测试及 Vue 组件语法通过，Rust 仅解析检查；尚未更新运行候选、未上线。原生应答回传和真实会话端到端验证仍待。

原会话实时接入与恢复续查（2026-11-07）：现场读取 X1T 官方帮助及 `claude agents --json --all`，确认一个 interactive/busy 会话；`claude attach` 是终端接入，`claude logs` 是最近终端输出，不能宣称结构化正文/审批事件订阅，未抢占或重发。AGY 官方 ACP 为客户端启动的 stdin/stdout 子进程；Remote Control 的 start/status/stop 不等于开放的第三方实时订阅接口。官方 Hooks 可报告生命周期，Sidecars 的 agentapi 文档只列发送/新建，不据此冒称可订阅所有运行中会话。外部 Claude/AGY 正文与审批实时接入仍未验收。

本轮实际修复：旧 Codex 未确认轮次先按摘要定位，命中后仅补读相同分页的 full items，最多三页/各二次请求；失败仍按已确认状态恢复并保留已有正文。125 项 Python 回归通过（19.245 秒），包含完整工具结果回填与读取失败降级。X1T 原验证会话 01a1b392-fcf4-73f1-84a1-b727afa11922 的官方只读历史接口读回原 turn、completed、4384 字符/2 items；使用隔离检查点，无提示发送、无项目修改。原常驻 app-server 已不可附着，本次验证用新只读 app-server 查询历史，不能充作正在生成的外部进程续接验收；真实工具/审批/成果故障补读仍待。摘要降级后再次补全机制仍待，不标记完整事件恢复完成。此次源码尚未更新运行候选，未上线/打包/安装 Office。


撤销提示与外部Codex故障订阅验证（2026-11-07）：审批错误改为按permission_id独立保存在对应卡片，不被轮询notice覆盖；HARNESS_TOOL_NOT_GRANTED显示中英文撤销提示、禁用允许但保留拒绝。5194真实请求c933df74-a763-49dd-98fc-b46dcfaa609e验证提示跨轮询保留，随后拒绝completed/工具error/审批denied/319字符、fixture未变、临时授权已恢复；前端index-r-xQyThj.js。独立官方app-server客户端创建专用会话01a1b392-fcf4-73f1-84a1-b727afa11922，仅生成100句无工具文本；Bob observer在152字符时核对PID身份后中断，重新订阅恢复4384字符与completed，同turn 01a1b392-fd12-7540-b523-98d7dcb10791，新观察1484事件，未通过Bob native_worker发任务、未重发、fixture未变。这证明app-server外部执行者通道，不证明任意TUI或AGY/Claude外部进程附着。此前尚未开始任何轮次的空会话附着超时，未发送模型请求，保留该边界。另修复完整旧轮次回执恢复时遗漏工具活动，先恢复activity再终态，124项Python（18.504秒）及33项Rust通过；此工具修复为回归证明，未冒称真实工具执行故障验收。候选节点SHA 424924e62703aa7c1202e2cd9d59aef209bff6b1b4af597a8dd5d82f32975397 已更新5194。仅摘要的旧回执仍无法重建完整工具/成果，外部各Harness全覆盖仍待。未上线/打包，Office组件未安装。

Claude授权快照与审批期间撤销实测（2026-11-07）：执行前持久化tool_authorization版本、审批模式和工具清单；批准时与宿主当前claudeTools取交集，旧无快照请求不允许新增批准但可拒绝。123项Python（18.456秒）及33项Rust通过。私有候选064638d440f8b070f7e1e8404645399a2ea094a9fa7a180f845650f8583baf8c已更新5194。网页正常发送a50ca39d-556e-4e55-b1f1-d3c6ffd9c285，在Read待审批后移除私有授权；点击仅允许本次未产生permission_decision，仍pending且无文件内容。随后网页拒绝，CLI返回拒绝说明，completed/error null，工具error、审批denied、唯一permission_decision，319字符，fixture哈希未变；临时配置已恢复。撤销阻止的是尚未提交批准的请求，不承诺撤回已执行副作用或批准后的原子撤销。发现UI未持续显示撤销原因，后续需补；外部CLI订阅与完整事件恢复仍未完成。未上线/打包，Office暂不安装。

Claude正常网页发送工具审批接通（2026-11-07）：HostGrant新增默认空claudeTools，仅从宿主配置传入原生发送；明确非空时进入interactive，工具参数仅列出获准工具并逐个ask，拒绝通配/default/参数注入，接收control_request再次核对工具名。121项Python通过（18.676秒），33项Rust通过，候选SHA256 164cbf338d586102be989382f93a7ee9734a2b6193c263ee36e8990bf9fb1765已更新5194。私有候选临时仅配Read，由网页正常输入发送请求2ab1af5f-05bd-46de-8002-6eb8e8395b08，原96161113会话出现待审批，网页仅允许本次后Read completed/审批approved/completed/error null，238字符回复，accepted/running/permission_decision/completed各1；fixture哈希不变。此次未替换worker启动参数，实走正式Rust→Python发送路径；完成后精确恢复临时宿主配置。只证明Read真实路径，非所有工具/写入或生产启用；执行回执尚未保存工具授权快照需补齐。外部CLI订阅、整机故障与Office仍未完成，未上线/打包。用户本轮明确同意向既有Hostinger私有目录发送源码进行构建。

Claude审批等待期间服务重启与网页允许验收（2026-11-07）：X1T专用只读会话请求bob-claude-restart-permission-17ecbf3a-638e-4cef-ba11-c026fc9fad6c，在Read待审批时核对私有9531服务PID/exe后停止并重启该读取服务，原worker仍存活、原审批ID保留。5194刷新显示同一Read请求及精确fixture范围，网页点击仅允许本次后completed/error null，237字符回复，Read completed/审批approved；accepted/running/permission_decision/completed各1，activity_snapshot 5/source_event 36/text_delta 13，fixture SHA不变。再次刷新仍显示独立用户轮次与完整回复、正常输入框。本次只读验证源码强制Read ask；正常产品发送入口仍关闭Claude工具，尚不能把该证明当成默认入口已启用、worker审批中崩溃或整机故障证明。候选二进制未变；生产/手机/APK未动，Office组件继续暂不安装。

独立回执缓存保留期与安全释放（2026-11-07）：收集进程关闭子进程后原子写入精确身份.done标记；新请求准入前在同一容量锁内，仅释放超过7天、completed/cancelled、已关闭、官方回执与持久正文/状态完全一致的缓存。请求账本和正文不删，unknown/运行中/无关闭标记/身份或正文冲突保留；最多检查128个缓存头并按request查账本，不全扫会话历史。110项回归通过（18.374秒）；扫描有界化后3项本地定向及X1T隔离临时目录3项（1.303秒）通过。未清理任何真实历史，当前源码未重编入5194，旧无done缓存保守保留。完整活动恢复、外部CLI订阅、整机故障和Office仍待，无生产/APK。

独立回执并发容量保护及候选网页复验（2026-11-07）：收集进程在SQLite跨进程写锁内检查文件数/总字节并创建或追加，单文件8MiB、目录64MiB/128文件；满额不启动新CLI或停止继续捕获，不清理活跃记录。109项本地通过（17.621秒），X1T两项真实子进程检查通过（0.939秒），含127已有记录时并发两次准入仅一次成功、总数128。33项Rust与前端构建通过；节点候选fb2676b2d663a5ca2165c0e36b5ddf95bb3a5154055792329e1c125330399d5d、前端index-FRShEV45.js已更新5194。浏览器打开96161113专用会话显示恢复后的完整80句，输入未发送测试草稿时发送按钮启用；旧25349会话仍未知并禁止发送。测试草稿已清空，无新增模型请求。自动过期清理/容量释放、完整活动事件恢复及外部CLI/Office门禁仍待；无生产/APK。

Claude独立回执收集与生成中监管崩溃恢复（2026-11-07）：受管CLI外增加独立结构化输出收集进程，先保存再转发，读取端消失后继续保存；原请求输入UUID/session/request写入收集头，恢复仅使用精确身份、完整行、官方success或interrupt双证据。单文件8MiB、总目录64MiB/128文件准入上限，满则拒绝新增、不删活跃回执；并发配额锁/保留策略仍需加固。108项回归通过（17.810秒），含真实收集进程输出读端关闭、错UUID与截断拒绝。X1T专用会话96161113-6ad0-4eca-9bc2-2e14f45234e0收到236字符时核对身份后中断worker，独立收集最终恢复completed/4501字符/error null，accepted/completed各1，原请求重入不重启，fixture哈希未变。实际完成监管崩溃后的正文/终态恢复，但不是整机掉电或全工具事件恢复。当前源码尚未编入5194；外部独立CLI完整订阅、收集端自身故障与容量并发、Office门禁仍待，无生产/APK。

Claude原生输入身份补齐（2026-11-07）：发送前将由session/request确定的UUID持久化为native_user_message_id，再随stream-json用户消息传入CLI；同文案不再作为未来恢复的唯一定位依据。107项Python回归通过（17.066秒）。X1T新专用只读会话96161113-6ad0-4eca-9bc2-2e14f45234e0返回READY/官方success，原生user记录精确保存传入UUID26bd36f3-86cc-493f-bfd6-70dd9d27ef6b。只读审计上轮故障记录发现thinking与text均可能携带end_turn，故不据此或完整正文把旧unknown改completed；旧请求缺发送前锚点仍保留unknown。界面保留活动卡时同步终态status，避免已确认后仍残留unknown。源码未重编入候选，官方终态长期独立收集/旧未知权威核对仍待；无安装/生产/APK。

运行中Claude监管中断与网页未知状态实测（2026-11-07）：专用只读会话生成期间核对/proc中的native_worker、原session和request后仅中断该worker；147字符已持久化，原请求unknown/HARNESS_PROCESS_LOST，重复原请求仍unknown，新请求被BUSY屏障拒绝，accepted/running/unknown各1，无completed/cancelled，fixture哈希未变。原CLI随后仍完成并落盘80句历史，暴露网页错误只显示恢复成功、遗漏未知回执提示。现独立跟踪执行状态，未知状态持续提示、暂停发送但保留草稿；5194真实浏览器输入测试草稿→发送禁用→刷新草稿仍在，已清空测试草稿。前端index-GrMd80lM.js已更新。此验证证明监管进程死亡不等于取消，尚未自动权威核对这类Claude未知轮次；完整外部订阅/整机恢复/Office仍未完成，未上线/打包。

Claude同文本不同轮次身份修复（2026-11-07）：历史解析器曾以最近user文字相等删除合法新输入，导致后续assistant并入上一轮。现仅按原生UUID去重；回执带native_user_message_id时按精确UUID核对是否已有输入，缺锚点旧回执仍保守保持既有逻辑，不猜测身份。119项Python通过（17.389秒），覆盖不同UUID同文本、重复UUID及相同文本的新回执。X1T专用真实会话两次相同Read请求只读核对通过：2个原生输入UUID→2条用户消息→各自独立回复，源JSONL SHA不变，未重新调用模型。33项Rust及候选构建通过，节点SHA256 2b9b66143d44beee91c451c7898446e32fc4eabbdacbbdeedbc7ef8177383f3b 已更新5194。5194浏览器读回确认07:20与08:38两条相同用户输入分别对应成功读取和拒绝读取的独立回复，底部原会话输入框可见。生产/手机/APK未变，Office仍暂不安装；Claude正常入口工具启用、网页允许及断线审批、外部订阅仍待。

Claude网页拒绝真实验收（2026-11-07）：Rust native_permission允许Claude前缀并继续精确Harness授权校验；33项Rust通过，候选13202857201c31382ebb4195ba73aa4ca9c1ed0e85fe3ac15b26d3899178cafd已更新5194。X1T原专用会话请求bob-claude-browser-permission-fb6d68a1-25f5-4b54-954f-f8d435cca29b：网页实见等待审批→点击拒绝→CLI返回未获批准→工具error/审批denied→completed，刷新仍保留回复且恢复输入框，fixture未变。验证脚本原断言错误地要求reject_once，实际UI明确提交null，随后只读核对原队列sent/choice null及唯一permission_decision denied，未重发请求；已补null拒绝回归，118项Python通过（17.994秒）。普通入口工具仍关闭；网页允许分支、审批中服务重启及正式发送启用待完成。另发现两次同文本用户消息在浏览器疑似合并，回复挤入上一条；需按原生消息身份核查，不能按文本去重。Office暂不安装，生产/手机/APK未变。

Claude单次工具审批真实闭环（2026-11-07，私有验证）：复用native_permissions队列，支持官方can_use_tool控制请求及精确request_id的allow/deny回复；仅单次选择、取消/过期转拒绝、写出前claim不重发、验证worker PID+session+request归属。未绑定会话及超限输入拒绝。wire恢复保留已有审批决定，不用工具快照覆盖。118项Python通过（18.478秒）。X1T专用会话96161113-6ad0-4eca-9bc2-2e14f45234e0请求bob-claude-permission-b049cb50-faca-4b03-a7d3-dc389535f349，私有验证源码只启用Read并强制ask；只批准精确无敏感fixture一次。实得1次permission_decision、Read completed、审批approved、253字符回复、fixture SHA不变；5194浏览器reload→展开执行记录实见已完成与本端已批准本次操作。审批由验证程序提交，尚非网页按钮交互；普通发送命令仍关闭Claude工具，Rust入口交互启用及拒绝/断线审批实测待完成。私有服务仍为上轮候选，仅本次执行使用新源码；未生产部署/打包，Office继续不安装。协议依据官方claude-agent-sdk-python query.py与subprocess_cli.py，不使用订阅Token逆向接口。

Claude监管失联期间持续正文恢复（2026-11-07，真实故障）：独立wire读取在尚无官方终态时也可恢复已核对input UUID/session的完整帧前缀正文和工具观察，保持unknown及禁止重发，不把进度当完成；末尾半帧忽略、错session不输出。116项Python回归通过（18.265秒），33项Rust通过。X1T专用会话96161113-6ad0-4eca-9bc2-2e14f45234e0请求bob-claude-wire-progress-c5757d63-f401-49f8-ae4e-1bc8a7dce230在124字符时仅SIGKILL精确Bob worker，独立CLI/采集器继续；202次unknown观察期间正文124→29197字符，最终completed/error null，accepted/running/unknown/completed各一次，fixture SHA未变，同请求无第二次执行。私有候选29ca1e48232d4e052882de61f6320ffac071dbe0fbd13f4356dfb6880ca251be已更新5194，浏览器reload实见240句完整回复及原输入框。这是Bob受管任务worker故障，不等同外部独立CLI附着或整机故障。当前恢复使用有界message_snapshot，增量传输效率仍可改进。Office组件暂不安装，生产/手机/APK未动。

Claude真实工具增量读取（2026-11-07）：发现仅更新event_snapshots不能让正常cursor轮询得到工具活动，现同事务在活动变化时追加activity_snapshot，重复原始事件不重复推送。115项Python回归通过（18.558秒），33项Rust定向通过。X1T已有真实会话eeddd53b-e6a8-434f-ae8a-e20ee1fee104中2条tool_use/2条匹配tool_result，在隔离临时账本通过实际source_event→增量读取接口得到4次活动更新、最终2项completed；源文件SHA未变，无模型调用或工具重执行。该证据是原始真实记录回放，不冒称外部CLI未落盘实时订阅或新工具调用。候选已更新统一5194，节点SHA256 c6a76c04c6c22dc5e758f784992144e7a01a07abfbd3dbd5d38a38748e9f36c3。下一步仍须完成真实运行时工具授权/审批/成果恢复与外部订阅。Office安装继续暂缓，未生产部署/打包。

Claude工具结果接收与候选更新（2026-11-07）：实际worker过滤器原先未保存user类型的tool_result，现已纳入source_event，避免只有工具开始而丢完成状态。114项Python回归通过（18.183秒），含实际worker接收循环；33项Rust定向通过。私有候选SHA256 a4c295a884cabdcc9048573ef2afe8b61548b7d1cbe1b42737481a6d07a2c3da已安装，统一5194刷新后恢复原Claude专用会话完整正文及底部输入框。此次浏览器只验证候选恢复可用，不等同真实工具事件验收；正式Claude命令仍关闭工具，需在既有授权边界下建立只读工具验证路径，不能把投影测试当成实际工具可用。生产/手机/APK未变，Office暂不安装。

Claude工具活动恢复（2026-11-07，源码验证）：结构化assistant tool_use及user tool_result按工具ID投影，精确session隔离；独立wire恢复在确认原request/input UUID及官方终态后，将工具快照与正文一起持久化，不重新执行。未收到工具结果保持unconfirmed，不以最终正文推断成功；64项有界，截断标partial，快照不带工具输出正文。113项Python回归通过（17.052秒），包含恢复入口无Popen、重复读回仅一份activity_snapshot、错会话及迟到tool_use不覆盖结果。当前尚未重编私有候选或完成真实工具/浏览器验证；审批、成果引用完整恢复及外部CLI订阅仍待，不视为1–8整体完成。Office组件继续暂不安装，未上线/未打包。

Claude官方终态持久化后的恢复窗口（2026-11-07）：source_event将精确session的success结果及精确request的interrupt确认/aborted_streaming分别折入快照；只对unknown执行使用双证据恢复，不抢占活跃worker。缺确认/错会话/错确认保持unknown，乱序可恢复，部分正文保留，不启动新执行。107项Python通过，随后unknown限定守卫定向复验通过；33项Rust通过。X1T取上一轮真实请求3ea66d2a-1f8a-4e9c-9065-79b6d5306e38的两条官方回执，在临时隔离数据库反序恢复cancelled/4020字符/单终态，原回执逐字不变。不是实际杀死运行中CLI或整机测试。最新私有候选19791f6a12af444352ee81d4295c7d874ef8f1c1189bdf2b200b37fe418266ad已更新5194，包含上一轮进程丢失重复投递修复。外部未落盘订阅、运行中监管崩溃、App/Office仍待；未上线/打包，Office安装暂缓。

进程丢失后重复投递状态修复（2026-11-07）：非AGY启动入口先核对原worker，再按持久状态返回unknown/not_accepted/终态，不再对未知原请求一律回答accepted；不重复启动，新请求仍被unknown屏障拒绝。进程存在不等于所有权，无法读回原请求身份不得推断alive。106项本地回归通过（16.089秒）；X1T临时隔离目录3项检查通过（0.597秒），包括真实Python子进程退出后原请求重入、部分正文保留及无第二次spawn。此为无模型的实际OS进程故障验证，不冒称运行中Harness或整机崩溃恢复；本增量尚未重编入5194，外部CLI未落盘订阅/Office/App门禁仍待。未安装组件、未上线/打包。

Claude网页停止与刷新恢复实测（2026-11-07）：33项Rust定向检查及前端构建通过，私有候选28faa2617f0016473450a1517ab3568a0a86479382eb642b3fc302e30a2d05a3、前端index-C9eB1qen.js已更新统一5194。在原专用只读会话通过正常输入发送80句验证请求，看到流式正文后点击停止；确认期间显示正在确认停止，刷新后仍恢复同一请求，终态确认后恢复正常输入。节点最终cancelled/error null，4020字符保留，accepted/running/cancel_requested/cancelled各1，281源事件与267正文增量。停止并非瞬时完成，未将点击或历史中断标记当作成功证据。受管Claude网页取消闭环已验，不等于外部独立CLI实时订阅、执行监管进程或整机崩溃恢复；App/手机未验，Office安装仍暂缓，无生产发布和APK。

Claude取消未确认异常补齐（2026-11-07）：缺少确认、控制回执编号不匹配、拒绝停止和传输中断均保留已有正文，不生成cancelled/completed。HARNESS_CANCEL_UNCONFIRMED修正为unknown，避免普通error误解为上一轮已经确定结束。104项Python回归通过（16.768秒），新增四种负向场景。App转发入口只读核对未发现Claude前缀阻断，但仍未编译/真机验收。本增量未更新5194候选或生产；网页停止按钮实测仍待。用户再次明确暂不安装Office转换及隔离组件，Office真实渲染保持未完成，先推进其他功能。

Claude官方取消实际worker闭环（2026-11-07）：按官方SDK双向stream-json控制协议先initialize、再发送原session用户输入；停止请求使用独立request_id的interrupt，必须同时得到精确success控制回执和aborted_streaming结果才置cancelled，支持终态/回执乱序，正常完成竞争仍completed。部分正文保留。Python/Rust/网页取消能力门已补源码，103项回归通过。X1T原专用会话实际worker取消成功，204字符保留，accepted/running/cancel_requested/cancelled各1，19源事件/11正文增量。当前新增未编入5194候选，网页停止按钮及App仍未验收；外部独立CLI/整机恢复/Office仍未完成，未上线打包。

Claude网页实时输出及生成中刷新（2026-11-07）：33项Rust定向通过，私有节点候选f7290b0475059edfe764f52c07f7a2f1e11ea636df8551686ec05c6ce7c104af已装入5194。侧栏真实Claude验证会话可恢复历史；正常输入框发送20句只读问答，生成中页面实见前5句及第6句未完、“正在流式输出”，随即刷新，恢复单条完整20句及正常终态，无重复正文/请求。原请求回执1655字符、105条text_delta、112条source_event，accepted/running/completed各1。受管Claude网页流与刷新已验，不等于外部独立CLI旁观/Claude取消或整机恢复；Office暂缓，生产/APK/手机不变。

Claude受管流事件身份与持久化（2026-11-07）：实际worker在原会话身份确认前拒绝正文/终态，不再允许未绑定流污染当前回执；确认后保存stream_event/assistant/result源事件及原有正文增量。102项Python回归通过。X1T独立只读Claude会话初始化后原session续聊真实完成，786字符、46条text_delta、53条source_event（含assistant/result/stream_event），accepted/running/completed各1。未占用既有工作会话，工具关闭。此项不等于外部独立Claude实时旁观，后者仍待；当前增量未重新编入5194节点候选，浏览器验收待，Office/生产/APK不变。

审批中私有服务重启恢复实测（2026-11-07）：正常网页发送的AGY只读请求停在pending后，验证候选进程身份并停止12秒再启动；server PID122060→124265，监管PID118332不变，请求及permission身份不变。页面刷新恢复同一审批→拒绝→原请求completed，输入区正常结束。持久事件accepted/running/permission_decision/completed各1，无重复派发。只验证私有HTTP服务重启及页面刷新，不等于整机/执行监管进程崩溃；短暂断线按钮隐藏未直接捕获，保持未验。未动生产/项目文件/Office组件，无APK。

AGY实际发送审批闭环（2026-11-07）：prompt_session联系宿主前核对精确工具授权，AGY正式请求携带interactive，其余工具保持deny。33项定向通过；私有候选e30f618afb67611e1b42f5b303e3b35def470153c64963245b685eb755e73cab更新。5194浏览器实际输入并发送只读pwd请求→等待审批→仅允许本次→工具completed→原会话返回exit0及目录→输入区恢复；本端approved不替代工具终态，两者分别验证。原fixture SHA保持不变。至此本地候选普通发送已启用AGY单次审批，允许及拒绝分别有真实证据；不代表生产或原生App已验。审批中的断线/服务重启与其他Harness完整事件仍待，Office组件暂缓，未上线/打包。

审批等待及空正文终态页面复验（2026-11-07）：actionable审批要求源permission_id、pending、未过期且执行仍active；断线/已终态不可点击，待审批卡自动展开，输入区优先显示等待审批。5项前端回归通过，私有前端index-DgEXjDF2.js已更新。X1T同一专用AGY会话新请求实测显示等待审批→网页拒绝→原回执completed空正文→输入区无回复中/无停止按钮，确认上轮残留修复。拒绝分支已验；允许分支/正式发送入口交互启用/App编译/外部CLI完整订阅仍待，Office组件暂不安装，未上线或打包。

网页审批真实往返（2026-11-07）：5194私有候选已更新，X1T专用AGY会话显示真实pwd权限详情，点击“拒绝”后节点回执completed/空正文，permission denied且不可重复操作，刷新恢复原会话。工具未收到完成状态仍诚实显示待核对，不以本端拒绝伪装工具取消。发现无正文终态/历史先结算时“正在回复”和“正在提交选择”残留，已修复为按全部执行状态重算及按具体审批决定清除提示；4项前端定向通过、构建并更新候选，修复后的新一次生成终态浏览器复验尚待。节点33项通过，候选SHA77811477f4d996afb57ad61dc6f646ba3348de2a36e295785e487816e670482c。正式发送仍默认deny，主动审批仅本次专用验证显式开启；允许分支、App编译、正式入口启用、外部CLI完整订阅和Office真实渲染仍待，生产/APK不变。

审批取消竞争及真实拒绝复验（2026-11-07）：队列claim事务内核对取消/过期，已queued的允许也改为拒绝，防止取消先发生仍放行；1024条总上限、仅清理过期非运行记录。原生App permission命令注册/转发源码已补，未编译验收。X1T专用AGY会话真实触发pwd权限请求并拒绝，发现官方end_turn无正文被HARNESS_EMPTY_REPLY误标unknown；现将明确end_turn作为终态并持久化prompt_result，不伪造正文。新专用会话复验：1次权限请求→queued拒绝→sent→completed，工作目录仅原fixture。101项Python通过。原未知请求未重发、未改写其旧回执；网页按钮实机往返/App编译、候选更新与交互模式启用仍待验，当前线上不变。

单次审批动作链路源码（2026-11-07）：native_permissions队列绑定session/request/源请求ID/监管token与PID，5分钟过期，仅保留源allow_once/reject_once选项，永久授权不提供；同选项幂等、冲突拒绝、失效进程/终态拒绝，写出前claim，写出断线不重发。实际ACP prompt循环已接队列登记/消费；app_native_permission节点命令/转发/工具授权、网页桥接及双语详情和单次允许/拒绝按钮已接源码。详情超8192字符禁用允许。98项Python与3项活动JS通过，节点33项定向/构建、前端构建通过；最后详情增量尚未重新编入节点候选。默认仍deny，只有显式approvalMode=interactive才等待，当前Rust prompt未启用；因此不宣称用户审批可用。下一步补原生App桥接、生命周期容量清理与真实审批往返/断线验收，验证后才启用。当前私有候选仍为上轮版本，生产/Office未动。

原生控制工具级授权补齐（2026-11-07）：审批接线审查发现native_cancel/native_events只有连接授权而未核对harnesses；现联系主机前精确核对工具，claude-jsonl映射claude，未知工具拒绝。33项Rust定向通过并完成私有候选构建安装，含上一轮AGY审批观察/本端拒绝记录（95项Python已验）；5194真实AGY会话reload后正文、附件准备状态及成功工具记录仍正常。实际用户审批操作仍未实现，下一项须以当前执行request、源权限请求ID及监管归属绑定动作，过期/已终态/未知结果拒绝重放；不得以已有默认拒绝代替交互验收。生产与Office安装保持不变。

AGY审批记录恢复（2026-11-07，源码）：实际prompt接收路径将session/request_permission持久化为pending；拒绝回复成功写出后记录本端denied决定及activity快照，写出异常则unknown/unconfirmed，不伪装对端收到或工具已取消。请求ID保留JSON类型隔离，已拒绝不被迟到pending覆盖，记录32项有界。前端双语区分“本端已拒绝”与“请在原工具中处理审批”。95项Python通过，新增工作路径测试覆盖拒绝写出断线，前端构建通过；未重建/安装该增量节点候选、未做真实权限请求浏览器验收。用户主动批准/拒绝交互仍未接通，不将默认拒绝当成审批功能完成。

原生file URI附件闭环修复（2026-11-07）：真实AGY回复使用file:///绝对路径，前端原样传入导致PATH_REJECTED，后端引用集合亦未归一导致NOT_REFERENCED。现前端统一预览/缓存路径、后端仅规范化助手显式本地file URI引用；远端authority、query/fragment、NUL、越界、隐藏相对路径和symlink保护保持。X1T 13项预览测试、31项Rust及前端定向测试/构建通过；私有5194浏览器实见“预览已准备”→48字节真实文件正文→返回原会话且输入框保留。上一条运行时验证文件预览待核查项已解决。生产/手机/Office均未变更，审批交互、外部CLI实时订阅及完整故障恢复仍未验收。

AGY真实读取成功及活动排序（2026-11-07）：同一专用会话读取无敏感验证文件，原生read工具completed、回复含准确marker/count，文件SHA未变化；浏览器reload恢复成功及前一轮失败记录。发现活动按请求ID顺序展示，现按源observed时间排序，3项JS测试/前端构建通过，统一5194已更新并实见先失败后成功。验证文件的附件按钮仍为“点击重试”，运行时验证目录的预览授权/引用尚需诊断，不由工具读取成功推断附件预览成功。审批交互、外部未落盘订阅和完整故障恢复仍待完成；Office暂缓安装。

AGY真实工具失败与网页投影验收（2026-11-07）：X1T专用空目录会话实际产生tool_call/read及tool_call_update/failed，353字最终回复解释目录不能由read_file读取；未执行shell、未新增文件。统一5194打开新会话并展开执行记录，实际显示“File .／失败”，输入框保留，恢复快照可读。31项Rust定向测试与release构建通过，私有候选已安装（SHA256见Evidence）；只证明真实失败工具事件及恢复展示，成功工具事件/审批交互/外部CLI订阅仍待验，生产未改、Office继续暂缓。

AGY工具事件投影（2026-11-07，源码）：实际ACP prompt分支现将原始source_event和工具activity_snapshot事务写入同一账本，网页现有NativeActivity可消费。tool_call_update按补丁合并，缺省标题/状态保留，终态不被迟到running覆盖；仅精确session、当前prompt捕获，不将load历史重放归入新请求。活动限制64项、标题512字符，超限明确partial。93项Python回归通过；真实工具事件与浏览器展示尚待验证、候选尚未重建，审批仍默认拒绝且未接通交互，不视为完整事件恢复完成。

AGY子进程中断验证（2026-11-07）：专用空工作区会话在22字partial后，仅终止已验证归属的ACP子进程组；请求转unknown/HARNESS_PROCESS_EXITED，partial保留。同请求重查不重派，新请求被HARNESS_SESSION_BUSY拦截，账本仅1请求/1次accepted，工作区未新增文件。修复同ID重查将unknown误报accepted的回执语义；X1T读回ack与receipt均unknown，监管记录0，91项Python回归通过。此增量仅源码及远端独立验证，尚未重建候选或上线。准确终态核对、外部CLI未落盘订阅、完整工具审批投影与整机故障仍未验收；Office依赖按用户要求暂不安装。

AGY生成中私有服务重启与浏览器重连（2026-11-07）：原会话新请求在收到6字partial后重启私有9531节点，监管进程未变；随后浏览器重新加载5194，读到同一请求最终2170字正文及结束标记。原账本accepted/running各1次，旧游标恢复64条事件、reset=false，无重复派发。只证明接入服务重启/浏览器重连，不代表AGY自身进程或机器重启后恢复。另一次测试因直接脚本与Rust桥接压缩源码格式不同被版本守卫拒绝，真实持久not_accepted回执可读，未派发；后续验证匹配已安装桥接源码格式，不放宽版本检查。下一步AGY执行进程/传输丢失后的准确核对与外部CLI事件订阅；Office依赖继续暂缓。

AGY网页取消闭环与拒绝回执修复（2026-11-07）：真实浏览器暴露旧监管版本拒绝被误报unknown，以及Rust取消路由/前端停止按钮仍限Codex/Pi两处接线遗漏。已为版本拒绝写入持久not_accepted回执（精确session/request/text校验、从不排队），网页识别明确拒绝后保留草稿并解除pending；无记录仍为unknown，不能据历史缺消息认定未发送。Rust及前端已允许AGY实际cancel_supported请求。原会话浏览器发送→停止按钮→官方ACP cancelled→页面“已停止回复”通过，监管PID103144复用；先前partial保留，候选重启后取消状态仍在。91项Python、负回执JS定向及31项Rust通过。候选已更新5194，生产未动；真实断线/机器重启及外部CLI订阅、Office安装验收仍未完成。

AGY正式请求驻留队列（2026-11-07，后续增量）：_native_start已将AGY请求写入SQLite会话队列，由带随机身份令牌的会话监管进程消费并复用官方ACP连接；精确进程命令行/会话/令牌验证，来源版本不一致时拒绝抢占。启动失败事务回滚，丢失进程保持unknown屏障，取消仍等待源终态；完成回执不会被后续事件写入错误覆盖。监管进程空闲300秒关闭，关闭连接与释放归属在同一事务，避免退出时吞掉新请求。91项回归通过；X1T原会话经两次独立_native_start调用均完成，监管PID99866相同，ACP请求ID3→4，实际证明正式入口复用而非只复用测试对象。原始工具/审批事件已存储，通用界面投影与外部CLI所有权仍未完整实现；真实取消、断线/整机重启矩阵继续待验。不安装Office依赖、不上线、不打包。 随后31项Rust定向测试/离线release构建通过，私有9531候选已替换，统一5194入口保持；程序哈希见现有Evidence，浏览器交互待验。

AGY ACP发送接线与真实双轮（2026-11-07，私有源码验证）：实际_native_worker的AGY分支已改为AgyAcpSession，经initialize能力协商、session/load消耗历史重放后发送session/prompt。当前回复、原始事件与取消终态写入既有请求账本；权限请求默认拒绝，session/cancel发送不等于取消成功，须等待同一prompt的cancelled终态。断线保留partial及unknown，同request不重派、新request保持busy。85项定向回归通过；X1T原AGY会话在同一ACP进程完成两轮只读验证，均收到2个正文delta，未观测工具调用。边界：本次验证连接对象跨轮复用，正式请求入口仍每request启动worker，尚未接通跨HTTP请求的驻留队列/单写者监管；未证明外部CLI活跃会话安全接管、真实取消/断线、完整工具审批投影。私有候选二进制尚未重新构建安装，生产未更新；Office安装继续暂缓。下一步接通持久监管及真实故障矩阵，不把本轮双轮成功当作完整事件恢复完成。

AGY官方ACP实机发现（2026-11-07，替代此前仅依据主帮助未列ACP的推断）：X1T 1.4.12的agy acp --help存在，真实initialize返回协议1、load/list/resume/close能力；session/list读取14个会话，session/load加载已验证原会话并返回148条正文/思考/工具更新。session/resume返回相同sessionId，但实测也重放148条更新，与官方“不重放历史”描述不一致。此轮未发送prompt、未写项目文件、未安装或启用daemon。下一施工顺序：①以协商能力而非根帮助文本选择适配器；②驻留ACP连接单写者监管并复用，初始化重放与当前prompt响应分段、重连不重派prompt；③原session/prompt的正文/工具/审批/取消接入统一事件账本；④用专用只读会话验证实时中断恢复，另验证外部独立CLI是否共享实时更新，不能把load成功当作外部订阅成功。既有stream-json发送暂保留，不切生产；长历史分页限制和Office暂缓安装保持。

工具返回证据展示（2026-11-07）：真实专用会话的rollout含custom_tool_call_output，返回bwrap回环网络配置权限失败，而官方turn items只有消息。现按同一turn与call_id关联有界返回摘要（600字符、最多256待关联调用），跨轮/重复返回不混入，不凭任意输出推断成功。前端工具概括改为双语中性“调用”而非“执行了/修改了”，避免只凭发起记录宣称成功。82项Python、31项Rust、前端定向检查及两端构建通过；统一5194实际展开显示原调用与失败返回，正文无重复、输入框保留。此项是已落盘证据展示，不等于外部CLI未落盘实时订阅、审批动作或全事件矩阵已完成。

事件恢复后续修复（2026-11-07）：审批请求早于原生轮次确认时，现与通知共用有界缓冲，确认精确turn后仅重放观察记录，不重复回复审批、不隐式批准；其他轮次不归入。浏览器发现同轮两段原生正文与合并回执重复，现依据rollout的turn_context/task_started精确turn归并，仅完整正文相等才抑制追加，不按相似文本跨轮去重。81项Python、31项Rust及私有候选构建通过；统一5194已更新，浏览器刷新真实专用会话确认两段正文各仅一次、工具调用标记及输入框保留。上轮终端失败已通过官方有界items接口核对：仅user/agent消息，无工具条目；不能由该接口证明工具执行事件成功。Office依赖仍暂不安装。

原生发送事件增量（2026-11-07）：Codex发送路径已将工具/过程通知按原request及精确turn持久化；轮次确认前有界缓冲，确认后归属，不混入其他轮次。78项Python回归通过。X1T专用只读真实调用已完成，47字符回复、32条文本增量及轮次/消息过程通知写入日志，工作区文件哈希未变化；但远端环境权限使终端调用失败，未产生工具条目，因此真实工具执行事件验收仍未通过，不以协议测试替代。未重发该请求、未放宽权限。此次候选构建终态尚未取得证据，不宣称已更新候选；生产、APK及Office依赖均未变更。下一步先核对该调用的官方失败事件与工具可用边界，再补真实工具/取消/重连验收。

外部Claude进程边界核验（2026-11-07）：X1T安装Claude 2.1.319提供官方agents --json（含交互式及后台活动会话）；logs是终端输出，不作为事件源。原生Claude续聊前现核对官方活动清单，同session即使idle也拒绝另启resume；接口失败/结构未知不当成无人持有。实际X1T idle会话验证阻止成功、续聊进程数0；76项Python、31项Rust检查及私有候选通过。该检查不是原进程实时控制实现，也不能消除外部进程在探测后启动的竞争；活动会话原通道接入仍待。AGY 1.4.12确认支持持续stream-json及官方remote-control daemon，但仍未证明Bob可订阅外部进程未落盘输出。

未知回执有界核对（2026-11-07）：不再thread/read拉全历史。先只取会话元数据并确认身份，最多3×20轮摘要定位原turn，再以thread/items/list读取该turn条目（20条/页、最多16页、1MiB及时间预算）。条目必须带相同turnId；不支持、超预算或不完整均保留unknown。已核对X1T安装程序的实际官方schema，避免参考源码版本差异；真实已取消轮次和已完成950字轮次通过临时回执恢复，不发新消息。73项Python、30项Rust与私有构建通过，统一5194候选已更新，未上线。更旧的超范围轮次仍保留unknown，不宣称全历史恢复完成。 原请求恢复现同时写入工具活动快照，沿用原request/turn并保留未确认审批；不会因整轮completed把明确inProgress工具算作成功。X1T真实已完成轮次经隔离回执恢复1349字正文、11条commandExecution/imageView活动及成功/失败状态，未发任务。74项Python、30项Rust通过，私有候选已更新；此项不等于全事件类型及审批动作验收。

长任务监管修复（2026-11-07）：移除已确认原生轮次的180秒执行截断，180秒仅限制启动/身份与Codex轮次确认；已确认任务继续由正式终态、确认取消或进程退出结算，不因普通静默自动杀掉。补上读取线程结束且队列无EOF哨兵时的退出检测，避免永久等待。69项Python回归、30项Rust定向检查通过；X1T隔离协议子进程静默185秒后完成，证明真实管道不会被旧上限关闭（不是长时模型生成验收）。已更新统一5194私有候选，未发布生产。全工具真实长任务、显式任务预算及整机故障矩阵仍需继续。

私有节点服务重启验收（2026-11-07）：浏览器专用只读请求重启前为running且有确切原生turn/存活worker；仅重启统一5194背后的私有bob-node，重启后同request/turn/worker身份记录完成，正文950字、同内容请求数1。浏览器刷新恢复完整回复，结束标记1处、停止按钮0个。证明该私有服务边界的恢复，不等于整机重启/断电。另已完成专用Codex worker强制退出验证：先降级unknown，同请求幂等返回不启动新进程，新请求被阻止；按确切原生turn核对恢复cancelled。整机故障与其他Harness矩阵仍未验收。生产与用户项目任务未受影响。


PDF端到端验收（2026-11-07）：独立验证工作区的双页PDF经真实Codex会话生成文件引用，统一5194现有成果弹窗成功读取并渲染，两页naturalWidth/Height均906×1280、complete=true；返回原会话和再次打开通过。文件1118字节，SHA256见Evidence；不是模拟后端，也不是用户原项目PDF。此项关闭基础PDF真实链路验收，复杂PDF/大文件分页与DSH独立端到端仍待补，Office依赖仍按用户决定暂不安装。


浏览器取消验收增量：统一5194通过真实专用Codex会话发送并点击停止，页面确认“已停止回复”、恢复发送入口。实测发现旧取消正文在新一轮/刷新后丢失，现已将cancelled回执正文按原时间归并历史并去重，隐藏完整的工具内部中断/续接控制信封。66项流回归、30项定向检查及构建通过；候选刷新与日程往返复验：旧部分正文只显示一次，内部标记消失，无运行/停止状态复活。浏览器停止入口与取消历史保留已验；整机故障、多工具全事件等剩余门槛仍保留。


Codex真实取消后端验收（2026-11-07）：独立只读验证会话在临时空工作区运行，47字符时请求取消，观察pending与实际派发，官方exact turn终态interrupted；回执/恢复快照均cancelled。新进程再次读取官方终态与持久快照一致，pending清除，526字符部分正文保留，工作区0文件。已闭合Codex后端真实取消与新读取进程恢复；浏览器停止按钮交互、整机断网/重启、外部进程取消授权仍不能据此判完成。首轮空会话续接被拒绝、验证脚本短超时引起bootstrap中断的失败记录已保留，未重放不明请求。


取消与重连增量：已修复Codex服务端请求ID被误当作控制响应，以及Pi身份核对期间取消后仍派发提示的竞争；65项流回归、30项Rust定向检查及候选构建通过。X1T正在运行的真实原轮次只读socket主动关闭并重连成功，隔离临时回执由unknown恢复running，正文一致、单快照、两条工具记录恢复；未发送提示、未取消用户任务。此项证明连接重建，不能替代物理断网、服务重启或真实执行取消验收。


当前候选状态（2026-11-07）：统一5194已安装最新事件恢复候选，未发布生产、未打包。Codex控制等待与实时循环通知按确切轮次持久化，工具/审批观察进入有界活动快照；已完成回执补回活动不重新运行，断线将未确认状态显式降级。官方请求结束只标记请求结束，不等于批准。63项流回归、2项前端活动检查、27项Rust定向检查及候选构建通过。

真实验证：X1T原会话只读观察获得实时通知；已知旧轮次以最多3×20条官方摘要核对，临时unknown回执恢复为completed且保留正文。最新本地浏览器刷新、切换日程再返回后，仍保留两组真实工具记录（1条和12条），并恢复当前回复状态。此次没有发送提示、修改远端项目或触发审批。此验证不等于物理断线、真实取消或全工具恢复矩阵验收。

成果预览：真实HTML成果及七张配图已在本地浏览器显示，缓存后台核对更新已接通。DSH的PDF/Office读取已接入与原生会话共用的受限转换器，保留引用授权、工作区路径、读取前后版本校验与2MiB预算；未变化文档不重复转换。30项Rust定向检查通过，含现有PDF转换器的真实跨语言页面图像输出与损坏输入检查。X1T原会话最近60条消息没有PDF引用，真实原会话PDF浏览器验收仍待完成；不得以样本转换代替该验收。用户决定暂不安装LibreOffice/bubblewrap，Office真实渲染保持未验收，其余工作继续。 文档预览等待预算已修正：原生宿主110秒、跨节点文档转发240秒（覆盖转换与文件读取），普通类型仍35秒；转发保留白名单内的渲染器缺失/转换超时等错误，未知错误不回显。31项Rust定向检查通过；私有候选已更新，本地双页PDF复验两页906×1280均完成、返回会话正常。此浏览器复验不证明慢转换或公网代理的长请求可用，异步转换/慢链路验收仍待。

剩余门槛：外部Claude/AGY等实时源、审批操作与真实取消、跨轮次/重启及物理断线矩阵、无原生轮次身份的未知发送核对、全事件类型及完整文档格式验收。不得将上述定向通过等同完整八项完成。具体证据沿用native-session-stream。

最新源码增量：官方读取响应与服务端审批请求按消息类型隔离，52项Python通过；现有成果弹窗已接自包含HTML隔离渲染，2项策略检查与Vite构建通过。浏览器实测、网页资源包、PDF/Office和原生文件读取仍未完成；未发布。接续期间通知缓冲/快照去重仍须实现，完整八项保持进行中。详见同一原生会话流Evidence。

当前连续目标：完成实时来源、身份对应、全类型事件、未知发送核对、审批/取消、跨轮重启监管、成果引用与查看、真实故障矩阵八项；成果必须实际展示HTML网页、PDF、Office文档（至少docx/xlsx/pptx逐格式验证，旧格式明确支持边界），不能以源码或下载代替渲染。沿本节和既有F5任务推进，不另建总计划。手机仍等待用户通知。

2026-11-07后续源码增量：已定位并修复Codex长会话附着拉全历史导致8MiB上限拒绝；改元数据附着加最近1轮有界读取，X1T当前源码只读实测成功，50项Python通过。本增量未发布，完整目标仍未完成，详见同一原生会话流Evidence。


2026-11-07 当前增量（网页及三节点已发布）：聊天进入时默认定位最新，内容/图片增高跟随底部；主动上翻保持阅读锚点，下箭头返回最新，预览返回仍恢复原位置。真实 Winter Shelter 生成期间重启私有读取端，从125字符恢复至548字符，原请求仅一条、Git指纹未变。VPS3 Pi 官方 abort 后收到 aborted/settled 才确认取消。X1T 已有 Codex 官方 app-server 的 Unix WebSocket 只读旁观已接实际事件路径，可读取服务内存中的当前轮次；不向外部会话发送新 turn、不接管审批。Rust27、Python34、前端21项定向验证通过，本地浏览器和生产认证RPC通过。详细证据见 [原生会话流](evidence/2026-11-05-native-session-stream.md)。

**当前优先级**：先完成事件恢复，再扩展各工具外部订阅与成果读取，Claude额度不抢占此顺序。2026-11-07 本轮已实现 epoch/游标失效后的替换快照、事件有界保留、回执与终态事件双向修复、旧外部轮次权威核对；本地48项Python、24项前端、27项Rust检查及两节点真实记录副本验证通过，网页主服务及X1T/VPS3后端均已备份发布，生产认证链路恢复读回通过，结果见同一Evidence。**剩余**：外部Claude/AGY及独立Codex TUI未落盘订阅、缺轮次unknown权威核对、Codex真实取消、跨轮监管、全事件类型与手机终验。不能把正文/终态恢复宣称为所有工具的完整事件恢复。此次恢复为生成中读取服务重启，不冒称物理断网或手机终验；Tauri仅源码接线，手机暂停、无APK。

2026-11-05 DSH 认证分流修复：统一节点的列表/正文/预览/发送改走节点认证，DSH 凭据仅由宿主管理。X1T 真读 41 会话、Winter Shelter 最近 10 条与身份保持通过，网页已发布；固定签名 APK Run 41884589345 已完成并校验（源码 0154fef），手机未测。证据见 [2026-11-05-dsh-node-routing.md](evidence/2026-11-05-dsh-node-routing.md)。

现行监管选择按 BOB-D-013：优先官方服务/持久程序接口，tmux 为兼容路径；下文早期实现记录不代表全部工具必须套 tmux。

2026-11-04真实配对审查：设备地址挑战认证流程未验收。生产回环API实测未认证可创建邀请、错误code可凭邀请路径兑换；测试凭据已撤销。优先修复机主授权、严格一次性兑换、随机凭据与节点身份，再推进统一设备入口。生产前端经SSH转发检查，不等于公网登录或手机验收。详见[evidence/2026-11-04-node-pairing-audit.md](evidence/2026-11-04-node-pairing-audit.md)。


2026-11-01实现增量（离线Fixture闭环，未调用真实Claude计费）：已在 `crates/bob-runtime/src/claude_harness.rs` 与 `crates/bob-node/src/claude_rpc.rs` 完成原生 `Claude Code` 执行适配器、`TmuxHarnessRunner` 保活与重启对账、原生 `--resume <session_id>` 续接、同工作目录跨工具（Claude/DSH）串行写锁（`HARNESS_WORKSPACE_BUSY`）与 `bob-task-card-v1` 交接挂载，以及 `V1 -> 指定父版 V2` 不可变快照与验证/待审/接受状态机；统一对话侧栏（`ConversationDirectory.vue` / `DshSessions.vue`）直接显示节点、项目工作区与 `Claude` 工具标签，不新增独立 Claude 界面。受控网页成果与 Markdown 文档成果两类 `V1 -> V2` 闭环、取消/超时/崩溃重启不重放均已通过离线 Fake-Claude 回归（`bob-runtime` 30/30、`bob-node` 6/6、前端 `86/86`），详见 `docs/evidence/BP4_06_CLAUDE_HARNESS_VERIFICATION.md`。同日经用户授权完成 X1T 现场只读盘点（`/home/bobbik/.local/bin/claude` `2.1.316 (Claude Code)`，`claude.ai` Pro 认证有效，无 `tmux` 自动走受控 `process` 模式）与真实两轮 `V1 -> V2`（`native_session_id: eeddd53b-e6a8-434f-ae8a-e20ee1fee104` `--resume` 续接、V1 快照 SHA-256 不变）实机闭环验证，证据见 `docs/evidence/x1t_live_claude_v1_v2_evidence.json`。

实施：① 按实际已安装Harness（`dsh`、`codex`、`claude`、`agy`）核对 capabilities/version/认证方式及原生会话寻址；② 直接续聊保留 `native_sessions` 原生绑定（如 `claude --resume <id>`、`codex exec resume <id>`、ACP `session/prompt`），无须先走BYOK；跨工具委派或原生会话失效时才挂任务卡/胶囊；③ 在支持 `tmux` 的节点采用“外层受控 `tmux` 防断线保活 + 内层结构化 JSON 回执落盘（禁止 `send-keys`/`capture-pane` 刮屏）”，支持随时 `tmux attach` 旁观；④ 以既有Task/Execution与runner契约记录派发、采用、取消、终态、验证、成果登记；⑤ 绑定父ArtifactVersion与输入hash，保留历史快照；⑥ 在统一对话中展示总结、重要失败、可展开过程和成果。

验收：专用受控网页任务V1→指定父版V2，实际可操作网页、依赖包与数据一致；V1哈希不变；对照第二类简单文档成果避免只修一个例子。重启、超时、重复/乱序回执、取消与迟到结果不重放；生成/验证/待审/接受分开。未知执行先对账，单次任务的修正次数与费用上限在启动前冻结。不把远端原生会话自动伪造为已治理的执行。

边界：优先已有DSH或Codex一条有证据路径，其余按能力诚实降级；不要求同时接齐全部Harness。不恢复X1T/B4建模，不修改真实项目，不擅自发布测试成果。跨工具并发写同目录禁止；人工IDE修改需重新核对工作树。

#### BP4-06 当前接续施工：单设备入口、多Harness与项目归组

2026-11-04 后续获批实施：两个指定域名的独立节点认证入口已部署，稳定身份、机主密码、一次性5分钟邀请、独立365天凭证、撤销及匿名/越权拒绝均经公网实测。线上连接界面已发布并保存两台新连接；节点重启后新凭证可读取VPS3 35条、X1T 40条真实目录，X1T DSH/AGY/Claude/Codex历史各一条已读回（DSH Winter Shelter最近10条）。本机DSH回环HTTP/WS及受控日志认证已接通，旧远端TLS约束保留。手机暂停、无APK、未发真实项目消息；不是所有工具续聊/流式已完成。VPS3独立DSH未发现（3080是旧Dashboard），Pi默认长期配置和AGY兼容仍待接。旧入口身份迁移、旧Online配对协议加固和客户端发布单独验收。详见 [统一节点入口证据](evidence/2026-11-04-unified-node-entry.md)。


#### BP4-06 原会话续聊与增量当前事实（2026-11-05，本地候选）

2026-11-07 本地候选增量：统一5194已更新。会话目录使用本地Harness图标（Codex/AGY/Claude/Pi/DeepSeek），保留可访问名称；Pi原始资产来自官方 https://pi.dev/logo.svg 。AGY官方ACP结束回执按request/session/整数serial三重核对恢复；真实只读原会话在官方结果落盘后强制退出监管程序，下一次读取从unknown恢复completed，207字符正文和一次已完成read工具保留，无重发、验证文件hash不变。浏览器已验证最新回复恢复、执行记录展开及readonly-check.txt预览/返回。另补recovery_pending：先确认结束但完整items未读到时，后续继续补读，不重复发布终态。

定向验证：127项Python、33项Rust通过；前端151模块构建通过。当前候选前端index-C1O6IkrL.js，bob-node SHA256 `01bdb6f8d5e1408aa213635277be4cdbc4eec217bd261c61b7993fa2d1ffb3a1`。未上线、未打包、未安装Office。外部独立Claude/AGY运行进程热附着、整机故障及跨工具审批/成果完整恢复仍未验收，不以本次受管ACP证明这些能力。详见既有native-session-stream证据。

本轮已实际发送只读消息：X1T Claude、AGY（Winter Shelter）与 VPS3 Pi 原生 ID 不变，分别捕获 6/7/38 段文本增量，三处工作区 Git status+diff 指纹前后不变。Pi 使用此前已授权的 DS for Remote 私有 profile。统一5194候选可读两节点真实目录及 Pi 原会话；页面实际发送接收到持久增量事件。已修复静默新建会话 fallback、绕权限默认参数、死进程假完成、Pi 截断假成功与用户快照串为助手消息。

此增量替代下方早期“仅探测/无实际发送”边界，但 BP4-06 仍在施工：X1T Codex 超时且官方原会话末 turn interrupted/无文本，保持 unknown 不重发；VPS3 Claude 未获回复；VPS3 Codex 原工作目录已不存在；DSH 实时订阅未接通。前端目前按游标轮询事件，跨轮持久进程、取消确认、全局事件容量及过期游标恢复尚未完成。手机仍暂停、未上线、未打包。代码与实测见 [本轮证据](evidence/2026-11-05-native-session-stream.md)。

连续施工顺序：① 完成 Pi/AGY/Claude 共用事件的 UI 身份/去重回归；② DSH 采用已核实 session/follow assistantStream 协议，不再 snapshot 后立即关流；③ Codex/Claude 未知请求按原生回执对账，修复确切失败点，不重配令牌或偷换会话；④ 目标端长驻监管、取消/重连、容量/过期游标与 Tauri 接线；⑤ 用户通知后再做手机验收。以上沿 BP4-06.b/c/d，不新增平行计划。

#### BP4-06 / F2/F5 标准适配器与流式当前任务（设计与首轮实现）

2026-11-06 接续增量：DSH `session/follow + assistantStream:true` 已接入实际节点 RPC 和共用聊天界面；连接持续复用、断线以官方基线替换临时正文、消息提交后与持久历史去重。X1T Winter Shelter 实际订阅及统一本地5194正文/底部输入框验证通过；线上已认证连接读回41条目录、最近10条正文，持续订阅为live且只建1条连接。未知原生回执不再把本地意图伪装accepted；Codex仅依据记录的确切thread/turn官方只读结果恢复，旧缺turn ID回执保留unknown。27项Rust、24项worker、12项前端定向检查及构建通过。网页/节点已发布；未新增真实模型请求、未修改项目文件、未测手机、无APK。DSH临时增量为有界内存视图，浏览器仍轮询，非节点持久事件全链路完成；外部CLI未落盘订阅、真实生成中/断网终验、取消确认与全局事件保留仍待验。证据见 [原生会话 Evidence](evidence/2026-11-05-native-session-stream.md)。

状态：用户随后授权必要修改并指定 VPS3 验证。已将 `harness_auth.rs` 原有静态能力与登录推断替换为宿主授权下的版本/原生协议探测；共用 `app_harness_*` RPC 沿 `agy_directory.rs` 执行 `harness_probe.py`，仅允许连接授权清单中的工具，非默认 profile 未支持时拒绝冒用默认身份。配置、官方支持、现场观测与 Bob 接通状态分开。JSONL 组件保留接受回执之前到达的通知，有帧/队列/总量/时间上限并回收自有子进程。

VPS3 实测：Codex 0.156.1 原生 app-server 握手、账户状态和目录请求通过（返回 0 条，不证明既有历史续聊）；Pi 0.85.1 同进程状态/隔离空历史/模型列表读取通过，但可用模型 0，短文本测试在接受前被拒绝，流式未通过；AGY 1.2.16 ACP 初始化进程退出，保持未知；Claude 2.1.74 本机登录状态通过。DSH 有既存端口观察，但本轮未完成服务身份/认证验证。详情及源码哈希见 [VPS3 验证证据](evidence/2026-11-04-vps3-native-adapters.md)。

后续指定凭据复验：用户授权 `DS for Remote`，VPS3 Pi + 注册表 `deepseek-flash` 实际收到 23 次文本增量，首段 972 ms、1063 ms 完成，`agent_settled` 与 idle 核对通过。凭据只临时注入，默认配置仍未改；网页流式/原会话续聊未验。

本地 runtime 62/62、最终认证定向 6/6、协议组件 11/11 通过；本轮只是原生探测接线与 VPS3 协议验证，不是新工具网页收发闭环。下一步先解决节点配对审查阻断项，再落节点持久事件/游标和一种可用工具的真实发送链路；Pi 模型认证及 AGY 版本兼容单独处理，不自动升级、提取其他工具凭据或重发拒绝/未知请求。以上为该轮探测时的历史边界；随后节点部署与配对结果见本节前述统一入口增量。未打包、未测手机。

1. **BP4-06.a 统一入口与能力真实性**：先完成已记录的节点认证/配对阻断项；再以 §6 契约拆分官方支持、现场探测、Bob 接通状态。探测限获准宿主/用户/profile；命令存在不等于登录或能续聊。源码落点先沿 `harness_auth.rs` / `agy_directory.rs` 与既有 node RPC，不新增平行任务账本。
2. **BP4-06.b 持久适配器**：保留 DSH 现有服务与原 session，接通 AGY 原会话路径，再接 Codex/Claude，随后 Pi；复用 registry/supervisor/session contract。启动及空闲回收有明确所有权，外部服务不自动重启；每种工具单独记录支持的版本范围。
3. **F2/F5 + BP4-06.c 真实事件链路**：从上游文本增量到节点持久事件再到网页渲染全链路接线，覆盖审批、取消、成果和终态。不能以“工具支持流式”或历史轮询证明 Bob 已流式；不只实现校验函数。
4. **BP4-06.d 兼容测试与产品闭环**：公共一致性测试套件覆盖至少 DSH 服务与一种 stdio 驱动；真实节点原会话测试另行记录版本、首段时间/终态时间、request/session ID 和断线重连结果。仅在模型请求授权明确时做有副作用的测试。

验收：未安装/未登录/未知版本、已接受但未完成、重复/乱序事件、UTF-8 分帧、增量后快照不重复、断流补齐、未知发送不重发、取消待确认、节点重启、慢客户端背压及同会话多入口竞争；既有 DSH 历史/凭据/草稿无损。统一本地页面显示缓存→核对→实际回复状态；新适配器无需另一套聊天 UI。手机仍等用户通知；本次无上线/打包授权，不改变既有运行事实和完成度。

回退：保持现有 DSH 兼容路径；新驱动可按配置停用，保留原会话、回执及未决执行，不能通过回退重发。组件规范见 wiki/harness-auth §6，原生监管选择替代旧的统一 tmux 假设。


本轮补充：AGY目录已提供短时复用与失败缓存回退；Winter冷读取曾超过60秒，放宽等待后的浏览器复验通过，性能问题仍待优化。

本地目录接线补验：统一5194此前恢复时本地remoteNodes为空，现已复用X1T既有DSH启动凭据保存到候选Vault。Rust agy_directory通过宿主配置中的SSH目标与AGY数据目录，执行有界只读索引探测，与同一connection_id的DSH列表合流；实际16条DSH+14条AGY。AGY使用命名空间ID防止误投DSH，日期解析支持索引时间字符串。390px浏览器确认AGY目录与未接通历史提示、DSH Winter原历史正文；页面无JS错误、内容宽384px。Rust定向2/2、目录JS4/4及候选编译通过；旧正文与AGY网页发送仍未接通，不以只读目录证明全功能统一设备入口。刷新恢复按AGY前缀保持禁止发送。使用已有本地候选数据，未上线、不打包、未测手机、未发送模型请求。


2026-11-02 用户更新施工优先级：先X1T统一设备入口与AGY已有会话发现、历史读取和原ID续聊，再扩展其余工具；替代此前Claude优先顺序，不替代宿主/认证/回执安全门。真实AGY 1.4.10原ID只读续聊已返回SUCCESS；正式只读发现源码实测14条主会话，8条有本地数据库、6条历史不在本地。旧正文为尚无已核查解码契约的二进制载荷，历史读取未通过。发现模块3/3测试通过，但Rust入口/侧栏/5194尚未接入，不能宣称产品闭环。详见[evidence](evidence/2026-11-02-agy-native-discovery.md)。先确认官方历史读取/导出能力，再接节点RPC和分页；保留DSH。未安装节点服务、未开启远控daemon；暂不上线、不打包、手机暂停。


2026-11-02施工增量：目标宿主探测已统一，认证不再由可执行文件存在推断；旧SSH路径缺目标端监管，暂拒绝新远端Claude派发并保留未知回执/占用，不能标tmux闭环完成。迁移0010和网页RPC/侧栏已接显式项目关联与版本CAS；只拥有本实例关联，不宣称跨实例共享同步完成。SQLite重开/冲突回归、前端89/89、节点8/8及Vue模板检查通过；真实X1T仅只读工具盘点，未做原会话派发。节点安装/私有部署授权待用户明确答复；手机暂停，本地页面未切换、未上线、未打包。详情见[evidence](evidence/2026-11-02-device-project-links.md)。


2026-11-02用户确认归并；设计依据见ARCHITECTURE“设备统一入口与跨设备项目组织”。代码审查基线d740979：已有Claude适配器、SSH路径、原生续接和隔离目录V1/V2证据；不是全部工具可自行添加的产品闭环。目录当前仍按连接+工作目录分组，跨设备Project关联未接通。此前证据的X1T执行为process模式，不能算远端tmux验收。

按以下顺序连续施工，沿现有编号，不另开计划：

1. **BP4-06 宿主正确性优先**：修复remote_ssh_target下Claude远端探测而tmux/cwd本地探测及启动的错位；认证未知保持unknown，不从命令存在推断已登录。明确目标端进程、回执、取消、超时和重启核对；无可靠远端监管不得宣称保活成功。
2. **BP4-06/07 设备能力入口**：稳定设备身份与地址分离；节点提供版本化工具清单和独立授权状态。设置中添加设备后选择可用Harness；保留DSH兼容适配和原有凭据/原session。工具端口为宿主内部配置。先探测后选择，不自动安装工具或新增公网端口。
3. **BP4-01/02 项目关联与双视图**：将Project身份与设备目录分开，持久保存会话/工作区关联和owner版本。实现推荐后确认、纠正及解除关联；按项目跨设备聚合，按设备展示同一记录。禁止按同名合并，禁止用关联操作隐式转移写权。
4. **BP4-06 原生接口组件化接线**：沿用统一目录与 Tauri 桥接，按 BOB-D-013 / wiki/harness-auth §6 接官方持久服务/程序接口；AGY ACP 或持续 JSON 流、Codex app-server、Claude 官方程序接口、Pi RPC 分别探测版本与认证。tmux 仅作兼容保活，禁止凭据提取或刮屏；原生历史读取与新建/恢复能力分别验收。
5. **BP4-04/06 自治护栏与接续验证**：在主路由与 `TaskCard` / 回执链路落实：① Observe/Ticket草案模式（非白名单任务先出草案卡确认）；② 回执同事务冻结 `ExecutionAssertion`（事前锦囊，置信度 < 0.90 展开，拦截无证据盲目质疑引发的错误反转）；③ 任务卡内联减法约束与必选检查全绿后收回写锁；④ 跨执行/跨工具换手注入 `WarmRestartState`（已定位文件+失败诊断+增量diff，替代冷重启）。以同项目讨论、DSH开发、Claude续改及另一节点研究为验收场景。

验收门：正常连接、认证未知/失效、工具缺失、节点离线、重复派发、取消/迟到回执、宿主重启和同名不同项目均有证据；项目关联跨刷新保留，原session与消息不复制，已有DSH连接无损。分开记录代码/本地/远端/浏览器/真机结果。手机验证待用户恢复要求；本次仅更新文档，不授权部署、打包、安装tmux或模型调用。

回退与停止：保留旧DSH入口及关联数据迁移前备份；身份冲突、授权不明或未知执行未核对时停止派发，允许查看缓存，不靠新建会话重试绕过。

### 3.7 第五项：BP4-07分段借用节点能力

2026-11-01实现增量：已在 `crates/bob-runtime/src/lib.rs` 完成 `FileVaultSecretStore` 随机熵源升级（`v2:` 种子+随机 nonce+MAC 防篡改校验）、旧版 v1 密钥 100% 兼容读回、损坏拒绝与 `rotate_master_seed` 原子轮换恢复；并在 `crates/bob-runtime/src/capability_borrowing.rs` 与 `crates/bob-node/src/lib.rs` 完成能力登记与协商（`supported/configured/authorized/reachable/busy/observed_at`）、`overseas_model_egress` 授权 Provider 白名单/调用者认证/大小与并发与预算上限/防重复计费/脱敏审计/禁止任意 URL 代理，以及 `gpu_display_render`（Godot/Blender/Browser 独立能力、绑定 `source_commit+dirty_manifest+input_hash`、快照重试不重复渲染、区分像素画面与几何功能验证）。`bob-runtime` 33/33、`bob-node` 7/7、前端 86/86 全部通过。

先做能力登记：支持/已配置/已授权/最近可达/忙碌/实测时间分别记录，来源是配置与探测，不从节点名称猜测。版本和能力协商失败不裸跑、不静默换供应商。

API出口子段：先审计FileVaultSecretStore随机性、权限设置失败、读回、损坏及轮换恢复；修复不得使旧密钥不可恢复。然后实现已授权Provider白名单、调用者认证、请求大小/并发/预算、取消、流式错误与脱敏记录；凭据不回传调用端，不做任意URL代理。用假Key验证拒绝/泄露/重复计费边界，再经批准用真实服务验证。不能据此批准凭据代理或新增公网暴露。

渲染子段：独立批准获准节点/目录/工具/时间后，再绑定源commit+dirty素材清单、输入hash、渲染配置和验证版本。Godot、Blender、浏览器分别报告能力。先一个有限任务，校验回传大小/hash、不可变快照、离线读取、部分传输重试不重新执行；像素图只证明画面，几何/功能另验。旧B4门槛保留，不以新编号绕过；不得打断节点已有工作。

### 3.8 第六项：BP4-05原生API子代理

2026-11-01实现增量：已在 `crates/bob-runtime/src/subagent_pool.rs` 与 `crates/bob-node/src/lib.rs` 完成有界原生 API 子代理（1 架构师 + 1 只读子代理）。实现严格最大深度限制（`depth == 1`，禁止嵌套派发子代理）、单架构师单任务并发上限 1（并发任务拒绝）、父授权交集只读沙箱（严禁工作区/状态写权，工作区路径越界与非授权子目录拒绝）、Token/预算费用/超时硬限制、API 提供方未知费用/身份标 `unknown`、级联取消证据记录（`cancellation_evidence`）、节点崩溃/重启防重放对账（`reconcile_on_restart`，`Interrupted` 终态保留账本，严禁自动重放派发模型调用防二次计费），以及主会话隔离防膨胀（原始步骤入隔离 spool，主会话只收敛包含磁盘核验文件哈希、分歧清单和模型推断对比的结构化结论 `SubagentSynthesizedConclusion`）。`bob-runtime` 38/38、`bob-node` 8/8、前端 87/87 全部通过，证据见 `docs/evidence/BP4_05_SUBAGENT_POOL_VERIFICATION.md`。

依赖：单执行者闭环与费用/凭据边界通过。第一版只做一个架构师+一个只读子代理，稳定后才增加有界并发；模型由能力、预算和用户配置决定，不固定“旗舰+Flash”。

实施与验收：复用Task/Execution/事件/结果引用；子任务权限取父授权交集，不能自授写权；最大深度、并发、总token/费用/时限明确。父取消会传播，子进程/请求结束有证据；重启不重复计费派发，重复结果不重复唤醒；失败和分歧可见，模型自报不是验证。API提供方不报告费用/身份则标unknown。主会话只有经核验的结构化结论和来源，不全量吞并子历史。

### 3.9 横向检查、发布与后续

- RV-11 BYOK真机、RV-13跨端附件、RV-19/21通知和检索保留原ID；按依赖推进，不作为所有DSH修复的前置。UI模块化随实际改动做，不进行无关重写。
- 每次发布候选附源码revision/dirty清单、制品hash、适用测试、数据兼容、备份恢复和遗留项；需具体授权后部署/发APK，独立公网或真机复验不能用构建通过替代。
- BP4最小交付门：手机原会话可持续使用+单执行者V1/V2治理闭环。跨节点能力和子代理为后续退出门，未完成时不宣称整个BP4完成，但不阻止合格最小增量交付。
- BP5逐项退役旧路径需数据读回、恢复证据及确认；“不复用code_runner”不等于获准删除旧工程或停服务。
- 完成以上前不加入全量向量记忆、多主数据库、自动全网克隆、无限多代理、自主改安全配置或自动模型选购。

## 原始反馈的当前处理

“贪食蛇”旧首屏、单圆点、文字归属的本地修正见上方当前增量；本次图片与 UI 续做见下方。旧“未实施”描述已被这些代码与证据取代，但不改写既有线上验收。

## BP3 / F1–F3 图片捕获续做（2026-10-31，已部署与构建，真机验收待补）

- 文字 + 系统拍照/选图入口 + PNG/JPEG/WebP（最大 8 MB）原件保存 + 视觉角色识别 + 编辑采纳 + 可选项目/新项目 + 知识页原图回看，已接入同一 Rust 内核和网页/Tauri 命令；不是只在 mock 里替换按钮。
- 原图按 SHA-256 独立保存；capture、项目笔记和原图关联在同一保存点提交，重试不重复。未关联草稿图片保留在当前节点，不静默删除。识别不创建决定或提醒，用户正文与建议不会互相覆盖。
- 移除悬浮窗内部 Logo、模型/扫码/日程快捷按钮；保留拍照与选图。输入无嵌套蓝圈，图片展开后长面板可滚动，触控按钮至少 44 px。统一规范仍为 DESIGN_PRINCIPLES。
- 修正网页节点的 modelEntryId/modelId 接收别名。视觉请求在释放节点状态锁后执行；请求绑定用户已看到的视觉角色，角色变更则拒绝，凭据不返回前端。请求有 60 秒时限；无自动重试。

验证：内核全套离线通过（1 个子进程夹具 ignored），新增原图/关联回归 2/2；bob-node 2/2、bob-runtime 3/3；原生命令 34/34、7 项专门测试 ignored；JS 18/18；实际 Vue + bob-node + SQLite 文字浏览器回归 8 项、图片流程 10 项通过。图片识别只使用本机假服务，验证真实 HTTP 图像载荷与编辑保存链路，不是外部模型质量验证。截图和结果见 [审计增量](evidence/2026-10-31-development-route-audit.md#图片捕获闭环实现增量)。

未运行：Android 相机/系统选图及权限拒绝真机操作、商业视觉服务识别、跨 Home 图片同步。当前 UI 明示原图仅存在当前节点；内核仅校验大小/格式签名，前端额外解码检查，不能宣称完整恶意图片扫描。新迁移 0008 为增量表，本次发布已备份数据库；回退保留图片表/原件，不删数据，旧客户端不会显示新图片。X1T/B4继续暂停。

## F2/F5 阅读体验与真实文件预览（2026-10-31，已上线）

首次打开远端会话在历史渲染后定位底部；后续刷新仅当读者原本靠近底部才跟随，阅读旧内容不抢滚动。定向组件6/6通过。

当前网页已按DSH权威游标分页：首次最近10条并定位底部，向上到顶读取更早10条，保持位置和草稿；无变化轮询只读轻量游标，不重取正文。输入框固定在历史滚动区外；三点菜单改为紧凑图标列表。真实Winter Shelter在统一本地页及实际生产服务390/1180px验证通过；前端20/20、Linux runtime18/18、node2/2及发布兼容检查通过，已备份上线，未用回退。首次10条仍因DSH工具记录传输约7.6MB，未宣称最小流量或完整历史同步。详见[本轮证据](evidence/2026-10-31-dsh-paging/REPORT.md)。未发送模型请求、未打包APK；生产验证经私有通道，不冒充手机真机或独立公网登录。

沉浸模式作为可持久化的客户端选项：隐藏常驻导航，保留右上角三点入口，收纳会话列表、设备连接、设置和退出沉浸；输入、停止、失败与必要决定保持可见。默认模式与沉浸模式共享同一会话和草稿，不另建Chat/Work入口。本轮已实现并发布，四项底栏按实际入口等分。390px实际浏览器验证模式持久化、真实图片/文本读取与下载；关闭预览恢复位置、草稿和焦点。文件限原会话引用及工作目录范围、2MiB，读取前后核对版本；当前文件不是历史ArtifactVersion。详见[evidence](evidence/2026-10-31-dsh-preview/REPORT.md)。APK/Tauri文件预览及分页、结构化成果清单、完整网页包仍待完成。

## 默认助理入口：BP3/F1、BP4-04/06 当前增量

用户批准完成实际接线、统一本地验证后上线，不打包APK。保留3.3原会话入口；将默认助理最小闭环插入复杂多代理前，不等待专职Agent池。步骤：持续助理Conversation与客户端入口恢复；当前模型返回结构化关联建议；以真实目录身份、明确执行意图和既有权限校验；通过现有持久派发发送；原会话回复带来源回到主对话；指定版本续改。讨论不派发，多候选先澄清，失联不改派，未知回执不重发。

验收覆盖无配置/离线、讨论、唯一关联、同名歧义、直达不改派、重复点击与重开、原会话结果回流。已接入网页实际发送路径：实例独立持久助理、配置模型的结构化关联、原生身份复核、先落盘再派发、request/turn绑定的结果回流。候选真实讨论与上下文保持、一次原会话只读询问、结果去重、刷新、390px及查看原会话定位底部均通过。发布与恢复最终结果见 evidence/2026-10-31-assistant-instance.md；手机原生派发/分页、远端取消、指定成果版本治理仍未完成；网页有界历史分页已由上方F2/F5增量完成。

## F2/F5 成果预取与手机接线（网页已上线，APK已构建）

用户授权更新网页并推送GitHub生成APK。可见图片/小文稿按最新优先、串行预取，每次打开会话最多6项；缓存最多12项/16MiB编码字符串预算，60秒失效，不写持久磁盘。点击复用同一请求/缓存；切会话清理缓存，关闭释放图片URL。顶栏仅图标和右上角状态点，保留可访问名称。Tauri新增DSH分页与预览命令，与网页共享组件。73项前端检查、Linux runtime18/18/node2/2及网页构建通过；真实图片预取与点击不重复请求、本地及生产390px验证通过；备份后已发布dsh-prefetch-20261031-r1，无回退。GitHub固定签名Run 41215873083成功，APK已下载并核对SHA256/16KB；成品见dist-release/prefetch-f6db9d9，未安装真机。详见[evidence](evidence/2026-10-31-dsh-prefetch/REPORT.md)。服务端缩略图、长期消息缓存、历史成果版本与真机验证仍未完成。


## 模型图标资源（2026-11-01，网页已发布）

model-assets-20261101-r1已备份后发布，仅补全public图标资源、移除Tavily/TinyFish图标并保留名称，保持菜单切换修复。实际生产服务经私有通道浏览器验证全部显示的模型图标解码成功，无工具破图。未改后端、数据库、凭据，未发APK；BP4-04仅保留本地候选。见evidence/2026-11-01-model-assets。外置前端构建必须同步public，不能只复制src。


## F2/F5 消息层级与处理提示（2026-11-01，本地候选）

用户随后暂停手机测试，当前未再次询问Winter进度。用户消息收窄右对齐并增加padding；节点真实running与已送达等待分开，状态读取10秒节流，失败清除旧观测，请求终态按request_id匹配。15项会话状态定向回归与前端构建通过；未真机复验、未上线、未打包。原会话历史/图片/固定输入框保留既有实现，后续按用户要求再验证手机。统一5194候选已更新。

#### BP4-06 研究补充的落地约束（2026-11-02）

首个验证节点沿用X1T，无需重新选节点。用户补充的主路由、ExecutionAssertion与WarmRestartState归入现有第5步，不能抢占前四步设备入口/原生会话的施工优先级。研究中的54.7%、226:1和20k尚未附可核查论文/实验条件，不纳入验收承诺；先补来源再判断适用性。界面不固定某个路由模型，模型由已有配置驱动。

先完成受限目录原生会话发现与原ID直达，再完成新建与持久监管；目录扫描只读取显式授权范围的元数据并有数量/字节上限，不读取全机任意用户历史。X1T环境安装/部署仍需明确答复；此前“请继续”不解释为对尚未解释清楚的安装请求的确认。所有后续实现遵循ARCHITECTURE“自治护栏的证据与反馈边界”。


### 3.10 第七项：F3 / BOB-D-014 单一收集箱与每日幕僚长分拣（One Inbox, Sorted Once a Day）推进路线（2026-11-08 立项，待施工）

依据 `BOB-D-014`、`DESIGN_PRINCIPLES` 与 `FEATURES §F3`，将原本分散的三处收件入口（Dashboard `#inbox`、闪念摘录、日程下待办收件箱）收敛为唯一的 **「收集箱（Inbox）」**，并复用已打通的自包含 `Bob Core` 幕僚长执行通道实现每日一次的批处理分拣。按以下四步递进施工，不抢占当前已排期的稳定性门禁：

1. **Step 1 — 内核数据契约与单一收集箱底座（`bob-kernel` + `bob-node`）**：
   - 演进现有 `captures` 表（增量迁移，不破坏历史记录），补齐 `triage_status`（`unsorted` | `sorted`）、`destination_kind`（`project_todo` | `calendar` | `note` | `merged`）、`destination_ref_id`、`destination_label` 与 `triage_batch_id` 字段。
   - 暴露统一 RPC / HTTP 端点（兼容外部 Share 与旧 Dashboard `#inbox` 转发写入），确保离线 Outbox 幂等提交与全端实时同流。
   - **进展（2026-11-08，源码+单测，未发布）**：migration 0011 新增独立 `capture_triage` 表（`captures` 原文与摘要不变，无行即 unsorted），并重建 `personal_ops_sync_outbox` 以接受 `capture_triage` 及既有 `delete` 操作（0007 约束遗漏，配对副本删除笔记/日程曾失败）；`LocalApp::list_inbox` / `set_capture_triage`（版本校验、相同请求幂等、状态与同步 outbox 同事务）、Replica→Home 接收与快照合并（无分拣时快照摘要与旧版一致）；bob-node RPC `app_inbox_capture` / `app_inbox_list` / `app_inbox_triage_set`（外部客户端经 owner 令牌 `/api/rpc/` 写入）。`cargo test --offline -p bob-kernel` 全绿（`tests/spark_inbox.rs` 新增 7 项），`cargo check --offline -p bob-node` 通过。未做：Tauri 端同名命令、外部采集专用窄权限令牌、真实旧库升级与公网验证。
2. **Step 2 — Bob Core 幕僚长每日分拣引擎（`bob-runtime` + `assistant.rs`）**：
   - 新增 `triage_sparks` 动作提案协议：读取全部 `unsorted` Sparks、当前项目清单（对齐 `project_registry.yaml` 与平台 `projects`）及近期日程摘要，调用当前选定的 `main` 模型产出结构化分拣草案 `DailyReviewBatch`。
   - 支持定时预处理（每日用户指定时刻后台生成草稿卡）与手动一键触发（「立即整理」），打开收集箱即可直接审阅，无需现场等待模型推理。
   - **进展（2026-11-08，源码+单测，未发布）**：`bob-runtime::inbox_triage` 提供分拣提示词与校验解析（编造/重复 Spark ID、未知项目、无时间日程、无标题、未知去向一律丢弃并计数；未覆盖 Spark 自动补 `keep`；多 ID 即合并，`merge` 视为笔记；`keep` 合并自动拆开；裸数组/围栏/夹杂说明文字均可解析），`planned_triage` 为 Step 4 生成主 Spark 去向与被合并 Spark 的 `merged` 链接。migration 0012 `triage_batches` 持久化草稿（新草稿取代旧草稿，`applied`/`dismissed` 关闭，同 ID 幂等）。bob-node `app_inbox_triage_draft`（有草稿直接返回不调用模型；`regenerate` 时锁外调用 main→flash→默认模型）与 `app_inbox_triage_close`。`cargo test -p bob-kernel` 109 项全绿、`bob-runtime --lib inbox_triage` 7 项与 `assistant` 4 项通过、`cargo check -p bob-node` 通过。未做：每日定时触发、真实模型实测、`project_registry.yaml` 路径接入（Step 4 写 todo.md 时需要）、Tauri 端命令。
3. **Step 3 — 五栏导航重划与「收集箱 / 每日分拣确认卡」UI（`bob-app`）**：
   - 主导航调整为 **`对话 | 今天 | 收集箱 | 知识 | 我的`**（将「日程」升级为聚焦今日行动的「今天」，将「票夹」收拢至「我的」单点入口）。
   - 在「收集箱」顶部实现卡片堆叠式 **Daily Review Card**（展示去向彩色胶囊与 `确认 / 改一下 / 全部确认` 按钮），全部视觉样式通过明暗双模 CSS 变量注入（遵循 `AGENTS.md` 严禁硬编码色彩与 i18n 双语同步铁律）。
   - **进展（2026-11-08，前端源码+单测，未发布）**：
     - **UI令牌与圆角收敛**：全局倒角半径减半（卡片/外框降至7-8px，按钮/输入框6px，微标签4px，模态弹窗12px），消除卡片和按键内的过度药丸圆角；
     - **输入框与下拉菜单令牌统一**：修复 `DshSessions.vue` 底部模式切换按钮（`Ask to approve ∨`）使用原生 `<select>` 导致的外框过度胶囊（999px）与展开弹窗纯方直角的割裂问题，替换为 `CustomSelect.vue`，触发器与相邻工具按键统一度量为 `--radius-btn: 6px; height: 1.8rem;`，弹出浮层统一为 `--radius-card: 8px` 并注入系统明暗变量；
     - **远程会话阅读宽度收束**：修复 `PrimaryChatPane.vue` 中远端节点会话宿主容器 `.remote-session-host` 撑满整个屏幕宽度（1920px）难于阅读的问题，对齐主 Agent 会话几何结构，统一定义为 `width: min(100%, 62rem); margin: 0 auto;` 舒适居中；
     - **宽屏响应式桌面布局（Screen Desktop A/B）**：依据 `bob-ui-concept.html` 宽屏规格，在 `layout-desktop-wide`（!isCompact）模式下自动隐藏底部导航栏，左侧激活 72px 宽度五栏垂直轨条（`.desktop-rail`），当处于“对话”标签时会话目录自动常驻泊靠于轨条右侧（`.desktop-docked-sidebar`，280px），中间区域展示舒适宽度对话；而在移动窄屏（isCompact）模式下无缝回退至底栏加抽屉模态浮层；
     - **单一收集箱与分拣卡落地**：新建 `InboxView.vue` 与 `useInbox.mjs`，顶部集成零决策速记输入框与每日分拣确认卡片堆叠（彩色去向胶囊、理由、快速换去向、改一下与确认/全部确认）；票夹单点收拢至“我的”（SettingsView.vue），清理日程与知识页冗余入口；QuickNoteOverlay 升级为零决策直写 `inboxCapture`；i18n 中英双语对齐；
     - **自动化验证**：164项前端单测全绿（含 `phase-a-ux.test.mjs` 模板与变量静态编译检查），`bob-kernel` 全部 Rust 单测通过。未做：生产环境打包发布（遵循重型操作防火墙，等待用户明确授权）。
4. **Step 4 — 确认落盘、PC 节点 `todo.md` 同步与「展开想法」闭环**：
   - 用户点击确认后，在同一事务内将 Spark 标为 `sorted` 并写入目标模块（日程 `ScheduleItem`、知识 `NoteItem` 或项目 `todo.md`）。
   - 项目 `todo.md` 写入：常驻节点已同步目录直接落盘；仅驻留于离线 PC 的仓库写入 `pending_todo_writes` 队列并在卡片上诚实展示 `Waiting for PC`，待 PC 节点上线后自动执行字段级追加并回填完成状态。
   - 在「知识」笔记详情页提供「展开这个想法（Develop this idea）」按钮，点击即携带原 Spark 与笔记上下文开启 Bob 对话，并可选支持每周回顾摘要（Weekly Look-back）。

## BP4 外部 Bot 只读/项目整理（2026-11-08，源码候选）

用户认可 `grok_bot` 专用账号，不开放节点执行；后续明确“暂不编译，先完成源码与轻量验证”，故本轮不编译、不启动新后端、不上线、不打包。

本轮边界复核（2026-11-08）：原实现已复用内核保存，但范围检查和业务操作混在MCP中；已提取project_service受限应用层，关联权限下沉为内核ProjectLinkAuthority，机主旧入口委托同一保存事务。原项目范围与版本同事务核对，外部关联字段显式投影，拒绝最大整数版本溢出。补2项内核回归（未运行）；原9项Bot用例保留。未宣称所有现存RPC已改造为统一身份系统。

已写入 bob-node 的受限 MCP 路由、逐请求授权、项目字段投影、授权会话关联 CAS 和脱敏审计；提供本机机主配置脚本，默认只读、24小时有效，无 HTTP 管理或权限提升接口。九项 Rust 权限/持久化/HTTP绕过用例已编写，尚未运行。未创建或交付真实 grok_bot 凭证，避免把源码准备当已可登录账号。

下一步获准编译后：定向测试 → 隔离本地 SQLite 候选 → 创建仅测试项目范围的 grok_bot → HTTP initialize/list/read → 整理保存/重启读回 → 只读拒写、越权/执行拒绝、版本冲突、撤销/过期 → 再开放用户选择的真实项目。暂不连接外部服务或发送私人项目内容。本轮不改现有收件箱/UI其他进行中修改。

Grok Bot 官方文档支持自定义 MCP；Grok 消费端连接器需要服务可达，localhost 无法直接供云端访问。Muse Model API 支持工具调用，尚未确认 Muse 消费端自定义 Bob 连接器及本账号可用性。来源及验证记录见 `evidence/2026-11-08-external-bot-access.md`。完整 MCP 兼容、管理UI、OAuth/远程TLS入口、分页正文和附件权限仍待后续，不标记已接通。

## F5 文件预览执行目录修正（2026-11-10，网页与节点已发布）

用户报告 BobBot 五个 Markdown/YAML 文件均无法预览。真实线上 RPC 约 0.7 秒返回 DSH_UNAVAILABLE；X1T 会话目录/正文可读，原生文件接口确认工作区根路径 not-found，而子项目路径可读。原生工具记录在回复前明确 `cd` 至子项目；根因为会话启动目录被当作链接项目目录，以及错误转发吞掉 not-found。

源码修正按轮次装配执行目录并保持原生文件授权、返回实际解析路径、缓存核验包含路径、错误提示中英区分。禁止全盘猜文件或修改 BobBot 工程。已获本轮定向编译、真实五文件候选核对和相关服务上线授权；不打包APK。171项前端、5项上下文和22项传输测试通过；X1T临时候选五个真实文件逐字节对比及not_modified缓存核验通过。X1T与Hostinger网页/后端已备份发布，线上五份文件均成功并返回正确resolved_path，约0.7–0.9秒；网页HTTP哈希通过。Edge经私有通道访问实际生产服务，侧栏打开原会话、点击UPDATES文稿、实际路径标题与Markdown正文渲染均通过，无错误提示；未独立验公网登录，未打包APK。证据见 [预览回执](evidence/2026-11-10-preview-context.json)。


### 预览歧义补修与收件箱通道核对（2026-11-10，已发布）

用户再次复现 `DSH_FILE_CONTEXT_AMBIGUOUS`，推翻此前“此次预览问题已整体解决”的判断。原因包括跨消息同名路径混用，以及同轮根目录/子目录被直接视为文件歧义。现补消息归属全链路、缓存隔离、冷缓存目标消息恢复和最多八个已观测目录的精确文件核对。候选编译通过；原序号4811五份真实文件冷缓存/逐字节/缓存重验通过；最新序号5045四个真实链接核验通过。45项相关前端、5项上下文与23项传输测试通过，前端构建通过。X1T与Hostinger节点/网页已备份发布，生产五文件RPC复验通过。Windows私有隧道浏览器因连接重置未完成；改用生产宿主已有Chromium，真实打开会话并点击最新5045文稿通过，消息序号请求正确、实际路径标题与6573字符正文渲染正常、错误0。未独立验公网登录，手机未测试、不打包。见现有preview-context证据及message-preview-browser截图。

Dashboard 生产本机 `/api/inbox` 实测200，19条，字段 content/created_at/filename/project/source/title；仅核对元数据，未给主助理或其他 Agent 开放私人正文。主助理当前发送路径仅提供目录与日程上下文，既有 `app_inbox_list` 是 Bob 独立数据源。读取能力及权限方案已归并到 ARCHITECTURE、DEVELOPMENT_ROUTE 和既有 Dashboard 待办，未新增平行计划。


### 侧栏关联与历史项目污染核查（2026-11-10，只读）

生产目录实读：81个平台项目，其中69个legacy_import；83条本地会话中80条属于历史导入项目。关联选择器直接枚举catalog.projects，未复用主列表的历史过滤，因此历史项目进入每条会话的下拉框；删除节点原会话不能清除此平台导入副本。

节点目录本次VPS2 47、VPS3 80、X1T 55条；按连接/会话匹配保存关联，VPS2有5条新Quant Lab自动交易会话未关联，X1T有3条（平面模型、Blender建筑建模、tmp目录的Bob Connection Verification），VPS3本次目录均有保存关联。X1T首次并发读失败，单独重读成功，不将一次失败视为无会话。建模归属仍需结合正文确认，不能仅凭模型/工具名决定项目。

代码核查：NavigationDrawer仅注入organizeSessionsViaAgent，模板未展示调用按钮；“智能”只切换分组。QuickNoteOverlay仍有“整理当下信息”入口，但执行的是主助理通用提示，不能宣称已经定向分析所有未关联会话或读取Dashboard私有收件箱。resolveCanonicalProject仅产生展示分组，不持久化project_session_links，因此可与“未关联项目”同时出现。

建议沿现有F2/F5收纳与项目关联任务处理：关联选择器默认只列活跃非导入项目，保留当前已选历史归属与“查看历史/归档”；恢复侧栏显式“整理”动作，限定未关联/新变化数据，给出项目建议和理由，尊重人工关联，待办提取另行审阅；会话行使用项目标签入口，支持搜索、选择已有项目和新建后关联。测试归档与永久删除分离，年龄与test关键词不作删除依据。当前未修改关联、删除会话或发布UI；节点删除须先形成精确ID/工作区/活动状态/备份清单，走工具原生删除能力。

## 连接卡片对齐与文件预览发布（2026-11-10）

连接页扫码区域不再复用带左右 padding 的诊断容器；列表与配对卡片使用相同边界，展开间距仅属于展开内容。候选真实后端浏览器验证：1458px 两卡 x=117.390625、宽1283.21875；390px 两卡 x=0、宽378，无横向溢出。172项前端检查和Vite构建通过。此次发布同时收录此前已验证的按消息序号定位真实工作目录的文件预览修复；APK使用既有固定签名流水线，手机安装与实机验收未执行。部署与构建回执见本节后续结果。

发布回执：源码 `07a7ce9` 已推送 `main` / `1.0.1`；网页生产服务发布成功，保留一致性备份 `quota-device-20261110-120524-88a16ff3`，发布后直接读取生产资源的 Chromium 1458/390px 两组卡片对齐通过。复用已验证的节点二进制，未重复重启 X1T。Android Actions [Run 42812623855](https://github.com/bobbik1984/bob-agent/actions/runs/42812623855) 成功，Bob品牌、固定证书签名及16KB对齐检查通过；成品 `dist-release/Bob-1.0.1-07a7ce9.apk`，下载与CI SHA256一致：`de6f34946b89ab43ba860dbf5c407943b693a3f0be0b010c0783fa2a8e5f4f50`。未安装手机，未独立进行公网登录浏览器验收。


## Jarvis 统一主助理与桌面小组件收敛施工合同（BOB-D-015，2026-11-11 源码与本地四重门禁验证完成，待授权上线与真机终验）

> **战略与范围**：依据 `BOB-D-015`，`bob-bot`（线上研究原型）长期作为轻量交互与新能力实验场，不再另起独立 Rust 服务端；全部经验证优势统一迁入本仓库主干（纯 Rust 内核 + Vue 3 前端，**零 Node.js 生产依赖**）。
> **绞杀者渐进原则（严禁大爆炸重写）**：以下 `WP-J1`～`WP-J6` 每一个工作包都必须**独立可编译、独立通过自动化回归、独立可上线**，且严禁破坏现有线上已运行的 L3 原生会话与设备连接链路。任何引入新依赖、编译构建或生产部署动作，仍须遵守 `AGENTS.md §3` 重型操作防火墙逐包获取执行授权。
> **最新源码与本地验证进展（2026-11-11）**：`WP-J1`～`WP-J6`（`T01`～`T12` 共 12 步）已完成全栈源码落地并通过本地离线四重门禁：**Cargo 全工作区 32 个套件共 359 项测试全绿（0 失败）**、`apps/bob-app/src-tauri` 离线 `cargo check` 0 错误、**前端 203/203 项单测与规范扫描全绿**（`zh-CN.json`/`en-US.json` 1,060 键对称、0 硬编码颜色、0 装饰 emoji）、`vite build` 生产构建通过；详见 [WP-J1～WP-J6 本地验证回执](evidence/2026-11-11-wp-j1-j6-jarvis-convergence-verification.md)。**诚实边界**：尚未向线上生产节点推送或重启服务，真实宿主 Chromium 公网办公站点接管、iOS Safari 真机 APNs 锁屏推送及 Android 真机安装验收待用户授权后执行。

### WP-J1（当前优先可派发包）：`bob-node` 异步双轨网关 + 纯 Rust `BobNativeHarness` 流式主脑

- **解决的核心卡点**：打通 `bob-node` 缺失的持续 SSE / WebSocket 异步通道，并将置顶主助理从单轮 JSON 意图分类器升级为真正自主的多步流式智能体。
- **涉及模块与文件**：
  - `crates/bob-node/`：入口由手写同步 `std::net` 升级为 `tokio + axum`；新增 `/api/jarvis/v1/stream`（逐字 SSE 事件流：`token`、`thinking`、`tool_start`、`tool_end`、`card`、`approval_required`、`done`），既有全部同步 `/api/*` 路由通过 `spawn_blocking` 原样桥接，确保 100% 向后兼容。
  - `crates/bob-runtime/`：新增 `native_harness.rs`、`tool_bus.rs`、`budget.rs`（参考旧版 `llm.rs` 多服务商流式解析、`tools.rs` 的 `ToolCallTracker` 重复调用熔断器与 `bob-bot` 月度花费上限账本，对接现有 `ModelHubRuntime` 与 `SecretStore`），替换 `assistant.rs` 单轮分类主路径。
  - 首批接入 `Bob Tool Bus` 的内生工具：既有 `personal_ops`（待办/日程/笔记/记忆/票夹）、`spark_inbox`（收集箱读写）、受公网地址守卫保护的只读 `web_fetch`、`show_card` 结构化卡片，以及带审批门禁的 `dispatch_to_session` / `organize_projects`。
- **退出门（Exit Gate）**：
  1. 既有 `bob-kernel`、`bob-runtime`、`bob-node` 测试全绿，现有 L3 远端会话读写零退化；
  2. 主助理跑通逐字流式回复与 ≥3 步连续工具调用闭环；故意构造死循环工具调用可被 `ToolCallTracker` 熔断；花费达 80% 告警、100% 硬拦截新模型调用。

### WP-J2：Jarvis 单一交流主界面 — Siri 流体彩色球 + 手机桌面式小组件网格

- **涉及模块与文件**：
  - `apps/bob-app/src/components/`：新建 `<JarvisOrb />`（纯 CSS 变量/Canvas 流体彩色球：空闲呼吸、思考涟漪、回复律动、待确认暖琥珀四态；对话展开后平滑收拢至左上角；支持 `prefers-reduced-motion` 与完整 `aria-label`）与 `<JarvisWidgetGrid />`（手机桌面式可增删、拖拽/排序小组件网格）、`<ActionCard />`（对应 `show_card` 与审批卡）。
  - `apps/bob-app/src/components/PrimaryChatPane.vue`、`App.vue`：移除主助理对话中的终端字符旋转棒（`TERMINAL_ROD_FRAMES`）；提供首批内建系统小组件（待确认与接管、今日日程待办、收集箱速记、日常任务；将「订阅额度双线」与「节点/活跃工程会话」收束为 L3 可选小组件）；未连接外部工作电脑与外部 CLI 时自动处于零噪音 L1 视图，次级抽屉（`今天 / 收集箱 / 知识 / 我的`）可从小组件或导航一键直达。
- **退出门（Exit Gate）**：390px 手机视口与 1440px 宽屏下明暗双模、中英双语、键盘/屏幕阅读器无障碍检查全绿；L1 空库状态下首屏无任何节点、端口、Harness 或额度术语。

### WP-J3：无 Node 依赖的小组件与工具自我改造（`QuickJS` 沙箱 + 3 分钟试用回退）

- **涉及模块与文件**：
  - `crates/bob-runtime/src/features.rs`：内嵌轻量 `rquickjs`（配置内存上限与指令中断计数，无子进程与原始 Socket 权限），仅注入受公网守卫保护的 `ctx.fetch`、限额键值存储 `ctx.storage`（≤8KB）、`ctx.registerTool`（`f_<pkg>_*`）与 `ctx.registerRoute`（`/api/f/<pkg>/*`）；对话新增 `feature_create`、`skill_create` 与远程 HTTP `mcp_add`（写入类工具默认需审批）。
  - `apps/bob-app/src/components/`：新建 `<FeatureWidgetFrame />`（无凭据独立源 `<iframe sandbox>` + `@scope` CSS + `postMessage` 桥接 `bob.call` / `bob.state` / `bob.save` / 主题变量，首帧缓存零闪烁绘制）与页头 `<FeatureTrialBanner />`（持久化 **3 分钟倒计时：保留 / 撤销**，超时、崩溃或服务重启自动回滚，保留最近 5 个历史版本）。
- **退出门（Exit Gate）**：在无 Node.js 的干净环境下，通过对话让 Jarvis 生成一个带后台拉取与前端视图的桌面小组件并挂载至 `<JarvisWidgetGrid />`；验证访问 `127.0.0.1` / 内网地址被硬拦截，且撤销、报错与试用期重启均 100% 回滚。

### WP-J4：常驻生活节律 — 时间对象合一、Routine、做梦晨报与多端推送（完成 L1 闭环）

- **涉及模块与文件**：
  - `crates/bob-kernel/` & `crates/bob-runtime/`：将 `ScheduleItem` 扩展为统一时间对象（日程/待办/提醒/定时任务/`RoutinePlaybook`），新增 `tokio` 定时调度器、对话轨迹提炼 `routine_save`（启用前强制 1 次应用内试运行）、无人值守高危动作转存待批草稿（`Draft`，7 天过期，批准后不经模型精确重放）。
  - 成长回路模块（参考归档旧版 `evolution.rs`、`dream.rs`、`daily_brief/`、`kb_indexer.rs` 与 `bob-bot` 护栏）：零成本遥测 → 夜间五阶段做梦（单次自动删减 ≤3 条并记可恢复日志）→ 每日结构化晨报小组件与推送 → 0～1 四档主动性滑块（`22:00–08:00` 安静时段硬规则）→ SQLite FTS5 与实体关系带来源检索。
  - `crates/bob-node/` 推送与渠道：接入 Web Push（`manifest.webmanifest` + `sw.js` + VAPID，直通 iOS 16.4+ Safari 主屏幕 APNs）、Android 推送接收、以及 `/v1/chat` 官方微信 iLink 桥兼容接口与企微/飞书 Webhook。
- **退出门（Exit Gate）**：完成 `FEATURES.md` 定义的 **L1 完整闭环验收**（含 iOS Safari 主屏幕 PWA 锁屏接收晨报与提醒推送）。

### WP-J5：`bob-browser` CDP 持久化浏览器与手机虚拟窗口人机接力

- **涉及模块与文件**：
  - 新建 `crates/bob-browser/`：纯 Rust 实现 Chromium/Chrome CDP 守护（持久化 `--user-data-dir`、空闲休眠回收、崩溃自愈）、带编号无障碍树快照提取（穿透跨站 iframe、开放 Shadow DOM 与自定义可点击元素、遮挡检测）、公网地址重定向守卫、密码/验证码框零入模硬拦截，以及按需 `Page.startScreencast` 帧推流与触控/输入映射（退出窗口立即 `stopScreencast` 归零 CPU）。
  - `crates/bob-node/` & `apps/bob-app/`：挂载 `/api/ws/browser-takeover` WebSocket 路由，实现 `<VirtualBrowserModal />`（手机/桌面视口与 UA 自适应、双指缩放、滑块拖动、底部原生中文输入法/短信验证码穿透输入栏、二维码存相册）与「网站授权中心」面板。
- **退出门（Exit Gate）**：通过 `bob-bot` 18 项本地浏览器测试集的全部等价用例，并在所在地区 VPS 上用 390px 手机浏览器完成 3～5 个真实办公站点（至少 1 个含验证码或 2FA）的接管登录与重启保活验收。

### WP-J6：我的电脑工作台（L2）、V1→V2 成果批注对照修改与一键部署收口

- **涉及模块与文件**：
  - `apps/bob-app/src-tauri/` & `crates/bob-runtime/`：桌面端启用同一 `BobNativeHarness`、获准目录文件/命令分级审批工具与本机 `bob-browser`（复用个人电脑浏览器环境规避机房 IP 风控）；完善离线 `Waiting for PC` 与恢复对账不重发。
  - 成果审阅组件：在文稿、网页、图片与报告预览上实现**局部批注/修改指派 → 自动携带父版本（`ArtifactVersion` V1）回传原执行者（主助理、PC 节点或 L3 外部 Harness）→ V1/V2 并排或切换对照审阅**。
  - 部署交付：提供集成 `bob-node` + Headless Chromium + 中文字体 + Caddy 自动 HTTPS 的单 VPS 一键部署方案（目标 2C/4G 24 小时连续稳定运行）。
- **退出门（Exit Gate）**：完成 `FEATURES.md` 定义的 **L2 与 L3 端到端闭环验收**。

---

## 4. 2026-11-12 生产上线与全量验证记录（Live Deployment Evidence）

### 4.1 生产上线环境（Production Target）
- **目标服务器**：Hostinger VPS2 (`srv1471410`, `187.124.144.87`)
- **服务单元**：`systemd` `bob-online.service`（绑定 `127.0.0.1:53020`，反向代理网关 `https://bob-online.bobbik.org`）
- **发布 Commit**：`c18a4287dfa51d866d7434fa6fefc768c1975e5b`（分支 `1.0.1`）
- **上线时间**：2026-11-11 17:20:15 UTC (北京时间 2026-11-12 01:20:15)

### 4.2 部署安全收口与一致性回执（Deployment Receipt）
```json
{
  "deployed": true,
  "codeCommit": "c18a4287dfa51d866d7434fa6fefc768c1975e5b",
  "pid": "2112030",
  "binarySha256": "58e26d7cc1932e0168ed5adfef350d0eed933f9caf32e233f9aadb4ba7034f56",
  "webSha256": "4796309c0aba39c36ec01806b136f09e37d0ba13b272da06a3ef4a8c432b24ee",
  "httpSha256": "250ef35cca0321d1d547c0ebeb5fb6723eb6ea1884f2f4cf2fe27d35eb14eeb0",
  "backup": "/opt/bob-online/operator-backups/quota-device-20261111-172011-6296d96b",
  "localHttpReadback": "passed",
  "connectionsMigrated": false,
  "publicBrowserVerified": false
}
```

### 4.3 现役服务运行与健康校验（Live Health Check）
- **数据库无损与完整性**：`PRAGMA quick_check` 返回 `ok`，现役 executions 全部归档状态无悬挂，现役 leases 释放状态。
- **服务状态**：`systemctl status bob-online.service` 状态为 `active (running)`，Main PID `2112030`，内存开销轻量（~3.1MB）。
- **RPC 就绪度**：
  - `app_bootstrap`：`ok: true`，会话初始化正常。
  - `app_personal_snapshot`：`ok: true`，现存捕获、记忆与个人数据 100% 完整保留。
  - `app_jarvis_approvals_list`：`ok: true, result: []`，WP-J1/WP-J2 审批流正常就绪。
  - `app_feature_sandbox_list`：`ok: true, status: ok`，WP-J3 QuickJS 沙箱插件引擎运行正常。
  - `app_budget_status`：`ok: true, status: ok`，WP-J1 预算控制与安全网关正常。
  - `app_growth_stats`：`ok: true, status: ok`，WP-J4 成长闭环与做梦系统正常。
- **Web 前端与 PWA 资产**：
  - `/manifest.webmanifest`：返回完整 Web App Manifest（直通 iOS Safari APNs 与桌面 PWA 安装）。
  - `/sw.js`：正常注册并接管 Service Worker 生命周期。
  - 核心单交流窗口、Jarvis 彩色球、组件网格与虚拟浏览器前端打包产物（`index-CGB6kmuM.js`, `index-CewEGIV9.css`）热更新完成，HTTP 200 正常响应。
- **公网网关**：`https://bob-online.bobbik.org/api/capabilities` 经 Cloudflare + Caddy 代理返回 HTTP 200，能力探针 `browser_mode: true` 正常点亮。


## 零对话项目侧栏收敛（2026-11-12）

- 当前网页已发布：默认隐藏零对话项目；当前客户端新建成功的空项目临时保留 30 分钟，抽屉重开保留、整页重载结束；有对话后正常显示。实现为 recent-projects 与创建成功路径接线，目录刷新不会复活旧空项目。
- 生产只读审计：`proj-cecd45e758`（VPS3 测试目录）仍被 68 条 project_session_links 引用；`proj-default`（Default Workspace）仍有 1 条 decisions 引用，另有同 ID 的 conversation/message 记录。它们不满足“空测试或占位记录”删除条件，未删除或归档任何业务数据；0 是侧栏可显示对话数，不是所有资料数。
- 验证：206 项前端测试通过；Vite 180 模块构建通过；候选浏览器使用真实生产目录确认旧空项目隐藏，拦截创建响应验证新建临时显示（未向生产添加测试记录）。生产已部署资源 `index-DoeVtgz2.js`，真实目录 15 个分组下两项均不显示。公网匿名入口 401，未声称匿名浏览器完成登录后验收；无手机安装/测试。
- 仅更新网页静态资源，后端和数据库未变。回滚备份为生产根目录 `web-before-empty-projects-20261112-085002`；恢复其 index.html 即可回退，旧哈希资源保留。部署 index SHA256：`8be54ec62770ba0893a031179998d4996c8a26cb3b11afce44c3587a1e762321`。

### 过期项目关联实核与清理（2026-11-12，补充并取代上轮仅保留结论）

零计数原因：侧栏用当前连接 ID 与 Harness/session ID 匹配；68 条旧关系都属于已移除的 `node-vps3`，所以不能匹配现役会话。不能将关联表行数称为当前项目会话数。

- 逐条核对现役 VPS3 80 条目录：27 条（Pi 7、Codex 20）确实仍存在；它们在新连接下已归属「实验与临时测试」，当前关系逐字段保持不变。
- 另外 41 条均是 `cp:*` 旧平台任务 ID（Codex 9、DSH 12、Pi 1、AGY 19），无现役原生会话对应、无其他连接的同 ID 关联。清理的是已移除连接的失效项目关系，不据此断言历史任务正文已消失，不删除历史会话或文件。
- 通过 `app_project_link_set` 和原版本 CAS 解除 68 条旧关联，保留版本化解除记录供跨端同步，避免旧数据复活。确认项目再无任何数据库引用后，归档「VPS3 测试目录」并递增版本；目录 RPC 已不再返回该项目。`Default Workspace` 的决定记录未改动。
- 写入前完整 SQLite 备份及逐条分类清单保存于生产 `operator-backups/stale-project-links-20261112-093345/`；quick_check=ok，现役 VPS3 关联全部不变。回退只按 review.json 恢复指定关系（使用当前版本 CAS），必要时恢复项目状态；禁止覆盖整库丢弃后续消息。
- 日历小时标记改为 0–23；最后时段的无障碍结束时间仍为 24:00。207 项前端检查、178 模块构建通过；候选及生产浏览器均确认首格文本为 0。仅更新网页静态资源 `index-CQlobP8V.js`，回滚备份 `web-before-midnight-20261112-093447`。未打包 APK、未测手机。

### 三家周额度恢复（2026-11-12）

纠正 f0e5e96 同时移除月预算与订阅额度的范围错误：App.vue 恢复三家 Logo/双线/百分比/剩余时间，月预算不恢复，周窗口选择机制不变。207 项测试及前端构建通过；390/1280px 候选浏览器、生产页面均验证3家6条线及本地图标、无横向越界。当次真实数据 Codex weekly 57%、Antigravity gemini-weekly 37%，Claude 未获官方数据显示短横线。网页 bundle index-FvUSozrY.js；回滚备份 web-before-weekly-restore-20261112-094956。未打包 APK。

### 外部 Bot 待办最小闭环（2026-11-12 源码阶段）

用户要求实现共享说明书+授权 API 的待办通道。已新增独立 todos_read/todos_create 授权、MCP bob_todos/bob_todo_create、共用 todo_service 与 personal_ops 写入、0013 持久请求回执及原子保存；旧账号默认无待办权限。11项Python配置检查通过，Rust持久化/重启/范围过滤/撤销用例已补，尚未编译执行。接入例子归并 connectivity。未创建生产Token、未发布或向外部Agent传送私人数据。

下一步需本轮定向编译授权（已提出）：运行 bob-node Bot 专项测试，启动隔离候选进行真实 HTTP 创建、读取、幂等、重启读回与撤销验证，再决定正式项目授权和上线。不得将源码完成标记为 Grok/Muse 已能实连。

### 外部 Bot 待办本地验收通过（2026-11-12，取代上一节待编译状态）

用户明确批准定向编译与隔离本地启动。12项bob-node Bot测试、5项内核个人事务/配对同步测试通过，候选二进制构建成功；独立进程真实HTTP完成创建/同账本读取/幂等/冲突/重启/删除后重试/权限/撤销/过期与审计检查。候选已停止，测试授权已撤销；无生产修改或正式Token。可复跑脚本 scripts/verify-bot-todos-http.py，详细证据见 [Bot待办验收](evidence/2026-11-12-bot-todos.md)。下一步是用户明确正式项目授权范围、批准发布并配置消费端，Grok/Muse实连与真机读回仍未完成。

## 独立待办修复、验证与发布（2026-11-12）

- 发布前及发布后只读核对：Chase、Amex、MiniPC、二手、业绩、分部调研、天窗、灯盒关键词未匹配到所述8条待办；本轮未代为新增，避免与Grok后续写入重复。
- 15项Bot后端测试、5项内核个人资料与同步测试、13项隔离真实HTTP验证、19项前端桥接测试、12项授权配置检查通过；git diff --check通过。
- 统一 localhost:5194 使用隔离真实后端：创建无项目/无日期待办，单独设置截止，清空截止，刷新重新打开均正确；非模拟数据。测试待办仅在隔离候选。
- 生产后端 SHA256 `52170ab841619b89d8605a6c71e437f62586396026cffcdd0b26b736620e9719`，网页 index-C9c8dp-i.js / index-C5O6uROz.css。一致性备份 `quota-device-20261112-135321-ec29d216`，新旧读者兼容检查及服务读回通过。
- 公网 tools/list确认仅request_id/title必填；全部获准待办和独立待办查询均成功；执行工具拒绝。grok_bot原Token不变，新增独立待办范围，权限可撤销。生产未写测试待办。
- Google Drive既有接入说明更新为2026.11.12.2。只确认同步目录文件写入，不冒称消费Agent已重新读取。未打包或验证Android；真实Grok新增和配对手机验收待用户执行。

## 语义日期与原待办修改：当前验收（2026-11-12）

- 取代此前“只接受明确时刻”限制，日期-only全天栏、明确时刻小时格、开始/截止独立可选，网页与Bot共用内核；grok_bot新增todos_update权限，节点执行仍拒绝，Token不变。
- 16项Bot定向、6项个人事务/配对测试、全套bob-kernel、14项独立HTTP、23项前端桥接/日历、12项授权配置检查通过；正式后端与网页构建通过。原生调用测试参数已同步，未运行Android构建或手机验收。
- 统一localhost:5194隔离真实后端使用4条合成记录：日期进入全天栏、点击编辑正确读出日期、保存成功，下一周明确12:00截止正确显示。未将私人8条复制到候选。
- 正式接口核对8条原始备注、ID与版本后逐条CAS修改，读回仍为8条同ID、版本+1、标题/备注/归属/状态不变；未新增日程副本。两条“下班前”使用此前对话的17:00推定并保留说明，全天截止不编造钟点。
- 已发布二进制SHA256 `11616ebf15b14325538547c1b4e2fdfcfd5973f4eae99104b1dfad91efc88f22`；网页 `index-CzkDBtRz.js` / `index-CkrP3qbR.css`；一致性回滚备份 `quota-device-20261112-142608-8246f609`，服务及公网MCP读回通过。
- Google Drive既有说明升级2026.11.12.3；只确认本地同步目录更新，不声称Grok已刷新说明。未打包APK。详细记录见既有Bot待办Evidence。


## 日历标题优先与拖动上线（2026-11-12）

已完成分钟定位、重叠分栏、拖动改期及上下边界调整；修复事件endAt被后端忽略问题。30项前端、7项内核个人事务、16项Bot检查及构建通过；统一候选浏览器真实拖动和RPC读回成功。生产网页/后端已更新并留回滚备份；无生产待办数据修改，无APK/手机验收。细节与已知边界见 [日历拖动Evidence](evidence/2026-11-12-calendar-drag.md)。


### 手机版日历同步发布核验（2026-11-12）
- 用户授权推送 GitHub 并生成新版 APK，沿用固定签名与应用身份。
- 新增真实 SQLite 配对往返检查：网页所有者日期 → 手机、手机日期/起止时间修改 → 所有者 → 手机，重复投递不新增，提醒时间一起同步；并发修改同一事件的时间必须拒绝旧版本，不能仅因标题/状态相同而当成已成功。
- 定向内核检查 8/8 通过；源码 `47141cf` 的 Android Run `43245325406` 成功，固定签名/品牌/16KB 对齐通过，成品存于 `dist-release/Bob-1.0.1-calendar-47141cf.apk`。网页同步冲突修复已上线并留回退；两台手机触控与最终同步显示待安装确认。详见现有日历拖动 Evidence。


### 手机三天日历与列对齐（网页已发布、APK 已生成）

日期栏、全天栏、小时网格统一时间轴宽度/日期列数/滚动条预留；640px及以下显示今天起三天，每次翻三天，桌面保持周一至周日七天。全天标题最多三行，区域限高且可滚动，完整内容可点开详情。30项定向检查及前端构建通过；本地浏览器390px、493px与1280px读取三段每列left/width完全一致，三天翻页11/12–14到11/15–17正确，合成全天待办拖动11/13→14保存成功并已恢复。源码 c7555da 已推送 1.0.1；网页资源 index-OkV4nQPN.js / index-Crg2R22N.css 已发布，服务器 HTTP 及资源字节核对通过。Android Run 43260240230 成功、Artifact 13963343395 已下载校验，固定签名/Bob 品牌/16KB 对齐通过。手机安装、触摸操作及最终同步由用户验收。


### 主助理首页收简（2026-11-13，本地候选）
移除整机 readiness 保存/待投递汇总条（此前与空会话内容不一致）；逐消息状态、失败提示和审批不删除。空会话小组件默认关闭，通过右上角菜单主动打开；悬浮灵感速记保留。引导改为简短通用文案，不常驻 Home 协议说明。今日卡片解包真实 personalOps ref、挂载时刷新，并复用日历日期投影筛选今日事项（日期/时刻/跨日事件、完成取消过滤、同待办去重）。这不是每日主动推送功能。
18项定向测试和182模块前端构建通过，统一本地5194浏览器390px：默认无统计条/小组件，菜单主动打开实见今日三项隔离后端测试事项，关闭恢复简洁。未改用户待办、未调用模型、未上线或打包；手机等待后续发布验证。

首页对称性复验：原根壳与 history 各占6px单侧滚动槽；现根壳与对话区使用无占宽原生滚动，空白态不画轨道。390px浏览器读回根宽390、history及首页中心均195px，左右边界10.3958/379.6042；前端182模块构建通过。仅本地候选，未上线或打包。

空对话球视觉调整：使用可用对话区弹性居中，球径随视口7–9rem，宽范围青蓝/紫粉渐变和低强度24秒旋转。13项检查及182模块构建通过；390×844球中心(195,321.78)，history中点375.33；1280×900球与history横向中心同660，球纵向372.33/history中点425.88。本地候选已更新，未上线/打包。

### 首页精简、对称布局与彩色球发布（2026-11-13）
- 源码 `e6da7e7f6af8a6f8dfffad7dabadaade089a2461` 已推送 1.0.1；网页资源 `index-DosaH2Q1.js` / `index-CzhzJdHT.css` 已发布，正式服务 HTTP 及资源字节读回一致。index SHA256 `6c68bc69b188e6f994a9e1fddd0335042114c9f61c626292c6ae1323857c6cbf`。
- 空对话不显示整机待投递统计及默认小组件；可从菜单主动打开，消息自身状态保留。今日事项正确读取响应式个人事务并按日期筛选。根壳与对话滚动槽不占宽，Agent 球在菜单与输入框之间略偏上居中、宽范围柔和渐变。
- Android Run `43271430985` / Artifact `13968512833` 成功；固定签名、Bob品牌和16KB对齐通过。成品 `dist-release/Bob-1.0.1-home-e6da7e7.apk`，272987508 字节，SHA256 `adc323bc8c890e8b73f7665e7182be7860eb349aea9601c1f6426e23ce808f3e`。仍为 ARM64 debug 安装包，沿用应用身份。
- 既有18项今日事项/主界面检查及后续13项球体/小组件回归、182模块构建通过；390px和1280px本地浏览器位置验证见上文。未连接手机，真机由用户安装验收。
- 网页回退：以生产根目录 `web-before-home-refine-20261112-173803/index.html` 恢复当前 index；旧哈希资源保留。未更改后端、数据库与用户待办。此前本地候选“未上线/打包”记录已由本发布回执更新。

### 待办完成项按年折叠（2026-11-13，本地源码）
待办主列表排除 completed，底部“已完成”默认折叠，内部年份也默认折叠且新年优先；项目过滤对两区一致，保留详情、重开、删除等原操作。重开随状态读回返回主列表。不删除或迁移记录。现有记录无 completed_at，以 updated_at 归年；无有效日期归“年份未知”，不声称精确完成年份。3项 Node 检查及 Vue 模板校验通过；未构建、浏览器实测、上线或打包。回退仅需恢复本轮前端改动，无数据迁移。

### 水滴形象与空对话键盘适配（2026-11-13，本地源码）
PrimaryChatPane 按历史区 ResizeObserver 实测高度缩放水滴，空态容器允许收缩且不滚动；不足240px收起辅助提示，保留标题与水滴，移动端输入字号至少16px避免聚焦放大。复用现有 VisualViewport 壳高度/偏移。JarvisOrb 改为中性透明水滴与高光折射，9秒小幅非对称形变；状态标签、审批暖色、减少动态偏好保留。两组件 Vue 模板检查通过；未构建、上线或打包。独立水滴外观预览已生成，但内置浏览器拒绝 file 协议，未完成视觉验收；真实手机键盘验收待完成，不能声称故障已实测消除。

水滴微动修订：按用户反馈增加淡蓝色，移除 idle/thinking/speaking/needs_human 的整球等比脉冲及旋转色层。使用单一可插值 strain 与倒数轴缩放保持投影面积恒定，轻压后多次小幅衰减回弹，减少动态时静止。仍为本地源码与外观预览，未发布/打包/手机验收。

### 水滴首页与已完成待办发布（2026-11-13）
用户明确授权上线。18项定向检查通过，Vite183模块构建成功，网页资源 `index-uOQiz1dy.js` / `index-BOlmdIoH.css` 已发布；正式服务HTTP及资源字节校验一致，index SHA256 `b382f925d7b38b1fb901b3173c6c7c1d4f8bfbb353fb2e1af8af4935cef71dcd`。水滴为淡蓝透明、互逆轴挤压回弹，取消整球呼吸缩放；首页按可用高度缩放且窄空间隐藏辅助提示。已完成待办按年份双层默认折叠一并发布。
回退备份 `web-before-water-tension-20261113-050222`；旧哈希资源保留，恢复其index即可回退。未修改后端/数据库、未打包APK。统一本地5194浏览器连接拒绝，未声称视觉或真实手机键盘验收通过；此前“未发布”记录由本回执更新。

### Claude 接管询问取消入口（2026-11-13，本地源码）
新增“取消打断”及提示内 Escape：只关闭确认并恢复输入焦点，保留草稿/附件和远端进程；sending 时禁用。原看板关闭行为保留，不引入新的远端调用。待发布及浏览器验收，本次未打断真实 Claude 进程。

---

## 5. 后 `WP-J6` 系统性进化施工合同（`BOB-D-016`：`WP-E1` → `WP-E4`，2026-11-14 交接基线）

> **致后续接手 Agent（必读接手说明）**：
> - **当前代码基线**：分支 `1.0.1`（及对齐同步的 `main`），基线提交 `6b93e62` 之后；`WP-J1`～`WP-J6` 全栈源码及离线四重门禁验证已完备（见 [2026-11-11 验证回执](evidence/2026-11-11-wp-j1-j6-jarvis-convergence-verification.md) 与 [COMPONENT_MAP](COMPONENT_MAP.md) §1.1）。
> - **核心目标**：依据 [DECISIONS](DECISIONS.md) `BOB-D-016` 与 [DEVELOPMENT_ROUTE](DEVELOPMENT_ROUTE.md)，在不破坏已有 L1/L2/L3 兼容性与“零 Node.js 必经依赖”铁律的前提下，按 `WP-E1 → WP-E2 → WP-E3 → WP-E4` 分波补齐白领场景与竞品（如 Grok Bot）的差距，并放大 Bob 在本机协同、`V1→V2` 批注审阅、自生长小组件与低成本安全门禁上的差异化优势。
> - **强制护栏**：
>   1. 禁止硬编码任何个人域名、IP 或私密节点名（示例域名用 `bob.example.com`）；
>   2. Rust 离线测试需使用外置缓存且保持 `cargo test --offline --workspace` 通过；前端严禁硬编码颜色（仅用 CSS Variables）与装饰性 emoji，可见文案必须同步更新 `zh-CN.json` 与 `en-US.json`；
>   3. 对线上常驻节点执行重启、构建或 APK 打包前，须确认已获得用户明确授权；
>   4. 每完成一张子任务卡，须同步在本节或 `docs/evidence/` 登记可验证回执。

---

### 5.1 第一波次：`WP-E1` 实景闭环、快慢双模分流与 3 步开箱向导（本地源码与四重门禁已完成，待授权部署/真机）

> **2026-11-14 本地验收完成摘要**（详细回执见 [2026-11-14 WP-E1 验证回执](evidence/2026-11-14-wp-e1-systemic-evolution-verification.md)）：
> - **`WP-E1.1`～`WP-E1.4` 本地全栈落地**：已完成容器/装机 `fonts-noto-cjk` 字体与 `--user-data-dir` `0700` 锁清理及崩溃自动重启、反代 HTTPS/WSS `Origin` 校验、IME 组合输入与 390px 滑块拖拽、接管一键存为 `RoutineStore`（`WP-E1.1`）；10 家服务商严格对齐 `model_providers.json` 的 Clerk/Maker 双模预设、`<1s` `Resident Clerk` + 异步 `Maker Worker` + 持久 `StepLedger` + Thread 订阅直通与追问批处理（`WP-E1.2`）；CSPRNG 一次性 6 位初始配对码（指数退避限流 + 防抢注）与 3 步响应式 `<OnboardingWizard />`（`WP-E1.3`）；沙箱提示词与静态门禁（禁外链 CDN / 硬编码颜色）、`FeatureWidgetFrame.vue` 零闪烁 `srcdoc` 与运行时报错自动回退及双白领小组件闭环验证（`WP-E1.4`）。
> - **四重门禁全绿**：`cargo test --offline --workspace` **386 项通过 / 0 失败 / 0 警告**（含本地真实无头浏览器 CDP E2E）；`cargo check --offline --manifest-path apps/bob-app/src-tauri/Cargo.toml` **0 错误通过**；`node --test tests/*.test.mjs` **236 / 236 通过**（中英 i18n **1,151 / 1,151 键 100% 对称**，零硬编码色值，零装饰性 emoji）；`pnpm build` **185 模块构建通过**。
> - **当前边界与下一顺位**：按本轮授权边界，**尚未重启线上常驻生产服务器、尚未打包 Android APK**；下一顺位可进入 **`WP-E2`（纯 Rust 办公文档深度处理与标准化高频 IM 双向通道）** 或按用户指令将 `WP-E1` 发布至常驻节点做线上实测。

#### 任务卡 `WP-E1.1`：常驻节点无头浏览器实装（CJK 字体 + 隔离卷）与真实办公站点接管验收（本地实现与本地真实无头浏览器 E2E 已完成，线上站点待部署后补验）
- **背景与缺口**：当前 `crates/bob-browser` 仅在本地模拟 CDP 测试通过；生产侧 `deploy/Dockerfile` 缺少中文字体，线上宿主机尚未验证 Chromium + 独立 `--user-data-dir` 的重启保活与 390px 手机接管实操。
- **允许改动范围**：
  - `deploy/Dockerfile`、`scripts/deploy-vps-oneclick.sh`、`crates/bob-browser/src/*`、`crates/bob-node/src/browser_ws.rs`
- **关键动作**：
  1. 在 `deploy/Dockerfile` 运行时镜像与宿主机装机检查中补充 `fonts-noto-cjk`（及基本表情/无衬线字体回退），确保中文网页无方框乱码；强化 `--user-data-dir` `0700` 目录权属检查与 Chromium 崩溃自动重启恢复。
  2. 核对 `browser_ws.rs` 在反代（HTTPS/WSS）下的 `Origin` 校验逻辑（兼容自定义公网域名与本地直连），完善中文输入法（IME composition）与滑块拖拽在 390px 移动端视口的手势流畅度。
  3. 选取 3～5 个真实办公/内容站点（用测试账号），在常驻节点跑通“自动浏览 → 撞登录/滑块弹窗 → 手机 390px `<VirtualBrowserModal />` 完成接管 → 服务重启后验证持久化 Cookie 仍有效 → 一键存为 Routine 复跑”。
- **验收门禁**：`cargo test --offline -p bob-browser -p bob-node` 全绿 + 写入脱敏真实验收记录至 `docs/evidence/`。

#### 任务卡 `WP-E1.2`：置顶 Jarvis `Resident Clerk + Async Worker` 快慢双模分流（已完成本地源码与端到端单测）
- **背景与缺口**：目前 `crates/bob-runtime/src/native_harness.rs` 与 `crates/bob-node/src/jarvis.rs` 采用单条 SSE 执行流响应请求，复杂网页抓取或多步工具调用期间用户体验存在等待阻塞感；配置层面也缺少对“同一 API Key 自动绑定快模型（Clerk）+ 慢模型（Maker）”的一键默认配对逻辑。
- **允许改动范围**：
  - `crates/bob-runtime/src/{native_harness.rs, model_presets.rs (新增), tool_bus.rs}`
  - `crates/bob-node/src/jarvis.rs`
  - `apps/bob-app/src/composables/useJarvisHome.mjs`、`apps/bob-app/src/components/PrimaryChatPane.vue`
- **关键动作**：
  1. 抽离并建立服务商预设映射表（DeepSeek / Gemini / OpenAI / Anthropic / OpenRouter 的推荐 `clerk_model` 与 `maker_model`，严格遵循不硬编码外部注册表之外猜测值的原则，允许用户高级微调）。
  2. 将 `/api/jarvis/v1/stream` 交互拆分为双层：用户输入先经过极轻量的 `Resident Clerk`（仅携带记忆摘要 + `dispatch_background_task` / 轻量读取权限，秒级首字响应或直接回答）；涉及浏览器/多步工具的任务立刻返回一个后台任务 ID，由 `tokio::spawn` 异步 Worker 独立运行并把结构化步骤卡与审批卡实时推回主对话流与状态机，聊天输入框保持随时可用、断线重连可从持久化 Step Ledger 恢复进度。
  3. 对齐原型实验室文档（`bob-bot/docs/SUPER_AGENT_NOTES.md`）中的 `Thread 订阅直通` 与 `事件批处理`，同一任务后续追问优先直通当前活跃 Worker 上下文，避免 Clerk 重复路由浪费预算。
- **验收门禁**：`cargo test --offline -p bob-runtime -p bob-node` 新增快慢分流与后台任务断线恢复单测全绿 + 前端 `node --test tests/*.test.mjs` 全绿。

#### 任务卡 `WP-E1.3`：首次访问 3 步图形化开箱向导与一次性配对码鉴权（已完成本地源码与端到端单测）
- **背景与缺口**：当前首次开箱缺少全屏三步引导 UI，非技术用户面对多 Tab 设置页与模型/URL/Model ID 表单仍有认知障碍；同时未初始化实例缺少面向容器部署的安全首次接管机制。
- **允许改动范围**：
  - `crates/bob-node/src/{lib.rs, node_access.rs, jarvis.rs}`
  - `apps/bob-app/src/components/OnboardingWizard.vue`（新增）、`apps/bob-app/src/App.vue`、`apps/bob-app/src/locales/{zh-CN.json, en-US.json}`
- **关键动作**：
  1. 后端在首次检测到未配置管理员口令及模型 Key 时，启动日志生成并打印**一次性 6 位初始配对码**（内存中加有效期与错误退避，防止抢先注册），提供只读状态字段 `needsOnboarding`（仅返回布尔状态与预设服务商列表，绝不回传凭据或泄露系统信息）。
  2. 前端实现简洁响应式 `<OnboardingWizard />` 三步流程：**Step 1 验证配对码与设定访问口令 → Step 2 选择服务商卡片并粘贴 1 个 API Key（一键实测连通性并自动配对 Clerk/Maker 双模）→ Step 3（可选）为助理命名 / 跳过直接进入 Jarvis 水滴主屏**。
- **验收门禁**：支持 390px 手机端与 1280px 桌面端、明暗双模与双语 i18n 全键对称，附带向导流程与抢注拒绝单测；完成并验证零硬编码颜色/emoji。

#### 任务卡 `WP-E1.4`：对话现场生成与迭代桌面小组件真实模型闭环调优（已完成本地源码与沙箱闭环单测，待商业 Key 在线补验）
- **背景与缺口**：`WP-J3` 已建立 `/api/f/:pkg/*` 安全路由、`FeatureWidgetFrame` 与 3 分钟试用 5 版回退机制，但仍需使用真实商业大模型对生成提示词、主题 CSS 变量注入与错误自动恢复提示做实测打磨。
- **允许改动范围**：
  - `crates/bob-runtime/src/{features.rs, native_harness.rs}`（提示词规范段）
  - `apps/bob-app/src/components/{FeatureWidgetFrame.vue, FeatureTrialBanner.vue, JarvisWidgetGrid.vue}`
- **关键动作**：
  1. 优化内置提示词对沙箱约束的说明（禁用外链 CDN 与硬编码色值，指导模型使用宿主透传的 `--bg-*`、`--text-*`、`--accent-primary` 与 `bob.call`/`bob.save` 通信规范）。
  2. 验证“一句话新建小组件 → 主屏展示无白屏闪烁 → 试用 3 分钟内二次微调 → 点击保留或到期自动回退”全流程实测录屏/日志。
- **验收门禁**：在真实模型调用下跑通 ≥2 个实用白领小组件（如每日待办焦点卡、汇率/报价换算卡），确认重启服务与超时场景均可干净回退。

---

### 5.2 后续波次路线图预览（`WP-E2` → `WP-E4`，前置依赖完成后按序启动）

#### 波次二：`WP-E2` 白领办公文档深度处理与高频 IM 触达（依赖 `WP-E1.2`）
- **核心交付**：
  1. **纯 Rust 办公文档读写与生成工具集**（`crates/bob-runtime/src/doc_tools.rs`）：基于轻量纯 Rust crate（零外部 Office/Python/Node 依赖）支持读取解析 **`.pdf` / `.xlsx` / `.docx` / `.csv`**，并支持生成结构化 **`.xlsx` / `.md` / 交互式 HTML 报告**，产出自动登记为 `ArtifactVersion`，在聊天/工作台中点击即可直接进入 `<ArtifactRevisionReview />` 做 `V1→V2` 圈选批注续改。
  2. **标准化 `ChannelAdapter` 双向通道**（`crates/bob-node/src/channels/`）：基于 `Source / Trigger / Thread / Outbox` 幂等架构，完善与扩展 **飞书（Lark）、Telegram Bot** 与现有 **微信官方桥（`/v1/*`）**：支持仅响应已绑定用户、IM 群聊/私聊触发日常任务、返回简洁行动卡 + 审批/虚拟窗口一次性跳转链接，严格执行“严禁经 IM 明文传递网站密码”红线。

#### 波次三：`WP-E3` 三路径傻瓜安装向导与桌面扫码直连（依赖 `WP-E1.3`）
- **核心交付**：
  1. **路径 A（桌面零服务器直装端）**：在 `apps/bob-app/src-tauri` 完善系统托盘常驻模式与首次启动 3 步向导（选 Key → 勾选 1 个授权工作目录 → 屏幕直接显示手机扫码配对二维码，复用 `bob-transport` 双向 TLS 完成移动端绑定）。
  2. **路径 B（90 秒预编译云端向导与一键模板）**：新增 `.github/workflows/release-bob-node.yml` 发布 `linux/amd64` & `linux/arm64` 多架构预编译镜像（GHCR）与静态二进制；升级 `scripts/deploy-vps-oneclick.sh` 实现交互式零编译拉取（自动查内存/Swap → 仅问是否拥有域名并配置 Caddy 或隧道 → 打印一次性 6 位首次配对码）；补充 **1Panel / CasaOS / NAS** 标准 Compose 安装模板与独立 `docs/USER_GUIDE.md` 图文傻瓜安装教程。

#### 波次四：`WP-E4` MCP 连接器客户端与 `QuickJS` 插件沙箱体系（依赖 `WP-E1` + `WP-E3`）
- **核心交付**：
  1. **纯 Rust 远程 MCP 客户端与 OAuth 2.1 令牌保险箱**（`crates/bob-runtime/src/mcp_client.rs`）：激活并落地此前保留在 `features.rs` 中的 `McpStore`，支持连接标准 MCP Streamable HTTP（及桌面端可选本地 `stdio`）外部服务（Google Workspace、Notion、GitHub、飞书等），动态将远端工具接入 `BobToolBus`；除经过审核标为只读的工具外，写操作统一强制标为 `ActionRiskLevel::SideEffect` 走审批卡与无人值守 `Draft` 门禁。
  2. **内嵌 `rquickjs` (`QuickJS`) `.bobpkg` 微应用引擎**（`crates/bob-runtime/src/features.rs` 沙箱升级）：以纯 Rust `rquickjs` 替代当前仅有的静态路由声明，强制 `16MB` 内存与 `500ms` CPU 硬上限、零文件系统/环境变量/子进程权限、公网 HTTP 请求唯一穿透 `net_guard`，依然受 3 分钟试用 + 5 版本自动回退守护。
  3. **统一消费者级「能力目录」**：在设置与主屏新增入口，统一收纳 MCP 连接器、`QuickJS` 小组件与 Skill/Routine 模板，对外隐藏技术名词，实现小白点选即装、极客可查来源与源码审阅的渐进体验。
