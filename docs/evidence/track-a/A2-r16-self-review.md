# Track A A2 / SEC-01 R16 自复核与实际 Relay 路由验证

日期：2026-09-23  
范围：本地代码、自复核、隔离网络测试；未触及真实设备、公网服务或发布。

## 裁决

A2 的本地实现与自复核证据通过；这不是独立第三方复核，也不是真机或部署验收。项目既有 `sha2 = "0.10"` 依赖授权仍为 `DEPENDENCY_AUTHORIZATION_PENDING`，因此不把整个 A2 行政门禁伪写成无条件 `ACCEPTED`。A3/A4 保持锁定。

## 本轮发现与修复

1. 在 R15 已将撤销消息改成 `notify` / `ack` 外层封套之后，新增对仓库内**实际** `bob-relay` Axum 路由的本地 WebSocket 测试：直接的旧 `device_revocation` 类型被丢弃，新 `notify.payload.device_revocation` 和 `ack.payload.device_revocation_ack` 均双向送达，发件设备身份由 Relay 填入。此项不再只依赖允许广播任意类型的模拟 Relay。
2. 全目标回归初次运行揭示两项身份重置测试读取进程全局配置、随测试顺序变化。两项测试现持有配置测试互斥锁、使用独立临时配置并在结束时清理。生产身份匹配守卫保持不变。

## 可复现验证

- `cargo check --all-targets`：exit 0，包含 `bob-relay`；存在既有警告。
- `cargo test --all-targets -- --test-threads=1`：库测试 308 passed、0 failed、1 ignored；`bob-relay` 测试 1 passed、0 failed；其他目标 0 项测试。初次运行曾出现 2 项配置隔离失败，整改后完整重跑通过。
- `pnpm test`：10 files、53 passed、0 failed，exit 0。
- `git diff --check`：exit 0；Git 的 LF/CRLF 提示不是空白错误。

## 剩余边界

没有运行 Tauri release 构建、真实手机—PC 配对、公网 Relay 或生产部署；这些不能从本地测试推断。A4 仍承担固定双端版本的真机验收。`sha2` 依赖的显式授权需用户决定；授权前不解除 A3/A4 门禁，也不推送或发布。

## 授权补记（2026-09-23）

用户随后明确答复“这个可以保留”，授权保留 `sha2 = "0.10"`。结合上述已通过的本地自复核与测试，A2 / SEC-01 本地节点完成；A3 / SEC-02/03 可进入本地审计和设计，不代表 A3 已通过。A4 真机验收与候选构建仍未授权、仍锁定。此补记不追溯改变先前测试时间和结果。
