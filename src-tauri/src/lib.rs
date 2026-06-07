mod llm;
mod filesystem;
mod plugins;
mod web;
mod dream;
mod calendar;
mod sidecar;
mod outbox;
mod scheduler;
mod tools;
mod kb_extractor;
mod kb_indexer;
mod db;
mod http_api;
mod wechat;
mod keychain;
mod browser;
mod doctor;

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use percent_encoding::percent_decode_str;

// ═══════════════════════════════════════════════════════════
// 数据目录与配置管理
// ═══════════════════════════════════════════════════════════

pub(crate) fn get_data_dir() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("bob-agent");
    fs::create_dir_all(&path).unwrap_or_default();
    path
}

/// 日志自动大小截断与轮询旋转写入助手 (T-1310)
pub(crate) fn write_log_with_rotation(log_path: &Path, message: &str, max_size_bytes: u64) {
    if log_path.exists() {
        if let Ok(meta) = fs::metadata(log_path) {
            if meta.len() > max_size_bytes {
                let backup_path = log_path.with_extension("log.bak");
                let _ = fs::rename(log_path, &backup_path);
            }
        }
    }
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = f.write_all(message.as_bytes());
    }
}

/// 知识库 Wiki 目录：优先读取用户在设置面板中配置的 wikiDir，
/// 未配置时 fallback 到 AppData/bob-agent/wiki/
pub(crate) fn get_wiki_dir() -> PathBuf {
    let config = read_config();
    if let Some(wiki_dir) = config.get("wikiDir").and_then(|v| v.as_str()) {
        if !wiki_dir.is_empty() {
            let p = PathBuf::from(wiki_dir);
            let _ = fs::create_dir_all(&p);
            return p;
        }
    }
    let dir = get_data_dir().join("wiki");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn get_config_path() -> PathBuf {
    get_data_dir().join("config.json")
}

fn read_config() -> Value {
    let path = get_config_path();
    if let Ok(data) = fs::read_to_string(path) {
        if let Ok(json) = serde_json::from_str(&data) {
            return json;
        }
    }
    serde_json::json!({})
}

fn write_config(config: &Value) {
    let path = get_config_path();
    if let Ok(data) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, data);
    }
}

// ═══════════════════════════════════════════════════════════
// SQLite 数据库引擎 (已解耦至 db.rs)
// ═══════════════════════════════════════════════════════════

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ═══════════════════════════════════════════════════════════
// Tauri Commands — 配置
// ═══════════════════════════════════════════════════════════

#[tauri::command]
fn system_is_setup_complete() -> bool {
    let config = read_config();
    config.get("onboarded").and_then(|v| v.as_bool()).unwrap_or(false)
        || config.get("model").is_some()
}

#[tauri::command]
fn config_get(key: String) -> Option<Value> {
    let config = read_config();
    config.get(&key).cloned()
}

#[tauri::command]
fn config_set(key: String, value: Value) {
    let mut config = read_config();
    if let Some(obj) = config.as_object_mut() {
        obj.insert(key, value);
    }
    write_config(&config);
}

#[tauri::command]
fn config_get_all() -> Value {
    read_config()
}

// ═══════════════════════════════════════════════════════════
// Tauri Commands — 对话和消息 (已解耦至 db.rs)
// ═══════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════
// Tauri Commands — LLM
// ═══════════════════════════════════════════════════════════

#[tauri::command]
async fn llm_chat(messages: Vec<Value>, conversation_id: Option<String>, app: tauri::AppHandle) -> Value {
    llm::stream_chat(app, messages, conversation_id, None).await
}

#[tauri::command]
async fn llm_vision(messages: Vec<Value>, image_base64: String, conversation_id: Option<String>, app: tauri::AppHandle) -> Value {
    llm::stream_vision(app, messages, image_base64, conversation_id).await
}

#[tauri::command]
fn llm_get_models(provider: Option<String>) -> Value {
    llm::get_models(provider)
}

#[tauri::command]
fn llm_get_model_pool() -> Value {
    llm::get_model_pool()
}

#[tauri::command]
fn llm_get_active_models() -> Value {
    llm::get_active_models()
}

#[tauri::command]
fn llm_assign_model_role(model_id: String, role: String) -> Value {
    llm::assign_model_role(model_id, role)
}

#[tauri::command]
fn system_get_api_keys() -> Value {
    llm::get_api_keys()
}

#[tauri::command]
fn system_set_api_key(provider_id: String, api_key: String) -> Value {
    llm::set_api_key(provider_id, api_key)
}

#[tauri::command]
fn system_add_custom_model(model_id: String, display_name: String, provider: String, base_url: String, api_key: String) -> Value {
    llm::add_custom_model(model_id, display_name, provider, base_url, api_key)
}

#[tauri::command]
fn system_remove_custom_model(model_id: String) -> Value {
    llm::remove_custom_model(model_id)
}

#[tauri::command]
fn llm_rescan_models() -> Value {
    llm::get_models(None) // currently static
}

#[tauri::command]
async fn llm_refresh_models(provider_id: String) -> Value {
    llm::refresh_models_for_provider(provider_id).await
}

#[tauri::command]
fn llm_get_registry() -> Value {
    llm::read_registry()
}

#[tauri::command]
fn llm_save_registry(registry: Value) -> Value {
    llm::write_registry(&registry);
    json!({ "ok": true })
}

// ═══════════════════════════════════════════════════════════
// Tauri Commands — 系统工具 (T-608)
// ═══════════════════════════════════════════════════════════

#[tauri::command]
fn system_get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn system_open_data_dir() -> bool {
    let dir = get_data_dir();
    open_path_in_explorer(dir.to_string_lossy().as_ref())
}

#[tauri::command]
fn system_open_log_dir() -> bool {
    // Tauri 的日志默认存在 data_dir 下
    let dir = get_data_dir().join("logs");
    let _ = fs::create_dir_all(&dir);
    open_path_in_explorer(dir.to_string_lossy().as_ref())
}

#[tauri::command]
fn system_get_log_path() -> String {
    get_data_dir().join("logs").to_string_lossy().to_string()
}

#[tauri::command]
fn system_open_file(file_path: String) -> bool {
    open_path_in_explorer(&file_path)
}

#[tauri::command]
fn system_show_in_folder(file_path: String) -> bool {
    let p = Path::new(&file_path);
    let folder = if p.is_file() {
        p.parent().map(|pp| pp.to_string_lossy().to_string()).unwrap_or(file_path.clone())
    } else {
        file_path.clone()
    };
    open_path_in_explorer(&folder)
}

/// 跨平台打开文件/文件夹
fn open_path_in_explorer(path: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn().is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn().is_ok()
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn().is_ok()
    }
}

#[tauri::command]
fn system_get_tool_statuses() -> Value {
    let config = read_config();
    let api_keys = llm::get_api_keys();
    let keys_map = api_keys.as_object();

    let mut statuses: Vec<Value> = Vec::new();

    // 检查 Tavily
    let tavily_ok = keys_map.map_or(false, |m| {
        m.get("TAVILY_API_KEY").and_then(|v| v.as_str()).map_or(false, |s| !s.is_empty())
    });
    statuses.push(json!({
        "name": "Web Search (Tavily)",
        "isActive": tavily_ok,
        "description": "联网搜索能力",
        "missingCredentials": if tavily_ok { vec![] } else { vec!["TAVILY_API_KEY"] }
    }));

    // 检查 TinyFish
    let tinyfish_ok = keys_map.map_or(false, |m| {
        m.get("TINYFISH_API_KEY").and_then(|v| v.as_str()).map_or(false, |s| !s.is_empty())
    });
    statuses.push(json!({
        "name": "Web Fetch (TinyFish)",
        "isActive": tinyfish_ok,
        "description": "网页内容抓取",
        "missingCredentials": if tinyfish_ok { vec![] } else { vec!["TINYFISH_API_KEY"] }
    }));

    // 检查外部技能目录
    let skills_ok = config.get("externalSkillsDir").and_then(|v| v.as_str()).map_or(false, |s| {
        !s.is_empty() && Path::new(s).exists()
    });
    statuses.push(json!({
        "name": "External Skills",
        "isActive": skills_ok,
        "description": "外部认知技能目录",
        "missingCredentials": if skills_ok { vec![] } else { vec!["externalSkillsDir"] }
    }));

    json!(statuses)
}

// ═══════════════════════════════════════════════════════════
// Tauri Commands — 浏览器增强
// ═══════════════════════════════════════════════════════════

#[tauri::command]
fn system_browser_detect() -> Value {
    let path = browser::detect_browser();
    let enabled = browser::is_browser_enabled();
    json!({
        "detected": path.is_some(),
        "path": path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        "enabled": enabled,
    })
}

#[tauri::command]
fn system_browser_enable() -> Value {
    browser::enable_browser();
    json!({ "ok": true })
}

// ═══════════════════════════════════════════════════════════
// Tauri Commands — Outbox (声明式配置)
// ═══════════════════════════════════════════════════════════

#[tauri::command]
fn system_write_outbox(operations: Vec<Value>) -> Value {
    outbox::write_outbox(operations)
}

// ═══════════════════════════════════════════════════════════
// Tauri Commands — 闪念速记 (Quick Notes)
// ═══════════════════════════════════════════════════════════

#[tauri::command]
fn system_append_quick_note(content: String) -> Value {
    use std::io::Write;
    let path = get_data_dir().join("quick_notes.md");
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let entry = format!("\n- [{}] {}\n", timestamp, content.trim());
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut f) => {
            let _ = f.write_all(entry.as_bytes());
            log::info!("[QuickNote] appended {} chars to {:?}", content.len(), path);
            json!({ "ok": true, "path": path.to_string_lossy() })
        }
        Err(e) => json!({ "error": format!("写入速记失败: {}", e) }),
    }
}

// ═══════════════════════════════════════════════════════════
// Tauri Commands — 知识库与本地引擎路径定位/迁移 (T-1309/T-1310)
// ═══════════════════════════════════════════════════════════

#[tauri::command]
fn system_open_llm_engine_dir(app: tauri::AppHandle) -> bool {
    use tauri::Manager;
    if let Ok(resource_dir) = app.path().resolve("llm-engine", tauri::path::BaseDirectory::Resource) {
        let _ = fs::create_dir_all(&resource_dir);
        open_path_in_explorer(resource_dir.to_string_lossy().as_ref())
    } else {
        false
    }
}

#[tauri::command]
async fn system_migrate_wiki_dir(old_dir: String, new_dir: String, mode: String) -> Result<Value, String> {
    let old_path = if old_dir.is_empty() {
        get_wiki_dir()
    } else {
        std::path::PathBuf::from(old_dir)
    };
    let new_path = std::path::PathBuf::from(new_dir);

    if old_path == new_path {
        return Ok(json!({ "ok": true, "message": "新旧目录相同，无需迁移" }));
    }

    if mode == "copy_merge" || mode == "copy_overwrite" {
        if !new_path.exists() {
            fs::create_dir_all(&new_path).map_err(|e| format!("无法创建目标目录: {}", e))?;
        }
        migrate_directory_recursive(&old_path, &new_path, &mode)?;
    }

    Ok(json!({ "ok": true }))
}

fn migrate_directory_recursive(src: &Path, dst: &Path, mode: &str) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(src).map_err(|e| format!("无法读取目录 {:?}: {}", src, e))? {
        let entry = entry.map_err(|e| format!("读取目录条目失败: {}", e))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dst.join(&file_name);

        if path.is_dir() {
            if !dest_path.exists() {
                fs::create_dir_all(&dest_path)
                    .map_err(|e| format!("无法创建目标目录 {:?}: {}", dest_path, e))?;
            }
            migrate_directory_recursive(&path, &dest_path, mode)?;
        } else {
            if dest_path.exists() {
                if mode == "copy_merge" {
                    let name_str = file_name.to_string_lossy().to_string();
                    if name_str == "index.md" || name_str == "log.md" {
                        merge_list_or_log_file(&path, &dest_path)?;
                    } else if name_str.ends_with(".md") {
                        let old_content = fs::read_to_string(&path)
                            .map_err(|e| format!("无法读取源文件 {:?}: {}", path, e))?;
                        let mut target_content = fs::read_to_string(&dest_path)
                            .map_err(|e| format!("无法读取目标文件 {:?}: {}", dest_path, e))?;
                        
                        target_content.push_str("\n\n---\n\n# 合并的旧文件内容\n\n");
                        target_content.push_str(&old_content);
                        
                        fs::write(&dest_path, target_content)
                            .map_err(|e| format!("无法写入合并文件 {:?}: {}", dest_path, e))?;
                    } else if name_str.ends_with(".json") {
                        merge_json_file(&path, &dest_path)?;
                    } else {
                        fs::copy(&path, &dest_path)
                            .map_err(|e| format!("无法覆盖文件 {:?}: {}", dest_path, e))?;
                    }
                } else {
                    // copy_overwrite
                    fs::copy(&path, &dest_path)
                        .map_err(|e| format!("无法覆盖文件 {:?}: {}", dest_path, e))?;
                }
            } else {
                fs::copy(&path, &dest_path)
                    .map_err(|e| format!("无法复制文件 {:?}: {}", dest_path, e))?;
            }
        }
    }
    Ok(())
}

fn merge_list_or_log_file(src: &Path, dst: &Path) -> Result<(), String> {
    let src_content = fs::read_to_string(src).map_err(|e| format!("无法读取源列表文件: {}", e))?;
    let dst_content = fs::read_to_string(dst).map_err(|e| format!("无法读取目标列表文件: {}", e))?;

    let mut lines: Vec<String> = dst_content.lines().map(|s| s.to_string()).collect();
    for line in src_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") || trimmed.starts_with("> ") || trimmed.is_empty() {
            continue;
        }
        if !lines.iter().any(|l| l.trim() == trimmed) {
            lines.push(line.to_string());
        }
    }

    fs::write(dst, lines.join("\n")).map_err(|e| format!("无法写入合并后的列表文件: {}", e))?;
    Ok(())
}

fn merge_json_file(src: &Path, dst: &Path) -> Result<(), String> {
    let src_content = fs::read_to_string(src).map_err(|e| format!("无法读取源 JSON: {}", e))?;
    let dst_content = fs::read_to_string(dst).map_err(|e| format!("无法读取目标 JSON: {}", e))?;

    if let (Ok(mut src_val), Ok(mut dst_val)) = (
        serde_json::from_str::<Value>(&src_content),
        serde_json::from_str::<Value>(&dst_content),
    ) {
        if src_val.is_object() && dst_val.is_object() {
            let src_obj = src_val.as_object_mut().unwrap();
            let dst_obj = dst_val.as_object_mut().unwrap();
            for (key, val) in src_obj.iter_mut() {
                if !dst_obj.contains_key(key) {
                    dst_obj.insert(key.clone(), val.clone());
                }
            }
            if let Ok(merged) = serde_json::to_string_pretty(&dst_val) {
                fs::write(dst, merged).map_err(|e| format!("无法写入合并后的 JSON: {}", e))?;
            }
        } else if src_val.is_array() && dst_val.is_array() {
            let src_arr = src_val.as_array_mut().unwrap();
            let dst_arr = dst_val.as_array_mut().unwrap();
            dst_arr.extend(src_arr.clone());
            if let Ok(merged) = serde_json::to_string_pretty(&dst_val) {
                fs::write(dst, merged).map_err(|e| format!("无法写入合并后的 JSON: {}", e))?;
            }
        } else {
            fs::copy(src, dst).map_err(|e| format!("无法覆盖 JSON 文件: {}", e))?;
        }
    } else {
        fs::copy(src, dst).map_err(|e| format!("无法覆盖 JSON 文件: {}", e))?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════
// Tauri App 启动
// ═══════════════════════════════════════════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = db::init_db(&get_data_dir());
    let wechat_state = std::sync::Arc::new(wechat::WechatState::new());

    let browser_state = std::sync::Arc::new(browser::BrowserState::new());

    tauri::Builder::default()
        .manage(db::DbState(Mutex::new(db)))
        .manage(sidecar::SidecarState { child: Mutex::new(None) })
        .manage(wechat_state.clone())
        .manage(browser_state.clone())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        // ── bob:// 本地文件协议：让聊天气泡原生渲染本地图片/视频 ──
        .register_uri_scheme_protocol("bob", |_app, request| {
            let uri = request.uri().to_string();
            let raw_path = uri
                .strip_prefix("bob://localhost/")
                .or_else(|| uri.strip_prefix("bob://"))
                .unwrap_or("");
            // 去掉可能的 query string
            let raw_path = raw_path.split('?').next().unwrap_or(raw_path);
            let decoded = percent_decode_str(raw_path).decode_utf8_lossy().to_string();
            let file_path = std::path::Path::new(&decoded);

            if !file_path.exists() || !file_path.is_file() {
                return tauri::http::Response::builder()
                    .status(404)
                    .body(b"Not Found".to_vec())
                    .unwrap();
            }

            let mime = match file_path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref() {
                Some("png")          => "image/png",
                Some("jpg" | "jpeg") => "image/jpeg",
                Some("gif")          => "image/gif",
                Some("webp")         => "image/webp",
                Some("svg")          => "image/svg+xml",
                Some("ico")          => "image/x-icon",
                Some("bmp")          => "image/bmp",
                Some("mp4")          => "video/mp4",
                Some("webm")         => "video/webm",
                Some("mov")          => "video/quicktime",
                Some("pdf")          => "application/pdf",
                _                    => "application/octet-stream",
            };

            match std::fs::read(file_path) {
                Ok(data) => tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", mime)
                    .body(data)
                    .unwrap(),
                Err(_) => tauri::http::Response::builder()
                    .status(500)
                    .body(b"Read Error".to_vec())
                    .unwrap(),
            }
        })
        .invoke_handler(tauri::generate_handler![
            // 配置
            system_is_setup_complete,
            config_get,
            config_set,
            config_get_all,
            // 对话 (来自 db.rs)
            db::db_conversations,
            db::db_conversation_create,
            db::db_conversation_get,
            db::db_conversation_delete,
            db::db_conversation_rename,
            db::db_conversation_update_cost,
            // 消息 (来自 db.rs)
            db::db_messages,
            db::db_message_add,
            db::db_search_messages,
            // LLM & ModelHub
            llm_chat,
            llm_vision,
            llm_get_models,
            llm_get_model_pool,
            llm_get_active_models,
            llm_assign_model_role,
            llm_rescan_models,
            llm_refresh_models,
            llm_get_registry,
            llm_save_registry,
            system_get_api_keys,
            system_set_api_key,
            system_add_custom_model,
            system_remove_custom_model,
            // 文件系统
            filesystem::system_get_file_meta,
            filesystem::system_scan_folder,
            filesystem::system_read_file,
            // 文件夹跟踪
            filesystem::system_get_tracked_folders,
            filesystem::system_add_tracked_folder,
            filesystem::system_remove_tracked_folder,
            // LLM-Wiki 知识库引擎
            kb_extractor::system_estimate_kb,
            kb_indexer::system_build_kb,
            // 插件/技能
            plugins::system_get_plugins,
            // 网页抓取
            web::system_fetch_url,
            // 做梦引擎
            dream::system_summarize_session,
            dream::system_get_dream_report,
            dream::system_dismiss_dream,
            dream::system_get_memory_entries,
            dream::system_delete_memory_entry,
            // 日程管理
            calendar::system_list_events,
            calendar::system_parse_event,
            calendar::system_confirm_event,
            calendar::system_delete_event,
            calendar::system_update_event_status,
            calendar::system_update_event_time,
            // Cron 调度
            scheduler::system_list_cron_jobs,
            scheduler::system_add_cron_job,
            scheduler::system_remove_cron_job,
            scheduler::system_toggle_cron_job,
            // Sidecar
            sidecar::start_offline_engine,
            sidecar::stop_offline_engine,
            sidecar::get_offline_engine_status,
            // 系统工具
            system_get_version,
            system_open_data_dir,
            system_open_log_dir,
            system_get_log_path,
            system_open_file,
            system_show_in_folder,
            system_get_tool_statuses,
            db::system_factory_reset,
            // Outbox (声明式配置)
            system_write_outbox,
            // 闪念速记
            system_append_quick_note,
            // 知识库与本地引擎路径定位/迁移 (T-1309/T-1310)
            system_open_llm_engine_dir,
            system_migrate_wiki_dir,
            // WeChat
            wechat::login_qr::wechat_get_login_qr,
            wechat::login_qr::wechat_check_login_status,
            wechat::login_qr::wechat_get_current_status,
            // 浏览器增强
            system_browser_detect,
            system_browser_enable,
            // Doctor 自检
            doctor::system_health_check,
            doctor::system_auto_fix,
            // 聊天就绪校验
            llm::system_validate_chat_ready,
        ])
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 如果已经有一个实例在运行，就把已有窗口唤出来
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // 开发模式：点 ✕ 真正关闭，方便反复调试
                // 正式打包：点 ✕ 只是隐藏到托盘
                if cfg!(debug_assertions) {
                    // dev mode — 直接关闭
                } else {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
            _ => {}
        })
        .setup(|app| {
            app.handle().plugin(tauri_plugin_shell::init())?;
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // 读取用户上次保存的主题，动态设置原生窗口底色，防止在亮色模式下启动闪黑屏
            if let Some(window) = app.get_webview_window("main") {
                let config = read_config();
                if let Some(theme) = config.get("theme").and_then(|v| v.as_str()) {
                    if theme == "light" {
                        let _ = window.set_theme(Some(tauri::Theme::Light));
                    } else {
                        let _ = window.set_theme(Some(tauri::Theme::Dark));
                    }
                }
            }

            // 启动 Outbox Reconciler 后台守护
            let reconciler_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                outbox::start_reconciler(reconciler_handle).await;
            });

            // 启动 Cron 调度引擎 (T-1211)
            let scheduler_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                scheduler::start_scheduler(scheduler_handle).await;
            });

            // ── 清理遗留的 "vaulted" 标记 ──
            // 架构已从 OS Keychain 迁回 config.json 明文存储。
            // 旧版本中被标记为 "vaulted" 的 Key 实际上已从 config.json 中丢失，
            // 这里将其清除，避免前端显示"已配置"但实际无法读取的幽灵状态。
            {
                let mut cfg = read_config();
                let mut changed = false;
                if let Some(api_keys) = cfg.get_mut("apiKeys").and_then(|v| v.as_object_mut()) {
                    let to_remove: Vec<String> = api_keys.iter()
                        .filter(|(_, v)| v.as_str() == Some("vaulted"))
                        .map(|(k, _)| k.clone())
                        .collect();
                    for k in to_remove {
                        log::info!("Clearing legacy 'vaulted' marker for provider: {}", k);
                        api_keys.remove(&k);
                        changed = true;
                    }
                }
                // 清理旧的单一 apiKey 字段中的 vaulted 标记
                if cfg.get("apiKey").and_then(|v| v.as_str()) == Some("vaulted") {
                    if let Some(obj) = cfg.as_object_mut() {
                        obj.insert("apiKey".to_string(), serde_json::json!(""));
                        changed = true;
                    }
                }
                if changed {
                    write_config(&cfg);
                    log::info!("Legacy 'vaulted' markers cleared from config.json");
                }
            }

            // ── 模型注册表初始化 ──
            llm::init_model_registry(app.handle());
            
            // ── 后台刷新模型列表 ──
            let _refresh_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // 延迟 8 秒，让 UI 先加载完毕
                tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                llm::refresh_models_on_startup().await;
            });

            // ── 内置技能库初始化 ──
            // 记录安装包资源中 skills 目录的绝对路径，用于“双轨合并读取”架构
            {
                use tauri::Manager;
                let mut cfg = read_config();
                let bundled_skills = app.path().resource_dir().map(|r| r.join("skills"));
                if let Ok(src) = bundled_skills {
                    if let Some(obj) = cfg.as_object_mut() {
                        obj.insert(
                            "bundledSkillsDir".to_string(),
                            serde_json::json!(src.to_string_lossy().to_string()),
                        );
                    }
                    write_config(&cfg);
                    log::info!("bundledSkillsDir updated to {:?}", src);
                }
            }

            // ── T-1004: 冷热记忆迁移 (启动时同步执行，极快) ──
            dream::migrate_stale_sessions();

            // ── T-1003: 异步记忆压缩 (后台 Clerk 模型提炼) ──
            let dream_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // 延迟 5 秒再开始压缩，让 UI 先加载完毕
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                dream::compress_sessions_async(dream_handle).await;
            });

            // ── Phase 2: 启动本地 HTTP API (127.0.0.1:3721) ──
            http_api::start_http_server(app.handle().clone());

            // ── 浏览器增强空闲回收 ──
            {
                use tauri::Manager;
                let bs = app.state::<std::sync::Arc<browser::BrowserState>>();
                browser::start_idle_watcher(bs.inner().clone());
            }

            // ── 启动本地微信机器人 ──
            {
                let wechat_state = app.state::<std::sync::Arc<wechat::WechatState>>();
                *wechat_state.app.write().unwrap() = Some(app.handle().clone());
                wechat::monitor::start_monitor(wechat_state.inner().clone());
            }

            // ── T-304: 全局快捷键 Ctrl+Shift+B 唤起窗口 ──
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
                use tauri::Manager;
                let shortcut: Shortcut = "Ctrl+Shift+B".parse().expect("invalid shortcut");
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app_handle, _shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.show();
                                    let _ = window.unminimize();
                                    let _ = window.set_focus();
                                }
                            }
                        })
                        .build(),
                )?;
                app.global_shortcut().register(shortcut)?;
            }

            // ── System Tray Initialization ──
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            use tauri::menu::{Menu, MenuItem};
            use tauri::Manager;

            let quit_i = MenuItem::with_id(app, "quit", "退出 Bob", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "显示面板", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Bob Agent")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        // 退出前杀掉离线引擎
                        let state = app.state::<crate::sidecar::SidecarState>();
                        if let Ok(mut child_lock) = state.child.lock() {
                            if let Some(mut child) = child_lock.take() {
                                let _ = child.kill();
                            }
                        }
                        std::process::exit(0);
                    },
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
