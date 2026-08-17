use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use walkdir::{DirEntry, WalkDir};

#[tauri::command]
pub fn system_get_file_meta(path: String) -> Value {
    let p = Path::new(&path);
    if !p.exists() {
        return json!({ "exists": false, "error": "File not found" });
    }

    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return json!({ "exists": false, "error": "Failed to read metadata" }),
    };

    let is_dir = meta.is_dir();

    // 提取扩展名
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 映射为前端图标类型
    let file_type = if is_dir {
        "folder"
    } else {
        match ext.as_str() {
            "doc" | "docx" | "pdf" | "txt" | "md" | "rtf" | "odt" => "document",
            "xls" | "xlsx" | "csv" | "ods" => "spreadsheet",
            "js" | "ts" | "rs" | "py" | "html" | "css" | "json" | "yaml" | "toml" | "vue"
            | "jsx" | "tsx" => "code",
            "zip" | "rar" | "7z" | "tar" | "gz" => "archive",
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "ico" => "image",
            "mp4" | "avi" | "mov" | "mkv" | "webm" => "video",
            "ppt" | "pptx" | "odp" => "document",
            _ => "file",
        }
    };

    // 提取修改时间 (毫秒时间戳)
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    json!({
        "exists": true,
        "name": name,
        "ext": ext,
        "type": file_type,
        "size": meta.len(),
        "mtime": mtime,
        "isDir": is_dir,
        "isDirectory": is_dir
    })
}

// 跳过不需要扫描的厚重目录
fn is_hidden_or_ignored(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| {
            s.starts_with('.')
                || s == "node_modules"
                || s == "target"
                || s == "dist"
                || s == "build"
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

    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let size = match fs::metadata(&file_path) {
        Ok(m) => m.len(),
        Err(_) => return json!({ "error": "无法读取文件元数据" }),
    };

    if size > 5 * 1024 * 1024 {
        // 5MB 限制
        return json!({
            "name": name,
            "content": format!("[大文件 {} ({:.2} MB)，为防止内存溢出，已忽略内容提取。]", name, size as f64 / 1024.0 / 1024.0),
            "size": size
        });
    }

    // 交由统一的解析引擎提取文本
    match super::kb_extractor::extract_single_file(p) {
        Ok(mut content) => {
            let original_chars = content.chars().count();
            let limit = 15000;
            if original_chars > limit {
                content = content.chars().take(limit).collect();
                content.push_str("\n\n[⚠️ 系统提示：原文件过长，已截断显示前 15000 字以保护上下文窗口。如果需要对整份长文件进行深度分析检索，请将该文件放入一个独立文件夹，然后将文件夹拖入聊天框以建立知识库。]");
            }
            json!({
                "name": name,
                "content": content,
                "size": size
            })
        }
        Err(e) => json!({
            "error": e,
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
        let mut folders: Vec<String> = obj
            .get("trackedFolders")
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
        let mut folders: Vec<String> = obj
            .get("trackedFolders")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        folders.retain(|f| f != &folder_path);
        obj.insert("trackedFolders".to_string(), json!(folders));
        super::write_config(&config);
    }
    true
}
