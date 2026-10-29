# Evidence — 前身项目只读盘点（2026-10-28）

- **时间**：2026-10-28 14:21–14:30 +08:00
- **执行者**：Antigravity（对话 `0b4da6f6`），在本机 X1C 上
- **方法**：只读 Git 命令（`status --porcelain -uall`、`branch -a -vv`、`tag`、`remote -v`（已脱敏）、`worktree list --porcelain`、`stash list`、`count-objects -v`、`log origin/main..main`），文件列表，以及代码中字符串模式计数。搬移的 worktree 属主不是当前用户，用单次 `git -c safe.directory=<path> -C <path> status` 读取，未修改全局配置。
- **未做**：没有修改、备份、构建或运行任何东西；没有读取 `.env` 或用户配置的内容；没有访问生产环境。
- **限制**：结果是某一时刻的快照，旧仓库仍有活动开发；运行侧的事实只来自文档。

## 1 旧 App（`<workspace_root>/bob-agent`）

| 项 | 观察 |
|---|---|
| HEAD | `a4ff5fd`（`main`，与 `origin/main` 一致）；比旧 Online CURRENT_STAGE 记录的 `2598428` 多 2 个提交 |
| 工作树 | 干净（0 项） |
| Stash | 2 个：`On diagnostics/android-startup-20261001: temp todo…`；`WIP on main: f78fe30 … insight mode, wechat reader…` |
| 本地分支 | `main`、`main-Bobbik-X1C`、`public-release`（v0.4.0，落后 460）、`mobile-pc-rebuild`、`mobile-pc-recovery-0923`、`diagnostics/android-startup-20261001`（领先 origin 1）、`feature/document-export`、`feature/document-export-Bobbik-X1C` |
| 远端 | `origin` 指向 GitHub `bobbik1984/bob-agent`；另有失效引用 `remotes/private/main`（对应 remote 已不存在） |
| Tags | 19 个，从 v0.2.0 到 v0.9.30，另有 `latest-desktop`、`latest-mobile` |
| Git 对象 | 0 个 pack；11,872 个 loose object；**2 个垃圾文件**（`objects/pack/tmp_pack_*`、`objects/ea/tmp_obj_*`，约 333KB，来自中断的 git 操作） |
| 版本 | `package.json` 中 `bob.agent` 0.9.30；Tauri identifier `bob.agent`；`src-tauri/gen/android` 存在 |
| LICENSE | 文件**不存在**；README 声称 MIT |
| 规模 | `src` 与 `src-tauri/src` 下 196 个源文件，约 11.2 万行 |
| 硬编码 | 13 个文件中出现 35 处 `bobbik`（含 `sync_engine.rs`、`device_trust.rs`、`tunnel.rs`、`crypto.rs`）；20 个文件中有 106 处 IPv4 字面量；1 处 `Assistant/common` 引用（`lib.rs`）；1 处 Windows 绝对路径（`llm.rs`） |
| 凭据 | `keychain.rs` 注释说明 OS Keychain“已停用”；Rust 源码中 `apiKeys` / `api_keys` 出现 77 次；ARCHITECTURE 写 API Key 明文存储（T-303）。远程 Agent 使用 `tauri-plugin-device-vault`。**LLM Key 明文存储高度疑似，待 WP-01 确认** |
| 根目录杂物 | `.env`、`payload.zip`、`website.zip`、`bob-relay.bin`，以及大量 `fix_*.py`、`patch*.py`、`test_*.py`、`*.txt` 日志 |
| 文档 | `docs/PRODUCT_VISION.md`（2026-08-15）、`BOB_EVOLUTION_ROADMAP.md`（Phase 0–10）、`ARCHITECTURE.md`、`DECISIONS.md`（D-007–D-019，止于 8 月）、`REMOTE_HARNESS_INTEGRATION.md`、`contracts/bob_cloud_agent_interchange.yaml`（UDES v1，2026-09） |

## 2 旧 Online（`<workspace_root>/cloud-agent`）

| 项 | 观察 |
|---|---|
| HEAD | `97e3480`（`main`）；**领先 `origin/main` 4 个提交，未推送**：`97e3480`、`b71f818`、`24e67cb`、`dc86a54` |
| 工作树 | 109 项已修改，1948 项未跟踪 |
| 未跟踪分布 | `docs/evidence` 1418；`deploy/` 约 120；`packages/ui` 77（components 18、utils 10、styles 6、locales 2、测试约 20）；`packages/server` 72（`src/runtime` 17、`src/routes` 6、test fixtures 8）；`references/` 约 70；`scripts/` 约 20 |
| 已修改分布 | `packages/server` 32、`packages/ui` 22、`docs/wiki` 8、`docs/evidence` 7、`deploy/vps3` 5，另有 `pnpm-lock.yaml`、`package.json`，以及仓库内的 `docs/BOB_PROJECT_VISION.md`、`BOB_DEVELOPMENT_ROUTE.md`、`BOB_NODE_BOOTSTRAP_CONTRACT.md` |
| Worktree | `codex/r06d-r05e-20261005` 注册在 `<workspace_root>/.worktrees/cloud-agent-r06d-r05e`，**该路径已不存在，被标记为 prunable** |
| Git 对象 | 0 个 pack；7,926 个 loose object |
| 远端与 tag | `origin` 指向 GitHub `bobbik1984/cloud-agent`；tag 只有 `v0.1` |
| 包名 | `antigravity-nexus` 0.1.0，无 license 字段；**LICENSE 文件不存在** |
| 规模 | `packages/*/src` 下 141 个源文件，约 6.1 万行 |
| 硬编码 | 1 处 `bobbik`；7 个文件中有 17 处 IP；9 个文件中有 35 处 vps、contabo、hostinger、x1t 引用（`lab/`、`runtime/orchestrator.ts`） |

## 3 搬移的 worktree（`cloud-agent-r06d-r05e`）

| 项 | 观察 |
|---|---|
| 实际位置 | 工作区规定的外置 worktree 缓存目录（2026-10-25 搬移） |
| `.git` 指向 | `cloud-agent/.git/worktrees/cloud-agent-r06d-r05e` |
| 属主 | `CodexSandboxOffline`（不是当前用户 `xm_bo`），触发 git 的 dubious ownership 保护 |
| 未提交改动 | **共 2707 项**：未跟踪 `docs/evidence` 1394、`packages/ui` 535、`scripts/lib` 140、`packages/server` 134、`deploy/vps3` 8、`docs/wiki` 5、`docs/plans` 4、`.ssot-staging/b4` 3、`.ssot-staging/b4-real` 3；已修改 `docs/evidence` 86、`packages/server` 84、`packages/ui` 37、`docs/wiki` 11、`packages/shared` 6；另有部分 `docs/evidence` 子目录无法打开 |
| 风险 | 在主仓库执行 `git worktree prune` 会删除其管理目录，使这些改动与仓库失去关联。`.ssot-staging/b4*` 可能与 ONL-B4 寻找原始备份有关（未打开） |

## 4 其他

- `<workspace_root>/bob-relay/` 不是 Git 仓库（只有 `src/`、`deploy.ps1`、`package.json` 等文件）。
- 工作区根目录有 `.tmp-cloud-agent-settings-khhk4gvn/`（未查看内容）。
- 运行侧（来自旧 Online CURRENT_STAGE，未独立核验）：生产部署在 Hostinger；`native-session-20261028-r1` 已获批上线（schema 17）；已登录公网验证、真机测试、生产回退均未完成。

## 5 Unknown

旧 App 同步引擎的模型；LLM Key 的实际存储位置；两个仓库的测试数量与 CI 状态；Git 历史中是否有 secrets；`.ssot-staging/b4*` 的内容；Hostinger 的实际状态。
