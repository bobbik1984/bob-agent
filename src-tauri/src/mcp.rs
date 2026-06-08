//! MCP (Model Context Protocol) stdio 引擎
//!
//! 管理 MCP Server 子进程的生命周期，通过 stdin/stdout 进行 JSON-RPC 2.0 通信，
//! 发现远程工具并将其合并到 Bob 的工具系统中。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::sync::mpsc;

/// MCP Server 配置（从前端/config 传入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// 一个运行中的 MCP Server 实例
struct McpServerInstance {
    #[allow(dead_code)]
    child: Child,
    stdin_tx: mpsc::Sender<String>,
    /// 缓存的工具 schema 列表（OpenAI function calling 格式）
    tools: Vec<Value>,
    /// 等待响应的 pending requests: id -> oneshot sender
    pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
}

/// MCP 管理器 — 全局单例
pub struct McpManager {
    servers: RwLock<HashMap<String, McpServerInstance>>,
    next_id: Mutex<u64>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    /// 分配唯一的 JSON-RPC request ID
    async fn next_request_id(&self) -> u64 {
        let mut id = self.next_id.lock().await;
        let current = *id;
        *id += 1;
        current
    }

    /// 启动一个 MCP Server 子进程并完成握手
    pub async fn start_server(&self, name: &str, config: &McpServerConfig) -> Result<(), String> {
        // 检查是否已在运行
        {
            let servers = self.servers.read().await;
            if servers.contains_key(name) {
                return Err(format!("MCP server '{}' is already running", name));
            }
        }

        log::info!("[MCP] Starting server '{}': {} {:?}", name, config.command, config.args);

        // 构建子进程
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // 注入环境变量
        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        // Windows: 隐藏控制台窗口
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn '{}': {}", config.command, e))?;

        let stdin = child.stdin.take().ok_or("Failed to capture stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        // stdin 写入通道
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(64);

        // stdin 写入任务
        let mut stdin_writer = stdin;
        let server_name_clone = name.to_string();
        tokio::spawn(async move {
            while let Some(msg) = stdin_rx.recv().await {
                if let Err(e) = stdin_writer.write_all(msg.as_bytes()).await {
                    log::error!("[MCP:{}] stdin write error: {}", server_name_clone, e);
                    break;
                }
                if let Err(e) = stdin_writer.flush().await {
                    log::error!("[MCP:{}] stdin flush error: {}", server_name_clone, e);
                    break;
                }
            }
        });

        // stderr 日志任务
        let server_name_stderr = name.to_string();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::warn!("[MCP:{}:stderr] {}", server_name_stderr, line);
            }
        });

        // pending responses map
        let pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // stdout 读取任务 — 解析 JSON-RPC 响应
        let pending_clone = pending.clone();
        let server_name_stdout = name.to_string();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                // 尝试解析为 JSON-RPC 响应
                match serde_json::from_str::<Value>(&line) {
                    Ok(msg) => {
                        if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
                            // 这是一个响应，找到对应的 pending request
                            let mut pending = pending_clone.lock().await;
                            if let Some(sender) = pending.remove(&id) {
                                let _ = sender.send(msg);
                            }
                        }
                        // 通知类消息（无 id）暂时忽略
                    }
                    Err(e) => {
                        log::debug!("[MCP:{}:stdout] non-JSON line: {} ({})", server_name_stdout, &line[..line.len().min(100)], e);
                    }
                }
            }
            log::info!("[MCP:{}] stdout reader exited", server_name_stdout);
        });

        // 创建实例（工具列表稍后填充）
        let instance = McpServerInstance {
            child,
            stdin_tx: stdin_tx.clone(),
            tools: Vec::new(),
            pending: pending.clone(),
        };

        // 先插入实例
        {
            let mut servers = self.servers.write().await;
            servers.insert(name.to_string(), instance);
        }

        // 握手: initialize
        let init_id = self.next_request_id().await;
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "bob-agent",
                    "version": "0.4.0"
                }
            }
        });

        let init_response = self.send_request(name, init_id, &init_request).await?;
        log::info!("[MCP:{}] initialize response: {}", name, serde_json::to_string(&init_response).unwrap_or_default());

        // 发送 initialized 通知
        let initialized_notif = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.send_notification(name, &initialized_notif).await?;

        // 发现工具: tools/list
        let tools_id = self.next_request_id().await;
        let tools_request = json!({
            "jsonrpc": "2.0",
            "id": tools_id,
            "method": "tools/list",
            "params": {}
        });

        let tools_response = self.send_request(name, tools_id, &tools_request).await?;

        // 解析工具列表并转换为 OpenAI function calling 格式
        let mcp_tools = tools_response
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        let openai_tools: Vec<Value> = mcp_tools
            .iter()
            .filter_map(|tool| {
                let tool_name = tool.get("name")?.as_str()?;
                let description = tool.get("description").and_then(|d| d.as_str()).unwrap_or("");
                let input_schema = tool.get("inputSchema").cloned().unwrap_or(json!({"type": "object", "properties": {}}));

                // 加前缀避免与内建工具冲突: mcp_{server}_{tool}
                let prefixed_name = format!("mcp_{}_{}", name, tool_name);

                Some(json!({
                    "type": "function",
                    "function": {
                        "name": prefixed_name,
                        "description": format!("[MCP:{}] {}", name, description),
                        "parameters": input_schema
                    }
                }))
            })
            .collect();

        log::info!("[MCP:{}] Discovered {} tools", name, openai_tools.len());

        // 更新工具缓存
        {
            let mut servers = self.servers.write().await;
            if let Some(instance) = servers.get_mut(name) {
                instance.tools = openai_tools;
            }
        }

        Ok(())
    }

    /// 发送 JSON-RPC 请求并等待响应
    async fn send_request(&self, server_name: &str, id: u64, request: &Value) -> Result<Value, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        // 注册 pending
        {
            let servers = self.servers.read().await;
            let instance = servers.get(server_name).ok_or(format!("MCP server '{}' not found", server_name))?;
            let mut pending = instance.pending.lock().await;
            pending.insert(id, tx);
        }

        // 发送请求
        let msg = serde_json::to_string(request).map_err(|e| e.to_string())? + "\n";
        {
            let servers = self.servers.read().await;
            let instance = servers.get(server_name).ok_or(format!("MCP server '{}' not found", server_name))?;
            instance.stdin_tx.send(msg).await.map_err(|e| format!("Failed to send to stdin: {}", e))?;
        }

        // 等待响应（10 秒超时）
        match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(format!("MCP server '{}' response channel closed", server_name)),
            Err(_) => {
                // 清理 pending
                let servers = self.servers.read().await;
                if let Some(instance) = servers.get(server_name) {
                    let mut pending = instance.pending.lock().await;
                    pending.remove(&id);
                }
                Err(format!("MCP server '{}' request timed out (10s)", server_name))
            }
        }
    }

    /// 发送 JSON-RPC 通知（无需响应）
    async fn send_notification(&self, server_name: &str, notification: &Value) -> Result<(), String> {
        let msg = serde_json::to_string(notification).map_err(|e| e.to_string())? + "\n";
        let servers = self.servers.read().await;
        let instance = servers.get(server_name).ok_or(format!("MCP server '{}' not found", server_name))?;
        instance.stdin_tx.send(msg).await.map_err(|e| format!("Failed to send notification: {}", e))?;
        Ok(())
    }

    /// 获取所有 MCP 工具的 schema（合并到内建工具列表中）
    pub async fn get_all_tool_schemas(&self) -> Vec<Value> {
        let servers = self.servers.read().await;
        let mut all_tools = Vec::new();
        for instance in servers.values() {
            all_tools.extend(instance.tools.iter().cloned());
        }
        all_tools
    }

    /// 调用 MCP 工具
    /// `full_name` 格式: mcp_{server}_{tool}
    pub async fn call_tool(&self, full_name: &str, args: &Value) -> Value {
        // 解析 server name 和 tool name
        let without_prefix = match full_name.strip_prefix("mcp_") {
            Some(s) => s,
            None => return json!({"error": format!("Invalid MCP tool name: {}", full_name)}),
        };

        // 找到第一个 _ 分隔 server 和 tool
        let (server_name, tool_name) = match without_prefix.find('_') {
            Some(pos) => (&without_prefix[..pos], &without_prefix[pos + 1..]),
            None => return json!({"error": format!("Cannot parse MCP tool name: {}", full_name)}),
        };

        let id = self.next_request_id().await;
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args
            }
        });

        match self.send_request(server_name, id, &request).await {
            Ok(response) => {
                // MCP 工具响应格式: { result: { content: [...] } }
                if let Some(result) = response.get("result") {
                    if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
                        // 提取文本内容
                        let texts: Vec<&str> = content
                            .iter()
                            .filter_map(|item| {
                                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                    item.get("text").and_then(|t| t.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        return json!({"ok": texts.join("\n")});
                    }
                    return json!({"ok": result});
                }
                if let Some(error) = response.get("error") {
                    return json!({"error": error});
                }
                json!({"ok": response})
            }
            Err(e) => json!({"error": e}),
        }
    }

    /// 停止指定 MCP Server
    pub async fn stop_server(&self, name: &str) {
        let mut servers = self.servers.write().await;
        if let Some(mut instance) = servers.remove(name) {
            log::info!("[MCP] Stopping server '{}'", name);
            let _ = instance.child.kill().await;
        }
    }

    /// 停止所有 MCP Server
    pub async fn stop_all(&self) {
        let mut servers = self.servers.write().await;
        for (name, mut instance) in servers.drain() {
            log::info!("[MCP] Stopping server '{}'", name);
            let _ = instance.child.kill().await;
        }
    }

    /// 根据配置启动所有 MCP Server
    pub async fn start_all_from_config(&self, config: &HashMap<String, McpServerConfig>) {
        for (name, server_config) in config {
            match self.start_server(name, server_config).await {
                Ok(()) => log::info!("[MCP] Server '{}' started successfully", name),
                Err(e) => log::error!("[MCP] Failed to start server '{}': {}", name, e),
            }
        }
    }

    /// 获取运行中的 server 名称列表
    pub async fn list_servers(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }
}

/// 全局 MCP 管理器
static MCP_MANAGER: std::sync::OnceLock<McpManager> = std::sync::OnceLock::new();

pub fn get_manager() -> &'static McpManager {
    MCP_MANAGER.get_or_init(McpManager::new)
}

// ═══════════════════════════════════════════════════════════
// IPC 命令
// ═══════════════════════════════════════════════════════════

/// 读取 MCP 配置
#[tauri::command]
pub async fn mcp_get_config() -> Value {
    let config = super::read_config();
    let mcp_servers = config.get("mcpServers").cloned().unwrap_or(json!({}));
    json!({ "mcpServers": mcp_servers })
}

/// 保存 MCP 配置并重启所有 Server
#[tauri::command]
pub async fn mcp_set_config(config: Value) -> Value {
    let mcp_servers = config.get("mcpServers").cloned().unwrap_or(json!({}));

    // 持久化到 config.json
    {
        let mut cfg = super::read_config();
        if let Some(obj) = cfg.as_object_mut() {
            obj.insert("mcpServers".to_string(), mcp_servers.clone());
        }
        super::write_config(&cfg);
    }

    // 停止所有旧的 Server
    let manager = get_manager();
    manager.stop_all().await;

    // 解析配置并启动新的 Server
    if let Ok(servers) = serde_json::from_value::<HashMap<String, McpServerConfig>>(mcp_servers) {
        manager.start_all_from_config(&servers).await;
    }

    json!({ "ok": true })
}

/// 在 Bob 启动时调用，根据已保存的配置启动所有 MCP Server
pub async fn init_from_saved_config() {
    let config = super::read_config();
    let mcp_servers = config.get("mcpServers").cloned().unwrap_or(json!({}));

    if mcp_servers.is_null() || mcp_servers == json!({}) {
        return;
    }

    match serde_json::from_value::<HashMap<String, McpServerConfig>>(mcp_servers) {
        Ok(servers) if !servers.is_empty() => {
            log::info!("[MCP] Loading {} server(s) from saved config", servers.len());
            get_manager().start_all_from_config(&servers).await;
        }
        _ => {}
    }
}
