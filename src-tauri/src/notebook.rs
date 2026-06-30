
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct NoteFrontmatter {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
}

#[derive(Serialize)]
pub struct NoteInfo {
    pub id: String, // format: "daily/2026-06-29.md" or "topics/My Topic.md"
    pub title: String,
    pub tags: Vec<String>,
    pub created: String,
    pub updated: String,
    pub preview: String,
}

fn parse_frontmatter_and_content(raw_content: &str) -> (NoteFrontmatter, String) {
    let mut frontmatter = NoteFrontmatter::default();
    let mut content = raw_content.to_string();

    if raw_content.starts_with("---") {
        if let Some(end_idx) = raw_content[3..].find("---") {
            let yaml_str = &raw_content[3..3 + end_idx];
            if let Ok(parsed) = serde_yaml::from_str::<NoteFrontmatter>(yaml_str) {
                frontmatter = parsed;
            }
            content = raw_content[3 + end_idx + 3..].trim_start().to_string();
        }
    }
    (frontmatter, content)
}

fn get_preview(content: &str) -> String {
    let preview: String = content.chars().take(150).collect();
    if preview.len() < content.len() {
        format!("{}...", preview)
    } else {
        preview
    }
}


/// T-1901: 获取笔记根目录

fn resolve_note_path(path: &str) -> PathBuf {
    if path.starts_with("wiki/") || path.starts_with("wiki\\") {
        let rel = &path[5..];
        crate::get_wiki_dir().join(rel)
    } else {
        get_notes_dir().join(path)
    }
}

/// T-1901: 获取笔记根目录
pub fn get_notes_dir() -> PathBuf {
    let config = crate::read_config();
    let dir = if let Some(workspace_dir) = config.get("workspaceDir").and_then(|v| v.as_str()) {
        if !workspace_dir.is_empty() {
            PathBuf::from(workspace_dir).join("notes")
        } else {
            crate::get_data_dir().join("notes")
        }
    } else {
        crate::get_data_dir().join("notes")
    };
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn get_daily_notes_dir() -> PathBuf {
    let dir = get_notes_dir().join("daily");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn get_topics_dir() -> PathBuf {
    let dir = get_notes_dir().join("topics");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn get_assets_dir() -> PathBuf {
    let dir = get_notes_dir().join("assets");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// T-1901: 初始化笔记目录结构
pub fn init_notebook_dirs() {
    let _ = get_daily_notes_dir();
    let _ = get_topics_dir();
    let _ = get_assets_dir();
}

fn get_file_modified_time(path: &std::path::Path) -> String {
    if let Ok(metadata) = std::fs::metadata(path) {
        if let Ok(modified) = metadata.modified() {
            let datetime: chrono::DateTime<chrono::Local> = modified.into();
            return datetime.format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }
    String::new()
}

/// T-1902: 扫描 notes/ 目录，返回 { daily: [...], topics: [...] }
#[tauri::command]
pub fn notebook_list_notes() -> Result<Value, String> {
    let mut daily = Vec::new();
    let mut topics = Vec::new();
    let mut sources = Vec::new();

    if let Ok(entries) = fs::read_dir(get_daily_notes_dir()) {
        for entry in entries.filter_map(Result::ok) {
            if entry.path().extension().map_or(false, |ext| ext == "md") {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                let id = format!("daily/{}", file_name);
                let content = fs::read_to_string(entry.path()).unwrap_or_default();
                let (frontmatter, text) = parse_frontmatter_and_content(&content);
                
                let mut created = frontmatter.created;
                let mut updated = frontmatter.updated;
                if created.is_empty() || updated.is_empty() {
                    let file_time = get_file_modified_time(&entry.path());
                    if created.is_empty() { created = file_time.clone(); }
                    if updated.is_empty() { updated = file_time; }
                }

                daily.push(NoteInfo {
                    id,
                    title: frontmatter.title,
                    tags: frontmatter.tags,
                    created,
                    updated,
                    preview: get_preview(&text),
                });
            }
        }
    }

    if let Ok(entries) = fs::read_dir(get_topics_dir()) {
        for entry in entries.filter_map(Result::ok) {
            if entry.path().extension().map_or(false, |ext| ext == "md") {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                let id = format!("topics/{}", file_name);
                let content = fs::read_to_string(entry.path()).unwrap_or_default();
                let (frontmatter, text) = parse_frontmatter_and_content(&content);
                
                let mut created = frontmatter.created;
                let mut updated = frontmatter.updated;
                if created.is_empty() || updated.is_empty() {
                    let file_time = get_file_modified_time(&entry.path());
                    if created.is_empty() { created = file_time.clone(); }
                    if updated.is_empty() { updated = file_time; }
                }

                topics.push(NoteInfo {
                    id,
                    title: frontmatter.title,
                    tags: frontmatter.tags,
                    created,
                    updated,
                    preview: get_preview(&text),
                });
            }
        }
    }

    
    if let Ok(entries) = fs::read_dir(crate::get_wiki_dir().join("sources")) {
        for entry in entries.filter_map(Result::ok) {
            if entry.path().extension().map_or(false, |ext| ext == "md") {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                let id = format!("wiki/sources/{}", file_name);
                let content = fs::read_to_string(entry.path()).unwrap_or_default();
                let (frontmatter, text) = parse_frontmatter_and_content(&content);
                
                let mut created = frontmatter.created;
                let mut updated = frontmatter.updated;
                if created.is_empty() || updated.is_empty() {
                    let file_time = get_file_modified_time(&entry.path());
                    if created.is_empty() { created = file_time.clone(); }
                    if updated.is_empty() { updated = file_time; }
                }

                sources.push(NoteInfo {
                    id,
                    title: frontmatter.title,
                    tags: frontmatter.tags,
                    created,
                    updated,
                    preview: get_preview(&text),
                });
            }
        }
    }
    
    Ok(json!({ "daily": daily, "topics": topics, "sources": sources }))
}

/// T-1902: 读取指定 .md 文件
#[tauri::command]
pub fn notebook_read_note(path: String) -> Result<Value, String> {
    let full_path = resolve_note_path(&path);
    match fs::read_to_string(&full_path) {
        Ok(raw_content) => {
            let (frontmatter, content) = parse_frontmatter_and_content(&raw_content);
            Ok(json!({
                "ok": true,
                "frontmatter": frontmatter,
                "content": content
            }))
        }
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() }))
    }
}

/// T-1902: 写入 .md 文件 + 即时层副作用
#[tauri::command]
pub fn notebook_save_note(path: String, content: String) -> Result<Value, String> {
    let full_path = resolve_note_path(&path);
    
    // T-1906: 即时层副作用
    // 1. Sync checkbox to events table (to be implemented later)
    // 2. Sync double-links to KG (to be implemented later)

    let mut final_content = content.clone();
    if let Ok(raw_content) = fs::read_to_string(&full_path) {
        if raw_content.starts_with("---") {
            if let Some(end_idx) = raw_content[3..].find("---") {
                let yaml_str = &raw_content[..3 + end_idx + 3];
                final_content = format!("{}\n\n{}", yaml_str, content);
            }
        }
    }

    match fs::write(&full_path, final_content) {
        Ok(_) => Ok(json!({ "ok": true })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() }))
    }
}

#[tauri::command]
pub fn notebook_create_note(title: String, tags: Vec<String>) -> Result<Value, String> {
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%z").to_string();
    let frontmatter = NoteFrontmatter {
        title: title.clone(),
        tags,
        created: now.clone(),
        updated: now.clone(),
    };
    
    let yaml = serde_yaml::to_string(&frontmatter).unwrap_or_default();
    let content = format!("---
{}
---

", yaml);
    
    // Sanitize title for filename
    let safe_title = title.replace("/", "_").replace("\\", "_");
    let file_name = format!("{}.md", safe_title);
    let path = get_topics_dir().join(&file_name);
    
    if path.exists() {
        return Ok(json!({ "ok": false, "error": "Note already exists" }));
    }
    
    match fs::write(&path, content) {
        Ok(_) => Ok(json!({ "ok": true, "path": format!("topics/{}", file_name) })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() }))
    }
}

#[tauri::command]
pub fn notebook_delete_note(path: String) -> Result<Value, String> {
    let full_path = resolve_note_path(&path);
    match fs::remove_file(full_path) {
        Ok(_) => Ok(json!({ "ok": true })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() }))
    }
}

#[tauri::command]
pub fn notebook_move_note(path: String, target_category: String) -> Result<Value, String> {
    let old_full_path = resolve_note_path(&path);
    if !old_full_path.exists() {
        return Ok(json!({ "ok": false, "error": "Original note not found" }));
    }
    
    let file_name = old_full_path.file_name().unwrap().to_string_lossy().to_string();
    let new_id = format!("{}/{}", target_category, file_name);
    let new_full_path = resolve_note_path(&new_id);
    
    if new_full_path.exists() {
        return Ok(json!({ "ok": false, "error": "Target already exists" }));
    }
    
    match fs::rename(&old_full_path, &new_full_path) {
        Ok(_) => Ok(json!({ "ok": true, "new_id": new_id })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() }))
    }
}

#[tauri::command]
pub fn notebook_rename_note(old_path: String, new_title: String) -> Result<Value, String> {
    // Only support renaming topics for now
    if !old_path.starts_with("topics/") {
        return Ok(json!({ "ok": false, "error": "Cannot rename daily notes" }));
    }
    
    let full_old_path = get_notes_dir().join(&old_path);
    if !full_old_path.exists() {
        return Ok(json!({ "ok": false, "error": "Original note not found" }));
    }
    
    let safe_title = new_title.replace("/", "_").replace("\\", "_");
    let new_file_name = format!("{}.md", safe_title);
    let new_path = get_topics_dir().join(&new_file_name);
    
    if new_path.exists() {
        return Ok(json!({ "ok": false, "error": "Target name already exists" }));
    }
    
    match fs::rename(&full_old_path, &new_path) {
        Ok(_) => {
            // Should also update the title in frontmatter, but we skip for brevity here
            Ok(json!({ "ok": true, "new_path": format!("topics/{}", new_file_name) }))
        },
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() }))
    }
}

#[tauri::command]
pub fn notebook_append_daily(content: String) -> Result<Value, String> {
    use std::io::Write;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let time = chrono::Local::now().format("%H:%M").to_string();
    
    let path = get_daily_notes_dir().join(format!("{}.md", today));
    
    let entry = format!("\n- [{}] {}\n", time, content.trim());
    
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut f) => {
            let _ = f.write_all(entry.as_bytes());
            Ok(json!({ "ok": true, "path": format!("daily/{}.md", today) }))
        }
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
    }
}

#[tauri::command]
pub fn notebook_save_asset(file_name: String, data: Vec<u8>) -> Result<Value, String> {
    let safe_name = file_name.replace("/", "_").replace("\\", "_");
    let path = get_assets_dir().join(&safe_name);
    
    match fs::write(&path, data) {
        Ok(_) => Ok(json!({ "ok": true, "path": format!("assets/{}", safe_name) })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() }))
    }
}

#[tauri::command]
pub fn notebook_search(query: String) -> Result<Value, String> {
    // Basic implementation: list all files and check content
    let mut results = vec![];
    let query_lower = query.to_lowercase();
    
    let mut search_dir = |dir: &PathBuf, prefix: &str| {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if content.to_lowercase().contains(&query_lower) {
                            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            results.push(format!("{}/{}", prefix, file_name));
                        }
                    }
                }
            }
        }
    };
    
    search_dir(&get_daily_notes_dir(), "daily");
    search_dir(&get_topics_dir(), "topics");
    
    Ok(json!({ "ok": true, "results": results }))
}

// Removed old stubs
