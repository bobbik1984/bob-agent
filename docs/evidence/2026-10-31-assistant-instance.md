# 默认助理、独立实例与原会话回流验证

日期：2026-10-31。用户批准实际发送、统一本地验证后上线，不打包。允许复用本地已配置API凭据。密钥仅经受保护通道进入Credential Store，无源码/证据输出。

## 基线与制品

- 基线：`4186208ced91839cefe9f1efecb11d032cfa412b` + 保留的未提交修改。没有Git提交或推送；精确源码集合见同名sources.json。
- release：`assistant-instance-20261031`，已部署，未生成APK。
- source SHA256：`29135b622d3fed8408a733a8c1dfe059dd277bf1dd96cd91a0f2c24556c39ca3`
- Linux binary SHA256：`35fa35e57730905684f389a3fa7390bb282b99f81d802b307d00ac0ef3a5a6da`
- web tree SHA256：`a1170ee690d8a9dc944bc30f3e33afb47f86a0349cf33d1e65a622be6049be2e`

## 实现

BOB-D-012明确独立实例和共享对象归属；本轮没有进行全对象owner迁移。持久助理ID由实例origin派生，浏览器复用同一Conversation。网页实际发送进入bob-node assistant模块，配置模型给出结构化reply/clarify/dispatch；目标只能来自现有真实目录，目录不完整不得派发。发送原始用户正文，不将模型建议作为新授权。

迁移0009增加派发来源关联，不另造任务状态机。远端发送前持久保存request/connection/native session，未知回执不自动重发；原生DSH的user.source.rpcId、turn/start、assistant/message及turn/end绑定返回结果。重复序号冲突、替换历史、同轮多请求或中断时不擅自归因。回复以native_session_result保存，展示“原会话回复”，不是检查通过或人工接受。

## 验证

- Windows kernel/runtime/node回归通过；新增内核3项，Runtime 15项、node 2项。内核1项既有子进程夹具跳过。
- Tauri native-tests 34通过、7专项跳过；不是手机真机或APK验收。
- JS定向18项通过。最终Vite构建113模块通过；主bundle略大于500KB的警告保留，未当失败掩盖。
- Linux kernel/runtime/node全套通过，1项既有夹具跳过；release编译通过。0测试的doc-test不计功能验证。
- 统一本地5194使用真实独立bob-node候选、SQLite及现有DSH认证。两轮普通讨论不派发，重载后记住测试标记。
- 一次原DSH会话只读询问：request `d6cd512c-f1ad-4503-9edb-700b8aece520`，session `session-dab36d32-af81-4daf-96e1-2e6692cfaaac`，回复seq9478。会话ID列表前后一致，无新建会话。回复内容与原生结果逐字、序号匹配。
- 重复execute与poll不重复发送或回流；候选重启后同样通过。390px无横向溢出；查看原会话成功，滚动位置17191/17191；刷新保留历史，浏览器page error为空。
- 检查中发现并修复原会话按钮访问Vue ref错误、原生回复误标CLI测试回复、助理标题和侧栏重复项。首轮浏览器断言失败保留为修复经过，最终只读复验通过，没有再次派发。

## 发布与恢复

发布前确认生产无活动/未知执行、无未释放工作区租约，SQLite quick_check通过。隔离启动旧生产binary读取新Schema9数据库通过；只证明本次添加表的读取兼容。

部署备份目录标识 `assistant-instance-1793454637`（现有服务backups下），保留旧binary、web、service unit与停止服务后的data快照。切换后binary/web hash一致，RPC bootstrap/assistant/personal通过，服务active。公网匿名访问401，认证保护未变。上线后通过SSH隧道访问同一生产服务，以390px浏览器发送一次普通问候，真实模型回复“助理已就绪”，无远端派发，刷新保留，page error为空；见同日assistant-production.json及截图。未执行破坏性数据库回滚；需要回退时停止服务、还原备份binary/web并启动，保留加法迁移数据；恢复整库另行审阅，避免丢失上线后消息。

## 范围与不足

- 网页最小闭环已验证；原生App自动派发/结果轮询尚未接入，不将Tauri编译等同能力已到手机。
- DSH结果是来源与回执验证，不是供应商实际模型证明、成果版本/行为验证或用户接受。没有启动B4/Blender。
- 原生会话标题可能滞后：DOS标题的会话实际已继续到其他游戏项目。回复如实显示了这一点。下一步沿BP4-04/06补任务卡提取及真实工作锚点，不能只靠标题判定任务身份。
- 当前转发原文，远端会话可能对“向该会话询问”的元指令产生多余解释；有界任务卡需保留原始指令并区分路由语句，不授权扩大工作范围。
- 当前历史仍是最近200条原始事件窗口；未实现10条可见消息的上翻网络分页。超窗/歧义结果不可伪装已核对。
- 远端取消、未知发送的全部现场故障、手机真机、V1/V2成果治理、多实例所有权迁移仍未验收。
- 公网已登录浏览器独立复验未执行：本轮浏览器验证通过受认证SSH隧道访问同一服务，不能冒充公网登录链路。
