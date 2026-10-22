# 0.9.30 双端候选交付

## 冻结来源与授权

用户明确授权推送 GitHub 并通过 Actions 生成同版本 Windows / Android 安装包。未授权替换官网稳定下载别名，本轮不进行该操作。

- 源码：`1d5af0e76f88e7473a5073f14f23fde19e711259`，`main`。
- 主应用与安装器版本：`0.9.30`；Android versionCode：`9030`。
- 推送前前端回归：16 files / 99 passed，exit 0。
- 版本契约与 Git 空白检查通过；系统保护本地验证见 [RH-02](RH-02-system-unlock.md)。
- Windows Actions：[39752315689](https://github.com/bobbik1984/bob-agent/actions/runs/39752315689)。
- Android Actions：[39752315657](https://github.com/bobbik1984/bob-agent/actions/runs/39752315657)。
- Windows 与 Android 构建均为 completed / success；Android Sign APK 与 Upload APK Artifact 均 success，不代表真机验收。

## 产物核验

Windows Artifact：`12415165611`，同源版本化名称；归档 SHA-256 `5862fb3e718df69411dfd7a08268995dd75feba0ea61bce7c7e5b7cc5ed2dabc`，下载后匹配 GitHub 返回摘要，ZIP 完整性及 EXE 文件头检查通过。

Android Artifact：`12415656226`；归档 SHA-256 `d5f3c84edbd9e6fc53fa863855137b10f5e419ab920eca808e3acb6ae2e5c404`，完整下载与 GitHub 摘要一致，APK ZIP 完整性通过。Actions 签名步骤包含 `zipalign -c 4` 与 `apksigner verify --verbose`，步骤通过；本地未安装 APK。

下载（需要登录 GitHub，解压 Artifact ZIP 后安装其中版本化文件）：

- [Windows 安装包与便携包](https://github.com/bobbik1984/bob-agent/actions/runs/39752315689/artifacts/12415165611)
- [Android 已签名 APK](https://github.com/bobbik1984/bob-agent/actions/runs/39752315657/artifacts/12415656226)

| 文件 | 字节数 | SHA-256 |
|---|---:|---|
| bob-v0.9.30-installer.exe | 44457472 | `92d2318e340b1a68e1649b47e2ac6fabc6a5f28bdfd9188c14a101cccba3c4e7` |
| bob-v0.9.30-portable.zip | 34221060 | `a687e5c4661973c9bbf3cfcc0c1ae38d7c05e417ac3ccc538267ee76f2aac82e` |
| bob-v0.9.30-signed.apk | 94454114 | `001738339ce98e0795f4e253c3175f6ece483e83a59daa2a1c14e08696243fc9` |

本地目录：`dist-release/candidate-v0.9.30-1d5af0e76f88/`。不执行安装，不替换稳定下载别名。

## 手机系统解锁手工验证

前置：Android 11 或更新系统，已设置系统锁屏 PIN / 图案 / 密码。覆盖安装同签名候选 APK，不卸载、不清除应用数据。无需向任何人提供 PIN、指纹、人脸或 DSH Token。

1. 打开 Bob，确认首屏正常、原项目与工作记录仍可查看。
2. 进入 **设置 → 连接 → 远程执行端 → 使用系统解锁**。应出现系统认证界面，不应要求创建 Bob 密码。
3. 第一次取消认证：应仍为锁定状态，已有连接不得丢失；再次点击应可重新认证。
4. 选择系统 PIN / 图案 / 密码并成功认证：应解锁连接管理界面。支持强生物识别的手机可另外验证指纹或人脸；弱人脸不可用时选择系统锁屏凭据，不属于要求新增 Bob 密码。
5. 首次使用时填写连接名称与 DSH 地址 / Token，保存并连接；进入 **工作** 选择远程会话，发送一条无副作用的测试消息，检查实时回复。保存过的连接应可选择重连，不必再次粘贴 Token（远端认证过期除外）。
6. 手动锁定连接库，再次系统解锁；然后完全退出 Bob 并重开，重复解锁。连接名称、地址和最近会话应保留，不应重复发送上一次指令。
7. 开启连接的自动重连后短暂断网再恢复，检查可恢复网络错误的重连；如果远端 Cookie 已过期，应提示更新认证，不应无限重试或声称连接成功。

系统解锁只解封本机保存的连接凭据，不等于 DSH 已连接；两个步骤分别记录。Windows 使用当前 Windows 登录账户 DPAPI 保护，不要求额外 Bob 密码，也不保证每次弹 Windows Hello。旧未发布试验版密码库才需要一次旧密码迁移，普通 0.9.29 用户不应出现这个步骤。

禁止在日常手机上通过移除系统锁屏、卸载 Bob 或清除数据测试密钥失效；这些负向测试仅适用于专用测试数据。

## 验收边界

Android 系统认证、真机连接及恢复流程尚未手工验收。CI 成功只证明构建与签名步骤通过，不能据此标记上述手工测试 PASS。
