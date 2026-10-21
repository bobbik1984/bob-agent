# 0.9.28 同源双端候选构建

## 已验证事实

- 来源：main 提交 `1f23bb58f57bfb4a65082c51ad76db8f9e3541bb`，冻结 RH-00/RH-01 浏览子集；不含随后的 RH-02 发送/审批/取消工作树。
- 主应用、PC 安装器、Android 版本同为 0.9.28；Android versionCode 9028；版本契约检查通过。
- [Windows CI](https://github.com/bobbik1984/bob-agent/actions/runs/39632877788)：completed / success，包含应用、独立安装器、便携包及 Artifact 上传。
- [Android CI](https://github.com/bobbik1984/bob-agent/actions/runs/39632877750)：completed / success，包含 APK 构建、签名、对齐和签名校验及 Artifact 上传。
- 产物已保存在 GitHub Actions。真实安装、启动及功能验收尚未执行；没有更新官网稳定下载入口。

| 平台 | Artifact ID | ZIP 字节数 | GitHub 返回的 Artifact SHA-256 |
|---|---|---|---|
| Windows | 12361113007 | 71067148 | fca464514e1a4a06072276c497d2ec1f03a97795b2816ba7b20bfcf80f5ef529 |
| Android | 12360613906 | 34687505 | 4369aabce1f5e6ab25d6f0cee170fb0d3210c386eab4243c6bb06a8c744aaeb4 |

以上 Artifact 压缩包已完整下载，并与 GitHub 提供的 SHA-256 逐一校验一致。

## Release 已发布

首次归集因 WinError 10054 中断；随后补齐严格 HTTP Range 校验及完整摘要校验后续传，未跳过任何完整性验证。版本与说明均从冻结提交读取，避免当前 0.9.29 工作树污染旧版归集。

- [候选 Release v0.9.28-rc.1f23bb5](https://github.com/bobbik1984/bob-agent/releases/tag/v0.9.28-rc.1f23bb5)：GitHub 确认 draft=false、prerelease=true，包含 Windows 安装器、便携包、签名 APK、SHA256SUMS.txt。
- 本机归档：`dist-release/candidate-v0.9.28-1f23bb58f57b/`；完整安装文件已校验，不再是残缺下载。
- 未改变官网稳定入口，未将 0.9.29 DSH 执行增量混入冻结包。

| 文件 | SHA-256 |
|---|---|
| bob-v0.9.28-installer.exe | 0cfb56a92b0881c3872d063440eb3bee160f5ca2e80f248765c802b99a57029c |
| bob-v0.9.28-portable.zip | 2e8ac9ad921f43ffcb5f006313d7f7d31d263d40e3573e807896b9d634fccc61 |
| bob-v0.9.28-signed.apk | c809465f6faf49eed88bf75bb887925cdc10218b2e44c7a9c7a2fa4b71be8970 |

## 后续实现边界

RH-02 本地实现见 [原生文字执行证据](RH-02-native-execution.md)，尚未提交到本次冻结候选。无 X1T 现场、无双端安装证据；旧 A4/A5 门禁不改变。
