# S1 同包名覆盖安装预检（2026-10-01）

## 用户选择与边界

用户选择用同一个 `bob.agent` 包名覆盖安装，不维护并行安装。候选代码仍来自 v0.9.15 施工线；这不是将 v0.9.16～v0.9.21 提交整体拣选回来。候选包标记为 0.9.22，仅在 `mobile-pc-rebuild` 分支触发云端构建；该分支的签名 APK只上传 CI Artifact，不发布 GitHub Release 或覆盖官网稳定入口。

## 已核实的手机与 APK 事实

- Pixel 8 当前安装 `bob.agent` 0.9.21，Android 包 versionCode 为 9021，APK Signing Scheme 为 v3；`run-as bob.agent` 被系统拒绝，原因是安装包不可调试。
- 已只读提取当前已装 `base.apk` 到忽略版本控制的 `dist-release/pre-rebuild/bob-v0.9.21-installed-base.apk`；大小 91,586,914 字节，SHA-256 为 `14BC812CF19077681AE81CA53128496BEEF09B48E3D3048CBE506DEFC0D5E11A`。这只备份**安装包，不包含应用私有数据**。
- 从已装 APK 与归档 v0.9.15 签名 APK 的 `META-INF/BOBBIK.RSA` 中读取证书，DER 证书 SHA-256 均为 `DAFB9FBDC736F2C70864A7005CB199B70AB5816A6833E8EC3E5BA743FEE338F5`。最终候选仍需独立核对同一证书。
- v0.9.15 与 v0.9.21 的 `src-tauri/src/db.rs` 差异只涉及 busy timeout，不包含新增或删除表的迁移。但其他启动模块仍可能写入共享应用数据；这不能代替完整数据备份证明。

## 安装前尚缺的恢复条件

当前 Android 17 的 ADB 未提供可用的 `adb backup` 命令；已装应用不可 `run-as`，故未取得应用私有数据库与配置的可验证离机备份。CI 候选可以先构建，但**不得仅凭 APK 备份就声称数据可恢复**。安装前至少需取得可安装的更高 versionCode 恢复包，且必须明确向用户说明私有数据仍无独立备份的剩余风险。禁止卸载、清除数据或强制降级。
