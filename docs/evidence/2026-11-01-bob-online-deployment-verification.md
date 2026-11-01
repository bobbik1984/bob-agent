# 2026-11-01 bob-online.bobbik.org 部署更新与在线验证报告

## 1. 部署信息概览

- **部署目标**：Hostinger VPS2 (`187.124.144.87`, `srv1471410`)
- **服务单元**：`/etc/systemd/system/bob-online.service`
- **监听端口**：`127.0.0.1:53020`
- **公网域名**：
  - 主入口：**`https://bob-online.bobbik.org`**（Cloudflare Tunnel → Caddy:80 → 127.0.0.1:53020）
  - Caddy 别名：`http://bob-agent.bobbik.org`（已在 Caddyfile 中配置对应反向代理规则）
- **备份信息**：
  - Web 静态资源备份：`/opt/bob-online/backups/web_dist_1793519641`
  - Caddy 配置备份：`/etc/caddy/Caddyfile.bak_1793519641`

---

## 2. 更新内容与构建验证

### 2.1 前端应用 (`apps/bob-app`)
- **构建输出**：`pnpm run build` (Vite 6.0.0)
- **部署目录**：`/opt/bob-online/web_dist/`
- **关键资源**：
  - `dist/index.html` (404 B)
  - `dist/assets/index-CC1Y7IRH.css` (63.53 kB)
  - `dist/assets/index-COC37_lZ.js` (548.7 kB)
  - `dist/logos/*` (包含 antigravity, claude, codex, deepseek, gemini 等所有品牌与模型图标)

### 2.2 后端内核 (`crates/bob-node` & `crates/bob-runtime`)
- **同步范围**：
  - `Cargo.toml`, `Cargo.lock`
  - `crates/bob-kernel`（含 `migrations/0001.sql` ～ `migrations/0009.sql`）
  - `crates/bob-transport`
  - `crates/bob-runtime`（新增 `subagent_pool.rs`、`project_context.rs`、`claude_harness.rs`、`runtime_rpc.rs` 等）
  - `crates/bob-node`（集成 Claude RPC 与 Subagent 运行时派发）
- **编译构建**：在 VPS2 执行 `~/.cargo/bin/cargo build --release -p bob-node`，顺利完成编译（退出码 0）。
- **进程重启**：`sudo systemctl restart bob-online.service`，新进程 PID `572498` 正常接管。

---

## 3. 现场连通性与接口测试证据

### 3.1 本地回环接口 (`127.0.0.1:53020`)
```bash
$ curl -s http://127.0.0.1:53020/api/capabilities
{"camera_scan":false,"local_process_runner":true,"tls_home_host":true,"os_secret_store":true,"browser_mode":true}

$ curl -I -s http://127.0.0.1:53020/
HTTP/1.1 200 OK
Content-Type: text/html; charset=utf-8
Cache-Control: no-store
Access-Control-Allow-Origin: *
Content-Length: 454
Connection: close
```

### 3.2 Caddy 反向代理网关验证
```bash
# bob-agent.bobbik.org 与 bob-online.bobbik.org 在 Caddy 端均受 common_auth 保护：
$ curl -s -o /dev/null -w '%{http_code}\n' -H 'Host: bob-agent.bobbik.org' http://127.0.0.1:80/
401

$ curl -s -o /dev/null -w '%{http_code}\n' -H 'Host: bob-online.bobbik.org' http://127.0.0.1:80/
401

# API 路由正常穿透 Basic Auth 直通 53020：
$ curl -s -H 'Host: bob-agent.bobbik.org' http://127.0.0.1:80/api/capabilities
{"camera_scan":false,"local_process_runner":true,"tls_home_host":true,"os_secret_store":true,"browser_mode":true}
```

### 3.3 公网 HTTPS 探测
```bash
$ curl.exe -s -I "https://bob-online.bobbik.org"
HTTP/1.1 401 Unauthorized
Date: Sun, 01 Nov 2026 08:05:30 GMT
Connection: keep-alive
Server: cloudflare
www-authenticate: Basic realm="restricted"
```
（注：由于公共 DNS 解析记录中对应 Hostinger VPS2 的公网 CNAME 为 `bob-online.bobbik.org`，浏览器请直接访问 **`https://bob-online.bobbik.org`** 进行测试）。
