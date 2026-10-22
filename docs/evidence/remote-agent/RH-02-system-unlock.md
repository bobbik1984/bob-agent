# RH-02 — 0.9.30 系统解锁增量

日期：2026-10-22。基线 HEAD：`d47f2fae6b3656262e28509a4c06c1538e29eacf` 加既有未提交 0.9.30 工作树。用户授权修改和测试，不含本轮发布。结论：代码及 Windows/前端本地测试完成；Android 原生编译和系统认证实机验收未执行。不是双端交付完成声明。

## 已采纳决定与实现

- 不再要求新用户另记 Bob 密码；保留上一轮连接记忆、安全重连、原项目及工作记录功能。
- `remote_agent/system_protection.rs`：Windows 当前用户 DPAPI 封装随机 32 字节数据密钥，禁用交互式 DPAPI UI，不使用机器级共享保护。它依赖当前 Windows 登录账户，不是每次 Windows Hello 验证。
- 本地插件 `plugins/tauri-plugin-device-vault/`：Android 11+ 框架 BiometricPrompt，允许强生物识别或系统锁屏凭据，CryptoObject 与 Android Keystore 逐次认证 AES-GCM 密钥绑定。不读取系统 PIN、人脸或指纹数据。弱人脸不作为该密钥的认证方式；可以用系统 PIN/图案/密码。低版本、无锁屏、认证取消、密钥缺失或失效均拒绝，没有明文或固定密码回退。
- 插件没有前端 invoke handler/密钥 IPC 命令；Rust 原生调用返回的密钥只用于原生加密库，不返回 Vue。Android 调用从阻塞工作线程发起，不在启动/主界面渲染线程等待认证。认证取消、两分钟超时或 Activity 销毁结束本次请求，允许用户重试。
- `store.rs`：沿用独立 SQLite 表，`salt` 字段在系统格式中保存版本前缀及系统封装密钥；正文仍为 AES-256-GCM。旧 16 字节 salt 行不自动覆盖；只有检测到旧库才提示一次旧密码，解密后经系统保护，完整快照在单笔 SQLite 事务内替换。迁移失败保留旧库及连接、会话、未知结果标记。Argon2 仅用于该旧试验版迁移。
- 前端正常状态只有“使用系统解锁”按钮，没有 Bob 密码框；只有原生返回迁移要求时显示旧密码输入。认证失败不设 unlocked；异常结构不当成功；没有启动自动弹窗。解锁保持到用户锁定连接库或退出 App，切换页面仍可保持连接。
- 没有改动 Bob 手机→PC 身份体系、DSH 服务端或在线 Agent；未读取真实 Token、真实连接库或日常主库。新测试使用隔离临时文件。

## 可复现验证

| 检查 | 结果 | 范围 |
|---|---|---|
| `cargo test --lib --offline -- --test-threads=1` | 363 passed / 0 failed / 1 ignored，exit 0 | Windows 宿主；现有 131 条警告仍在，不宣称零警告 |
| 新增 `dpapi_current_user_roundtrip_and_tamper_rejection` | PASS | 真实 Windows CryptProtectData/UnprotectData，不是 mock |
| 新增 `system_vault_real_windows_persistence_without_bob_password` | PASS | 真实 DPAPI + 临时 SQLite 保存、关闭、重新解锁 |
| 新增 `system_vault_cancel_migrate_failure_and_reopen_preserve_data` | PASS | 可注入保护器；旧密码错误、取消、SQLite 触发器拒写、迁移、会话标记保留、遗忘及密文篡改；不是 Android 生物识别测试 |
| 新增 `system_auth_failure_does_not_create_vault_snapshot` | PASS | 取消无凭据快照、密钥不可用不覆盖已有文件；合成保护器 |
| `pnpm --config.verify-deps-before-run=false test` | 16 files / 99 passed，exit 0 | 包含 15 项应用服务测试；新增无 Bob 密码、取消/异常响应、一次性迁移交互 |
| 离线 Vue 页面测试 | 手机 390×844 zh-CN / 桌面 1280×900 en-US，exit 0 | 断言普通解锁无密码框；保存→Work→原项目/目标→发送/审批/取消回归，无外网请求；认证为夹具模拟 |
| `node scripts/version-contract.mjs check` | 0.9.30 / Android 9030 | 主应用/安装器同版本源清单，不代表包已构建 |
| `git diff --check` | exit 0 | Git 换行转换提示不算空白错误 |

离线 UI 复现仍使用 `scripts/export-remote-agent-preview.mjs` + `BOB_UI_OFFLINE=1` 和现有 Playwright 运行 `scripts/test-remote-agent-ui.mjs`。示例已移除“输入任意 12 位密码”，明确系统认证仅模拟。

## Android 未执行边界与候选验收

当前宿主未找到可用 Java/Android SDK 工具链；没有擅自安装、生成 APK、推送 GitHub 或运行发布构建。Windows 编译不检查 Kotlin；以下全部标记 **UNEXECUTED**，不能据本报告标双端通过：

1. 同源 0.9.30 Android CI 编译：本地原生插件被发现、Gradle 资源与 manifest 合并、Rust Android 条件分支编译通过；正式签名沿用现有签名。
2. Android 11+ 新库：系统 PIN/图案/密码认证，保存 DSH 连接，锁库/杀进程再打开；不要求 Bob 密码；原连接与会话保留。
3. 支持强指纹/人脸的设备：系统认证成功后才解封；不支持的人脸不能绕过，系统锁屏凭据仍可用。
4. 取消、认证失败、超时、切后台/销毁 Activity：不误解锁、不新建凭据快照、不覆盖原库、不无休止阻塞；可重新发起。
5. 无系统锁屏、低于 Android 11、Keystore 密钥丢失/失效：明确错误、无明文回退、不自动换 key 覆盖旧数据。破坏性验证只用专用测试安装/数据，不在用户真实库移除锁屏或清除应用数据。
6. 系统认证后 DSH Cookie 过期依然要求更新 DSH Token，不能把系统认证成功当作远端认证成功。
7. 若实际存在旧试验版密码库：正确旧密码一次迁移；取消/错误/存储失败仍能使用原库；一般 0.9.29 用户没有这个迁移步骤。

保护范围：Windows 同一已登录用户下的恶意进程、已入侵的解锁 App 内存不在本地加密保证范围。跨设备备份不携带可直接使用的系统私钥；密钥错误时勿卸载/清除数据。不宣称硬件 Keystore 在所有手机都受硬件支持。

## 设计核对来源

- [Android BiometricPrompt](https://developer.android.com/reference/android/hardware/biometrics/BiometricPrompt)：框架 API 与取消错误码；未使用仅 AndroidX 提供的负按钮错误常量。
- [Android 生物识别与加密](https://developer.android.com/identity/sign-in/biometric-auth)：认证绑定 CryptoObject 与系统凭据组合。
- [Windows CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)：CurrentUser 数据保护边界。
- [Tauri 原生移动插件](https://v2.tauri.app/develop/plugins/develop-mobile/) 及本地 Tauri 2.11.5 原生接口源码：插件注册、线程与返回类型核对。
