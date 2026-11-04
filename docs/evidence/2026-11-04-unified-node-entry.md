# 统一设备入口部署与真实验证 — 2026-11-04

用户授权完整实现两个设备域名入口，选择两台共用配对密码，并在私密窗口输入。密码只经 Windows DPAPI 暂存、SSH 标准输入初始化到节点密码摘要；未写入源码、日志、Evidence 或聊天。手机测试仍暂停，未生成 APK。

## 已部署与验证

- 两台设备均以独立用户级 `bob-node-access.service` 常驻，数据目录权限 0700，UMask=0077，linger=yes；节点二进制为 node-access-20261104-r2。未替换旧 Core/DSH/项目文件。
- VPS3 经原 Cloudflare 隧道、Caddy 新增单域名路由与私有容器桥接进入节点；X1T 使用机主已有隧道管理凭据建立 Bob 专用用户隧道，保留原系统隧道与 DSH 域名。实际地址/端口/隧道身份唯一记在 cluster-ops 台账。
- 匿名 RPC=401；正确密码=200；一次性邀请兑换=200；重放=401；设备越权读模型设置=401；撤销后=401。两端公网浏览器实测，均非接口 mock。
- VPS3 真实目录：Claude 8、Pi 7、Codex 20。来源是节点对原生记录的读取；不等同于官方 app-server thread/list 的返回，也不证明这些历史都能原生续聊。未发现独立运行的 VPS3 DSH；3080 实为旧 Dashboard，不能计作 DSH。
- X1T 真实目录：DSH 20、AGY 16、Claude 3、Codex 1；各选一个真实会话读回成功，其中 Winter Shelter 的 DSH 分页返回最近10条。其他原生读取本轮返回历史快照（并非已经全部实现10条分页）。
- X1T DSH 通过本机回环端口及同用户活动进程的受控日志自动恢复 Cookie。发现并修复旧连接器一律拒绝 HTTP 的问题：仅字面回环 HTTP/WS 放行，远端仍强制 TLS。
- Vue 连接流程已接入设置：地址探测、机主验证、保存独立客户端凭证、生成另一份手机邀请。真实本地后端浏览器通过；线上真实应用经认证 SSH 通道完成 VPS3 与 X1T 的连接保存，X1T 保存后列表可见。公网 Bob Online 仍要求既有登录，未绕过或改变其访问策略。
- 线上私有回环读回已确认：新 VPS3 凭证存在且读取35条；新 X1T 凭证存在且读取40条。两台节点更新重启后凭证仍有效。原 VPS3 配置另外返回68条，属于旧入口，未与新节点目录混为同一份验收，未删除旧会话关联。

## 验证及诚实边界

Rust 认证定向2/2、回环地址策略1/1、前端协议4/4通过；Vue生产构建通过；本地浏览器密码连接及二维码显示通过。真实扫码兑换用协议验证，未进行手机摄像头或APK验收。未向用户真实项目会话发送新消息。

仍未验收：逐工具原生续聊、节点持久事件到网页流式、取消/审批、Pi默认凭据长期配置、AGY ACP版本兼容、VPS3 DSH安装/认证、旧入口身份迁移以及手机客户端新版交换逻辑。不能将本次设备入口完成写成整个BP4-06完成。旧 Online 配对接口的历史审查问题不因新节点保护自动消失，需单独迁移；当前新设备入口走node/v1。

## 回退

节点保留r1二进制与 `.service.before-r2`；回退服务文件仅操作本轮bob-node-access，认证库不回退。VPS3 Caddy已保留本轮前备份，新私有桥接可独立停用；X1T新用户隧道独立于原系统隧道。线上前端发布采用新资源后替换index，旧资源保留，原index位于发布备份目录。具体现场路径由cluster-ops台账定位。

## 脱敏检查输出

```json
{
  "vps3": {
    "anonymous": 401,
    "login": 200,
    "exchange": 200,
    "replay": 401,
    "list": 200,
    "list_ok": true,
    "list_error": null,
    "session_counts": {
      "claude-jsonl": 8,
      "pi": 7,
      "codex": 20
    },
    "settings_denied": 401,
    "revoked": 401
  },
  "x1t": {
    "anonymous": 401,
    "login": 200,
    "exchange": 200,
    "replay": 401,
    "list": 200,
    "list_ok": true,
    "list_error": null,
    "session_counts": {
      "dsh": 20,
      "agy": 16,
      "claude-jsonl": 3,
      "codex": 1
    },
    "pages": {
      "dsh": {
        "status": 200,
        "ok": true,
        "error": null,
        "messages": 10
      },
      "agy": {
        "status": 200,
        "ok": true,
        "error": null,
        "messages": 2
      },
      "claude-jsonl": {
        "status": 200,
        "ok": true,
        "error": null,
        "messages": 60
      },
      "codex": {
        "status": 200,
        "ok": true,
        "error": null,
        "messages": 24
      }
    },
    "settings_denied": 401,
    "revoked": 401
  }
}
```

## 当前源码 SHA256

- `crates/bob-node/src/node_access.rs`: `4bc60aaeaf44a21b52f07d984bab4aff017be7e1caa477a9f291cbb77abd0783`
- `crates/bob-node/src/lib.rs`: `d7a85ce3d1bbea8531b091b1bb16e008c5d38bd3497bd6ee7563253be81806ef`
- `crates/bob-runtime/src/node_pairing.rs`: `7b8cd5957d3fae1b9a8a2c042b5d5de9891414089242ae3496e5e21bbb77f530`
- `crates/bob-runtime/src/dsh_transport.rs`: `483fd06fc2fce2e8ec20f402a3de7965709c28190b9b9e1f94bd0cff05cb28f1`
- `apps/bob-app/src/NodeConnect.vue`: `6b39d2612eadf2f422fb49a13962002e7891034187d81ace943bf845cfbb6163`
- `apps/bob-app/src/node-connection.mjs`: `379b256d517c3b381e826453975c4896a17f4a655a7e667bd140568a4e76271f`
