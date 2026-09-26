# Track A A2 / SEC-01 — R15 本地边界复核补充

日期：2026-09-23。状态：`LOCAL_REMEDIATION_VERIFIED / INDEPENDENT_REVIEW_PENDING`，不是 A2 Gate 通过、真机验收或发布声明。

## 发现与修复

R14 的 Relay 注册选择器仅在配置缺少 `device_id` 时使用撤销出件箱中的旧身份。若旧身份的撤销证书尚未收到对端确认，而 App 已配置新身份，连接会直接注册新身份；旧证书仍留在 `pending` 或 `sent`，却失去用旧身份向 Relay 投递的机会。

`src-tauri/src/sync_engine.rs` 的 `resolve_relay_registration_id` 现在先查询未确认的撤销记录。只要存在旧身份，连接严格处于撤销专用模式，不开放普通 RPC；旧记录全部进入 `delivered` 后，才允许以当前配置的新身份恢复普通连接。记录存在但旧身份为空、或数据库查询失败时明确报错，不降级为普通连接。

同一生产选择函数的回归测试覆盖：无新身份时选择旧身份、有新身份但撤销未确认时仍选择旧身份、证书已确认后切换到新身份；既有测试还覆盖入队、超时回退与重投。

第二处复核发现：`recover_device_identity_state` 原先在核对当前配置前就删除旧 journal 指向的密钥文件。如果 App 已生成新身份，而旧 journal 尚未收敛，会误删新密钥。恢复器现先读取受检配置，并比较当前设备 ID 与旧 journal 的被撤销 ID；若内存有解锁密钥，也核对其派生 ID。任一身份不符或配置无法读取时，保留密钥、配置与旧 journal，返回错误以维持准入阻断。既有正常收敛测试改用独立临时配置，避免碰触真实用户配置；新增负向测试分别验证配置已换代与内存密钥已换代时均零删除。

## 最新工作树验证

- `cargo test --lib test_sec01_relay_revocation_outbox_is_queued_and_old_identity_is_revocation_only -- --test-threads=1`：1 passed，0 failed。
- `cargo test --lib test_sec01_old_reset_recovery_preserves_new_identity_before_key_deletion -- --test-threads=1`：1 passed，0 failed。
- `cargo test --lib test_sec01_recover_device_identity_state_convergence -- --test-threads=1`：1 passed，0 failed。
- `cargo test --lib test_sec01_ -- --test-threads=1`：70 passed，0 failed。
- `cargo test --lib -- --test-threads=1`：308 passed，0 failed，1 ignored。
- `pnpm test`：10 个文件、51 passed，退出码 0。
- `git diff --check`：退出码 0；Git 对既有文件仍有 LF/CRLF 转换提示。
- 本机 Android platform-tools 可用，但 `adb devices` 未发现已连接的手机；未安装、修改或打包手机 App。

## 未越过的门禁

这次是执行者自查与本地回归，不是独立复核。未进行真实手机—PC 扫码、跨设备同步或公网 Relay 验收；未进行发布、Git 提交或推送。另有同工作树的 `A2-r15-relay-contract.md` 记录 Relay 类型封套整改，两份证据须一起接受复核，不能相互替代。`sha2 = "0.10"` 的依赖授权仍待用户明确答复。A3 / SEC-02/03 与 A4 / P6-DEVICE 继续锁定。

`todo.md` 与 `progress.yaml` 仍是 OneDrive 重解析占位文件；安全补丁工具拒绝写入，未采用绕过工具改写。因此两处投影仍保留本轮前的 307 / 51 测试数字，最新的 308 / 70 / 51 以本证据及实际测试输出为准，待文件可安全落地后再对齐。
