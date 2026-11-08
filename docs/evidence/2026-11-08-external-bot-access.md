# 外部 Bot 权限候选核对（2026-11-08）

授权：本地最小闭环；用户后续明确暂不编译，仅源码及轻量验证。无上线、打包、真实账号发放或外部数据传输。

## 官方接入证据
- [Grok Bot 工作机制](https://docs.x.ai/grok-bot/how-grok-bot-works)：明确支持插件及自定义 MCP。
- [Grok 连接器](https://docs.x.ai/grok/connectors)：可添加 Custom MCP URL；云端需可达地址。账号/团队策略仍需实测。
- [xAI Remote MCP](https://docs.x.ai/developers/tools/remote-mcp)：API 支持远程 MCP；不同于已登录的 Grok Bot 连接器。
- [Meta Model API](https://dev.meta.ai/docs/overview)：Muse Spark API 支持工具调用。此证据不证明 Muse 消费端可自由添加自定义连接器。

## 实施与验证边界
现有工作树含收件箱/UI等未提交修改，本次仅新增 bot_access 模块及在既有 lib.rs 加窄路由。没有调整已有功能逻辑或发布。

Rust 9项用例已写入，未编译运行；本地候选端口探测无可用服务，不声称本地实际闭环通过。轻量检查结果补在本节后。范围只涵盖项目元数据和关联，不包含聊天正文、成果字节、节点执行及任何模型请求。

回退：移除 lib.rs 的 bot_access 模块声明与独立路由以及新增源码/脚本；没有数据库迁移或生产状态变更。未来测试凭证用 revoked=true撤销，不复用机主凭据。

## 已执行轻量检查

- `python -B scripts/test_bot_account.py`：8/8通过；使用临时目录，验证默认只读、仅凭证摘要、禁止覆盖、拒绝通配项目/会话、拒绝执行权限字段、显式整理及有效期上限；Windows私密文件ACL设置成功。测试结束删除测试凭证。
- `rustfmt --emit stdout --config skip_children=true`：bot_access.rs及lib.rs解析返回0；这是语法解析，不是Rust类型检查、编译或运行验证。
- todo经project_aligner reviewed-patch归并，保持未完成；没有修改progress完成度。

尚未执行：9项Rust用例、真实HTTP权限闭环、当前网页登录、Grok/Muse实际连接。未提供可登录账号或可达远程URL。仍需管理界面、完整MCP协议兼容/认证握手和入口隔离审查后才能上线。

## MCP入口与统一业务层复核（2026-11-08）

发现：旧候选已调用LocalApp关联保存，但MCP模块还混有业务/范围检查，原项目检查在事务外。修正为bot_access协议适配 → project_service受限应用操作 → LocalApp授权保存。机主原方法委托Owner授权分支，Bot使用Scoped分支；原项目范围、Owner和CAS在同一SQLite写事务校验。无新增数据库迁移。返回关联字段改显式白名单，拒绝i64::MAX版本，防止expected+1溢出。

认证依然分主体维护：Bot专用凭据不能升级为机主，业务权限上下文不能由请求体反序列化。统一的是本轮关联业务及内核权限事务，不宣称全平台所有RPC已统一或完整MCP兼容已验。

检查：账号Python8/8通过；5个修改Rust文件rustfmt只解析通过；本轮跟踪文件diff --check通过。新增2项内核回归覆盖机主/Bot共享版本与原项目范围、绕过传输层的只读/会话/解绑/溢出拒绝。连同原9项Bot用例均未编译执行。没有真实账号、网络调用、候选启动、生产发布或APK。

后续授权运行验收仍沿CURRENT_STAGE外部Bot节；首次运行应同时执行bob-kernel project_links与bob-node bot_access定向测试及真实HTTP闭环。源码解析不能替代类型检查或运行验收。
