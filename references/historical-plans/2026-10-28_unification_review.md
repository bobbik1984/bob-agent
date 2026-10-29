> **来源材料，不是当前指令。** 统一提案的遗漏审查与首轮编制（2026-10-28）。用户随后确认：新目录、目标用户为白领与普通开发者、Tauri Android（iOS 后期）、个人环境优先，见 BOB-D-001 至 BOB-D-004。盘点事实已转入 `docs/evidence/2026-10-28-legacy-inventory.md`；AD-1、AD-2、AD-7 已分别转为 BOB-D-005、BOB-D-006、BOB-D-007（proposed）。

# Bob 统一计划：遗漏审查与首轮编制（步骤 A–E 起草）

> 日期：2026-10-28　状态：**PROPOSED，停在审阅门**
> 输入：`BOB_UNIFIED_DEVELOPMENT_PLAN_2026-10-28_PROPOSED.md`（下称“提案”）、Drive 上的 Vision、Route、Architecture、CURRENT_STAGE、DECISIONS，以及 bob-agent 仓库原始文档、两个仓库的只读 Git 和代码扫描
> 观察时间：2026-10-28 14:21–14:30 +08:00，本机 X1C。**没有改动任何仓库、Drive 文档或运行环境，也没有做任何备份。**
> 文档性质：这是一份带日期的来源材料（统一标准第 2.3 节）。采纳的结论应归并到 VISION、ROUTE、ARCHITECTURE、DECISIONS，本文不另立施工队列。

---

## 0 一页结论

1. **方向正确，但提案漏掉了最关键的事实：现在有两个 Work Kernel，不只是“App 加上 Core”。** bob-agent 自带一套成熟的 Persistent Work Core（Rust 和 SQLite，加 Event Journal，含 Project、Goal、Milestone、Task、Decision、Artifact、Evidence、Risk、Change、Commitment）。Online Core 则持有 Project、Task、Execution、Artifact、Verification、Review。两边**实体同名但语义不同**，提案只笼统地称之为 “Local Work”。统一的核心问题是**两个内核如何分工、如何晋升和交接**，不是“仓库怎么放”。
2. **还有第三条状态通道没被提及**：bob-agent 的 PC↔Android 同步引擎（`sync_engine.rs` 530KB，`device_trust.rs` 324KB），走 LAN 和 VPS3 relay（`relay.bobbik.org`）。它直接影响“无服务器也能用”和“活动数据库不同步”这两条原则能不能成立。
3. **“开源 BYOK、不必架服务器”与“手机操作 PC 文件”之间存在一个网络拓扑缺口**：手机找到 PC 需要汇合点（relay、Core 或 P2P overlay）。开源用户的默认方案是什么，需要单独决策（AD-2）。
4. **资产风险比提案写的更紧迫**：cloud-agent 有 4 个提交未推送、2057 项未提交改动（其中约 150 个未跟踪的源码和测试文件）。已搬移的 worktree 里还有 **2707 项未提交改动**，提案却描述为“可 prune”。另外，两个仓库都在 OneDrive 上，全是 loose objects，bob-agent 还残留了中断的 tmp pack。建议把“保护基线审批”提到最前面。
5. **开源前置条件尚未具备**：README 写 MIT，但两个仓库都没有 LICENSE 文件；代码里硬编码了个人基础设施（bob-agent 有 13 个文件出现 `bobbik`，20 个文件里有 106 处 IP 字面量）；OS Keychain 已停用，LLM Key 高度疑似明文存储；cloud-agent 的包名是 `antigravity-nexus`，有商标风险。

**建议**：先批准“保护基线”（§6），再由你裁决 §5 的 7 个架构决策（重点是 AD-1 内核拓扑、AD-2 连接、AD-3 手机形态）。之后才进入步骤 F 的最小切片。

---

## 1 对你大想法的理解（复述，请纠正）

- Bob 是**一个**用户自己的、开源、BYOK 的个人助理。手机、PC（bob-agent）、Online（cloud-agent）是它的不同使用形态，不是三个产品。
- 价值：容易开始，做到一半能接着做，成果和决定找得回来，能接入不同设备、模型和 Harness。
- 不搞全部重写，也不冻结技术债。每个模块都按 retain、adapt、replace、merge、retire 逐项裁决，理由要有证据。
- Online 是可选的常驻增强，不是首次使用的门槛。App 和 Online 各自都能独立使用。

---

## 2 遗漏与盲点（按影响排序）

| # | 遗漏 | 证据 | 为什么重要 | 建议落点 |
|---|---|---|---|---|
| G1 | **两个 Work Kernel 并存，实体同名不同义** | bob-agent `docs/BOB_EVOLUTION_ROADMAP.md` Phase 1，`DECISIONS.md` D-008 / D-018；Online `DECISIONS.md` D-001 / D-004 | 不先界定，任何“统一 UI”都会退化成双写，或靠名称合并 | 步骤 C 核心问题，D 的 AD-1 |
| G2 | **PC↔Android 同步引擎和 relay** 没进计划 | `bob-agent/LLM_WIKI.md` §11；`src-tauri/src/sync_engine.rs`、`device_trust.rs`；relay 部署在 VPS3 | 这已经是“一个 Bob 跨设备”的现有实现，与提案 §4.2“不同步活动 DB”的关系要说清楚；它本身也依赖服务器 | AD-6 |
| G3 | **手机到 PC 的连接拓扑**没定义 | 提案旅程 1 要求“不必架服务器”，旅程 2 要求“手机读 PC 文档” | 第一个切片能否成立取决于它；开源默认不能依赖 `bobbik.org` | AD-2，并在 C 中补威胁建模 |
| G4 | **Kernel 技术栈分裂** | App 是 Rust（Tauri v2，原则写着“PC 零外部运行时，Android 不加重依赖”）；Online 是 Node/TS（pnpm monorepo） | “把 Core 嵌进 App”和“共享领域代码”这两条路，都会撞上语言和体积约束 | AD-1、AD-4 |
| G5 | **手机形态冲突** | Online Vision §8 说 “Web/PWA 默认，Native Mobile 需证明不足”；App 已有 Tauri Android（Keystore、Biometric、APK 发布） | 提案只写了“手机 Bob”，没选形态；iOS 完全没提 | AD-3 |
| G6 | **两边的愿景并不相同** | App 是“让复杂工作不断线”，面向普通白领，主打日程、待办、笔记、Today；Online 是 Universal Work Access，面向 Harness 和开发工作 | 提案已承认“未读齐 App 原始愿景”。现已读到，差异见 §3 | 步骤 A |
| G7 | **编号、ID、命名空间冲突** | 两边 DECISIONS 都有 D-007…D-019，含义不同；Phase 编号有 App 的 0–10、Online 的 0–6 和提案的 A–J；“Bob Core”在两边各指一物；同一个项目叫 cloud-agent、bob-online、Nexus、online-agent，包名还是 `antigravity-nexus` | 文档或决策一合并，就会出现静默错引 | 新增前置 A0：术语表和命名空间 |
| G8 | **资产保护的实际风险** | 见 §4 B 部分 | 提案说复杂迁移前要做保护，但实际风险已经存在，与迁移无关 | 把 §6 提到最前 |
| G9 | **个人助理领域能力缺席** | App 有日程、待办、Markdown 笔记和 Source、Dream/SOUL 记忆、微信网关、Web Drop E2EE、MCP 客户端、知识图谱、Doctor、Daily Brief、PPTX 导出、本地 llama sidecar | 旅程 1 的“基本助理”在提案里是抽象的。这些模块需要逐个裁决去留，并判断是否适合开源（微信网关有 ToS 和法律风险） | 步骤 D 的组件去留表 |
| G10 | **旧契约并存** | `bob-agent/docs/contracts/bob_cloud_agent_interchange.yaml`（2026-09，基于文件交换的 UDES）与 `core-client-contract-v1`（2026-10-27）同时存在 | 新 Agent 可能照着旧契约实现 | 标记 superseded（需批准） |
| G11 | **文档和决策漂移** | Online D-005 仍写 VPS3 Core，生产实际已在 Hostinger；CURRENT_STAGE 记录 App 基线为 `2598428`，实际 HEAD 是 `a4ff5fd`；Online 的 README、FEATURES、USER_GUIDE 停在 9 月 19 日；App DECISIONS 停在 8 月；cloud-agent 仓库里有 Drive 文档的副本（`docs/BOB_PROJECT_VISION.md` 等，状态为已修改） | 统一后的文档 SSOT 放在哪里还没定 | AD-7 |
| G12 | **缺威胁模型和安全审查门** | 旅程 2 需要远程读取个人 PC 文件；Architecture §7 要求 Agent 使用默认拒绝的环境 | PC 同时充当 Work Host，攻击面最大 | 步骤 C 增设安全门，F 之前必须通过 |
| G13 | **与进行中施工的协调** | MWNP-01、R06-D、RH 仍在同两个仓库里推进 | 盘点结果很快会过期，需要冻结观察点，或在执行前重新核对 | 加入执行规则 |
| G14 | 其他较小缺口 | iOS；没有服务器时的推送；BYOK 费用可见性；两套 UI 的 i18n 对齐；已安装用户（v0.9.30，`%APPDATA%\bob.agent\bob.db`）的升级迁移；遥测和崩溃上报的隐私；`project_registry.yaml` 里 `bob_mobile`、`bob_notebook` 等 idea 条目的归并；AGY / Codex 订阅在开源场景下的许可 | — | 分别放入 D 和 J |

---

## 3 步骤 A 起草：两边原始愿景的差异

| 维度 | bob-agent（`docs/PRODUCT_VISION.md`，2026-08-15） | Online（`BOB_PROJECT_VISION.md`，2026-10-06） | 冲突或张力 | 待你裁决 |
|---|---|---|---|---|
| 一句话 | “Bob 让复杂工作不断线”，个人工作编排层 | Personal Assistant 加 Universal Work Access Layer | 互补：前者偏知识、日程、决定，后者偏执行和治理 | 统一后的一句话定位 |
| 目标用户 | 不想学 AI 配置的普通白领 | 有多台设备、多个 Harness 的用户（实际是开发者本人） | **主用户不同**，首屏和默认能力会因此不同 | 主角是谁，次角是谁 |
| 核心对象 | Project、Goal、Milestone、Task、Decision、Risk、Change、Commitment，加上 Note/Source（Markdown 为权威） | Topic、Spark、Project、Task、Execution、Evidence、Review、Decision | Task 和 Decision 同名不同义；Goal 对 Execution 的对应关系不明 | AD-1 |
| 权威位置 | 本机 SQLite 和 Markdown（D-008） | Bob Core State Store 是唯一记录系统（D-001、宪法第 10 条） | “Kernel owns truth”到底指哪个 Kernel | AD-1 |
| 跨设备 | “PC 与手机共享一致状态”，通过 relay 和 LAN 同步 | Web/PWA 访问同一个 Core，禁止多节点 SQLite 双向同步 | 同步与集中访问是两种范式 | AD-6 |
| 安装 | 绿色 15MB，零外部运行时，填 Key 即用 | 单租户自部署，需要干净环境安装 | 正好对应提案的 standalone 和 connected 两种模式 | 确认这个映射 |
| 智能路由 | Complexity Router（Direct、Deep、Advanced）已实现 | 自动路由是渐进增强，默认手动 | 不冲突，但需要决定是否沿用 App 的 Router | D 去留表 |
| 手机 | Android 原生（Tauri） | Web/PWA 默认 | 冲突 | AD-3 |
| 不做什么 | 不追求在编程上超过 Codex | 不做 IDE，不做远程桌面 | 一致 | — |

**建议的统一表述（草案）**：Bob 是用户自己的个人助理，**默认在本机完整可用**（App 内核负责助理域和本地事项）；需要常驻执行、跨主机治理或多端访问时，**接入自部署的 Online Work Kernel**。每件事项在任一时刻只有一个“家”，在两个家之间转移时必须显式晋升或交接，并保留来源。

---

## 4 步骤 B 起草：现场事实（只读，2026-10-28 14:21–14:30）

### A 仓库

| 项 | bob-agent | cloud-agent |
|---|---|---|
| 路径 | `D:/OneDrive/.../Gemini/bob-agent` | `D:/OneDrive/.../Gemini/cloud-agent` |
| Remote | `github.com/bobbik1984/bob-agent`；另有失效的 `remotes/private/main` 引用（remote 本身已不存在） | `github.com/bobbik1984/cloud-agent` |
| HEAD | `a4ff5fd`（`main`，与 origin 一致）。**比 CURRENT_STAGE 记录的 `2598428` 多 2 个提交**（P2P 链路、Work UI 重设计） | `97e3480`（`main`，**领先 origin 4 个提交，未推送**） |
| 工作树 | 干净 | **109 项已修改，1948 项未跟踪** |
| 未跟踪重点 | — | `packages/ui/src/components`（18）、`server/src/runtime`（17）、`ui/src/utils`（10）、`server/src/routes`（6）、样式和 locales、约 25 个测试；`deploy/` 下约 120 个发布脚本；1418 个 evidence 文件 |
| Stash | 2 个（其中一个是 “WIP insight mode, wechat reader…”） | 0 |
| 本地分支 | 8 个，含 `public-release`（v0.4.0，落后 460 个提交）、`mobile-pc-rebuild`、`diagnostics/android-startup-20261001`（领先 origin 1） | `main`、`chore/wp1-06-fixture`、`codex/r06d-r05e-20261005` |
| Tags | 19 个（v0.2.0 到 v0.9.30，另有 latest-desktop、latest-mobile） | `v0.1` |
| Git 对象 | **0 个 pack，11,872 个 loose object，另有 2 个垃圾 tmp pack**（中断的 git 操作） | 0 个 pack，7,926 个 loose object |
| LICENSE | **没有**（README 声称 MIT） | **没有** |
| 包名和版本 | `bob.agent` 0.9.30，identifier `bob.agent` | `antigravity-nexus` 0.1.0 |
| 代码规模（只计扫描到的 src） | 196 个文件，约 11.2 万行（Rust 加 Vue/JS） | 141 个文件，约 6.1 万行（TS/Vue） |
| 个人基础设施硬编码 | 13 个文件出现 `bobbik`（含 sync_engine、device_trust、tunnel、crypto）；20 个文件有 106 处 IP 字面量；1 处 `Assistant/common` 引用 | 1 处 `bobbik`；17 处 IP；9 个文件有 35 处 vps、contabo、x1t 引用（lab、orchestrator） |
| 凭据存储 | `keychain.rs` 标注“已停用”，LLM Key **高度疑似明文**存在 config 中（ARCHITECTURE 写 T-303 待迁移；未读取内容）。远程 Agent 使用独立的 `remote-agent-vault.db`（AES-GCM，加 DPAPI 或 Android Keystore） | 原则是 Core 只存 `credential_ref`（宪法 §7.4）；实际情况待核 |
| 根目录杂物 | `.env`、`payload.zip`、`website.zip`、`bob-relay.bin`，以及大量 `fix_*.py`、`patch*.py`、`test_*.py` | 一个 `.tmp-cloud-agent-settings-*` 目录在工作区根（未查看内容） |

### B 已搬移的 worktree（关系到 B4 HOLD）

- 注册路径 `Gemini/.worktrees/cloud-agent-r06d-r05e` 已不存在，git 标记为 **prunable**。
- 实际位置 `D:\Code\Archive_Cache\.worktrees\cloud-agent-r06d-r05e`，属主是 **`CodexSandboxOffline`**（不是当前用户）。我用单次命令参数 `-c safe.directory` 只读查看，没有改全局配置。
- **其中有 2707 项未提交改动**：未跟踪的有 `packages/ui` 535、`packages/server` 134、`scripts/lib` 140、`docs/evidence` 1394；已修改的有 `server` 84、`ui` 37、`shared` 6。另有 **`.ssot-staging/b4`、`.ssot-staging/b4-real`**，可能是 B4 原始证据的线索，没有打开。
- ⚠️ 如果在主仓库执行 `git worktree prune`，这个工作树会与仓库失去关联，这些未提交成果就成了孤立文件。**提案 §2 中“可 prune”的描述应改为“禁止 prune，待保护”。**

### C 运行侧（只摘自文档，未独立核验）

- 生产 Online 在 **Hostinger**；`native-session-20261028-r1` 已获批上线（SHA-256 `d4dc08d7…`，schema 17）。已登录公网验证、真机测试、生产回退都还没做（CURRENT_STAGE 第 58–66 行）。
- App 正式版 0.9.30。Core 只读适配已在本地实现，尚未发布。

---

## 5 步骤 C/D 起草：全局冲突和待决架构选项

### 5.1 冲突矩阵（初稿）

| 维度 | 文档宣称 | 代码或运行实际 | 冲突 |
|---|---|---|---|
| 工作真相 | Online D-001：Core 是唯一权威 | App 也有完整 Work Core，D-008 也自称权威 | **双重权威**（领域不同，名称重叠） |
| 跨端状态 | Online：禁止 SQLite 双向同步 | App：PC↔Android 记录级同步，经 relay | 范式冲突，需要明确适用范围 |
| 主控节点 | D-005：VPS3 | 实际是 Hostinger；relay 仍在 VPS3 | 决策过期 |
| 手机 | Web/PWA 优先 | Tauri Android 已上线 | 形态冲突 |
| 凭据 | Edge-first，使用 vault | App 的 LLM Key 疑似明文 | 安全缺口 |
| 契约 | core-client-contract-v1 | 旧 UDES yaml 仍在 docs | 双契约 |
| 开源 | MIT | 没有 LICENSE；私有设施硬编码；有 `public-release` 双仓流程（`OPEN_SOURCE_WORKFLOW.md`，无历史快照式发布） | 无法直接发布 |

### 5.2 待你裁决的 7 个架构决策（每项给出推荐和替代方案，均为 proposed）

**AD-1 内核拓扑（最关键）**

| 选项 | 说明 | 代价 |
|---|---|---|
| ① **双内核分域（推荐起点）** | App Personal Kernel（Rust）负责对话、记忆、日程待办、笔记、本地事项和本地 Goal；Online Work Kernel（TS）负责常驻执行、Harness、Verification、Review。每个事项有唯一的 `home`，晋升就是显式转移所有权，附带 provenance | 需要定义晋升和交接契约，以及 ID 映射 |
| ② 单内核 TS，App 内嵌 sidecar | 复用 Online Core | 违背“零外部运行时”，Android 基本不可行 |
| ③ 单内核 Rust，Online 改写或包装 | 一套领域代码 | 等于重写 Online，风险最高 |
| ④ 契约优先（与 ① 叠加） | 领域实体和状态机用语言中立的 schema（JSON Schema 或 OpenAPI）定义，Rust 和 TS 各自生成类型，并共用一致性测试 | 前期投入是 schema 和测试集 |

推荐 **① 加 ④**：同名实体逐个对齐语义（Task 对 Task、Decision 对 Decision、Artifact 对 Artifact），或者改名区分（例如 App 的 Task 改为 Todo 或 LocalTask）。

**AD-2 跨设备连接**：可选 LAN 直连、用户自部署 relay、Online Core 作为汇合点、P2P overlay（你已在用 Headscale/Tailscale）、官方托管 relay。推荐抽象成 Connection Provider，**开源默认不依赖任何托管服务**，至少两种真实实现（LAN 加自部署 relay，或 LAN 加 Core）。

**AD-3 手机形态**：可选保留 Tauri Android 并让 Online 用 PWA；只用 PWA；Tauri 同时覆盖 Android 和 iOS。需要确认 iOS 是否在范围内。

**AD-4 UI 收敛方式**：可选抽取共享设计 token 和组件包，两个 Vue 应用共同消费；或者单一 UI 代码库加平台适配（`BobWorkClient` / `PlatformServices`，即 Route Phase 4 的方案）。两边都是 Vue 3，这两条路都走得通。

**AD-5 仓库组织**：近期是多仓库加一个共享契约包（或 schema 目录），monorepo 在 C/D 通过后再评估。提案的立场是对的，补充一条：共享 schema 需要一个中立的归宿。

**AD-6 PC↔手机同步引擎的去留**：可选保留为 App 内部的“同一用户多设备”同步，仅限 App Personal Kernel 的数据；或者由 Online Core 替代；或者两者并存但明确划分领域。现在不能把它当作 Online Work 的同步通道。

**AD-7 统一后文档 SSOT 的位置**：可选 Drive `bob-online/docs` 升级为 `bob/docs`；放进某个仓库；或者新建一个中立的产品文档目录。同时要定 ID 命名空间（例如 `APP-D-008`、`ONL-D-008`），以及旧 Phase 编号的映射规则。

### 5.3 步骤 D 去留表（框架，待 C 通过后填写）

| 组件 | 来源 | 初判 | 依据或待证 |
|---|---|---|---|
| Design System、Vue 组件、i18n | App | retain（D-003 已定） | — |
| Work Core（Project/Goal/Task…） | App | adapt（对齐语义或改名） | AD-1 |
| Complexity Router、Goal Runtime | App | retain 或 adapt | 与 Online Resolver 的关系 |
| sync_engine 和 device_trust | App | 待定 | AD-6 |
| relay（VPS3） | App | adapt（去个人化，可自部署） | AD-2 |
| 日程、笔记、Dream、KG、MCP | App | retain（助理域） | 开源适配性 |
| 微信网关 | App | 待定（可能移出开源核心） | ToS 和法律 |
| Core API、Task/Execution/Verification/Review | Online | retain | D-001 |
| Online WebUI（Studio、NodeWork） | Online | merge 进 Canonical UI | D-003 |
| Harness 适配（DSH、AGY、Codex、pi） | Online | retain，逐个列能力矩阵 | — |
| UDES interchange yaml | App | retire（标 superseded） | 已被 contract-v1 取代 |

---

## 6 可恢复基线提案（**只是提案，未执行，需逐项批准**）

| # | 对象 | 方法（草案） | 目的地 | 排除项 | 验证 |
|---|---|---|---|---|---|
| P1 | cloud-agent 全部 refs | `git bundle create --all`（只读，不改仓库） | `D:\Code\Backups\bob-unify-<date>\`（非 OneDrive） | — | 在临时目录 clone 该 bundle，比对 refs |
| P2 | cloud-agent 工作树改动 | `git diff --binary HEAD` 加未跟踪文件 allowlist 打包，附 manifest 和 SHA-256 | 同上 | `.env*`、DB、密钥、`node_modules`，evidence 原始敏感件需先审 | 逐文件核对 hash |
| P3 | 搬移的 worktree（2707 项） | 同 P2；属主是 sandbox 用户，读取方式需批准 | 同上 | 同上；`.ssot-staging/b4*` 先列清单再决定 | 同上 |
| P4 | bob-agent refs 和 stash | `git bundle --all`，stash 导出为 patch | 同上 | `.env`、`data/` | clone 验证 |
| P5 | App 运行数据 | 待定：`%APPDATA%\bob.agent\bob.db` 的一致性快照（SQLite backup API） | 加密存放 | Key 和 Cookie vault 不纳入 | 隔离环境读回 |
| P6 | Hostinger Core | 已有发布前备份（manifest 记录在 CURRENT_STAGE），只核对，不新建 | — | — | — |

附带建议（同样需要批准）：仓库搬离 OneDrive，或至少执行 `git gc` 打包。修复中断的 tmp pack 前，必须先完成 P1 和 P4。

---

## 7 对提案路线 A–J 的补丁建议

- **新增 A0：术语和命名空间**（对应 G7）。统一产品名、组件名，规定 ID 前缀和三套 Phase 编号的映射，作为 A 的前置。
- **A**：采用 §3 的差异表；补上“主用户”裁决。
- **B**：采用 §4；加入“观察冻结点”，F 之前重新核对差异（G13）。
- **C**：增设两个必过门：**内核分域门**（AD-1）和**连接与威胁模型门**（AD-2、G12）。
- **D**：去留表采用 §5.3 的框架；增加“同名实体语义对齐表”。
- **E**：加入 AD-7（文档 SSOT），处理 ID 冲突和旧契约退役。
- **F**：最小切片的前置改为：AD-1、AD-2 已批准，§6 的 P1–P4 已验证，威胁模型已审。
- **J**：补充 LICENSE 落地、依赖许可证审计、去个人化（域名、IP、节点名改为配置）、包名和商标（`antigravity-nexus`）、复用并修订已有的 `OPEN_SOURCE_WORKFLOW.md` 双仓流程、已安装用户的升级迁移。
- **与既有路线的映射**（避免平行队列）：A–E 属于 PDLC Stage 2，产出归并到 ROUTE、ARCHITECTURE、DECISIONS；F 对应 Online Route Phase 4（Canonical UI）和 App RH 系列；H 对应 R07 / RH-04；J 对应 Phase 6。

---

## 8 需要你决定的事项（审阅门）

1. **保护基线**：是否批准 §6 的 P1–P4（只生成 bundle 和 patch，不改仓库），以及目的地 `D:\Code\Backups\`。
2. **主用户**：统一后的 Bob 首先服务“普通白领助理”，还是“多设备开发者”。
3. **AD-1**：是否以“双内核分域加契约优先”作为 C/D 的起点假设。
4. **AD-2**：开源默认连接方案是 LAN 加自部署 relay，还是 LAN 加 Core，还是 Headscale/Tailscale。
5. **AD-3**：手机是否继续走 Tauri Android；iOS 是否在范围内。
6. **AD-7**：统一产品的文档放在哪里（建议 Drive 统一到一个目录，仓库只保留 JIT 指针）。
7. **报告落盘**：本文是否复制到 `H:\My Drive\bob-online\docs\Current Project Docs\`（作为带日期的来源材料），以及是否把提案中“可 prune”的表述更正。

---

## 附：未知项（unknown）

- App 的 sync_engine 同步哪些表、冲突策略是什么、哪一端为主（没有读源码细节）。
- LLM Key 是否确实明文（只看了代码结构，没读 config 内容）。
- Hostinger 生产的实际状态（只看了文档，没登录）。
- `.ssot-staging/b4*` 的内容是否就是 B4 的原始 Core 备份。
- 两个仓库的测试数量和 CI 状态（没有统计或运行）。
- 仓库的 Git 历史里是否出现过 secrets（需要专门扫描，未执行）。
