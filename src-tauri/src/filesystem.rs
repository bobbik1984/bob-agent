use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::{DirEntry, WalkDir};

#[tauri::command]
pub fn system_get_file_meta(path: String) -> Value {
    let p = Path::new(&path);
    if !p.exists() {
        return json!({ "error": "File not found" });
    }

    let name = p.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return json!({ "error": "Failed to read metadata" }),
    };

    let is_dir = meta.is_dir();
    
    json!({
        "name": name,
        "size": meta.len(),
        "isDir": is_dir,
        "isDirectory": is_dir
    })
}

// 跳过不需要扫描的厚重目录
fn is_hidden_or_ignored(entry: &DirEntry) -> bool {
    entry.file_name()
         .to_str()
         .map(|s| {
             s.starts_with('.') || 
             s == "node_modules" || 
             s == "target" || 
             s == "dist" || 
             s == "build"
         })
         .unwrap_or(false)
}

#[tauri::command]
pub fn system_scan_folder(folder_path: String) -> Value {
    let p = Path::new(&folder_path);
    if !p.exists() || !p.is_dir() {
        return json!({ "error": true, "message": "无效的文件夹路径" });
    }

    let mut file_count = 0;
    let mut dir_count = 0;
    let mut total_size: u64 = 0;
    let mut stats: HashMap<String, u32> = HashMap::new();

    // 限制最大遍历深度，防止卡死
    let walker = WalkDir::new(folder_path)
        .max_depth(50)
        .into_iter()
        .filter_entry(|e| !is_hidden_or_ignored(e));

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // 忽略权限不足等错误
        };

        if entry.path().is_dir() {
            // 根目录本身也会被产出，我们减去它
            dir_count += 1;
        } else if entry.path().is_file() {
            file_count += 1;
            
            // 累加大小
            if let Ok(metadata) = entry.metadata() {
                total_size += metadata.len();
            }
            
            // 提取扩展名进行分类统计
            if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                let ext_lower = format!(".{}", ext.to_lowercase());
                *stats.entry(ext_lower).or_insert(0) += 1;
            } else {
                *stats.entry("无后缀".to_string()).or_insert(0) += 1;
            }
        }
        
        // 防御性拦截：文件数过多直接熔断，防止前端渲染崩溃
        if file_count > 50000 {
            break;
        }
    }
    
    // dir_count 包含了顶层目录，为了精确需要减去 1
    let final_dir_count = if dir_count > 0 { dir_count - 1 } else { 0 };

    json!({
        "error": false,
        "fileCount": file_count,
        "dirCount": final_dir_count,
        "totalSize": total_size,
        "stats": stats
    })
}

// ═══════════════════════════════════════════════════════════
// T-606: 文件读取 — 支持拖拽文件进聊天窗口
// ═══════════════════════════════════════════════════════════

/// 读取文本文件内容，附带 500KB 安全上限
#[tauri::command]
pub fn system_read_file(file_path: String) -> Value {
    let p = Path::new(&file_path);
    if !p.exists() {
        return json!({ "error": "文件不存在" });
    }
    if p.is_dir() {
        return json!({ "error": "不能读取文件夹，请使用文件夹扫描" });
    }

    let name = p.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let size = match fs::metadata(&file_path) {
        Ok(m) => m.len(),
        Err(_) => return json!({ "error": "无法读取文件元数据" }),
    };

    // 500KB 安全上限，防止把巨型文件塞进 LLM 上下文
    const MAX_SIZE: u64 = 512 * 1024;
    if size > MAX_SIZE {
        return json!({
            "error": format!("文件过大 ({:.1}MB)，上限 500KB", size as f64 / 1024.0 / 1024.0),
            "name": name,
            "size": size
        });
    }

    match fs::read_to_string(&file_path) {
        Ok(content) => json!({
            "name": name,
            "content": content,
            "size": size
        }),
        Err(_) => json!({
            "error": "文件不是有效的 UTF-8 文本（可能是二进制文件）",
            "name": name,
            "size": size
        }),
    }
}

// ═══════════════════════════════════════════════════════════
// T-607: 文件夹跟踪 — 持久化到 config.json
// ═══════════════════════════════════════════════════════════

#[tauri::command]
pub fn system_get_tracked_folders() -> Value {
    let config = super::read_config();
    config.get("trackedFolders").cloned().unwrap_or(json!([]))
}

#[tauri::command]
pub fn system_add_tracked_folder(folder_path: String) -> bool {
    let mut config = super::read_config();
    let p = Path::new(&folder_path);
    if !p.exists() || !p.is_dir() {
        return false;
    }

    if let Some(obj) = config.as_object_mut() {
        let mut folders: Vec<String> = obj.get("trackedFolders")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        if !folders.contains(&folder_path) {
            folders.push(folder_path);
            obj.insert("trackedFolders".to_string(), json!(folders));
            super::write_config(&config);
        }
    }
    true
}

#[tauri::command]
pub fn system_remove_tracked_folder(folder_path: String) -> bool {
    let mut config = super::read_config();
    if let Some(obj) = config.as_object_mut() {
        let mut folders: Vec<String> = obj.get("trackedFolders")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        folders.retain(|f| f != &folder_path);
        obj.insert("trackedFolders".to_string(), json!(folders));
        super::write_config(&config);
    }
    true
}
