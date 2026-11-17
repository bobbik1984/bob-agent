# Bob — Current Stage（1.0.1 施工与交接合同）

> **当前基线（2026-11-17）**：分支 `main` / `1.0.1`，代码基线 `7978e2e`；已完成纯 Rust 单一内核、Jarvis 自主主助理与桌面小组件收敛（`WP-J1`～`WP-J6`）、`WP-E1` 本地全栈与长期记忆/日程解析闭环、`BOB-D-017`（`WP-C1`）去 `X1C` 远程底座与多参谋长总线，以及 `project_registry.yaml` 正式项目 SSOT 与 `/opt/*` 运行态后台排除。
> 2026-10-31 至 2026-11-17 逐日历史流水、发布哈希与回退细节已完整归档至 [历史阶段归档（2026-10-31～2026-11-17）](evidence/2026-11-17-historical-stage-archive-oct31-nov15.md) 与 [2026-10-29 历史归档](evidence/2026-10-29-historical-stage-log.md)。

---

## 1. 当前基线与已交付版图（截至 2026-11-17，代码基线 `7978e2e`）

| 能力域 / 阶段 | 已交付能力与验证范围 | 权威回执与未验边界 |
|---|---|---|
| **BP0–BP3 / S0–S4 统一内核与常开节点** | `bob-kernel` + `bob-transport` + `bob-runtime` + `bob-node` 单一 Rust 内核；SQLite 迁移 `0001`–`0014`、Outbox 懒式发送/对账、Web 与 Tauri 桌面/Android 共用前端；常开 `bob-online` 服务部署及只读幂等导入 | [S0–S4 验证报告](evidence/2026-10-31-s0-s4-bob-online-verification.md)；跨公网真机重连与旧系统 BP5 退役仍待确认 |
| **BOB-D-015（`WP-J1`～`WP-J6`）Jarvis 主助理与自扩展** | `BobNativeHarness` SSE 流式 ReAct + `$10`/月上限 + `BobToolBus`；水滴 `<JarvisOrb />` + `<JarvisWidgetGrid />`；无 Node 3 分钟试用沙箱小组件（5 版回退）；Routine/待批草稿/五阶段做梦/晨报/VAPID Web Push；`bob-browser` CDP `<VirtualBrowserModal />`；PC 获准目录工具与 `V1→V2` `<ArtifactRevisionReview />` | [WP-J1～WP-J6 验证回执](evidence/2026-11-11-wp-j1-j6-jarvis-convergence-verification.md)；常驻节点真实办公站点接管与真机终验并入 `WP-E1` |
| **WP-E1 本地底座（`WP-E1.1`～`E1.6`）** | `Resident Clerk + Async Worker` 快慢双模分流 + `StepLedger`；10 家模型预设对齐；CSPRNG 6 位初始配对码 + `<OnboardingWizard />`（已有网关外部认证节点保留原密码直登）；UTC+8 ISO 日期解析与防三年偏移门禁；同会话有界续聊、CAS `schedule_update`、失败幂等回告与个人事务自动刷新 | [WP-E1 验证回执](evidence/2026-11-14-wp-e1-systemic-evolution-verification.md)、[主助理连续性证据](evidence/2026-11-17-main-agent-continuity.md)；浏览器视觉细节与真机 APK 用户验收待执行 |
| **WP-E1-M 长期记忆与多问题覆盖** | 近期 24 条消息窗口、带来源/作用域（`conversation`/`project`/`global`）持久记忆、CAS 纠正与事务内遗忘边界、16 条新消息阶段摘要游标、相关历史自动召回、逐问题持久化清单与受限补核对、审批期间遗忘失效；258 项回归与 Gemini/DeepSeek 真实对话验证通过 | [长期连续性证据](evidence/2026-11-17-assistant-memory.md)；本轮未打包 APK，浏览器/手机用户终验、离线副本同步、专门编辑 UI 与逐行增量索引待补 |
| **BOB-D-017（`WP-C1`）去 `X1C` 远程底座与多参谋长总线** | `~/work` 七分区与 `chief_briefing.py` 跨节点脱敏态势聚合（无 `X1C` 在线依赖）；`chief_service.rs` 扩展 `bob_briefing` / `bob_order_submit` / `bob_orders` 并入 Jarvis `pending_approval` 统一审批；`bob-node qr` 紧凑终端二维码、Android 可选生物安全锁、独立语义日期待办、日历三天视图/拖动改期、L3 工人会话默认 `full_auto` + 灾难命令硬拦截 | 已合入主干并通过后端单测与线上回读；手机端待办/日历与外部消费端全流程复验待补 |
| **正式项目 SSOT 与 `/opt/*` 后台隔离（`7978e2e`）** | `project_registry.rs` 对接 `project_registry.yaml` SSOT（区分 `bob-agent` 与独立原型库 `bob-bot`）；禁止 AI 自动创建非正式桶；收集箱下拉过滤 `proj-default` 与非正式项目并新增「+ 手动输入新项目...」；过滤 `/opt/*` 非 Bob 启动后台进程（`bob_project: false`，如 `Quant Bot`）；测试会话归入 `__bob_test_archive__`，探索/历史导入默认折叠 | 189 项 `bob-runtime`、6 项 `agy_index_probe`、270 项前端单测全绿并已发布；手机端零对话/归档同步待随下次发包复验 |

> **核心结论（2026-11-17 线上基线约束）**：L3 节点工人会话（DSH/Codex/Claude/AGY）在主力节点默认使用 `full_auto`（仅硬拦截删根/格式化/强推/删表等灾难命令与 `AskUserQuestion`），与 **L1/L2 个人与办公 `Action Gates`（高危网页、外部 MCP 写入、无人值守 Routine `Draft`、参谋长 `ChiefOrder`、消费端人工批准）严格隔离**，绝不因 L3 编程自治而放开个人副作用门禁。

---

## 2. 当前活跃工作包与待补验收缺口（Active Work Packages & Open Gates）

### 2.1 `WP-E1` 线上实景闭环与移动端用户终验
1. **`WP-E1.1` 常驻节点无头浏览器实装与真实办公站点接管**：
   - 在已集成 `fonts-noto-cjk` 与 `0700` `--user-data-dir` 的常驻节点上，选取 3～5 个真实办公/内容站点（测试账号），跑通“自动浏览 → 撞登录/滑块/验证码 → 手机 390px `<VirtualBrowserModal />` 接管 → 重启服务 Cookie 保活 → 一键存为 Routine 复跑”，并将脱敏记录落盘 `docs/evidence/`。
2. **`WP-E1.4` 对话现场生成小组件实测调优**：
   - 用真实商业 LLM 跑通至少 2 个实用白领小组件（如每日待办焦点卡、报价/汇率换算卡）的“生成 → 首帧无闪烁展示 → 3 分钟内二次微调 → 保留及超时/重启回退”全流程实测记录。
3. **浏览器视觉与 Android 真机用户终验（多包待合流复核）**：
   - 核验主助理连续性（`WP-E1`）、长期记忆（`WP-E1-M`）、水滴首页与移动键盘空态缩放、已完成待办按年折叠、日历拖动改期与手机三天视图（现有签名包对应 `e6da7e7` / `c7555da` / `d740979`；本轮记忆/项目清理改动尚未重新触发 Android CI 打包）；顺带复核离线副本同步与零对话项目跨端归档。

### 2.2 `BP4-06/07` 设备多路径归并、多 Harness 监管与工具策略扩展验收
- **外部独立 CLI 实时旁观与全事件恢复**（详见 [原生会话流证据](evidence/2026-11-05-native-session-stream.md)）：
  - 已有 `DSH` 实时生成重启恢复、`Pi` 取消确认与 `Codex` 常驻 `app-server` 内存只读旁观；待补独立 `Claude` / `AGY` / `Codex TUI` 外部进程实时订阅（须确认官方持久接口，禁止凭据提取或刮屏）、`DSH` 待问答交互全链路、`Codex` 缺轮次 `unknown` 权威核对及完整多工具故障矩阵。
- **`BP4-07-TP4` 工具启用策略扩展实测**（已通过三节点停用/重启/重启用与历史只读保留，详见 [ARCHITECTURE](ARCHITECTURE.md)）：
  - 剩余扩展场景：真实审批等待中途停用工具、在途长任务取消确认、离线节点断网恢复同步、移动端设置开关验收。
- **单节点工具兼容与额度观测缺口**：
  - `X1T` Claude 官方 `statusLine` `rate_limits` 实时回调观测与持续展示验证；
  - 跨 Harness 工作区成果实际渲染（HTML 资源包 / 复杂 PDF / Office；其中 Office 依赖安装按用户指示暂缓）与不可变 `ArtifactVersion` 历史快照；
  - 各节点既有外部工具限制跟踪：`VPS3` AGY（`1.2.16` 不支持 ACP）、`VPS3` DSH 尚未接线、`VPS2` Pi 模型未配置、`X1T` Claude 官方账号限制。
- **设备多路径归并收尾**（见 [统一节点入口证据](evidence/2026-11-04-unified-node-entry.md)）：
  - 保留统一节点主路径与 Tailscale 备选路径分开健康探测，禁止换路径自动重发未知执行；旧专用入口在能力与关联迁移核对后再停用。

### 2.3 独立运维通道
- GitHub Actions 托管工作流 `.github/workflows/bob-operations.yml` 与 `scripts/bootstrap-bob-operations.py` 已就位；若后续需要从外部纯云端 runner 免人工 SSH 运维，由所有者一次性执行 bootstrap 写入 `BOB_DEPLOY_SSH_CONFIG` Secret 并跑通 `inspect` 回执。
- 当前日常开发与部署遵循 `~/work/common/AGENTS.md` 对等节点准则：常驻节点（`VPS2` / `VPS3` / `X1T`）持有各自独立 Git 仓库与边缘授权，可经获准运维通道直接完成构建、备份与发布。

### 2.4 单一收集箱与每日幕僚长分拣（`BOB-D-014`）
- 内核 `0011`/`0012`（`capture_triage` / `triage_batches`）、`bob-runtime::inbox_triage`、`InboxView.vue` 五栏导航与「+ 手动输入新项目...」已就位；剩余待落地 **Step 4**（每日定时触发、确认后自动写入常驻/PC 工作区 `todo.md` 含离线 `Waiting for PC` 队列、知识笔记「展开这个想法」、旧 Dashboard `#inbox` 30 天转发衔接）。
- 施工前待用户最终拍板的 4 项默认值（已列于 `todo.md`）：① 每日分拣默认 `21:00`（连续多日未处理合并为单卡）；② V1 严格 `100%` 人工点击确认；③ V1 先文字 + 图片，语音捕获后置；④ 旧 `#inbox` 过渡保留 `30` 天。

---

## 3. 后续系统性进化波次（`BOB-D-016`：`WP-E2` → `WP-E3` → `WP-E4`）

1. **`WP-E2` 白领办公文档深度处理与高频 IM 触达**（依赖 `WP-E1` 稳定）：
   - `crates/bob-runtime/src/doc_tools.rs`：纯 Rust 解析 **`.pdf` / `.xlsx` / `.docx` / `.csv`** 并生成格式化 `.xlsx` / `.md` / HTML 报告（零外部 Office/Python/Node 依赖），结果入 `ArtifactVersion` 直通 `<ArtifactRevisionReview />` 做 `V1→V2` 批注修订。
   - `crates/bob-node/src/channels/`：基于 `Source / Trigger / Thread / Outbox` 幂等模型落地标准化 **`ChannelAdapter`**（飞书 / Telegram / 官方微信桥增强），IM 内严禁明文收发网站密码或验证码。
2. **`WP-E3` 三路径零门槛安装体系与桌面扫码直连**（依赖 `WP-E1.3`）：
   - **路径 A（桌面零服务器直装端）**：`apps/bob-app/src-tauri` 系统托盘常驻 + 首次启动 3 步向导 + 屏幕二维码直连手机（`bob-transport` 双向 TLS）。
   - **路径 B（90 秒预编译云端向导）**：CI 工作流 `.github/workflows/release-bob-node.yml` 发布含 `Chromium` + `fonts-noto-cjk` 的多架构镜像与静态二进制；升级 `scripts/deploy-vps-oneclick.sh` 与 **1Panel / CasaOS / NAS** Compose 模板，更新 `USER_GUIDE.md` 图文安装教程。
3. **`WP-E4` MCP 连接器客户端与 `QuickJS` 插件沙箱**（依赖 `WP-E1` + `WP-E3`）：
   - `crates/bob-runtime/src/mcp_client.rs`：纯 Rust MCP Streamable HTTP 客户端 + OAuth 2.1 令牌保险箱（桌面可选 `stdio`），外部写工具默认标为 `SideEffect`（交互弹审批卡、Routine 落为 `SavedActionDraft`）。
   - `crates/bob-runtime/src/features.rs` 沙箱升级：内嵌纯 Rust **`rquickjs`（`QuickJS`）** 隔离沙箱（16MB 内存 / 500ms CPU 硬上限、零文件系统/环境变量/子进程权限、公网请求唯一穿透 `net_guard`，保留 3 分钟试用与 5 版本自动回退）。
   - **统一消费者级「能力目录」**：整合 MCP 连接器、`QuickJS` 小组件与 Skill/Routine 模板。

---

## 4. 施工护栏、验证门禁与回退规约

1. **红线约束**：产品源码与常规文档严禁硬编码个人域名、IP 或宿主绝对路径（示例域名使用 `bob.example.com`）；文件 I/O 一律 UTF-8（禁止 PowerShell 原生文本管道改写含中文文件）；前端严禁硬编码色彩值（仅用 CSS Variables）、严禁装饰性 emoji，可见文案必须同步更新 `zh-CN.json` 与 `en-US.json`。
2. **授权与重型操作防火墙**：依赖安装、大型 Rust/Tauri 打包发布、生产数据库/服务重启、凭据新建/轮换、对外发送真实商业模型或网络请求，必须获得针对具体对象的明确授权；同一受控仓库保持单一有效写入者，与独立原型库 `bob-bot` 分界操作。
3. **验证与回退**：每次交付必须在 `docs/evidence/` 保留真实命令输出、输入版本、运行环境与**未验范围**；服务发布前须做一致性数据库备份与静默回滚锚点（回退仅恢复程序与静态资源，严禁以旧数据库覆写新记录）。常用离线验证入口：
   - Rust 全工作区：`cargo test --offline --workspace`（Windows 下若触发 `worker_ledger` 文件锁争用可加 `--test-threads=1`）
   - Tauri 壳校验：`cargo check --offline --manifest-path apps/bob-app/src-tauri/Cargo.toml`
   - 前端单测：`node --test apps/bob-app/tests/*.test.mjs`
   - 原生目录探针单测：`python3 crates/bob-runtime/tests/test_agy_index_probe.py`
