# Track A A2 / SEC-01 R14 本地整改记录

日期：2026-09-23  
状态：`READY_FOR_INDEPENDENT_REVIEW`（执行方本地实现与回归完成；不是独立复核裁决）  
前序裁决：[R13 独立复核](A2-independent-review-20260923.md) 为 `CHANGES_REQUIRED`。

## 对 R13 四项阻断的整改

1. `peer_revocation_outbox` 接入生产 Relay 启动、重连及 15 秒定时循环；`sent` 超过 60 秒未收到确认会回到 `pending`。旧密钥重置、配置中无新设备 ID 时，中继仅以出件箱里的旧 ID 建立撤销专用连接，只接受撤销 Ack，不开放普通 RPC。LAN 可用时另有 30 秒重试循环，经设备名册中匹配目标设备的 IP 调用 `/v1/device/revoke`。两条路径均只在对端签名 CommitAck 验证后标记 `delivered`；发送失败保持可重试。
2. 持久撤销证书不再受五分钟“过去时间”窗口约束；仅拒绝超前五分钟以上的时间戳。事件 ID、Nonce、目标绑定和 Ed25519 签名仍校验，且签名验证先于已处理事件的幂等 Ack，避免伪造事件 ID 索取签名确认。
3. 生产 Relay 分发器与 Test 42 共用 `process_relay_device_revocation_frame` / `process_relay_device_revocation_ack_frame`；撤销专用连接也使用同一 Ack handler。
4. 历史/未知重置状态的数据库事实查询、日志行解析错误均显式返回错误；事实不足或矛盾时保留日志和旧密钥，不再依赖错误文本推断是否可销毁身份。测试注入缺失出件箱表并核验旧密钥不变。

## 本地验证

- `cargo test --lib -- --test-threads=1`：306 passed，0 failed，1 ignored。
- `cargo test --lib test_sec01_ -- --test-threads=1`：68 passed，0 failed。
- `npm test`：10 files / 51 tests passed，exit 0。首次在受限沙箱内断言虽通过，但因既有 `D:\ignore_sync\node\vite-cache` 无写权限导致命令 exit 1；在允许访问既有缓存后复跑 exit 0。
- `git diff --check`：0 错误（Git 对既有文件报告 LF/CRLF 提示，不是格式错误）。

新增回归包含：离线一天后证书仍经真实 HTTP 路由被接收；未来证书被拒；已处理事件的伪造签名被拒；旧身份撤销专用注册、入队、超时重试；局域网仅允许本地、私网和 Tailnet 地址；未知恢复状态和数据库查询故障时密钥保全。原 Test 42 仍是本机模拟 Relay WebSocket 网络，不等同公网 Relay 或真机验收。

## 未越过的门禁

- 未运行 release / Tauri 打包，未进行真实手机—PC 或公网 Relay 验收，未部署、未推送。
- R14 的生产装配仍须由独立审查者复核；A2 不能自行标为 `ACCEPTED`。
- `sha2 = "0.10"` 依赖授权状态保持 `DEPENDENCY_AUTHORIZATION_PENDING`；A3、A4 继续 `LOCKED`，Phase 6 继续 `in_progress`。
