# RH-02 — 0.9.30 连接记忆、安全重连与 Work 分层

执行日期：2026-10-22。核验时 HEAD：`d47f2fae6b3656262e28509a4c06c1538e29eacf` 加当前未提交工作树。本报告不是 Release 或真机验收记录。

**历史证据边界**：下文是本轮最初密码库实现的测试快照，不再代表当前解锁交互。用户随后决定不另设 Bob 密码，已改为系统保护；最新实现、验证及 Android 未验边界见 [系统解锁增量](RH-02-system-unlock.md)。既有测试数字不覆盖该后续改动。

## 范围与事实

用户授权直接修改并测试连接记忆、安全重连、Work 重组，要求保留原有项目与工作记录。保留接手时 DSH snapshot 缺少 hasMore 的修复意图，进一步区别“未提供”与“类型错误”：未提供时近期快照按可能还有历史显示，非法类型拒绝。

- Rust `remote_agent/store.rs`：独立 SQLite 加密快照，Argon2id/AES-256-GCM 使用现有依赖；只保存交换后的 Cookie，不保存 launch token/密码。事务失败不更新内存。重开、错密码、密文篡改、遗忘和触发器拒写均测试。
- Rust `remote_agent/mod.rs`：保存/解锁/锁定/重连/更新/忘记命令；配置操作串行化，遗忘/锁定后不复活旧连接。发送/审批先持久化未知结果门槛，失败保留，明确回执后清除；它不是完整请求账本或远端 exactly-once。
- `dsh.rs`：Cookie 复用同端点 HTTP/WS，认证跳转和公网 HTTP 仍拒绝，401/403 区别于网络错误。没有修改 X1T 服务或读取真实凭据。
- 应用级 `remote-agent-service.js`：页面切换不销毁，懒恢复，有限退避，30 秒稳定窗口才重置预算，认证/协议错误不自动重试。断开保存失败可见；锁定中迟到保存不恢复界面连接。
- Settings 负责配置，Work 负责会话执行；本地记录整体保留为折叠区。桌面五项子导航迁入记录区，简报导航可展开原目标。不涉及主库迁移、删除或远程项目自动导入。
- 加密库密码需用户首次设置，进程重启解锁一次；本轮没有实现 OS 硬件密钥库或无密码恢复。重连寿命由服务端决定，不作固定 30 天承诺。

## 可复现检查与结果

| 检查 | 结果 | 证据边界 |
|---|---|---|
| `cargo test --lib remote_agent --offline -- --test-threads=1` | 16 passed / 0 failed | 本地真实 HTTP/Mux 夹具、持久化加密库和故障注入，不是公网 X1T |
| `cargo test --lib --offline -- --test-threads=1` | 359 passed / 0 failed / 1 ignored | 编译目标 0.9.30，现有 unused/dead-code warnings 保留，非零警告声明 |
| `pnpm --config.verify-deps-before-run=false test` | 16 files / 96 passed | 其中应用级恢复 12 项、原连接控制器 12 项；不安装依赖 |
| `node scripts/version-contract.mjs check` + 版本契约 Node 测试 | 0.9.30 / 9030，一致；5 passed | 主应用、安装器清单对齐，不代表产物已构建 |
| Vue SFC parse/compile | 6 个受影响组件通过 | App/Work/连接设置/远端面板/记录 tabs，仅语法与模板检查 |
| 离线真实 Vue 组件浏览器测试 | 390×844 zh-CN、1280×900 en-US 通过 | 实际 WorkView、RemoteAgentPanel、连接设置；模拟数据，无外网请求 |
| `git diff --check` | exit 0 | 可能提示 LF→CRLF 工作树转换，不是空白检查失败 |

浏览器复现：先运行 `node scripts/export-remote-agent-preview.mjs` 生成自包含测试 HTML，再用已有 Playwright 与 `BOB_UI_OFFLINE=1` 运行 `scripts/test-remote-agent-ui.mjs`。不安装依赖、不构建 Tauri/Android 安装包。测试包含解锁/保存→Work→原项目可访问→简报打开旧目标→会话发送→审批→取消未确认→断开后配置仍在；检查无页面错误、横向溢出或远端 HTML 执行。预览数据只在内存，不能用它验证原生加密保存。

浏览器截图保存在宿主临时目录 `bob-remote-agent-ui/`；可重新生成，未纳入包含真实内容的证据。此次没有访问真实 Bob 数据库，原生新测试使用独立临时连接库与本地夹具。

## 准出与剩余事项

本轮本地修改与自动化验证完成；RH-02 整体保持 active。0.9.30 未提交/推送、未触发双端 CI、未发布，A4/A5 不因本轮测试解锁。

后续候选验收：同源 PC/APK 下的首次保存、进程重启解锁后恢复、Token 失效、手机前后台/断网、持续长任务、结果不明人工核对；均须现场记录。用户“已初步联通”的反馈不是本轮执行者的真机证据。

仍未实现：系统密钥库免解锁、远端新建会话、完整请求对账/历史分页、计时/continued 问题、产物预览下载及其他适配器。原项目/工作记录的存储权威仍属于 Bob Work Core。
