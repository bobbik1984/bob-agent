# DSH 节点认证分流修复

用户授权修复、上线及 GitHub 构建 APK；手机测试仍等待通知。

## 原因与修复

X1T 重启后本地 DSH Cookie 仍有效，当前启动令牌可换取 Cookie，Winter Shelter 的 projections/page 直接读取均成功。线上统一设备入口可列出会话，但正文误走 DSH 直连认证并返回 DSH_AUTH_REJECTED。不能据此判定启动令牌发生变化。

共享 Runtime 无凭据探测 `/api/node/v1/info`，按协议识别 Bob 节点。网页在各 DSH 操作前分流；Tauri 列表、正文、预览、发送同样走节点 Bearer 认证。目标节点继续负责本地 DSH Cookie/受控日志恢复。节点请求失败不改走其他设备；手机发送失败禁止跨 peer 自动重发。

## 验证与发布

- 前端控制器 102/102；节点发现回归 1/1（Bob、旧 DSH、失败及探测无凭据）；Vite 与 Linux release 构建通过。
- 使用生产实例既有凭据原位验证，不复制 Vault、不发送模型消息：41 会话，Winter Shelter 最近 10 条，返回连接身份保持。
- 生产程序与网页已更新，实际 RPC 再次通过上述读取检查。
- 程序 SHA256：`5eedc15a5efc519809814d2eece23fe6d84e422d556619f75fe1fec99e647c93`。
- 远端发布备份名：`dsh-route-1793814873`；回退恢复该备份的程序及网页，保留当前数据库，不清除配对凭据。
- APK 构建结果待本轮 GitHub 工作流补齐；不代表手机安装/真机验收。
