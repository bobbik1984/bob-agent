# todo — 需求池（仅保留未完成与待补尾缺口）

- [x] 修复额度菜单选字触发浏览器崩溃：忽略空目标失焦，完成双浏览器四组验证并发布前端。
- [ ] 接入官方订阅额度持续采集：Codex app-server、AGY quota JSON、X1T Claude 官方结构化额度；显示来源/更新时间，失效保留缓存，不使用一次性快照冒充实时数据。

- [x] 2026-11-05 最新开发版 0888876 网页发布及真实 X1T 阅读/图片、390/1180px 菜单隔离验证；见 docs/evidence/2026-11-05-quota-device-paths.md。设备关联迁移、官方额度、Actions SSH 和手机验收仍按原队列保留。

> **记录准则**（遵循 [项目开发与文档统一标准](../docs/standards/project_documentation_standard.md) §3.7）：
> 1. **已完成项 (`[x]`)** 一律归档至 [`docs/archive_done.md`](docs/archive_done.md)，不在本文件堆积战报；
> 2. **已获得的能力与验收标准** 统一登记于 [`docs/FEATURES.md`](docs/FEATURES.md)；
> 3. **当前排期施工步骤** 唯一维护于 [`docs/CURRENT_STAGE.md`](docs/CURRENT_STAGE.md)；本文件仅保留**尚未解决的需求、缺陷与部分完成项（`[/]`）的剩余缺口**。

---

## 一、 近期优先队列：F2/F5 会话阅读与流式交互四项优化（2026-11-02 确认）

施工顺序与细则见 [`docs/CURRENT_STAGE.md §3.3`](docs/CURRENT_STAGE.md)，验收标准见 [`docs/FEATURES.md`](docs/FEATURES.md)。手机实测等待用户通知。 统一节点 DSH 认证分流已在网页发布并实测，APK 及其真机验收以本轮 evidence 为准；保留重启自动恢复与各工具流式终验缺口。

- [/] **Phase A 四项体验优化真机终验**（本地代码与 `tests/phase-a-ux.test.mjs` 93 项前端单测、55 项 `bob-runtime` 单测已全绿归档至 `docs/archive_done.md`；**剩余缺口**：待用户通知连接手机后执行 Android 真机状态栏安全区、手势返回与真实多节点流式联调终验）。

---

## 二、 BP4 设备统一入口、多 Harness 触达与受控自治缺口

详细施工步序见 [`docs/CURRENT_STAGE.md §3.6`](docs/CURRENT_STAGE.md)，架构与合规护栏见 [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)。

- [/] **BP4-06 宿主边界与原生常驻监管**：按 BOB-D-013 优先官方服务/持久程序接口，tmux 仅作兼容保活；VPS3 原生协议只读探测已验证，**剩余缺口**：新节点配对与两域名入口已部署并实测；X1T Claude/AGY 与 VPS3 Pi 原会话只读增量已实测并接入本地网页；仍需 DSH 实时订阅、Codex/Claude 未知请求对账、跨轮持久监管、取消确认、全局事件容量/过期游标及断线恢复。见 docs/evidence/2026-11-05-native-session-stream.md。施工见 CURRENT_STAGE §3.6。
- [/] **BP4-06/07 单设备多 Harness 入口与会话发现/新建**：X1T 连接已合流 DSH/AGY 目录；**剩余缺口**：自动发现已授权工作区的 Claude（`~/.claude/projects`）/ Codex 历史会话并合流至 Tauri `app_dsh_list`，在侧边栏提供“新建 CLI 会话”入口，完善设备能力配置 UI。
- [/] **BP4-06 节点统一认证维护与多工具递进接线**（规范见 [`docs/wiki/harness-auth.md`](docs/wiki/harness-auth.md)）：已将原生宿主授权/版本/认证探测接入共用 RPC，移除静态能力成功推断；VPS3 Codex 协议/目录、Pi RPC 与 Claude 登录状态已验。**剩余缺口**：Pi 已配置指定 DS for Remote 私有 profile 并验证原会话，AGY 已用 stream-json 接续；剩余 Codex 原会话实际回复、VPS3 Claude 无回复定位、VPS3 缺失工作区、DSH 实时订阅与独立程序握手、完整持久事件/恢复/取消、Hermes/OpenClaw 实测。见 [`VPS3 evidence`](docs/evidence/2026-11-04-vps3-native-adapters.md)，整体保持进行中。
- [/] **BP4-01/02/03 跨设备项目关联与工作区交接终验**：本实例 SQLite `project_session_links` 与内核交接状态机已通过；**剩余缺口**：真实跨节点文件/dirty 素材传输验证、跨实例关联同步与 Android 原生命令接线。
- [/] **BP4-04/05/06 受控自治护栏接线**：胶囊符号骨架、只读子代理池、`ExecutionAssertion`（事前锦囊防盲目质疑反转 + 全绿锁写）、任务卡减法约束与 `WarmRestartState` 热重启状态已在 `context_capsule.rs` 落盘通过单测；**剩余缺口**：在前端主对话 UI 挂载 Observe/Ticket 草案确认卡与低置信度复核提示面板。

---

## 三、 移动端真机待验与 F1–F4 剩余体验缺口

- [/] **APK `d740979` 真机覆盖安装与验收**（安装包已就绪于 `dist-release/bob-latest.apk`，等待手机连接通知）：核验沉浸模式顶栏汉堡包、抽屉置顶「Agent」主对话、DeepSeek 单一模型显示、Tavily/TinyFish 已配置状态及票据卡夹拍照/选图识别。
- [/] **F2/F5 DSH 原生会话冷网性能与未知回执**：补齐 DSH 服务端缩略图/正文筛选优化、冷网超长历史加载性能、权威未知回执查询及大量会话收纳。
- [ ] **F2 设备标识增强**：补齐可靠友好名、硬件类型、最后观测时间；区分连接 Home 与任务执行节点。
- [ ] **F1/F3 闪念项目搜索与原图跨端同步（RV-13 延续）**：闪念项目选择增加搜索框（避免大量项目时拉长原生列表）；实现 `capture_media` 原图跨 Home 同步、离线缺图提示与未引用草稿清理。
- [ ] **RV-21 会话历史全文检索与对话内发图/附件**：SQLite FTS5 消息搜索；对话输入框拍照/选图/文件上传、发送前预览与离线队列。
- [ ] **RV-19 提醒通知通道**：`reminder_at` 到期时触发 Windows Toast / Android 本地通知。
- [ ] **RV-14 / RV-15 桌面端布局与组件拆分优化**：`App.vue` 按视图/composable 持续模块化；桌面宽屏左侧导航栏与设置主视图优化。
- [ ] **XP-02 / XP-03 运行时下沉收尾**：将 `src-tauri` 剩余业务命令进一步下沉至 `bob-runtime`，并用 Rust 原生 `rustls` 客户端替换桌面端 `curl` 子进程。
- [/] **RV-01~05 / RV-07 / RV-11 / RV-12 审查项真机终验**：复核所有个人事务写入失败 `role="alert"` 显式提示、未实现控件置灰标注及真实 BYOK Key 双端断网/重开完整旅程证据。

---

## 四、 待用户裁决、前身修正与开源前提（BP6）

### 待裁决与产品长期缺口（G14）
- [ ] **BOB-D-007**：是否需要在 Google Drive 放文档只读镜像供跨端 Bot 读取（模型注册表已通过 `cloud_sync/registry/` 打通）。
- [ ] **G14 长期项**：无服务器时 Android 后台提醒限制策略、BYOK 费用可见性与预算面板、旧 App `0.9.x` (`bob.db`) 数据迁移工具、遥测与崩溃上报隐私策略。

### 开源发行前提（BP6）
- [ ] 选定许可证并补齐 `LICENSE`，完成依赖许可证审计与 Git 历史 secrets 扫描。
- [ ] 去个人化：域名、IP、节点名全部改为外部 Provider 配置注入；清理旧 `antigravity-nexus` 包名残留。
- [/] 凭据安全边界复核：确保普通同步配置不携带秘密、Online token 不随普通快照回传前端。

### 前身项目备忘（不在本仓库直接修改，回对应项目执行）
- [ ] 旧 Online 统一提案 §2 中“worktree 注册项可 prune”改为“禁止 prune（含 2707 项已备份未提交改动）”。
- [ ] 旧 Online `ONL-D-005` 仍写 VPS3 为 Core（实际生产在 Hostinger），需标过期。
- [ ] 旧 App `docs/contracts/bob_cloud_agent_interchange.yaml` 标为 `superseded`。
