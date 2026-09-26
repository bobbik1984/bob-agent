# A2 / SEC-01 — R14 Relay 交互收口（本地实现与验证）

> 2026-09-23；基线 HEAD `d7c582ba584bc69dde2e4a822ba673f27e708a5c`。这是当前脏工作树的执行证据，不是独立复核裁决、真机验收或发布声明。

## 本轮处理

- `trigger_wakeup_via_relay` 改为从受检身份获取发送者，并使用与接收端相同的规范化载荷签署 `wakeup`；测试用同一消息构造函数穿透接收端校验。
- 生产 Relay 往返增加签名回复：回复的设备身份、目标、完整载荷及 `ref_message_id` 一并签署。等待中的请求只有在终态类型、精确请求 ID、预期对端、签名与有效可信会话全部匹配后才释放；配对首轮 ACK 仅允许邀请目标公钥签名的 bootstrap 例外。无引用 ID 的旧 ACK 和未认证的诊断回执不再唤醒请求。
- 既有同步 `pull`、`push`、`push_db` 出站 Proxy 改用同一受检签名入口；出站 RPC 显式包含发送设备身份。缓存回复重新签名时先去掉旧封套，避免哈希不一致。
- 已受信任设备的 `notify` 先建立有效会话并发出签名 ACK，成功后才在运行时名册中标记 `trusted`；会话创建失败只登记为 `discovered`。PoP 的发送者 ID 必须与持钥证明主体一致。
- 生产 Ping 循环与测试共用身份连续性检查函数；认证设备登记的生产包装层与负向测试共用先解析身份、再改动名册的函数，数据库不可用时明确拒止。

## 复验

- `cargo test --lib test_sec01_relay_ -- --test-threads=1`：11 passed，0 failed。
- `cargo test --lib -- --test-threads=1`：305 passed，0 failed，1 ignored（最终工作树复跑）。
- `pnpm test`：10 个文件、51 passed；首次运行测试断言全过但因外置 Vitest 缓存无写权限退出 1，按现有缓存权限复跑后退出 0。
- `git diff --check`：0 whitespace errors；既有多个文件提示 LF/CRLF 工作树转换，未执行自动改写。
- 新增/加强负向断言包括：无签名、冒名、内容篡改、错误请求关联、过期会话、失败诊断回执、配对 ACK 公钥绑定、唤醒载荷篡改、Notify 会话创建失败、配置损坏/身份变更时 Ping 与登记零授权。

## 门禁与残余边界

- 仅本地代码和测试变更；没有远程服务改动、APK/AAB、Release 包、Git 提交或推送。
- 未在真实手机—PC 双端或线上 Relay Python 服务上进行端到端对照；当前仓库不包含该线上服务的可复现转发实现。需要固定双方版本后验证转发是否保留签名覆盖的路由字段。
- R13 独立复核记录仍为 `CHANGES_REQUIRED`。当前工作树的其他撤销 Outbox、长时离线与恢复路径变动不得凭本报告视为独立验收通过；须单独复审。
- 已尝试同步 `todo.md` 与 `progress.yaml`，但两者是 OneDrive 重解析占位文件，安全补丁工具拒绝写入；本轮未绕过保护强制替换。它们仍显示 R13 裁决，待文件安全落地后需按本证据补记 R14 的本地验证状态，不得直接标为 A2 已验收。
- `sha2 = "0.10"` 依赖仍待用户明确授权；A3 / SEC-02/03 与 A4 / P6-DEVICE 继续锁定。

## 交接

`BOB-AGENT-TRACK-A-A2-SEC-01-RELAY-R14`: `LOCAL_IMPLEMENTATION_VERIFIED / INDEPENDENT_REVIEW_PENDING / REAL_DEVICE_NOT_TESTED / PRODUCTION_UNCHANGED / DEPENDENCY_AUTHORIZATION_PENDING / A3_A4_LOCKED`。
