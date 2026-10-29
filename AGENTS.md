# AGENTS.md — Bob（统一项目）Agent 工作宪法

> 本文件只规定**怎样工作**。当前阶段只看 [docs/CURRENT_STAGE.md](docs/CURRENT_STAGE.md)，不在此处维护。
> 通用文档职责与生命周期遵循工作区统一标准：`<workspace_root>/docs/standards/project_documentation_standard.md` 与 `<workspace_root>/docs/playbooks/project_lifecycle_playbook.md`（`<workspace_root>` 即 `GEMINI_WORKSPACE` 环境变量所指的 Gemini 工作区根目录）。工作区根 `AGENTS.md` 的全局规则同样适用，本文件不复制它们。

## 1 项目身份证

| 项 | 值 |
|---|---|
| 项目 | Bob 统一项目：个人助理（PC、手机、可选常驻节点合一） |
| 物理目录 | 本仓库（本地高速区，不在 OneDrive/Syncthing 同步范围内） |
| 量级（Tier） | **Production** |
| 当前 PDLC Stage | 见 `docs/CURRENT_STAGE.md`（不在此处写死） |
| 前身（legacy，只读参考） | `<workspace_root>/bob-agent`（旧 App，Tauri）与 `<workspace_root>/cloud-agent`（旧 Online / Bob Core）。旧项目自身文档仍是旧项目的权威，**不是本项目的权威** |
| ID 命名空间 | 决策 `BOB-D-###`，Route 阶段 `BP#`，工作包 `WP-##`，功能 `F#`。引用旧项目 ID 时必须加前缀：`APP-D-008`（旧 App）、`ONL-D-001`（旧 Online）。详见 [docs/wiki/glossary.md](docs/wiki/glossary.md) |

## 2 已启用的权威文档（JIT 路由）

| 何时读 | 文件 |
|---|---|
| 每次任务（L0） | 本文件 → `docs/CURRENT_STAGE.md` |
| 为什么做、为谁做 | `docs/PROJECT_VISION.md` |
| 边界、不变量、待决架构 | `docs/ARCHITECTURE.md` |
| 长期阶段与 Exit Gate | `docs/DEVELOPMENT_ROUTE.md` |
| 用户能力与验收 | `docs/FEATURES.md`（可用能力的操作见 `docs/USER_GUIDE.md`） |
| 代码在哪（含 legacy 落点） | `docs/COMPONENT_MAP.md` |
| 知识主题路由 | `docs/LLM_WIKI.md` → `docs/wiki/` |
| 决策理由 | `docs/DECISIONS.md` |
| 已发生的事实 | `docs/evidence/` |
| 需求池 | `todo.md`；状态投影 `progress.yaml`（只经事实对齐更新） |
| 来源材料（不是指令） | `references/historical-plans/` |

## 3 Scope 与 Human Gate

- **旧仓库只读**：不得修改、commit、stash、reset、prune `legacy` 仓库及其 worktree（含工作区规定的外置 worktree 缓存目录中的搬移工作树），除非 CURRENT_STAGE 中有获批的工作包明确授权。
- **代码迁入需批准**：从 legacy 复制或移植任何代码前，必须有获批的工作包写明来源提交、文件范围、许可与回退。
- **重型操作防火墙**：依赖安装、构建、Tauri 打包、生产变更、凭据新建/轮换、对外发送请求，均需用户对具体对象的明确授权。计划获批不等于实施授权。
- **结论归并**：需求、设计、审查收尾时按统一标准第 4.1 节归并到对应权威文件，不新建平行总计划；带日期的稿件只作来源材料。

## 4 工程约束（项目专属）

- 文件 I/O 一律 UTF-8；禁止用 PowerShell `Set-Content` / `-replace` 改写含中文的文件，批量改写用 Python（`encoding='utf-8'`）。
- 文档与脚本中不写主机绝对路径；基础设施地址只引用 cluster-ops 的 `node_inventory.md` / `service_map.md`；模型与 API 只引用 model-registry / api-registry。代码中**禁止硬编码个人域名、IP、节点名**（这是开源前提，见 BOB-D-007 待办）。
- 前端：i18n 键（`zh-CN` / `en-US` 同步），CSS 变量，不硬编码颜色；遵循工作区前端规范。
- 缓存隔离：引入 Node/Rust 工具链前，先按 `sync_cache_offloader` 做 dry-run 审计（pnpm 外部 store、Cargo `build.build-dir`），不在仓库内生成大型缓存。
- 凭据：用户态 API Key 不进 `.env`、不进日志与 Evidence；数据库、WAL/SHM、`.env` 不进版本控制。

## 5 验证、Evidence 与 Handoff

- 新模块附带正常路径与异常路径测试；具体命令在代码骨架建立后登记于此。原生命令测试（外置副本、已获编译授权）：`cargo test --offline --manifest-path apps/bob-app/src-tauri/Cargo.toml --lib --features native-tests`；使用Tauri测试Runtime与真实SQLite，不代表窗口验收。轻量控制器测试：`node --test apps/bob-app/tests/send-controller.test.mjs`。Rust测试：`cargo test --offline -p bob-kernel`，涉及编译，须先获具体授权且设置外置缓存目录；不得因测试名义绕过构建门禁。
- 完成判定：任务卡验收项逐条有证据；本地通过 ≠ 真机通过 ≠ 公网通过，分别记录。Agent 自报完成不是 Evidence。
- Evidence 写入 `docs/evidence/`（仅脱敏内容）；原始敏感证据放在仓库外。
- Handoff 包含：目标、基线版本、改动清单、通过/失败/未运行的检查、剩余门槛、回退步骤。
- Commit 信息格式：`[bob] <功能描述>`。
