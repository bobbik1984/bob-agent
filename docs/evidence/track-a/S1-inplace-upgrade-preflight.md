# S1 同包名覆盖安装预检（2026-10-01）

## 用户选择与边界

用户选择用同一个 `bob.agent` 包名覆盖安装，不维护并行安装。候选代码仍来自 v0.9.15 施工线；这不是将 v0.9.16～v0.9.21 提交整体拣选回来。候选包标记为 0.9.22，仅在 `mobile-pc-rebuild` 分支触发云端构建；该分支的签名 APK只上传 CI Artifact，不发布 GitHub Release 或覆盖官网稳定入口。

## 已核实的手机与 APK 事实

- Pixel 8 当前安装 `bob.agent` 0.9.21，Android 包 versionCode 为 9021，APK Signing Scheme 为 v3；`run-as bob.agent` 被系统拒绝，原因是安装包不可调试。
- 已只读提取当前已装 `base.apk` 到忽略版本控制的 `dist-release/pre-rebuild/bob-v0.9.21-installed-base.apk`；大小 91,586,914 字节，SHA-256 为 `14BC812CF19077681AE81CA53128496BEEF09B48E3D3048CBE506DEFC0D5E11A`。这只备份**安装包，不包含应用私有数据**。
- 从已装 APK 与归档 v0.9.15 签名 APK 的 `META-INF/BOBBIK.RSA` 中读取证书，DER 证书 SHA-256 均为 `DAFB9FBDC736F2C70864A7005CB199B70AB5816A6833E8EC3E5BA743FEE338F5`。最终候选仍需独立核对同一证书。
- v0.9.15 与 v0.9.21 的 `src-tauri/src/db.rs` 差异只涉及 busy timeout，不包含新增或删除表的迁移。但其他启动模块仍可能写入共享应用数据；这不能代替完整数据备份证明。

## 安装前的恢复条件与剩余风险

当前 Android 17 的 ADB 未提供可用的 `adb backup` 命令；已装应用不可 `run-as`，故未取得应用私有数据库与配置的可验证离机备份。**不得仅凭 APK 备份就声称数据可恢复**。用户在了解这一剩余风险后，明确选择保留数据直接覆盖安装。全程禁止卸载、清除数据或强制降级。

安装前，已从 CI 下载并完整核验 0.9.22 候选包与 0.9.23 前向恢复包：两份 ZIP 的 CRC 检查均无坏成员；签名 APK 的证书 SHA-256 均为 `DAFB9FBDC736F2C70864A7005CB199B70AB5816A6833E8EC3E5BA743FEE338F5`，与手机原装 0.9.21 相同。候选 APK SHA-256 为 `9ABF845DF7C2DB39FA80AA0B7E96B20977EDE10F5C30CD52175678FA6BCC7ABB`；恢复 APK SHA-256 为 `E32DD180A7D551EDCFC42C102EB3BF4942DC99AA128391C642923BD6965156CE`。恢复包只是前向安装应急手段，不是私有数据备份，也尚未在手机上试装。

## 覆盖安装结果

- Pixel 8 在 2026-10-01 使用 `adb install -r` 成功覆盖安装 0.9.22；未使用 `uninstall`、`pm clear` 或降级参数。
- Android 包管理器报告 `versionCode=9022`、`versionName=0.9.22`，`firstInstallTime` 仍为 `2026-09-05 15:58:39`，表明此次是同包升级而非新安装；但这不构成数据库逐项完整性证明。
- 三次启动（首次升级后一次、两次 `force-stop` 后重启）均在约 7 秒观察窗口内进入实际对话界面，不再停在白底蓝 Logo；旧会话条目仍可见。最近日志未检出 Bob 的 FATAL EXCEPTION 或 ANR。截图保存在忽略版本控制的 `dist-release/pre-rebuild/`，不提交包含旧对话的原始图片。
- 这里只验证“现有已配对数据环境中的基本启动与界面呈现”。未做断网、PC 离线、未配对全矩阵、配对恢复或远程指令验收；也未证明后移监听是故障的唯一根因。S1 全部门槛及 A4 仍未通过。
