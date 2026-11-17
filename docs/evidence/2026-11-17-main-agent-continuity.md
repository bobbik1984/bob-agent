# 主助理连续性修复证据

- 授权：2026-11-17 用户要求修复、真实对话验证后上线、更新文档及待办；不重复报告生成。
- 生产根因：NativeHarness 未载入持久历史；个人工具无 update；原报告任务失败（旧模型 API ID 400），聊天仍声称后台处理中。
- 候选：VPS2 私有 main-agent-20261117；只发布本轮受测源码快照，不包含工作树中其他并行修改。
- 最终检查：完整 bob-runtime 181/181、bob-node 57/57、前端 Jarvis 18/18，共 256 项通过。两个原有 Claude 假进程用例依赖 python 名称，候选私有 test-bin 将它指向系统 python3 后通过；未安装或启用 Claude。Onboarding 模型列表 fixture 随稳定 API 名称同步。
- 候选构建：release bob-node 与 Vite 完成，build-ok 存在；二进制 SHA256 fc920d557c462943addf5dce528d2bd707bc7c7cdcdc9e1e0632cdaeca51ee59；前端 index-B3kVB7GY.js / index-DSNYEJ8p.css。
- Gemini nullable 参数适配依据官方 function-calling/OpenAPI Schema 约定：https://ai.google.dev/api/generate-content 。
- 生产数据回读：原待办版本 3，截止保持 1794888000000；补全报告提纲，旧备注保留；同名误建 event 删除后计数 0。业务行及配置备份在生产 operator-backups/main-agent-records-20261116-162129（UTC）。
- 生产角色：main/maker = deepseek::deepseek-flash；clerk/flash 保持已有 Gemini。
- 保密与授权：较宽生产复制方案曾被拒绝；用户随后明确授权最小范围临时候选。只在 VPS2 建立目标会话最近 24 条消息、两条目标业务记录、单条失败 ledger 及必要 Gemini/DeepSeek 加密凭据的副本；未复制完整生产库，不向本机导出业务正文或凭据。
- 真实候选：Gemini 原会话读取与承接指代通过；只对候选原待办追加验证标记，ID 不变、版本递增一次、原备注/开始/截止保留、记录数量不变且无 Worker 派发。重复轮询失败任务仅生成一条失败消息。重启候选后 DeepSeek 稳定 API ID 实际收发成功，正确读取追加标记、截止及失败状态。
- 发布：2026-11-17（Asia/Shanghai），VPS2 bob-online 后端与网页已更新。运行二进制 SHA256 与上述候选一致；本机 HTTP 页面 200、前端资源匹配；回退备份标识 quota-device-20261117-012452-51d21cb9（目录时间 UTC）。回退需在停止服务并核对当前版本后恢复该备份的程序/网页；不要为回退界面而覆盖最新业务库。
- 生产真实对话：首轮复合问题只答节点概览，未声称该轮完整通过；聚焦跟进明确答出原待办截止 2026-11-17 12:00、重复日程不存在、原 Worker failed。前后日历未变、无验证标记污染、原待办版本仍 3、无新任务派发、失败消息计数 1。未重跑或重新生成报告。
- 验证清理：候选进程退出、监听关闭；verify-data（含业务资料/加密凭据/原始回复/日志）与 compatibility-data 删除，candidate-probe.log / old-reader-probe.log 删除。仅保留源码、构建及脱敏的 validation-summary、deployment、production-checks、production-focused-checks、cleanup-checks 回执；生产资料与回退备份留在 VPS2。
- 尚未执行：浏览器视觉和手机用户终验。本轮未打包 APK、未推送 GitHub。
