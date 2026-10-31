# Development route audit — 2026-10-31

本文件仅记录审查证据，不是第二份开发计划。当前任务与顺序见 [CURRENT_STAGE](../CURRENT_STAGE.md)。

## 基线和范围

- 分支：1.0.1；HEAD：`899b2bbf1a549658be88ce9dea348899d89dcbd6`。工作区26个tracked修改，2个untracked目录项（gen/components，目录项不代表文件数）。未提交增量不等于该HEAD或已安装APK。
- 读取当前宪法、愿景、架构、功能、组件地图、决定、路线、todo/progress、wiki索引及相关源码、测试和既有Pixel报告。现有功能编号F1–F7，决策含BOB-D-009；未按聊天中的旧目录摘要推断进度。
- 既有Pixel报告：`2026-10-30-stage-s0-s2-pixel8-verification.md`，实现基线db9a9de，APK SHA256 `061d174008887976406c78d190f2f72b7ccf77b21130d0373fbe57f9395fbe62`。本次未独立下载/重装APK；报告主要记录原生IPC及重开读回，未证明本轮全部UI、模型或公网行为。

## 本次检查

- 通过：`node --test apps/bob-app/tests/execution-controller.test.mjs apps/bob-app/tests/kernel-bridge.test.mjs apps/bob-app/tests/pairing-scan-controller.test.mjs apps/bob-app/tests/send-controller.test.mjs apps/bob-app/tests/viewport.test.mjs`，37 passed / 0 failed / 0 skipped。
- Online 用例注入 fetchImpl 返回fixture，不是实际Online HTTP服务或公网验收。Node契约检查也不能证明Rust编译或真实IPC。
- 未运行：Rust编译/全量回归、浏览器、Pixel、模型计费、外部请求、生产核验、打包发布。未读取运行密钥或数据库，未修改业务代码。

## 可定位发现

| 发现 | 当前源码定位 | 结论与对应任务 |
|---|---|---|
| 凭据被组入设备共享配置 | direct_llm.rs 的 export_shared_config_from_dir / export_shared_config_for_sync；home_link.rs PeerSync | 与ARCHITECTURE不变量9冲突；不推断已实际泄漏，RV-13/S0先明确边界 |
| 配置副作用位于包级校验前 | home_host.rs PeerSync分支，merge_shared_config_with_app先于host_receive_peer_sync | 须对无效包证明配置无变化；不能因已有TLS就跳过业务校验 |
| Online凭据边界不完整 | direct_llm.rs build_model_hub_snapshot_and_sync向remoteNodes插入token；app_remote_node_save只处理非空token；App.vue persistOnlineNodeState吞错 | 已有vault而非断言明文存JSON；但renderer接触token、清槽/撤销失败表达待修，OL-02/04 |
| Online历史仅在Vue数组 | App.vue handleOnlineSelectProject / online-client.mjs | JS请求已存在；未完成原合同中的持久缓存、分页与来源一致性，OL-03 |
| 对话升级改变会话身份 | App.vue promoteConversationToWork调用createProject后createConversation | 原条款要求原conversation保留并关联Task；RV-17不能标完成 |
| 退避只完成一部分 | App.vue onMounted setInterval 1500ms，非聊天空闲模4，隐藏暂停 | 不是10–30秒空闲及失败指数退避，RV-18 |
| 快照测试直接写fixture | direct_provider_and_personal_ops.rs canonical_snapshot及std::fs::write | 尚非“字段变化失败”的固定快照门，RV-06 |
| 三端结构未落地 | Cargo.toml排除src-tauri，当前仅kernel/transport；CI为android-dev.yml | XP链路仍待开发；不把现有旧Online当同一Rust服务 |

上述路径无前缀时位于 apps/bob-app/src-tauri/src、apps/bob-app/src 或 crates/bob-kernel/tests；精确职责见COMPONENT_MAP。问题是静态可见差距，除列出的Node结果外未进行动态故障注入。

## 审计时工作区源码 hash

| 文件 | SHA256 |
|---|---|
| `apps/bob-app/src-tauri/resources/model_providers.json` | `bfe7e2df8bdf6dcbdfaeaa4a4e0213f1368753b8b6d25b43eec06fc685adcf09` |
| `apps/bob-app/src-tauri/src/direct_llm.rs` | `30e6d98aed478c06ffd19b2681aab74bea5057d15d2227ca95679c98bae60fdd` |
| `apps/bob-app/src-tauri/src/home_host.rs` | `4b4c849b1ff90600e62bb40b3b3769396e42792f5fd98707e6c186f37ee62271` |
| `apps/bob-app/src-tauri/src/home_link.rs` | `f3303c80a979f4fc2f2b50132f04e5055589520171261a52d2ae5365612239b4` |
| `apps/bob-app/src-tauri/src/lib.rs` | `e66c6d1dfdbbbbcd939c8492fce3a79148367614637361d01a22c3d298a4b846` |
| `apps/bob-app/src-tauri/src/pairing_commands.rs` | `efc3a9aaa86660e6f5cb2973bdd4b9ccdf4eeeb74d3f9dc21be65bd1b65aa9e1` |
| `apps/bob-app/src-tauri/src/pairing_commands/qr.rs` | `1a3db9c673d27362978bef8c34c1456b9cf9b87e6b90df251d257f4b4fa20b8b` |
| `apps/bob-app/src/App.vue` | `32aa4aa3d88d1c4d118011e091c7d247a0e522f546dba9658e59d40e53d2d69a` |
| `apps/bob-app/src/HomeConnection.vue` | `a7e81c2d216669f0efe6b0dc4566b3d46798d6532e0f281636719a89cf57967f` |
| `apps/bob-app/src/ManualPairing.vue` | `0bba74d6f162e8eadd9d3ad17b4ce8f43da209e8127809ce933f2c8638b66af6` |
| `apps/bob-app/src/assets/model_providers.json` | `bfe7e2df8bdf6dcbdfaeaa4a4e0213f1368753b8b6d25b43eec06fc685adcf09` |
| `apps/bob-app/src/kernel-bridge.mjs` | `79eaa2318ea56ff0de85476b48a75d625c5be3f5e786038a21768b8bbadb9e6a` |
| `apps/bob-app/src/locales/en-US.json` | `b1129b0a291d56aefdaad820df95b6fed492e8561b5a5241f2fbfeb749506da7` |
| `apps/bob-app/src/locales/zh-CN.json` | `5dfb7219c556dd67b67f211e94fc7ee4fecc9e65280dab24b7fdc4d962872b5c` |
| `apps/bob-app/src/style.css` | `3fa24da8f709479975ecddec156d78c6c9f95e566a86fc7e4c63064e57b56535` |
| `apps/bob-app/src/views/KnowledgeView.vue` | `e53ff4991609a82149801afe647127e8674f35903da2568249b4df54841f31de` |
| `apps/bob-app/src/views/ScheduleView.vue` | `3685f7a1af56b7db8fe85304f231d86a36903fe6c1ea45bd227f447b60439f24` |
| `apps/bob-app/src/views/WorkView.vue` | `577528779cb60300c16a50aa70749a074ef95e0d65b21c7510256fc5b494ca38` |
| `apps/bob-app/tests/fixtures/personal_snapshot.json` | `86fd277e4611f8130f55017696d7cfc4bf6fc0be959989ea1914d3a10359ab93` |
| `apps/bob-app/tests/ui-browser.py` | `f3ab6c60b90945b380df015b3bb34e980b76d6bcdab60120027a9edf13506417` |
| `crates/bob-kernel/src/home_auth.rs` | `beb1b596debd5df24a93a46fb092a0d859b1d6c3e60580e932bab433489f8f6f` |
| `crates/bob-kernel/src/home_result.rs` | `232568e5b6867c8ab2aacf3413591665b8e4b3a3f367a4bc87c9630253489d30` |
| `crates/bob-kernel/src/personal_ops.rs` | `90c474d6dd36eeff7c9310e5d724e729a0866866ffad2b4094b9b91ed79510d2` |
| `crates/bob-kernel/tests/multi_session_and_handoff.rs` | `0bfd8929150f17f9f4a325fe9375fb852c67c058c1c86d278c1bd8c9a90be216` |
| `crates/bob-kernel/tests/replica.rs` | `394a84c0fab6846f6284871d8c599770d6c3980542502da7134510675b422b3a` |
| `crates/bob-transport/src/control.rs` | `9fbc986f57474f48955810a27827e20a20a2a03defb5fc24f6492ae287bb1ed6` |

## 文档收尾检查

9个新增/修改文档UTF-8读取通过；Markdown相对文件链接无缺失；progress YAML可解析；git diff --check通过。CURRENT_STAGE约11.8KB。审计记录的26个既有源码/测试文件SHA256读回全部一致，本次未改变它们。todo/progress通过project_aligner的reviewed-patch先dry-run再应用，写前比对原hash并保留仓库外备份。未提交Git、未同步Drive、未发布；本轮请求仅涉及本仓库审查与开发路径。

## 首屏与灵感输入实现增量

2026-10-31，基于 1.0.1 HEAD `899b2bbf1a549658be88ce9dea348899d89dcbd6` 的既有脏工作区，保留其他 Agent 代码；本次未提交或部署。

代码证据：legacy import 调用 create_conversation 会更新 selected_conversation；改为 import_conversation(select=false)。并在网页端使用客户端选择，避免历史共享选择残留。尚未直接查询生产“贪食蛇”条目的 ID，不把代码定位写成生产数据库取证。

新增 capture_with_project：保存点覆盖项目、capture、项目 note 和个人同步意图；内容/归属冲突拒绝，重试返回同一记录。前端保存失败不再报成功或清空草稿。

实际验证：
- `cargo test --offline -p bob-kernel` 全部通过，含新增导入重开与原子捕获用例；1 个仅用于子进程的夹具 ignored。
- `cargo test --offline -p bob-node` 2/2。
- 原生 `--lib --features native-tests` 34 passed / 7 ignored；ignored 项没有冒充通过。
- client-view-state 与 kernel-bridge 的 Node 回归 17/17，含产品模板装饰 emoji 检查。
- `apps/bob-app/tests/capture-browser.py --url <isolated-node-url> --output <evidence-dir>`：通过真实 HTTP、真实 SQLite 的 8 项检查。服务无 Provider 凭据，不调用模型；一次 503 为模拟故障，其余保存/读取为真实本机操作。仅对隔离测试库运行脚本。
- 浏览器详细结果：[results.json](ui-capture-20261031/results.json)；[桌面](ui-capture-20261031/capture-desktop.png)、[390 像素](ui-capture-20261031/capture-390.png)、[英文暗色](ui-capture-20261031/capture-390-dark-en.png)。截图内容为专用测试记录。

未验证：生产首屏、Android 真机、项目关联跨 Home 同步、完整发布制品；照片识别未实现，项目搜索待补。未更改生产服务、未上传/同步 Drive、未接触 X1T/B4。

回退：只回退本增量相关 diff（不得整份覆盖同文件中的其他 Agent 改动）；无 schema 迁移，无旧记录删除。客户端视图状态可丢弃而业务记录不受影响。

本次关键源文件 SHA-256：
- `crates/bob-kernel/src/catalog.rs`: `b831424e4f7bac4456ef84a7a27876191ad62d0470fa77701834b3f6baf24c78`
- `crates/bob-kernel/src/personal_ops.rs`: `c5698ccb7f4c2a91a2bd75881d4a45aac612fcde18c5ca14d826bb3a54d339eb`
- `crates/bob-node/src/lib.rs`: `f6bbb5b44d29a06a8d905101a4ee57dd382391e415dd489af072524a42bb54b5`
- `apps/bob-app/src-tauri/src/lib.rs`: `0c346fc27d20e1bc6c99c466d96782da26cf079b34ee7e29eceaecb8207c1591`
- `apps/bob-app/src/App.vue`: `5dac0df10dea21744d662a0c3e47880ae6e670a4b0960699a759d7ddd20f9c51`
- `apps/bob-app/src/style.css`: `eddf62fc4c494bd4ad74e950fd50770f98ba4dc636b7220d7fedf81075a501a4`
- `apps/bob-app/src/client-view-state.mjs`: `5d10ff065551315fd4674a745ace6617a5d01a4a9acded783383093cf307c86a`
- `apps/bob-app/src/kernel-bridge.mjs`: `7bec0927341f4b458d0d45569638ae8d4f9598099f7e8020fc264ae627c1a060`
- `apps/bob-app/tests/capture-browser.py`: `627fa896eb4c480b52381d6f2413b2d1dd24db4442826a216e93b0d5b38df091`

## 图片捕获闭环实现增量

来源：用户本轮明确要求文字/拍照选图/识别/项目归属实装，扫码归设备连接、移除内部 Logo 与蓝圈。基于同一 1.0.1 脏工作区，未覆盖其他 Agent 修改、未提交或上线。

通过：内核全套离线测试（1 ignored）、新增 capture_media 2/2、runtime 3/3、node 2/2、native 34/34（7 ignored）、JS 18/18。真实本机浏览器文字回归 8 项；[图片流程 10 项结果](ui-capture-20261031/photo-results.json)。图像是浏览器生成的可解码设计草图，保存后 SHA-256 与原始 bytes 相等。识别只通过本机假视觉 HTTP 服务，图像发送/响应/校正/保存是真实链路，识别内容是模拟数据；503 为故障注入。没有调用付费模型。

浏览器发现并修复：长图片/建议面板受旧 14vh 上边距影响，底部按钮超出视口；改为视口内居中及受限滚动，并用实际点击验证。设置 modelEntryId 与节点 modelRef 差异同步修复，最终脚本使用前端真实字段。

截图：[桌面](ui-capture-20261031/photo-desktop.png)、[390 像素](ui-capture-20261031/photo-390.png)、[英文暗色](ui-capture-20261031/photo-390-dark-en.png)。图片选择、刷新恢复、失败保留、识别校正、项目笔记、知识页原图均操作通过；并非仅渲染静态 HTML。

未验证：手机相机/权限拒绝/系统图库、外部视觉模型质量、公网及完整发布制品。图片跨设备同步尚未实现；未改变 X1T/B4 暂停状态。照片先保存未引用草稿的回收机制仍待补，当前不自动删除原件。回退前备份 SQLite，保留 0008 新表和原件；旧前端不能消费图片不等于数据已丢失。

本轮新增/关键源码 SHA-256：
- `crates/bob-kernel/src/capture_media.rs`: `d3e978040f57a6e3d20faf0bd3896fddf20bc00b3f1e20ae5f309f4e19c30854`
- `crates/bob-kernel/migrations/0008.sql`: `f8e00cb9b21cfceb6d915829549319745af8d7a9981961878835c48cb89ac33b`
- `crates/bob-runtime/src/vision.rs`: `fec5b40ac502d66f77a20c9d004031c5906233c5209556e16b46f7a7d15b6d15`
- `apps/bob-app/src/App.vue`: `748498bd6bc48680536f4c09f9258ddd61dd8fc3e7e133e465030719fb22b9cc`
- `apps/bob-app/src/components/CaptureImage.vue`: `d1c6cd3284e95105bcc78332d9ca9eeb87c1fb4a5acdb34c4c36d6b550ab6711`
- `apps/bob-app/tests/photo-capture-browser.py`: `824690f1323789563b8302858882b1f6c1d6fee19b5770638d5f8b19fca7a816`
