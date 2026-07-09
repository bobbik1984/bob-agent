use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

/// 里程碑 8: 声明式配置 + 单向调谐 (Outbox/Reconciler)
///
/// 架构概要:
///   - AI (LLM) 通过前端写入 Outbox 文件声明配置变更意图
///   - Rust 后台守护者 (Reconciler) 单向轮询读取 → Schema 校验 → 合并到 config.json
///   - AI 永远不能直接修改 config.json（安全边界）
///
/// 文件:
///   - Outbox:     data_dir/outbox/bob_outbox.json (AI 写)
///   - Config:     data_dir/config.json            (Reconciler 写)
///   - Backup:     data_dir/config.bak.json        (Reconciler 自动备份)
///   - Audit Log:  data_dir/logs/reconciler.log    (Reconciler 审计)

// ═══════════════════════════════════════════════════════════
// 白名单常量 — 安全防火墙的核心
// ═══════════════════════════════════════════════════════════

/// 允许的 Outbox 操作类型
const ALLOWED_OPS: &[&str] = &[
    "set_api_key",        // 设置/删除 API Key
    "set_config",         // 修改通用配置项
    "add_tracked_folder", // 添加关注文件夹
    "install_skill_from_url", // 下载技能包
];

/// 允许通过 Outbox 修改的 config key (set_config 操作的二级白名单)
const SAFE_CONFIG_KEYS: &[&str] = &[
    "model",        // 主力模型
    "clerkModel",   // 助理模型
    "theme",        // 主题
    "uiScale",      // UI 缩放
    "language",     // 语言
    "workspaceDir", // 工作目录
];

/// 允许的模型供应商 ID (set_api_key 操作的二级白名单)
const KNOWN_PROVIDERS: &[&str] = &[
    "deepseek", "openai", "qwen", "doubao", "zhipu", "kimi", "minimax", "tavily", "tinyfish",
];

/// API Key 最小有效长度
const MIN_KEY_LENGTH: usize = 8;

// ═══════════════════════════════════════════════════════════
// 路径管理
// ═══════════════════════════════════════════════════════════

fn get_outbox_dir() -> PathBuf {
    let dir = super::get_data_dir().join("outbox");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn get_outbox_path() -> PathBuf {
    get_outbox_dir().join("bob_outbox.json")
}

fn get_backup_config_path() -> PathBuf {
    super::get_data_dir().join("config.bak.json")
}

fn get_audit_log_path() -> PathBuf {
    let dir = super::get_data_dir().join("logs");
    let _ = fs::create_dir_all(&dir);
    dir.join("reconciler.log")
}

// ═══════════════════════════════════════════════════════════
// T-802: Outbox 写入
// ═══════════════════════════════════════════════════════════

/// 将操作列表写入 Outbox 文件
/// 由前端 IPC 调用 (AI 通过 bob-config 代码块触发)
pub fn write_outbox(operations: Vec<Value>) -> Value {
    if operations.is_empty() {
        return json!({ "ok": false, "error": "操作列表为空" });
    }

    let outbox = json!({
        "$schema": "bob-agent/outbox/v1",
        "timestamp": super::now_ms(),
        "source": "llm-tool-call",
        "operations": operations
    });

    let path = get_outbox_path();
    let tmp_path = path.with_extension("tmp");
    match serde_json::to_string_pretty(&outbox) {
        Ok(data) => match fs::write(&tmp_path, data) {
            Ok(_) => match fs::rename(&tmp_path, &path) {
                Ok(_) => json!({ "ok": true, "path": path.to_string_lossy().to_string() }),
                Err(e) => {
                    let _ = fs::remove_file(&tmp_path);
                    json!({ "ok": false, "error": format!("原子写入重命名失败: {}", e) })
                }
            },
            Err(e) => json!({ "ok": false, "error": format!("写入临时 Outbox 失败: {}", e) }),
        },
        Err(e) => json!({ "ok": false, "error": format!("序列化失败: {}", e) }),
    }
}

// ═══════════════════════════════════════════════════════════
// T-803: 操作校验防火墙
// ═══════════════════════════════════════════════════════════

/// 校验单个操作是否合法
/// 返回 Ok(()) 表示通过, Err(reason) 表示拒绝
fn validate_operation(op: &Value) -> Result<(), String> {
    // 1. 必须有 op 字段
    let op_type = op
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or("缺少 'op' 字段")?;

    // 2. op 类型必须在白名单
    if !ALLOWED_OPS.contains(&op_type) {
        return Err(format!("未知操作类型: '{}'", op_type));
    }

    // 3. 按操作类型进行参数校验
    match op_type {
        "set_api_key" => {
            let provider = op
                .get("provider")
                .and_then(|v| v.as_str())
                .ok_or("set_api_key 缺少 'provider' 字段")?;

            if !KNOWN_PROVIDERS.contains(&provider) {
                return Err(format!("未知供应商: '{}'", provider));
            }

            let value = op
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or("set_api_key 缺少 'value' 字段")?;

            if value.len() < MIN_KEY_LENGTH {
                return Err(format!("API Key 长度不足 (最小 {} 字符)", MIN_KEY_LENGTH));
            }

            // 基本安全检查: 不允许包含换行符或控制字符
            if value.chars().any(|c| c.is_control()) {
                return Err("API Key 包含非法控制字符".to_string());
            }

            Ok(())
        }
        "set_config" => {
            let key = op
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or("set_config 缺少 'key' 字段")?;

            if !SAFE_CONFIG_KEYS.contains(&key) {
                return Err(format!("config key '{}' 不在安全白名单中", key));
            }

            if op.get("value").is_none() {
                return Err("set_config 缺少 'value' 字段".to_string());
            }

            Ok(())
        }
        "add_tracked_folder" => {
            let path = op
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("add_tracked_folder 缺少 'path' 字段")?;

            if path.contains("..") {
                return Err("路径中包含非法字符 (..)".to_string());
            }

            Ok(())
        }
        "install_skill_from_url" => {
            let url = op.get("url").and_then(|v| v.as_str()).ok_or("缺少 'url' 字段")?;
            if !url.starts_with("http") {
                return Err("url 必须以 http 开头".to_string());
            }
            Ok(())
        }
        _ => Err(format!("未实现的操作验证: '{}'", op_type)),
    }
}

// ═══════════════════════════════════════════════════════════
// T-804: 核心调谐逻辑
// ═══════════════════════════════════════════════════════════

/// 读取 Outbox → 逐条校验 → 合并到 config.json
/// 返回成功应用的操作数量
fn reconcile() -> Result<usize, String> {
    let outbox_path = get_outbox_path();

    // 读取 Outbox
    let content =
        fs::read_to_string(&outbox_path).map_err(|e| format!("读取 Outbox 失败: {}", e))?;

    let outbox: Value =
        serde_json::from_str(&content).map_err(|e| format!("Outbox JSON 解析失败: {}", e))?;

    let operations = outbox
        .get("operations")
        .and_then(|v| v.as_array())
        .ok_or("Outbox 缺少 'operations' 数组")?;

    if operations.is_empty() {
        // 空操作列表，直接清理
        let _ = fs::remove_file(&outbox_path);
        return Ok(0);
    }

    // 备份当前 config
    let config_path = super::get_config_path();
    if config_path.exists() {
        let _ = fs::copy(&config_path, get_backup_config_path());
    }

    let mut config = super::read_config();
    let mut applied = 0usize;
    let mut audit_entries: Vec<String> = Vec::new();
    let timestamp = chrono_like_now();

    audit_entries.push(format!(
        "[{}] RECONCILE START — outbox contains {} operations",
        timestamp,
        operations.len()
    ));

    for op in operations {
        match validate_operation(op) {
            Ok(()) => {
                apply_operation(&mut config, op);
                let op_desc = describe_operation(op);
                audit_entries.push(format!("[{}] ✅ APPLIED: {}", timestamp, op_desc));
                applied += 1;
            }
            Err(reason) => {
                let op_desc = describe_operation(op);
                audit_entries.push(format!(
                    "[{}] ❌ REJECTED: {} — {}",
                    timestamp, op_desc, reason
                ));
            }
        }
    }

    // 写入 config (仅当有成功操作时)
    if applied > 0 {
        super::write_config(&config);
    }

    audit_entries.push(format!(
        "[{}] RECONCILE DONE — {} applied, {} rejected",
        timestamp,
        applied,
        operations.len() - applied
    ));

    // 写入审计日志 (追加模式)
    write_audit_log(&audit_entries);

    // 清理 Outbox (已消费)
    let _ = fs::remove_file(&outbox_path);

    Ok(applied)
}

/// 将校验通过的操作应用到 config
fn apply_operation(config: &mut Value, op: &Value) {
    let op_type = op.get("op").and_then(|v| v.as_str()).unwrap_or("");

    match op_type {
        "set_api_key" => {
            let provider = op.get("provider").and_then(|v| v.as_str()).unwrap_or("");
            let value = op.get("value").and_then(|v| v.as_str()).unwrap_or("");

            // 存入 OS Keychain
            let marker = match super::keychain::store_key(provider, value) {
                Ok(()) => "vaulted",
                Err(e) => {
                    log::warn!(
                        "Outbox keychain store failed for {}: {} — falling back to plaintext",
                        provider,
                        e
                    );
                    value // 降级：直接存明文
                }
            };

            if let Some(cfg_obj) = config.as_object_mut() {
                // 确保 apiKeys 对象存在
                if !cfg_obj.contains_key("apiKeys") {
                    cfg_obj.insert("apiKeys".to_string(), json!({}));
                }
                if let Some(keys) = cfg_obj.get_mut("apiKeys").and_then(|v| v.as_object_mut()) {
                    if value.is_empty() {
                        keys.remove(provider);
                    } else {
                        keys.insert(provider.to_string(), json!(marker));
                    }
                }
            }
        }
        "install_skill_from_url" => {
            let url = op.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let config_copy = config.clone();
            tauri::async_runtime::spawn(async move {
                let skills_dir = config_copy.get("externalSkillsDir").and_then(|v| v.as_str()).map(|s| std::path::PathBuf::from(s))
                    .unwrap_or_else(|| crate::get_data_dir().join("skills"));
                
                if let Ok(resp) = reqwest::get(&url).await {
                    if let Ok(bytes) = resp.bytes().await {
                        let _ = crate::skills_sync::unpack_skills(&bytes, &skills_dir);
                        log::info!("[Outbox] Successfully installed skill from URL: {}", url);
                    }
                }
            });
        }
        "set_config" => {
            let key = op.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let value = op.get("value").cloned().unwrap_or(Value::Null);

            if let Some(cfg_obj) = config.as_object_mut() {
                cfg_obj.insert(key.to_string(), value);
            }
        }
        "add_tracked_folder" => {
            let path = op.get("path").and_then(|v| v.as_str()).unwrap_or("");

            if let Some(cfg_obj) = config.as_object_mut() {
                let mut folders: Vec<String> = cfg_obj
                    .get("trackedFolders")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                if !folders.contains(&path.to_string()) {
                    folders.push(path.to_string());
                    cfg_obj.insert("trackedFolders".to_string(), json!(folders));
                }
            }
        }
        _ => {} // 已被 validate_operation 拦截，不会到这里
    }
}

/// 生成操作的人类可读描述 (用于审计日志)
fn describe_operation(op: &Value) -> String {
    let op_type = op.get("op").and_then(|v| v.as_str()).unwrap_or("unknown");
    match op_type {
        "set_api_key" => {
            let provider = op.get("provider").and_then(|v| v.as_str()).unwrap_or("?");
            format!("set_api_key(provider={})", provider)
        }
        "set_config" => {
            let key = op.get("key").and_then(|v| v.as_str()).unwrap_or("?");
            format!("set_config(key={})", key)
        }
        "add_tracked_folder" => {
            let path = op.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("add_tracked_folder(path={})", path)
        }
        "install_skill_from_url" => {
            let url = op.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            format!("install_skill_from_url(url={})", url)
        }
        _ => format!("{}(?)", op_type),
    }
}

// ═══════════════════════════════════════════════════════════
// T-805: 后台轮询守护
// ═══════════════════════════════════════════════════════════

/// 启动 Reconciler 后台守护循环
/// 每 2 秒检查一次 Outbox 文件，如果存在则消费
pub async fn start_reconciler(app: AppHandle) {
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(2));

    loop {
        ticker.tick().await;

        let outbox_path = get_outbox_path();
        if !outbox_path.exists() {
            continue;
        }

        match reconcile() {
            Ok(count) => {
                if count > 0 {
                    log::info!("Reconciler: {} operations applied successfully", count);
                    let _ = app.emit("config:reconciled", json!({ "applied": count }));
                }
            }
            Err(e) => {
                log::warn!("Reconciler: reconcile failed: {}", e);
                // 损坏的 Outbox 也要清理，避免无限重试
                let _ = fs::remove_file(&outbox_path);
                // 记录错误到审计日志
                let timestamp = chrono_like_now();
                write_audit_log(&[format!(
                    "[{}] ❌ RECONCILE ERROR: {} — outbox removed",
                    timestamp, e
                )]);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// T-807: 审计日志
// ═══════════════════════════════════════════════════════════

/// 追加审计日志条目
fn write_audit_log(entries: &[String]) {
    let path = get_audit_log_path();
    let mut content = String::new();

    for entry in entries {
        content.push_str(entry);
        content.push('\n');
    }

    super::write_log_with_rotation(&path, &content, 5 * 1024 * 1024);
}

/// 简易时间戳生成 (避免引入 chrono 依赖, 复用 calendar.rs 的方案)
fn chrono_like_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // UTC+8 (北京时间)
    let now = now + 8 * 3600;

    let days = now / 86400;
    let time_of_day = now % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let mut y = 1970i64;
    let mut remaining = days as i64;

    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let months_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 1;
    for &md in &months_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y,
        m,
        remaining + 1,
        hours,
        minutes,
        seconds
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}
