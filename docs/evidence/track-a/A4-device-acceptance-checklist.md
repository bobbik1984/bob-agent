# Track A A4 / P6-DEVICE 双端真机安全与业务协同验收清单及记录模板

- **编制日期**：2026-09-25
- **当前状态**：**准备就绪，尚未执行 (PREPARED / UNEXECUTED / LOCKED)**
- **前置依赖**：A1 (恢复状态机与取消闭环)、A2 (SEC-01 可信身份与持钥配对)、A3 (SEC-02/03 传输脱敏、回执契约与防虚报) 本地复核全部通过。
- **任务目标**：本文件供用户与审查者审阅、校准并在后续具备条件时作为真机验收的唯一执行指南。**自动化测试全绿绝不代替真实物理硬件表现**；严禁将“准备就绪”误写为“真机已通过”。在用户协同完成双端实体设备测试并最终签字前，Gate A4 严格保持 **LOCKED**。

---

## 一、验收基线、测试资产与环境定义

### 1.1 候选构建版本标识 (Candidate Version Identification)
所有测试记录必须精确绑定以下受控构建标识，严禁在未标定版本的散装环境执行：

| 端别 | 候选版本命名规范 | 构建 Commit Hash | 产物形态 | 当前就绪状态 |
|:---|:---|:---|:---|:---|
| **PC 生产候选端** | `bob-v0.9.6-candidate-pc` | `ebb9b6241b12b5963b516fa8f60da308eb0141fc` | `dist-release/bob-v0.9.6-installer.exe` (43,704,320 字节)<br>SHA256: `069666992FEEA45A06C8CF19655966FA4D447189966D929589D092629C4EA2B3`<br>便携版: `dist-release/bob-v0.9.6-portable.zip` (33,503,394 字节)<br>SHA256: `6E4E8A6EE2B95ED46D1A5FD7D1800AC429971DE003565314CB9F959552058D60` | **已完成官方 CI 构建并下载就绪** (经官方 `windows.yml` 定制安装器流水线构建，具备无感自愈与防文件占用升级能力) |
| **PC 专用诊断端** | `bob-v0.9.6-diagnostic-pc` | `86ce393ed2611dd6c588b4e8b780c85c8da95f53` | `dist-release/bob-v0.9.6-diagnostic-pc.exe` (71,006,720 字节)<br>SHA256: `25FBAEC87F6B55B6455DAEB3BB45599D9ECA15B4640FEC473B2CCBC8E5066ABA` | **已本地生成就绪** (包含 `fault-injection` 特性门禁与启动缺参 Fail-Closed 防护，专供 Case 07 测试) |
| **Android 移动端** | `bob-v0.9.6-candidate-android` | `86ce393ed2611dd6c588b4e8b780c85c8da95f53` | `dist-release/bob-v0.9.6-signed.apk` (92,799,330 字节)<br>SHA256: `9F9C29477E4342D8203081ABC77C9C37D385AA8F72521D0AE3779064E7447835` | **已完成官方 CI 构建并下载就绪** (经 `android.yml` 在 Ubuntu 云端完成 NDK 编译、4KB 对齐与签名) |
| **协议版本** | `SYNC_PROTOCOL_VERSION: "0.9.6-sec01"` | 契约对齐 | JSON / WebSocket / Ed25519 PoP | 源码已冻结 |

### 1.2 专用测试工作区与数据库隔离防护方案 (Asset & DB Isolation SOP)
为了彻底杜绝破坏用户日常文件、真实项目或生产主数据库：

#### 1.2.1 隔离边界辨析与主库风险控制
- **工作区目录隔离的局限性**：创建并使用专用受限目录 `<app_data>/test_workspace/` 仅能限制磁盘上被读写的基准文本文件，**绝不能隔离应用进程共享的 SQLite 主数据库 (`bob.db`)**。
- **主数据库共享风险**：依据 [`src-tauri/src/lib.rs:78-102`](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/lib.rs#L78-L102)，Bob 桌面端运行时默认通过 `dirs::data_dir().join("bob.agent")` 解析出数据目录（Windows 环境下解析为 `%APPDATA%\bob.agent\bob.db`）。测试中的所有核心操作——包括配对会话 (`pairing_invitations`, `authenticated_sessions`, `trusted_devices`)、变更提案与工作流 (`staged_changes`, `work_objects`, `work_events`)、以及写入恢复预写日志 (`staged_write_recovery`)——均直接持久化在该单体数据库中。若在包含日常真实数据的生产库上进行破坏性写入或故障模拟，极易污染真实会话、产生脏数据孤岛、或使系统进入恢复阻塞的降级状态 (Fail-Closed)。
- **隔离铁律**：严禁直接在未经备份的日常生产主库上执行破坏性、写入失败或异常崩溃类测试。必须严格执行下列具备一致性校验的停机冷备份与归档恢复规程。

#### 1.2.2 停机一致性冷备份与归档恢复标准操作规程 (Stopped Consistent Cold Backup & Restore SOP)
在启动任何破坏性写入、候选故障注入或多端配对测试前，必须执行停机冷备份：

1. **执行前环境条件检查 (Pre-Execution Checks)**：
   - **确认实际解析的数据目录与主数据库物理路径**：
     - 执行 PowerShell：`Test-Path "$env:APPDATA\bob.agent\bob.db"`，确认物理文件存在；
     - 列出当前数据库相关文件组：`Get-ChildItem "$env:APPDATA\bob.agent\bob.db*"`。
   - **使用经验证可用的 SQLite 完整性检查工具（执行前硬性条件）**：
     - **核验工具支持方案与首选基线**：
       - **首选基线方案 (本机 Python 安全只读核验器)**：本机 ThinkPad X1C 具备经验证的 Python 运行时（Python 3.13.7 内置 SQLite 3.50.4），通过专用脚本 [`scripts/verify_db_integrity.py`](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/scripts/verify_db_integrity.py) 或标准只读命令执行核验。该脚本已经过双重防假验证（路径缺失拦截、非库文件拦截、真机零接触）；
       - **备选方案 (官方 CLI 工具，必须严格包装)**：若宿主机 PATH 具备官方 `sqlite3` CLI 工具（运行 `sqlite3 -version`），**严禁直接执行未经防护的裸命令**，必须使用带有物理文件存在与非空检查、`-readonly` 标志及严格 `'ok'` 判等的 PowerShell 安全包装；
     - **防假通过硬铁律 (Anti-False-Positive Strict Rules)**：
       - **严禁无防备的空库新建与裸命令**：Python `sqlite3.connect(path)` 与官方 CLI `sqlite3 <path> ...` 在路径错误或目标文件缺失时，均会默认在磁盘上静默新建 0 字节空数据库，导致 `PRAGMA integrity_check;` 虚假返回 `ok`，构成致命的假通过误判。**绝不允许使用未经防护的裸命令作为验收放行依据**；
       - **三重防假判定门槛（对所有工具统一强制生效）**：
         1. **目标物理文件存在与非空前验**：必须先核实目标快照文件物理存在且大小大于 0 字节（Python: `assert p.is_file() and p.stat().st_size > 0`；CLI PowerShell: `Test-Path -LiteralPath $target -PathType Leaf` 且 `(Get-Item -LiteralPath $target).Length -gt 0`）；
         2. **只读模式打开 (Read-Only Mode)**：必须以只读模式连接（Python: `sqlite3.connect(f"{p.as_uri()}?mode=ro", uri=True)`；CLI: `sqlite3 -readonly "$target" ...`），在此模式下若路径错误或文件缺失，SQLite 严禁创建新文件，并直接抛出报错中断；
         3. **严格单一 `'ok'` 判等**：查询 `PRAGMA integrity_check;` 获取的输出必须**严格且仅等于单一文本 `'ok'`**（Python: `assert rows == [('ok',)]`；CLI: 输出单行严格匹配 `'ok'`）；若返回损坏报告行、空结果或抛出异常，立即报错阻断并以非 0 状态码退出；
     - **操作红线**：严禁在未获明确授权下擅自安装第三方工具，严禁对真实生产数据库盲目运行探针或修改命令；
     - **硬门槛铁律**：哈希值与文件物理大小仅能核验快照复制传输一致性（防传输损毁），**绝对不能代替 SQLite 内部的 `PRAGMA integrity_check;`，无法证明数据库内容结构健康**。若执行前检查发现缺少经验证可用的 SQLite 完整性检查工具，冷备份文件可先予保留（仅作为防损记录），但**数据库健康验收与恢复后的放行必须强制标记为“未验证／暂停 (UNVERIFIED / PAUSED)”，绝对不得宣称已通过，绝对不得以此放行客户端投入使用**。

2. **前置停机与 WAL 状态确认 (Prerequisite: Full Shutdown & WAL State)**：
   - 必须先完全终止 Bob 桌面客户端进程（PowerShell: `Get-Process "bob-agent", "bob" -ErrorAction SilentlyContinue | Stop-Process`），确保无进程占用且文件句柄彻底释放。
   - 依据 [`src-tauri/src/db.rs:172`](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/db.rs#L172)，SQLite 运行在 `PRAGMA journal_mode=WAL;` 模式。
   - 检查 `%APPDATA%\bob.agent\` 目录下是否存在 `bob.db-wal` 及其大小：
     - **干净状态 (Clean Baseline)**：正常停机时 SQLite 自动执行 checkpoint，`bob.db-wal` 通常自动收敛或为 0 字节。此时 `bob.db` 为完整自包含的一致性快照；
     - **未收敛状态 (Active WAL)**：若 `bob.db-wal` 存在且非空（如前次异常退出导致），此时**绝对不能仅备份 `bob.db`**，必须将 `bob.db`、`bob.db-wal`、`bob.db-shm` 视作不可分割的三元组快照整体。

3. **一致性物理冷备份操作 (Atomic Snapshot Isolation)**：
   - 为避免文件名污染与混淆，**严禁在主数据目录下混放散落的备份文件**；必须创建独立的备份快照专属子目录：
     `$backup_dir = "$env:APPDATA\bob.agent\backups\snapshot_YYYYMMDD_HHMMSS"`
     `New-Item -ItemType Directory -Path $backup_dir -Force`
   - 将当前主库及其伴随文件完整复制到专属备份目录中：
     - 若仅有 `bob.db`（干净状态）：复制 `bob.db` 至 `$backup_dir\`；
     - 若存在 `bob.db-wal` 或 `bob.db-shm`：将 `bob.db`、`bob.db-wal`、`bob.db-shm` 全部完整复制至该目录，形成同时间戳快照集合。
   - **备份先验完整性核验与硬门槛 (Pre-Restore Verification & Hard Gate)**：
     - 在将备份快照允许用于后续恢复前，**必须先验证备份文件组本身物理可读且内部结构完整健康**；
     - 计算并记录快照目录下各文件的 SHA-256 哈希值与字节大小（记录物理传输一致性）；
     - **健康核验门槛**：必须使用经验证的工具执行数据库完整性检查，且必须明确返回 `ok`：
       - *首选基线方案 (Python 安全只读核验器 - 经双重实测基线)*：
         ```powershell
         python scripts/verify_db_integrity.py "$backup_dir\bob.db"
         ```
         或等价单行安全命令：
         ```powershell
         python -c "import sys, sqlite3, pathlib; p = pathlib.Path(sys.argv[1]).resolve(); assert p.is_file() and p.stat().st_size > 0, f'Database missing or empty: {p}'; con = sqlite3.connect(f'{p.as_uri()}?mode=ro', uri=True); rows = con.execute('PRAGMA integrity_check;').fetchall(); con.close(); assert rows == [('ok',)], f'Integrity check failed: {rows}'; print('ok')" "$backup_dir\bob.db"
         ```
       - *备选方案 (官方 CLI 工具 - 必须严格包装，严禁裸跑)*：
         ```powershell
         $target = "$backup_dir\bob.db"
         if (-not (Test-Path -LiteralPath $target -PathType Leaf) -or (Get-Item -LiteralPath $target).Length -le 0) { throw "Database missing or empty: $target" }
         $res = & sqlite3 -readonly $target "PRAGMA integrity_check;"
         if ($res -ne "ok") { throw "Integrity check failed: $res" }
         "ok"
         ```
         *(注：严禁直接运行 `sqlite3 "$backup_dir\bob.db" ...` 裸命令，否则路径缺失时会新建空库输出假通过 `ok`)*
     - 若缺少可用工具导致无法执行 `PRAGMA integrity_check;`，或核验未明确获得单一 `ok`，备份仅作保留记录，**数据库健康验收结论强制标为“未验证／暂停”，严禁以此放行恢复**。

4. **与备份方式强匹配的归档复原规程 (Archive-Then-Swap Restore SOP)**：
   - 彻底确认 Bob 桌面端客户端已完全停止。
   - **严禁使用宽泛删除命令**：绝对禁止执行诸如 `Remove-Item *` 等对生产主目录进行全量或模糊清理的操作，防止破坏日常其他配置或日志文件。
   - **安全归档脏会话 (Safe Quarantine)**：
     - 若测试产生了脏数据残留或测试会话需回滚，先将当前主目录下的待替换文件（`bob.db`，以及当前若存在的 `bob.db-wal`、`bob.db-shm`）整体安全移动至隔离归档目录（例如 `$env:APPDATA\bob.agent\dirty_quarantine_YYYYMMDD_HHMMSS\`），保留现场以便故障追溯。
   - **快照强匹配还原 (Matching Restore)**：
     - **恢复操作必须与所选备份方式严格对应**：
       - 若所选备份快照为**干净主库快照**（快照目录仅有 `bob.db`）：仅将备份目录的 `bob.db` 复制还原至主数据目录，并确认主数据目录下无任何遗留的旧 `-wal`/`-shm` 文件（已在上一归档步移走）；
       - 若所选备份快照为**三元组快照**（包含 `-wal`/`-shm`）：必须将备份目录中的 `bob.db`、`bob.db-wal`、`bob.db-shm` 同步完整复制回主数据目录，严禁恢复单文件而遗漏对应伴随文件。
   - **恢复后健康验收与放行硬门槛 (Post-Restore Acceptance Gate)**：
     - 核对还原后 `bob.db` 的 SHA-256 必须与备份快照记录完全吻合（物理还原一致）；
     - **放行硬门槛**：必须使用经验证的工具再次对主数据目录还原后的数据库执行只读完整性检查，确认输出明确为 `ok`：
       - *首选基线方案 (Python 安全只读核验器 - 经双重实测基线)*：
         ```powershell
         python scripts/verify_db_integrity.py "$env:APPDATA\bob.agent\bob.db"
         ```
         或等价单行安全命令：
         ```powershell
         python -c "import sys, sqlite3, pathlib; p = pathlib.Path(sys.argv[1]).resolve(); assert p.is_file() and p.stat().st_size > 0, f'Database missing or empty: {p}'; con = sqlite3.connect(f'{p.as_uri()}?mode=ro', uri=True); rows = con.execute('PRAGMA integrity_check;').fetchall(); con.close(); assert rows == [('ok',)], f'Integrity check failed: {rows}'; print('ok')" "$env:APPDATA\bob.agent\bob.db"
         ```
       - *备选方案 (官方 CLI 工具 - 必须严格包装，严禁裸跑)*：
         ```powershell
         $target = "$env:APPDATA\bob.agent\bob.db"
         if (-not (Test-Path -LiteralPath $target -PathType Leaf) -or (Get-Item -LiteralPath $target).Length -le 0) { throw "Database missing or empty: $target" }
         $res = & sqlite3 -readonly $target "PRAGMA integrity_check;"
         if ($res -ne "ok") { throw "Integrity check failed: $res" }
         "ok"
         ```
         *(注：严禁直接运行裸 CLI 命令作为放行依据)*
     - 若无可用工具执行该检查，或核验未明确获得单一 `ok`，恢复后的放行**强制标记为“未验证／暂停 (UNVERIFIED / PAUSED)”**，**绝对不得宣称恢复验收通过，绝对不得放行启动客户端**。

#### 1.2.3 专用诊断隔离测试 Profile 规范与安全防伪机制 (Diagnostic Profile Architecture & Hard Gates)
- **实现定位**：通过 Cargo Feature `fault-injection` 条件编译，仅在专用诊断构建 `bob-v0.9.6-diagnostic-pc` 中生效。生产默认发布构建（`bob-v0.9.6-candidate-pc`）不编译任何诊断代码，彻底不识别 `--test-data-dir` 参数与布防逻辑。
- **四大安全防伪硬门禁 (Four Hard Invariants)**：
  1. **绝对路径强制**：必须通过 `--test-data-dir=<PATH>`（或环境变量 `BOB_TEST_DATA_DIR`）传入绝对路径，严禁相对路径；
  2. **生产目录绝对排他 (Anti-Production Hard Gate)**：测试数据目录严禁指向、包含或被包含于日常 `%APPDATA%\bob.agent`（经 `canonicalize` 深度比对规范化路径），一旦检测到任何路径重叠立即 Fail-Closed 终止进程；
  3. **防伪标记硬校验 (Marker Validation)**：指定的测试目录下必须包含有效防伪标记文件 `.bob_test_profile`，其内容必须包含固定特征码 `BOB_A4_TEST_PROFILE_V1`。缺失标记或特征码不匹配时坚决拒斥启动；
  4. **工作区目录收敛 (Workspace Containment)**：所有受测文件必须严格收敛于该测试数据目录下的 `test_workspace/` 子目录，严禁路径逃逸（如 `..` 越界），且严禁指向测试目录自身的 `bob.db` 或配置文件。
- **与生产冷备份互补**：在日常生产环境执行全量功能验收时，继续执行 1.2.2 节经过核验的停机冷备份与归档恢复 SOP；在执行 Case 07 破坏性故障注入时，必须在上述隔离测试 Profile 环境下执行，实现主库物理级零接触。

### 1.3 设备与网络拓扑条件
- **PC 执行宿主**：Windows 11 物理机 (ThinkPad X1C)，运行 Tauri 桌面客户端，默认监听 `0.0.0.0:3722`。
- **Android 控制端**：真实物理手机 (Android 10+，非虚拟机)，安装候选测试 APK。
- **网络拓扑**：
  - **LAN 直连**：PC 与手机连接同一局域网 Wi-Fi (同网段，直接 HTTP/WS 3722 通信)。
  - **Relay 中继**：手机使用蜂窝移动网络 (4G/5G)，PC 使用固定宽带；通过中继服务 `wss://relay.bobbik.org`。
  - **异常弱网/断网**：手机开启飞行模式、或断开中继连接。

---

## 二、关键对象追踪与脱敏审计规范 (Identity & Sanitization)

### 2.1 关键对象追踪标识体系
每项测试必须按链路记录以下核心字段，确保全程可关联可追溯：

| 标识符 | 含义与源码对应 | 记录用途 |
|:---|:---|:---|
| `session_id` | 认证会话 UUID (`authenticated_sessions.session_id`) | 验证会话生命周期与撤销隔离 |
| `invitation_id` | 配对邀请 UUID (`pairing_invitations.invitation_id`) | 关联配对流程，不得记录密钥 |
| `request_id` | 单次 RPC/Sync 请求唯一 ID (`req-xxx`) | 验证幂等缓存 (`rpc_idempotency_cache`) 与防重放 |
| `change_id` | 文件修改提案 UUID (`staged_changes.change_id`) | 验证 StagedChange 审批流与原子写入 |
| `task_id` / `run_id` | 任务执行与目标管理 ID (`task-xxx`, `run-xxx`) | 验证远程 Agent 委派与执行生命周期 |
| `trace_id` | 跨端同步跟踪 ID (`sync_runs.json` 中的 `trace_id`) | 验证同步历史与诊断事件一致性 |
| `receipt.stage` | 投递回执阶段 (`applied` / `pending_apply` / `error`) | 验证投递状态真实性 |

### 2.2 🔴 证据脱敏铁律 (Evidence Redaction Rules)
1. **配对邀请安全**：验收记录与截图中，邀请 URL **必须**脱敏为 `bob://pair?v=0.9.6-sec01&id=inv-***&sec=***&iss=***`。**绝对禁止**将 `sec=` 后的真实高熵秘密写入记录表、测试报告或截图附件。
2. **API Key 与 Token 绝不上屏/落盘**：日志与网络抓包中，所有的 `apiKey`、`bot_token`、`secret` 必须全部屏蔽为 `***`。
3. **疑似泄露的处置机制**：若在抓包或移动端界面中观察到疑似未脱敏的敏感凭据：
   - 立即记录脱敏样本（如 `{"provider": "openai", "apiKey": "sk-***REDACTED***"}`）；
   - 立即终止测试，判定为 **FAIL**；
   - 记录紧急处置措施（如在云端控制台注销已暴露的密钥、重置本地会话、阻断代码发布）。

---

## 三、真机可执行验收用例集 (8 大核心场景)

### Case 01: 扫码或链接配对与首次同步解耦 (Pairing & Sync Decoupling)
- **测试目的**：验证 SEC-01 持钥证明配对机制，以及设备已配对但首次同步处于不同状态时的真实 UI 表达。
- **操作步骤**：
  1. PC 端打开“设置 → 连接中心”，点击“P2P 扫码配对”，界面展示二维码及邀请串。
  2. 手机端打开“连接中心”，点击扫码按钮扫描二维码（或手动粘贴邀请字符串）。
  3. 观察双端配对握手与首次同步的推进过程。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] 手机与 PC 成功交换 Ed25519 公钥并完成 PoP 验证；
  - [ ] PC 数据库 `pairing_invitations` 中对应的 `invitation_id` 记录 `consumed_at` 被更新，`authenticated_sessions` 生成新的活动会话；
  - [ ] 双端设备列表中即刻出现对方设备条目（显示设备名、`device_id` 前 8 位与在线状态）；
  - [ ] **首次同步 Applied**：若首次同步落盘成功，面板步骤打勾显示 `done`，提示“设备配对成功”；
  - [ ] **首次同步 Pending**：若首次同步返回 `pending_apply`，面板展示黄色警示，明确提示“待目标设备应用”，**绝不显示打勾，绝不显示已应用**；
  - [ ] **首次同步 Failed/Skipped**：若网络超时或失败，面板展示“设备已配对，但首次同步失败”或“未执行首次同步”，步骤标红，**绝不判定为配对失败，也绝不虚报同步完成**。
- **证据记录**：记录脱敏的 `invitation_id`、双端显示的 `device_id`、连接面板截图、`pairing_invitations` 表查询结果。

---

### Case 02: 撤销后的拒绝与阻断 (Revocation & Replay Rejection)
- **测试目的**：验证设备解绑后，密钥会话立即失效，历史会话和离线重放无法再发起任何受控指令。
- **操作步骤**：
  1. 在 PC 端连接列表中，找到已配对的手机，点击“解绑设备”并确认。
  2. 在手机端尝试发起数据同步，或下达远程执行指令。
  3. 尝试使用已撤销会话的旧签名载荷重新发送请求。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] PC 数据库 `trusted_devices` 表中该手机记录更新为 `status = 'revoked'` 并写入 `revoked_at`；`authenticated_sessions` 中对应会话 `is_active` 置为 0；
  - [ ] `peer_revocation_outbox` 产生对应的撤销通知记录；
  - [ ] 手机端发起请求时，立即被 PC 端 Fail-Closed 拒斥（HTTP 401/403 或 Relay Signed Error）；
  - [ ] 手机界面明确提示“设备已解除绑定或未授权”，阻断操作并引导重新配对；
  - [ ] 重放旧请求时，Rust 后端返回 `ERR-SEC01-REVOKED` 或签名过期，无任何副作用产生。
- **证据记录**：`trusted_devices` 表数据行快照、拦截 HTTP 状态码与脱敏错误信息。

---

### Case 03: 手机下达指令与连续追问 (Remote Execution & Turn Chain)
- **测试目的**：验证手机远程向 PC 下达 Agent 任务、流式查看思考与输出并保持多轮对话上下文，且 API Key 严守物理隔离。
- **操作步骤**：
  1. 手机端进入 Chat 界面，向已连接的 PC 发送指令：“请分析测试目录文件结构”。
  2. 观察 PC 执行过程及手机端结果回传。
  3. 手机端紧接着发送追问：“请基于刚才的结果，输出详细说明”。
  4. 检查两端通信报文与移动端本地存储。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] PC 作为无头执行端正常调用模型与工具，手机端流畅接收流式输出与思考过程；
  - [ ] 第二轮追问中，PC 能准确识别上一轮上下文，保持连续多轮对话记忆；
  - [ ] **密钥物理隔离红线**：手机端与 PC 端通信的所有 JSON 报文、中继中转日志、移动端本地存储中，**100% 不存在 PC 上的 LLM API Key 或 Bot Token**。
- **证据记录**：对话 `session_id`、连续两轮的 `request_id`、脱敏通信报文样本。

---

### Case 04: 文件修改提案的批准与拒绝 (Staged Changes Review Loop)
> **必须严格在专用测试目录 `<app_data>/test_workspace/` 下进行，区分新建文件与修改已有文件。**

#### 场景 4A: 新建文件 (New File Creation，`old_content` 为空)
- **操作步骤**：
  1. 手机端下达任务：“在测试工作区创建新文件 `test_new.txt`，内容为 `Hello Bob New File`”。
  2. PC 端生成 `StagedChange` 提案，手机端弹出差异审阅卡片。
  3. 分支 1 (批准)：手机端点击“批准 (Approve)”。
  4. 分支 2 (拒绝)：重新生成提案，手机端点击“拒绝 (Reject)”。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] **批准通过**：目标文件此前不存在；批准后新文件生成且内容精确一致；`staged_changes` 状态更新为 `applied` 并记录 `applied_at`；**确认无 `.bak` 备份文件生成**；`staged_write_recovery` 表记录被完整清理；
  - [ ] **拒绝通过**：拒绝后目标文件在磁盘上**依然不存在**（从未落盘）；`staged_changes` 状态更新为 `rejected`；无残留暂存。

#### 场景 4B: 修改已有文件 (Existing File Modification，`old_content` 非空)
- **源码与真实备份生命周期依据**：
  - 在 [`src-tauri/src/sync_engine.rs`](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/sync_engine.rs#L1217-L1223) 中，临时备份文件使用包含动态随机 UUID v4 的 Nonce 命名：
    `let nonce = uuid::Uuid::new_v4();`
    `let bak_path = Some(parent.join(format!(".{}.bak_bob_{}", file_name, nonce)));`
    `let tmp_path = parent.join(format!(".{}.tmp_bob_{}", file_name, nonce));`
  - 临时备份**绝非**此前误写的固定格式 `.{file}.bak_{change_id}`。
  - **真实备份生命周期**：
    1. **临时产生**：原子写入前，原文件被重命名为 `.{file}.bak_bob_{nonce}`，并在恢复表记录 WAL stage 为 `backed_up`；
    2. **原子替换与读回**：临时文件 `.{file}.tmp_bob_{nonce}` 重命名覆盖至目标文件，执行字节级读回校验；
    3. **自动清理**：在 SQLite 数据库单事务（更新 `staged_changes`、`work_objects`、`work_events`、清理 `staged_write_recovery`）提交成功后，系统执行 [`DiskWriteBackup::commit()`](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/sync_engine.rs#L1171-L1178)，显式调用 `std::fs::remove_file(bak)` **自动删除临时备份文件**；
    4. **回滚守护**：仅当数据库事务或校验失败时，系统通过 `DiskWriteBackup::rollback()` 使用临时备份恢复原文件。
- **操作步骤**：
  1. 在测试目录预置基准文件 `test_exist.txt`（内容 `Version 1.0`），记录其 SHA-256。
  2. 手机端下达修改指令，PC 生成 `StagedChange` 提案（修改为 `Version 2.0`）。
  3. 分支 1 (批准)：手机端审查 Unified Diff 后点击“批准”。
  4. 分支 2 (拒绝)：预置文件内容恢复为 `Version 1.0`，重新生成提案后点击“拒绝”。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] **批准通过 (Success Path)**：
    - 写入过程中创建符合 `.{file}.bak_bob_{nonce}` 规则的临时随机备份；
    - 写入校验通过且 SQLite 事务提交成功后，**临时备份文件被系统自动安全清理**；
    - **稳态验证**：目标文件内容更新为 `Version 2.0`（SHA-256 改变）；**测试工作区目录中绝无任何残留的 `.{file}.bak_bob_*` 或 `.{file}.tmp_bob_*` 文件**（核验其已被完整清理，而不是去查找名为 `.{file}.bak_{change_id}` 的残留文件）；
    - `staged_changes` 状态更新为 `applied` 并记录 `applied_at`；`staged_write_recovery` 表记录被完整清理清空。
  - [ ] **拒绝通过 (Reject Path)**：
    - 拒绝后目标文件保持 `Version 1.0`（SHA-256 与初始基准完全相同，未发生任何磁盘覆写）；
    - 磁盘从未产生过临时备份文件；`staged_changes` 状态更新为 `rejected`；无残留暂存。
- **证据记录**：`change_id`、目标文件路径、修改前后 SHA-256、`staged_changes` 表数据行。

---

### Case 05: 运行时取消确认与中断 (Execution Cancellation)
- **测试目的**：验证长任务执行过程中，移动端发起取消操作后能安全中断且无状态污染。
- **操作步骤**：
  1. 手机端下达耗时较大的长任务。
  2. 在任务执行进行中，手机端点击“停止/取消”按钮。
  3. 观察 PC 执行端响应速度及双端最终状态。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] PC 端立即捕获取消信号，中止当前执行进程；
  - [ ] `staged_changes` 中未审批的提案状态更新为 `cancelled`；
  - [ ] 任务状态迁移为 `cancelled`，禁止转入 `completed`；
  - [ ] 未提交的临时文件被完全回滚清理，无半截残损状态。
- **证据记录**：`task_id` / `request_id`、取消响应耗时、`staged_changes` 状态。

---

### Case 06: 断线重连与本地暂存耐久性 (Disconnection & Outbox Retention)
- **测试目的**：验证在网络中断时，发送端操作耐久留存本地，网络恢复后自愈，绝不丢数据也绝不虚报成功。
- **操作步骤**：
  1. 将手机置于飞行模式（断开所有网络）。
  2. 手机端修改配置或产生待同步数据，触发后台同步。
  3. 检查手机本地存储与界面状态。
  4. 恢复网络连接，观察自愈同步。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] 断网期间，手机端 Outbox 文件（`mobile_outbox.json`）严格保留在磁盘，**绝对不因发送失败而静默删除**；
  - [ ] 手机界面同步状态指示为错误或离线，`bob-last-sync-time` **绝对不更新**；
  - [ ] 恢复网络后，应用自动探测到网络恢复并触发重试；
  - [ ] 收到 PC 端确认提交回执 (`commit_ack` 且 `stage: "applied"`) 后，手机端本地 `mobile_outbox.json` 始得安全清除。
- **证据记录**：断网期间磁盘 `mobile_outbox.json` 存在快照、重试恢复后回执数据。

---

### Case 07: 受控写入故障回滚与崩溃恢复验证 (Write Failure & Crash Recovery)
> **限定隔离测试资产**：必须在专用隔离测试 Profile 目录下进行（如 `$env:TEMP\bob_a4_diagnostic\test_workspace\crash_probe.txt`），严禁在生产 `%APPDATA%\bob.agent\` 执行。测试前按 §1.2 确认生产主库零接触。

> [!CRITICAL] **核心边界与门禁规则**：
> 1. 本用例严格拆分为两部分：**Case 07A (受控写入故障与安全回滚)** 与 **Case 07B (中途崩溃窗口恢复与 WAL 表收敛)**。
> 2. **受控写入失败与普通重启不能证明中途崩溃恢复通过**。在物理真机设备上，从文件替换到 SQLite 单事务提交是紧密相连的微秒级指令流，物理执行时间窗口极其短暂，人工通过任务管理器强杀进程在统计学上无法可靠命中临界崩溃窗口。
> 3. **门禁铁律**：目前代码库已构建完整的受控故障注入基座（`fault-injection` 特性门禁、`diagnostic_profile.rs` 与 `fault_injection.rs`，并经 317 项 Rust 测试与 62 项前端测试全量验证），完成了缺参阻断、路径逃逸修补、一次性布防强制保证与 replaced 崩溃注入点校准。但在用户使用专用诊断构建（`bob-v0.9.6-diagnostic-pc`）于目标物理真机环境完成实测验证之前，**Case 07 整体绝不得判定为 PASS**。Gate A4 依然严格保持 **PREPARED / UNEXECUTED / LOCKED**。

#### 场景 7A: 受控写入故障与安全回滚 (Controllable Write Failure & Rollback)
- **测试目的**：验证当目标文件在原子替换阶段遭遇写入失败（模拟磁盘写保护、I/O 拒绝或权限不足）时，系统能够安全捕获 `PermissionDenied` 错误、执行 `DiskWriteBackup::rollback()` 将目标文件完全复原、清理临时文件、绝不产生脏数据倾斜且真实向界面及调用方报错。
- **双轨故障注入执行规程**：
  - **轨 A (标准隔离诊断 Profile 注入 - 推荐首选)**：
    1. **创建隔离测试环境**：
       ```powershell
       $test_dir = "$env:TEMP\bob_a4_07a_test"
       New-Item -ItemType Directory -Path "$test_dir\test_workspace" -Force
       Set-Content -Path "$test_dir\.bob_test_profile" -Value "BOB_A4_TEST_PROFILE_V1"
       Set-Content -Path "$test_dir\test_workspace\crash_probe.txt" -Value "Crash Probe Baseline 1.0"
       $base_hash = (Get-FileHash "$test_dir\test_workspace\crash_probe.txt" -Algorithm SHA256).Hash
       ```
    2. **布防受控写入故障**：写入单次生效（one-shot）布防文件 `fault_injection_armed.json`：
       ```powershell
       $armed = @{
           target_change_id = "chg_a4_07a_probe"
           target_file_path = "$test_dir\test_workspace\crash_probe.txt"
           stage = "WriteError07a"
           one_shot = $true
       } | ConvertTo-Json
       Set-Content -Path "$test_dir\fault_injection_armed.json" -Value $armed
       ```
    3. **启动专用诊断端**：
       ```powershell
       .\bob-diagnostic-pc.exe --test-data-dir="$test_dir"
       ```
    4. **触发审批写入**：手机端或 RPC 下达修改指令生成提案 `chg_a4_07a_probe`（内容变更为 `Malicious Change 2.0`），点击批准。
    5. **观察与核验**：
       - 系统在原子重命名阶段精准拦截并抛出：`07A 受控写入注入失败: 模拟磁盘写保护或权限拒绝 (PermissionDenied)`；
       - `fault_injection_armed.json` 在触发时被物理删除（自消耗完成）；
       - 客户端立即触发 `rollback()` 回滚，原文件 `crash_probe.txt` 恢复初始基准；
       - 前端/移动端弹出明确的写入失败报错弹窗，审批状态未转入 `applied`；
       - 临时文件 `.{file}.tmp_bob_*` 被安全清除。
  - **轨 B (操作系统只读属性候选方式 - 备选核验)**：
    1. 若未运行诊断构建，采用 `attrib +r "$test_dir\test_workspace\crash_probe.txt"`；
    2. 执行前必须先验证该只读属性是否能稳定导致重命名失败；若未能打入错误分支，必须记为“注入无效／未验证”，严禁判 PASS。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] **必须命中预期故障**：底层安全捕获写入或重命名拒绝错误，Fail-Closed 阻断事务提交流程；
  - [ ] **界面真实报错**：手机端与 PC 端界面均展示明确的写入失败报错提示，**绝对不转入 `applied`，绝对不虚报成功**；
  - [ ] **基线绝对无损**：回滚后原文件 `crash_probe.txt` 完好无损，SHA-256 与初始基线 `$base_hash` 逐字节一致；
  - [ ] **数据库无脏数据**：数据库事务未提交，`staged_changes` 保持原状态，`work_objects` 与 `work_events` 无脏数据写入；
  - [ ] **清理彻底**：`staged_write_recovery` 表中相关临时条目被及时清理，无悬挂的崩溃恢复记录；工作区无任何残留的 `.{file}.tmp_bob_*` 或 `.{file}.bak_bob_*` 垃圾文件。
- **证据记录**：布防参数快照、报错脱敏日志、测试文件前后 SHA-256、`staged_write_recovery` 查询结果。

#### 场景 7B: 崩溃窗口恢复与未决恢复表收敛 (Crash Window Recovery & WAL Convergence)
- **测试目的**：验证系统在原子写入的关键崩溃窗口内（Prepared 阶段、BackedUp 阶段、Replaced 阶段）遭遇进程突发异常终止（掉电、强杀或崩溃）后，下次启动时自愈引擎 [`recover_interrupted_staged_writes`](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/src/sync_engine.rs#L619-L630) 能安全读取 WAL 表并全自动收敛自愈，绝不损坏文件、绝不残留半截文件、且绝不进入死循环崩溃。
- **覆盖的 3 大崩溃窗口 (Three Crash Windows)**：
  1. **窗口 1 (`Prepared`)**：临时文件写入且强制刷盘成功，WAL 记录落库 `prepared` 阶段后、原目标文件尚未备份前发生崩溃；
  2. **窗口 2 (`BackedUp`)**：原目标文件已安全重命名为 `.{file}.bak_bob_{nonce}`，WAL 记录更新为 `backed_up` 阶段后、临时文件尚未替换至目标位置前发生崩溃（此时目标位置空缺，原文件位于备份）；
  3. **窗口 3 (`Replaced`)**：临时文件已替换至目标文件且读回校验通过，WAL 记录更新为 `replaced` 阶段后、SQLite 单事务提交前发生崩溃（此时目标文件已写入新内容，但事务未提交，原文件位于备份）。
- **目标真机可重复执行 SOP (User-Executable Diagnostic SOP)**：
  1. **前置环境准备 (创建隔离测试 Profile)**：
     ```powershell
     $test_dir = "$env:TEMP\bob_a4_07b_test"
     New-Item -ItemType Directory -Path "$test_dir\test_workspace" -Force
     Set-Content -Path "$test_dir\.bob_test_profile" -Value "BOB_A4_TEST_PROFILE_V1"
     Set-Content -Path "$test_dir\test_workspace\crash_probe.txt" -Value "Crash Probe Baseline 1.0"
     $orig_hash = (Get-FileHash "$test_dir\test_workspace\crash_probe.txt" -Algorithm SHA256).Hash
     ```
  2. **布防目标崩溃窗口 (选择一种阶段注入)**：
     - 将 `$stage` 设为 `"Prepared"`, `"BackedUp"`, 或 `"Replaced"`：
     ```powershell
     $stage = "BackedUp" # 可选: Prepared, BackedUp, Replaced
     $armed = @{
         target_change_id = "chg_a4_07b_probe"
         target_file_path = "$test_dir\test_workspace\crash_probe.txt"
         stage = $stage
         one_shot = $true
     } | ConvertTo-Json
     Set-Content -Path "$test_dir\fault_injection_armed.json" -Value $armed
     ```
  3. **启动诊断端并触发写入**：
     - 启动：`.\bob-diagnostic-pc.exe --test-data-dir="$test_dir"`
     - 手机端或控制端对 `crash_probe.txt` 提交变更提案 `chg_a4_07b_probe` 并点击批准。
  4. **观察预期崩溃中断 (Controlled Exit 77)**：
     - 诊断端捕获命中的阶段，输出审计日志：
       `[FaultInjection AUDIT] 07B Crash window simulated interrupt triggered! Stage: ..., Change: chg_a4_07b_probe, Target: ... Exiting with code 77`
     - 进程立即以退出码 **77** 退出（非 0，模拟硬性断电强杀）；
     - **防崩溃循环核验**：核查 `$test_dir\fault_injection_armed.json` **已被物理消费删除**（确保后续重启不会无限次退出）。
  5. **重启客户端并验证全自动收敛 (Reboot & Self-Healing Verification)**：
     - 再次启动诊断端客户端：`.\bob-diagnostic-pc.exe --test-data-dir="$test_dir"`；
     - 客户端启动时自动执行 `recover_interrupted_staged_writes`；
     - **各阶段自愈预期结果**：
       - 若注入 **`Prepared`**：自愈引擎清理孤立的 `.{file}.tmp_bob_*` 文件；原文件未受任何触碰，保持基准；清理 recovery 条目；
       - 若注入 **`BackedUp`**：自愈引擎将 `.{file}.bak_bob_*` 安全还原至目标文件，清理临时文件，清理 recovery 条目；
       - 若注入 **`Replaced` (修改已有文件)**：事务未提交，自愈引擎判定需保护历史资产，从 `.{file}.bak_bob_*` 安全回滚原内容至目标文件，清理临时文件与备份，清理 recovery 条目；
       - 若注入 **`Replaced` (新建全新文件)**：自愈引擎安全移除未提交的新文件，清理临时文件，清理 recovery 条目；
     - **数据库与系统状态校验**：
       - 查询测试数据库：`SELECT count(*) FROM staged_write_recovery;` 必须为 **0**（完全收敛）；
       - 数据库只读完整性检查通过：
         ```powershell
         python scripts/verify_db_integrity.py "$test_dir\bob.db"
         ```
         严格返回 `ok`；
       - 系统未进入 `is_recovery_degraded()` 降级阻塞状态，恢复正常响应。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] **精准命中崩溃窗口**：进程在 WAL 记录落库后严格按预设阶段退出，退出码为 77；
  - [ ] **单次消费防死循环**：布防文件在触发前被自动删除，重启时绝对不陷入连续崩溃死循环；
  - [ ] **自愈无损恢复**：重启后文件资产按既定策略无损自愈（原文件完好，或未提交新建文件被移除），SHA-256 符合预期；
  - [ ] **WAL 表完全收敛**：`staged_write_recovery` 表记录归零，无悬挂条目；
  - [ ] **数据库健康**：`verify_db_integrity.py` 验证返回 `ok`，系统未进入 Fail-Closed 降级阻塞状态；
  - [ ] **生产主库绝对零接触**：日常 `%APPDATA%\bob.agent\bob.db` 大小与修改时间在测试全程 100% 无任何变动。
- **当前状态标定与证据边界**：
  - **当前状态**：**基座就绪，尚未在物理真机执行 (PREPARED / UNEXECUTED / LOCKED)**。
  - **受控基座安全加固与负向测试已完备**：
    1. **诊断包缺参阻断**：`ensure_diagnostic_profile_initialized()` 在 `db::init_db()` 之前强制阻断，负向测试 `test_diagnostic_profile_missing_args_and_unmarked_dir_fail_closed_prod_untouched` 证明生产主库 100% 零触碰；
    2. **路径逃逸严格消解**：`clean_path` 净化所有 `..` 相对跳转并核验最长既有祖先目录归属，负向测试 `test_diagnostic_path_traversal_escape_and_nested_creation_negative` 覆盖多级不存在目录通过 `..` 越界逃逸并全部拦截，同时保障合法新建嵌套文件放行；
    3. **一次性布防原子消费**：07B 崩溃注入强制约束 `one_shot: true`（拒绝 `one_shot: false`），布防消费持久化采用物理删除与覆写双重校验，消费未成功时拒绝退出 77 防死循环；负向测试 `test_07b_rejects_non_oneshot_config` 与 `test_07b_rejects_interrupt_when_consumption_cannot_be_persisted` 验证通过；
    4. **校准 replaced 崩溃点**：移至读回校验成功后、DB 事务提交前，严密对齐架构定义，`test_07b_replaced_interrupt_and_recovery_existing_file` 与 `test_07b_replaced_interrupt_and_recovery_new_file` 均已重新通过；
  - 研发阶段已完成自动化单元测试与子进程中断测试全绿（317 项 Rust 测试 + 62 项前端测试通过）；但依据准出铁律，必须待用户在 ThinkPad X1C 实体设备上运行上述 SOP 并获得实测数据后，方可解锁此项。

---

### Case 08: 双端状态一致性与历史记录精确性 (`applied` vs `pending_apply` vs `failed`)
- **测试目的**：严格验证 Round 9 根治的误报漏洞在真机上的表现，确保界面展示、时间戳与历史日志绝对真实。
- **操作步骤**：
  1. **场景 8A (真实应用成功)**：正常发起一次完整双向同步，配置成功落盘。
  2. **场景 8B (配置暂存待应用)**：
     - *触发方式*：目标设备端配置写入锁占用或暂存入队（若真机难以自然触发，标记为**待设计故障注入方式**，例如通过测试选项模拟对端处于暂存状态）。
  3. **场景 8C (同步异常失败)**：在手机未联网或输入错误中继地址时触发同步。
- **可观察通过标准 (Pass Criteria)**：
  - [ ] **场景 8A**：双端指示灯显示为正常状态，显示“同步完成/已应用”，`localStorage` 的 `bob-last-sync-status = 'success'`，**更新同步时间戳**，`sync_runs.json` 记录 `status: "success"`；
  - [ ] **场景 8B**：双端指示灯显示黄色警示，明确显示“待应用”，`bob-last-sync-status = 'pending'`，**绝对不更新同步时间戳**，`sync_runs.json` 记录 `status: "pending"`；
  - [ ] **场景 8C**：双端指示灯显示红色，明确报错，`bob-last-sync-status = 'error'`，**绝对不更新同步时间戳**，启动静默监听降级，`sync_runs.json` 记录 `status: "failed"`。
- **证据记录**：两端 `localStorage` 脱敏快照、`sync_runs.json` 数据条目、双端界面截图对比。

---

## 四、A4 真机验收记录表模板 (Record Sheet)

> **注：所有日志与报文附件必须经脱敏处理，严禁包含真实密钥或 Token。**

| 用例编号 | 测试场景 | 测试网络 (LAN/Relay) | 关键追踪 ID | 预期状态 | 实际界面状态与回执 | 通过结论 (PASS/FAIL) | 脱敏附件索引 | 测试人员与时间 |
|:---|:---|:---|:---|:---|:---|:---|:---|:---|
| **TC-01** | 扫码配对及首次同步 | LAN / Relay | `inv-...` / `sess-...` | 状态解耦真实展示 | [待实测填入] | 待执行 | 截图/日志-01 | |
| **TC-02** | 撤销设备后续请求阻断 | LAN / Relay | `dev-...` / `sess-...` | 401/403 Fail-Closed | [待实测填入] | 待执行 | 截图/日志-02 | |
| **TC-03** | 远程指令与多轮追问 | Relay (蜂窝网) | `req-...` / 2 轮对话 | 结果回传且零Key暴露 | [待实测填入] | 待执行 | 报文日志-03 | |
| **TC-04A-1**| 新建文件提案：批准 | 专用测试工作区 | `sc-...` / `test_new.txt` | 新建成功无.bak | [待实测填入] | 待执行 | 差异卡片-04A1 | |
| **TC-04A-2**| 新建文件提案：拒绝 | 专用测试工作区 | `sc-...` / `test_new.txt` | 文件保持不存在 | [待实测填入] | 待执行 | 差异卡片-04A2 | |
| **TC-04B-1**| 修改文件提案：批准 | 专用测试工作区 | `sc-...` / `test_exist.txt`| 成功修改且临时.bak_bob_{nonce}已清理 | [待实测填入] | 待执行 | 差异卡片-04B1 | |
| **TC-04B-2**| 修改文件提案：拒绝 | 专用测试工作区 | `sc-...` / `test_exist.txt`| 原内容原Hash无损 | [待实测填入] | 待执行 | 差异卡片-04B2 | |
| **TC-05** | 远程任务中断与取消 | LAN / Relay | `task-...` / `sc-...` | 立即终止无残余状态 | [待实测填入] | 待执行 | 取消日志-05 | |
| **TC-06** | 飞行模式断网重连自愈 | 移动网络断开恢复 | `mobile_outbox.json` | 本地保留重连清除 | [待实测填入] | 待执行 | 磁盘快照-06 | |
| **TC-07A** | 受控写入故障与安全回滚 | 隔离测试工作区 (`diagnostic-pc`) | `sc-...` / `crash_probe` | 命中 WriteError07a 拒绝写入，触发 rollback，原文件无损，界面真实报错，recovery 表清理 | [待实测填入] | 待执行 | 故障日志-07A | |
| **TC-07B** | 崩溃窗口恢复与WAL收敛 | 隔离测试工作区 (`diagnostic-pc`) | `sc-...` / `crash_probe` | 维持门禁；按 Prepared/BackedUp/Replaced 退出码 77，单次消费防死循环；重启自愈收敛，原文件无损，WAL 归零，DB ok | [待实测填入] | 未执行 (LOCKED) | [待真机实测记录] | |
| **TC-08A**| 同步判定：Applied | LAN / Relay | `trace-...` / `sync-...` | 绿灯，时间戳刷新 | [待实测填入] | 待执行 | Storage-08A | |
| **TC-08B**| 同步判定：Pending | 待注入 / 局域网 | `trace-...` / `sync-...` | 黄灯待应用，时间不变 | [待实测填入] | 待执行 | Storage-08B | |
| **TC-08C**| 同步判定：Error | 弱网/异常响应 | `trace-...` / `sync-...` | 红灯报错，时间不变 | [待实测填入] | 待执行 | Storage-08C | |

---

## 五、验收准出与阻断规则 (Exit Gates & Blocking Criteria)

1. **一票否决红线 (Veto Criteria)**：
   - 通信报文、中继传输或日志中出现任何未经脱敏的 LLM API Key、第三方 Token 或系统敏感凭据；
   - 处于 `pending_apply`、`failed`、`unexpected` 状态时，界面或日志提前打勾宣称“已完成”或推进了同步时间戳；
   - 断网或重试失败导致本地未提交的数据丢失；
   - 撤销授权后的设备能够成功调用任何受信指令；
   - 测试过程污染或损坏了用户日常文件或生产数据库。
   *若触发上述任何一条，A4 验收立即终止，判定为 **FAIL**，退回研发修复。*
2. **通过与解锁标准 (Release Readiness)**：
   - 清单中所有可执行 Case 全部经真实双端硬件测试并判定为 **PASS**；
   - **Case 07 门禁与防假成功硬约束**：
     - **07A 约束**：只读属性仅是尚待验证的候选注入；必须先在隔离测试数据上证实能稳定触发指定错误分支；若未实际命中预期故障，必须记为“注入无效／未验证”，**严禁判定为 PASS**，不得宣称回滚已验证；其他受控注入方式需另行设计并在证明安全可恢复后再进入正式测试，不在本轮选择或实施；
     - **07B 维持既定门禁硬约束**：07B 必须在安全可控的故障注入方式下于目标物理环境实际测试通过；脱机探索绝不能当作 07B 真机验收的替代品；在 07B 于真机环境实际通过之前，**Case 07 整体绝不得判定为 PASS，Gate A4 绝对不得解锁**（若未来需缩减 A4 验收范围须由用户单独决策，本轮不得自行降低门槛）；
   - **数据库健康验收硬门槛**：执行前必须具备经验证可用的 SQLite 完整性检查工具（首选本机已验证的 Python 只读安全核验器，或具备严格物理存在非空前验与 `-readonly` 参数包装的官方 CLI 工具）；核验必须确保目标物理文件存在且非空、以只读模式连接并严格返回单一文本 `ok`；严禁使用未经防护的裸命令（如裸 `sqlite3` CLI 或裸 `connect()` 会在文件缺失时静默建库假通过）；若缺少经验证可用的工具或检查未获明确单一 `ok`，备份虽可暂存留存，但数据库健康验收与恢复放行**必须强制标记为“未验证／暂停 (UNVERIFIED / PAUSED)”，绝对不得宣称已通过，亦不得放行启动客户端**；
   - **快照备份一致性与无损恢复约束**：操作前必须严格按照 1.2.2 节完成前置停机与快照完整性核验；恢复时严格执行快照强匹配与脏会话归档，绝不允许直接执行宽泛删除；
   - 关键请求与产物身份（`request_id`, `change_id`, `receipt` 等）完整留存在记录表中；
   - 验收结果经用户亲自操作或确认授权签字后，方可解锁 Gate A4，进入正式发版流程。
