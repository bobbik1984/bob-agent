use serde_json::{json, Value};
use std::fs;

/// T-1304: Bob Doctor — 启动自检引擎
///
/// 提供系统健康检查，返回一组诊断结果：
/// - CONFIG_CORRUPT: config.json 不可读/格式错误
/// - DB_UNREACHABLE: bob.db 不存在或不可访问
/// - NO_API_KEY: 没有配置任何 API Key（主模型无法工作）
/// - NO_MODEL: 没有配置主模型
/// - DISK_NOT_WRITABLE: 数据目录不可写
/// - WIKI_DIR_MISSING: 知识库目录不存在

#[derive(Clone, Debug)]
struct CheckResult {
    code: String,
    severity: String, // "error" | "warning"
    message: String,
    fixable: bool,
}

impl CheckResult {
    fn to_json(&self) -> Value {
        json!({
            "code": self.code,
            "severity": self.severity,
            "message": self.message,
            "fixable": self.fixable,
        })
    }
}

/// 执行全面健康检查
#[tauri::command]
pub fn system_health_check() -> Value {
    let data_dir = super::get_data_dir();
    let mut results: Vec<CheckResult> = Vec::new();

    // 1. config.json 可读性
    let config_path = data_dir.join("config.json");
    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                if serde_json::from_str::<Value>(&content).is_err() {
                    results.push(CheckResult {
                        code: "CONFIG_CORRUPT".into(),
                        severity: "error".into(),
                        message: "配置文件 config.json 格式损坏，无法解析".into(),
                        fixable: true,
                    });
                }
            }
            Err(_) => {
                results.push(CheckResult {
                    code: "CONFIG_CORRUPT".into(),
                    severity: "error".into(),
                    message: "配置文件 config.json 无法读取".into(),
                    fixable: true,
                });
            }
        }
    }
    // config 不存在不算错误，首次启动正常

    // 2. bob.db 存在性
    let db_path = data_dir.join("bob.db");
    if !db_path.exists() {
        results.push(CheckResult {
            code: "DB_UNREACHABLE".into(),
            severity: "warning".into(),
            message: "数据库 bob.db 尚不存在，对话历史将在首次聊天后创建".into(),
            fixable: false,
        });
    }

    // 3. API Key 检查 — 必须至少有一个 provider 配置了 key
    let config = super::read_config();
    let has_any_key = config.get("apiKeys")
        .and_then(|v| v.as_object())
        .map(|m| m.values().any(|v| {
            v.as_str().map_or(false, |s| !s.is_empty() && s != "vaulted")
        }))
        .unwrap_or(false);

    if !has_any_key {
        results.push(CheckResult {
            code: "NO_API_KEY".into(),
            severity: "error".into(),
            message: "未配置任何 API Key，Bob 无法与大模型对话".into(),
            fixable: false,
        });
    }

    // 4. 主模型配置
    let has_model = config.get("model")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    if !has_model {
        results.push(CheckResult {
            code: "NO_MODEL".into(),
            severity: "warning".into(),
            message: "未选择主模型，Bob 将使用默认模型".into(),
            fixable: false,
        });
    }

    // 5. 数据目录可写性
    let test_file = data_dir.join(".doctor_write_test");
    match fs::write(&test_file, "test") {
        Ok(_) => { let _ = fs::remove_file(&test_file); }
        Err(_) => {
            results.push(CheckResult {
                code: "DISK_NOT_WRITABLE".into(),
                severity: "error".into(),
                message: format!("数据目录 {:?} 不可写", data_dir),
                fixable: false,
            });
        }
    }

    // 6. Wiki 知识库目录
    let wiki_dir = super::get_wiki_dir();
    if !wiki_dir.exists() {
        results.push(CheckResult {
            code: "WIKI_DIR_MISSING".into(),
            severity: "warning".into(),
            message: format!("知识库目录 {:?} 不存在", wiki_dir),
            fixable: false,
        });
    }

    // 构建返回值
    let issues: Vec<Value> = results.iter().map(|r| r.to_json()).collect();
    let has_errors = results.iter().any(|r| r.severity == "error");
    let has_warnings = results.iter().any(|r| r.severity == "warning");

    json!({
        "healthy": !has_errors,
        "severity": if has_errors { "error" } else if has_warnings { "warning" } else { "ok" },
        "issues": issues,
    })
}

/// 尝试自动修复指定的问题
#[tauri::command]
pub fn system_auto_fix(code: String) -> Value {
    match code.as_str() {
        "CONFIG_CORRUPT" => {
            let data_dir = super::get_data_dir();
            let config_path = data_dir.join("config.json");
            let backup_path = data_dir.join("config.json.bak");

            // 尝试从备份恢复
            if backup_path.exists() {
                if let Ok(content) = fs::read_to_string(&backup_path) {
                    if serde_json::from_str::<Value>(&content).is_ok() {
                        if fs::copy(&backup_path, &config_path).is_ok() {
                            log::info!("T-1304: config.json restored from backup");
                            return json!({
                                "ok": true,
                                "message": "已从备份恢复配置文件"
                            });
                        }
                    }
                }
            }

            // 备份不存在或也已损坏，重新创建空配置
            let empty_config = json!({});
            if let Ok(data) = serde_json::to_string_pretty(&empty_config) {
                if fs::write(&config_path, data).is_ok() {
                    log::info!("T-1304: config.json reset to empty");
                    return json!({
                        "ok": true,
                        "message": "配置文件已重置为初始状态，请重新配置 API Key 和模型"
                    });
                }
            }

            json!({ "ok": false, "message": "修复失败: 无法写入配置文件" })
        }
        _ => {
            json!({ "ok": false, "message": format!("不支持自动修复: {}", code) })
        }
    }
}
