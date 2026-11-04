# DSH 节点认证分流修复

用户授权修复、上线及 GitHub 构建 APK；手机测试仍等待通知。

## 原因与修复

X1T 重启后本地 DSH Cookie 仍有效，当前启动令牌可换取 Cookie，Winter Shelter 的 projections/page 直接读取均成功。线上统一设备入口可列出会话，但正文误走 DSH 直连认证并返回 DSH_AUTH_REJECTED。不能据此判定启动令牌发生变化。

共享 Runtime 无凭据探测 `/api/node/v1/info`，按协议识别 Bob 节点。网页在各 DSH 操作前分流；Tauri 列表、正文、预览、发送同样走节点 Bearer 认证。目标节点继续负责本地 DSH Cookie/受控日志恢复。节点请求失败不改走其他设备；手机发送失败禁止跨 peer 自动重发。

## 验证与发布

- 前端控制器 102/102；节点发现回归 1/1（Bob、旧 DSH、失败及探测无凭据）；Vite 与 Linux release 构建通过。
- 使用生产实例既有凭据原位验证，不复制 Vault、不发送模型消息：41 会话，Winter Shelter 最近 10 条，返回连接身份保持。
- 生产程序与网页已更新，实际 RPC 再次通过上述读取检查。
- 程序 SHA256：`d85c1dc7cc4b2f87750db3e2e854ab5c31684ff5085fca9aedf29afcae6aaf9c`。
- 远端发布备份名：`dsh-route-1793815852`；回退恢复该备份的程序及网页，保留当前数据库，不清除配对凭据。
- APK 已由本轮 GitHub 工作流完成，详情见下方；不代表手机安装/真机验收。

## 最终读取闭环

源码 `0154fef09b098a01c6baffb55d641c28f43bba6b`；补齐 Tauri 回执按目标节点核对，以及图片响应从宿主内部连接 ID 映射回调用端连接 ID（路径、session、内容 hash 不改）。

生产网页经临时私有隧道、使用真实后端，在 390px / 1180px 各验证：10 条历史、输入框可见、实际点击图片成功解码、返回原会话、无 pageerror。此证据为生产程序浏览器实测，不冒充手机真机或浏览器公网登录验收。

真实成果 `snapshots/web_winter_started.png` 通过转发读取，确认返回 session / connection 与所选会话一致。

GitHub 最终 APK 工作流 Run `41884589345`（源码 `0154fef`）；前两次运行由本轮主动取消，因为分别被回执与成果身份补丁取代。最终产物校验结果如下。

## APK 交付结果

- GitHub Run `41884589345`：success；Artifact `13351216733`；源码 `0154fef09b098a01c6baffb55d641c28f43bba6b`。
- 固定签名 ARM64 debug APK，包名 `org.bobagent.nextdev`；本机成品 `dist-release/node-routing-0154fef/app-universal-debug.apk`。
- APK SHA256：`7dc2eb99bd8f16c04bceb97ba2a0453bd82d70e11ad006d45120bf76d5a2d3ce`。
- 签名证书 SHA256：`dafb9fbdc736f2c70864a7005cb199b70ab5816a6833e8ec3e5ba743fee338f5`；CI apksigner verify 通过。
- 下载 ZIP 摘要与 GitHub Artifact digest 一致；解包 APK 与 SHA256SUMS 一致；本机再次执行 ELF 16KB 对齐检查通过。CI zipalign -P 16 通过。
- 生产分页追加读取更早 10 条通过；未变化查询返回 not_modified；未发送新模型消息、未修改远端项目文件，未安装手机。
