# Track A A2 / SEC-01 R15 本地整改：Relay 协议与重置身份守卫

日期：2026-09-23  
状态：`READY_FOR_INDEPENDENT_REVIEW`，仅指本地实现与自动化回归就绪；不是独立复核通过。

## 发现与修复

R14 测试使用会广播任意消息类型的本机模拟 Relay；仓库内的 `bob-relay` 实际只转发 `notify`、`ack`、`commit_ack`、`proxy`。R14 新增的顶层 `device_revocation` / `device_revocation_ack` 因此会被丢弃。生产发送与接收现改为现有 Relay 可转发的 `notify` / `ack` 外层封套，在 `payload` 中放撤销证书或签名 Ack。普通连接和撤销专用连接都识别该封套；测试 Relay 同步施加相同的类型白名单。签名、目标绑定、出件箱仅在有效 CommitAck 后变为 delivered 的约束保持不变。

重置身份入口现于写入日志或修改磁盘之前，对比已解锁旧私钥派生的设备 ID 与配置中的设备 ID；不一致时拒绝。配置已经写入后，`identity_reset_journal` 转为 `config_committed` 必须成功且恰好更新一行，否则拒绝继续销毁旧密钥。新增身份不一致负向测试。

## 实测

- `cargo test --lib test_sec01_relay_revocation -- --test-threads=1`：2 passed，0 failed。
- `cargo test --lib test_sec01_ -- --test-threads=1`：69 passed，0 failed。
- `cargo test --lib -- --test-threads=1`：307 passed，0 failed，1 ignored。
- `pnpm test`：10 files / 51 tests passed，exit 0。首次受限运行的 51 项断言通过，但隔离缓存写入遭 `EPERM`，命令 exit 1；允许访问既有测试缓存后复跑 exit 0。
- `git diff --check`：exit 0；既有 LF/CRLF 提示不是空白错误。

## 未验证与未授权边界

本机模拟 Relay 依据仓库内 `bob-relay` 的转发类型收紧，仍不等于实际部署的 Relay、真实手机—PC 链路或公网链路验收。未执行 release / Tauri 打包、远程部署或推送。`sha2 = "0.10"` 的依赖授权仍待明确确认。A2 仍需独立复核；A3/A4 保持锁定，Phase 6 保持 `in_progress`。
