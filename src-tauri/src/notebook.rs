use crate::db::DbState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tauri::State;

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
    // P1-1: projects 目录
    let projects_dir = get_notes_dir().join("projects");
    let _ = fs::create_dir_all(&projects_dir);
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

/// P1-1: 动态扫描 notes/ 目录，返回 { daily: [...], topics: [...], projects: {subdir: [...]}, sources: [...], custom: {subdir: [...]} }
#[tauri::command]
pub fn notebook_list_notes() -> Result<Value, String> {
    let notes_dir = get_notes_dir();
    let wiki_sources_dir = crate::get_wiki_dir().join("sources");
    let preset_dirs = ["daily", "topics", "projects", "sources", "assets"];

    // Helper: scan a flat directory for .md files
    let scan_dir = |dir: &std::path::Path, prefix: &str| -> Vec<Value> {
        let mut items = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(Result::ok) {
                if entry.path().extension().map_or(false, |ext| ext == "md") {
                    let file_name = entry.file_name().to_string_lossy().into_owned();
                    let id = format!("{}/{}", prefix, file_name);
                    let content = fs::read_to_string(entry.path()).unwrap_or_default();
                    let (frontmatter, text) = parse_frontmatter_and_content(&content);
                    let mut created = frontmatter.created;
                    let mut updated = frontmatter.updated;
                    if created.is_empty() || updated.is_empty() {
                        let file_time = get_file_modified_time(&entry.path());
                        if created.is_empty() {
                            created = file_time.clone();
                        }
                        if updated.is_empty() {
                            updated = file_time;
                        }
                    }
                    items.push(json!({
                        "id": id,
                        "title": frontmatter.title,
                        "tags": frontmatter.tags,
                        "created": created,
                        "updated": updated,
                        "preview": get_preview(&text),
                    }));
                }
            }
        }
        items
    };

    let daily = scan_dir(&notes_dir.join("daily"), "daily");
    let topics = scan_dir(&notes_dir.join("topics"), "topics");
    let sources = scan_dir(&wiki_sources_dir, "wiki/sources");

    // Scan projects/ subdirectories (nested one level)
    let mut projects = serde_json::Map::new();
    let projects_dir = notes_dir.join("projects");
    if let Ok(entries) = fs::read_dir(&projects_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().into_owned();
                let prefix = format!("projects/{}", dir_name);
                let items = scan_dir(&path, &prefix);
                projects.insert(dir_name, json!(items));
            }
        }
        // Also scan .md files directly under projects/
        let top_level = scan_dir(&projects_dir, "projects");
        if !top_level.is_empty() {
            projects.insert("_root".to_string(), json!(top_level));
        }
    }

    // Scan custom directories (anything not preset)
    let mut custom = serde_json::Map::new();
    if let Ok(entries) = fs::read_dir(&notes_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().into_owned();
                if !preset_dirs.contains(&dir_name.as_str()) {
                    let prefix = format!("{}", dir_name);
                    let items = scan_dir(&path, &prefix);
                    if !items.is_empty() {
                        custom.insert(dir_name, json!(items));
                    }
                }
            }
        }
    }

    Ok(json!({
        "daily": daily,
        "topics": topics,
        "sources": sources,
        "projects": projects,
        "custom": custom,
    }))
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
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
    }
}

/// T-1902: 写入 .md 文件 + 即时层副作用
#[tauri::command]
pub fn notebook_save_note(
    path: String,
    content: String,
    db: State<DbState>,
) -> Result<Value, String> {
    let full_path = resolve_note_path(&path);

    // T-1906: 即时层副作用 — Phase 1 将在此扩展 KG 同步

    let mut final_content = content.clone();
    let mut frontmatter_title = String::new();
    let mut frontmatter_tags = String::new();
    if let Ok(raw_content) = fs::read_to_string(&full_path) {
        if raw_content.starts_with("---") {
            if let Some(end_idx) = raw_content[3..].find("---") {
                let yaml_str = &raw_content[..3 + end_idx + 3];
                // Parse frontmatter for FTS indexing
                let fm_body = &raw_content[3..3 + end_idx];
                if let Ok(fm) = serde_yaml::from_str::<NoteFrontmatter>(fm_body) {
                    frontmatter_title = fm.title;
                    frontmatter_tags = fm.tags.join(" ");
                }
                final_content = format!("{}\n\n{}", yaml_str, content);
            }
        }
    }

    match fs::write(&full_path, &final_content) {
        Ok(_) => {
            if let Ok(conn) = db.0.lock() {
                // P0-2: Upsert notes_fts for full-text search
                let _ = conn.execute(
                    "DELETE FROM notes_fts WHERE note_path = ?1",
                    rusqlite::params![&path],
                );
                let _ = conn.execute(
                    "INSERT INTO notes_fts (note_path, title, content, tags) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![&path, &frontmatter_title, &content, &frontmatter_tags],
                );

                // P1-2: Sync note → KG node (note type)
                let note_id = format!("note:{}", path);
                let note_label = if frontmatter_title.is_empty() {
                    full_path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else {
                    frontmatter_title.clone()
                };
                let preview = content.chars().take(200).collect::<String>();
                let _ = crate::kg::upsert_node(
                    &conn,
                    &note_id,
                    &note_label,
                    "note",
                    &preview,
                    &path,
                    "",
                );

                // P1-2: Sync tags → KG tag nodes + tagged_as edges
                if !frontmatter_tags.is_empty() {
                    for tag in frontmatter_tags.split_whitespace() {
                        let tag_trimmed = tag.trim();
                        if tag_trimmed.is_empty() {
                            continue;
                        }
                        let tag_node_id = format!("tag:{}", tag_trimmed.to_lowercase());
                        let _ = crate::kg::upsert_node(
                            &conn,
                            &tag_node_id,
                            tag_trimmed,
                            "tag",
                            "",
                            "notebook",
                            "",
                        );
                        let _ =
                            crate::kg::insert_edge(&conn, &note_id, &tag_node_id, "tagged_as", 1.0);
                    }
                }

                // P2-3: Parse [[wikilinks]] → KG links_to edges
                // First clear old links_to edges from this note
                let _ = conn.execute(
                    "DELETE FROM kg_edges WHERE source_id = ?1 AND relation = 'links_to'",
                    rusqlite::params![&note_id],
                );
                // Then scan content for [[target]] patterns
                let wikilink_re = regex::Regex::new(r"\[\[([^\[\]]+)\]\]")
                    .unwrap_or_else(|_| regex::Regex::new("$^").unwrap());
                for cap in wikilink_re.captures_iter(&content) {
                    if let Some(target_match) = cap.get(1) {
                        let target_title = target_match.as_str().trim();
                        if target_title.is_empty() {
                            continue;
                        }
                        // Resolve target: try to find existing note node by label
                        let target_node_id =
                            crate::kg::resolve_node_id(&conn, target_title, "note");
                        // Ensure target node exists (might be a "ghost" node for a note not yet created)
                        let _ = crate::kg::upsert_node(
                            &conn,
                            &target_node_id,
                            target_title,
                            "note",
                            "",
                            "wikilink",
                            "",
                        );
                        let _ = crate::kg::insert_edge(
                            &conn,
                            &note_id,
                            &target_node_id,
                            "links_to",
                            0.9,
                        );
                    }
                }
            }
            Ok(json!({ "ok": true }))
        }
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
    }
}

#[tauri::command]
pub fn notebook_create_note(
    title: String,
    tags: Vec<String>,
    category: Option<String>,
) -> Result<Value, String> {
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%z")
        .to_string();
    let frontmatter = NoteFrontmatter {
        title: title.clone(),
        tags,
        created: now.clone(),
        updated: now.clone(),
    };

    let yaml = serde_yaml::to_string(&frontmatter).unwrap_or_default();
    let content = format!("---\n{}---\n\n", yaml);

    // Sanitize title for filename
    let safe_title = title.replace("/", "_").replace("\\", "_");
    let file_name = format!("{}.md", safe_title);

    // Determine target directory based on category
    let cat = category.unwrap_or_else(|| "topics".to_string());
    let target_dir = if cat.starts_with("projects/") {
        // e.g. "projects/bob-agent" -> notes/projects/bob-agent/
        let sub = &cat["projects/".len()..];
        let dir = get_notes_dir().join("projects").join(sub);
        let _ = fs::create_dir_all(&dir);
        dir
    } else {
        match cat.as_str() {
            "daily" => get_daily_notes_dir(),
            "topics" => get_topics_dir(),
            "projects" => {
                let dir = get_notes_dir().join("projects");
                let _ = fs::create_dir_all(&dir);
                dir
            }
            _ => {
                // Custom category: create under notes/{cat}/
                let dir = get_notes_dir().join(&cat);
                let _ = fs::create_dir_all(&dir);
                dir
            }
        }
    };

    let path = target_dir.join(&file_name);
    if path.exists() {
        return Ok(json!({ "ok": false, "error": "Note already exists" }));
    }

    let note_id = format!("{}/{}", cat, file_name);
    match fs::write(&path, content) {
        Ok(_) => Ok(json!({ "ok": true, "path": note_id })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
    }
}

#[tauri::command]
pub fn notebook_delete_note(path: String, db: State<DbState>) -> Result<Value, String> {
    let full_path = resolve_note_path(&path);
    match fs::remove_file(&full_path) {
        Ok(_) => {
            // P0-1: Cascade cleanup — remove orphaned FTS/KG data
            if let Ok(conn) = db.0.lock() {
                // Clean notes_fts
                let _ = conn.execute(
                    "DELETE FROM notes_fts WHERE note_path = ?1",
                    rusqlite::params![&path],
                );
                // 1. Get original file name from wiki_fts before deleting
                let mut original_sources = Vec::new();
                original_sources.push(path.clone());

                let file_name = full_path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                original_sources.push(file_name.clone());

                if path.starts_with("wiki/sources/") {
                    if let Ok(orig) = conn.query_row(
                        "SELECT file_name FROM wiki_fts WHERE wiki_path = ?1",
                        rusqlite::params![&path],
                        |row| row.get::<_, String>(0),
                    ) {
                        original_sources.push(orig);
                    }

                    // Clean wiki_fts
                    let _ = conn.execute(
                        "DELETE FROM wiki_fts WHERE wiki_path = ?1",
                        rusqlite::params![&path],
                    );
                }

                // 2. Delete document-level node
                // ID is either `path`, `file_{file_name}`, or `file_{orig_name}`
                let mut doc_node_ids = Vec::new();
                doc_node_ids.push(path.clone());

                for src in &original_sources {
                    let file_node_id = format!(
                        "file_{}",
                        src.to_lowercase().replace(' ', "_").replace('.', "_")
                    );
                    doc_node_ids.push(file_node_id);
                }

                for doc_id in &doc_node_ids {
                    let _ = conn.execute(
                        "DELETE FROM kg_edges WHERE source_id = ?1 OR target_id = ?1",
                        rusqlite::params![doc_id],
                    );
                    let _ = conn.execute(
                        "DELETE FROM kg_nodes WHERE id = ?1",
                        rusqlite::params![doc_id],
                    );
                }

                // 3. Delete entity nodes where their only source was this document,
                // OR better: delete any orphans (nodes with no edges) after the document was removed.
                // It's safer to just delete orphans that are NOT documents.

                // For direct graph cleanup based on the `source` field (which might be the only link if edges failed)
                // We will collect nodes where source matched, delete their edges, then delete them.
                // Wait, if an entity is shared, we SHOULD NOT delete it.
                // The safest way is to delete orphans.
                let _ = conn.execute(
                    "DELETE FROM kg_nodes WHERE node_type NOT IN ('note', 'file', 'project') AND id NOT IN (
                        SELECT source_id FROM kg_edges UNION SELECT target_id FROM kg_edges
                    )",
                    [],
                );

                // Fallback: If there are nodes whose explicitly set `source` was this file,
                // and they have NO other edges, they will be deleted by the orphan cleanup.
                // But what if the user really wants to delete all entities generated by this file?
                // The previous code deleted ALL nodes whose source matched.
                // Let's do that for now but only if they are orphans to be safe?
                // Actually, the user's complaint is "图谱里的还在" (nodes are still there).
                // Let's find nodes whose source matches, and delete them.
                let mut nodes_to_delete: Vec<String> = Vec::new();
                for src in &original_sources {
                    if let Ok(mut stmt) = conn.prepare("SELECT id FROM kg_nodes WHERE source = ?1")
                    {
                        if let Ok(rows) =
                            stmt.query_map(rusqlite::params![src], |row| row.get::<_, String>(0))
                        {
                            for r in rows.flatten() {
                                nodes_to_delete.push(r);
                            }
                        }
                    }
                }

                // Deduplicate
                nodes_to_delete.sort();
                nodes_to_delete.dedup();

                for node_id in &nodes_to_delete {
                    let _ = conn.execute(
                        "DELETE FROM kg_edges WHERE source_id = ?1 OR target_id = ?1",
                        rusqlite::params![node_id],
                    );
                    let _ = conn.execute(
                        "DELETE FROM kg_nodes WHERE id = ?1",
                        rusqlite::params![node_id],
                    );
                }

                if !nodes_to_delete.is_empty() {
                    println!(
                        "[notebook] Cascade deleted {} KG nodes for '{}'",
                        nodes_to_delete.len(),
                        path
                    );
                }
            }
            Ok(json!({ "ok": true }))
        }
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
    }
}

#[tauri::command]
pub fn notebook_move_note(path: String, target_category: String) -> Result<Value, String> {
    let old_full_path = resolve_note_path(&path);
    if !old_full_path.exists() {
        return Ok(json!({ "ok": false, "error": "Original note not found" }));
    }

    let file_name = old_full_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let new_id = format!("{}/{}", target_category, file_name);
    let new_full_path = resolve_note_path(&new_id);

    if new_full_path.exists() {
        return Ok(json!({ "ok": false, "error": "Target already exists" }));
    }

    match fs::rename(&old_full_path, &new_full_path) {
        Ok(_) => Ok(json!({ "ok": true, "new_id": new_id })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
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
        }
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
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
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
    }
}

#[tauri::command]
pub fn notebook_search(query: String, db: State<DbState>) -> Result<Value, String> {
    // P0-2: Use FTS5 for fast full-text search
    let mut results = vec![];

    if let Ok(conn) = db.0.lock() {
        // Try FTS search first
        let fts_query = format!("\"{}\"*", query.replace('"', ""));
        if let Ok(mut stmt) = conn.prepare(
            "SELECT note_path, title, snippet(notes_fts, 2, '[', ']', '...', 32) FROM notes_fts WHERE notes_fts MATCH ?1 LIMIT 50"
        ) {
            if let Ok(rows) = stmt.query_map(rusqlite::params![&fts_query], |row| {
                Ok(json!({
                    "path": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1).unwrap_or_default(),
                    "snippet": row.get::<_, String>(2).unwrap_or_default(),
                }))
            }) {
                for row in rows.flatten() {
                    results.push(row);
                }
            }
        }

        // If FTS returned nothing, fall back to brute-force for unindexed notes
        if results.is_empty() {
            let query_lower = query.to_lowercase();
            let mut search_dir = |dir: &PathBuf, prefix: &str| {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md")
                        {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if content.to_lowercase().contains(&query_lower) {
                                    let file_name = path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                    results.push(json!({
                                        "path": format!("{}/{}", prefix, file_name),
                                        "title": file_name.trim_end_matches(".md"),
                                        "snippet": "",
                                    }));
                                }
                            }
                        }
                    }
                }
            };
            search_dir(&get_daily_notes_dir(), "daily");
            search_dir(&get_topics_dir(), "topics");
        }
    }

    Ok(json!({ "ok": true, "results": results }))
}

// Removed old stubs

// ── P1-1: 用户自建文件夹 ──────────────────────────────────────
#[tauri::command]
pub fn notebook_create_folder(name: String) -> Result<Value, String> {
    let safe_name = name.replace("/", "_").replace("\\", "_").replace("..", "_");
    if safe_name.is_empty() {
        return Ok(json!({ "ok": false, "error": "Folder name cannot be empty" }));
    }
    let dir = get_notes_dir().join(&safe_name);
    match fs::create_dir_all(&dir) {
        Ok(_) => Ok(json!({ "ok": true, "folder": safe_name })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── P1-3: 标签系统 ──────────────────────────────────────────
#[tauri::command]
pub fn notebook_list_all_tags() -> Result<Value, String> {
    let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let notes_dir = get_notes_dir();

    // Recursively scan all .md files under notes/
    fn scan_tags(dir: &std::path::Path, tag_counts: &mut std::collections::HashMap<String, usize>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    scan_tags(&path, tag_counts);
                } else if path.extension().map_or(false, |ext| ext == "md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let (fm, _) = parse_frontmatter_and_content(&content);
                        for tag in fm.tags {
                            let normalized = tag.trim().to_string();
                            if !normalized.is_empty() {
                                *tag_counts.entry(normalized).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    scan_tags(&notes_dir, &mut tag_counts);

    // Sort by count descending
    let mut tags_list: Vec<Value> = tag_counts
        .into_iter()
        .map(|(tag, count)| json!({ "tag": tag, "count": count }))
        .collect();
    tags_list.sort_by(|a, b| {
        b["count"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["count"].as_u64().unwrap_or(0))
    });

    Ok(json!({ "ok": true, "tags": tags_list }))
}

#[tauri::command]
pub fn notebook_update_tags(path: String, tags: Vec<String>) -> Result<Value, String> {
    let full_path = resolve_note_path(&path);
    let raw_content =
        fs::read_to_string(&full_path).map_err(|e| format!("Failed to read note: {}", e))?;

    let new_fm = if raw_content.starts_with("---") {
        if let Some(end_idx) = raw_content[3..].find("---") {
            let yaml_str = &raw_content[3..3 + end_idx];
            let mut fm: NoteFrontmatter = serde_yaml::from_str(yaml_str).unwrap_or_default();
            fm.tags = tags;
            fm.updated = chrono::Local::now()
                .format("%Y-%m-%dT%H:%M:%S%z")
                .to_string();
            let body = &raw_content[3 + end_idx + 3..];
            let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
            format!("---\n{}---{}", yaml, body)
        } else {
            return Ok(json!({ "ok": false, "error": "Invalid frontmatter" }));
        }
    } else {
        // No frontmatter yet, create one
        let fm = NoteFrontmatter {
            title: full_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            tags,
            created: chrono::Local::now()
                .format("%Y-%m-%dT%H:%M:%S%z")
                .to_string(),
            updated: chrono::Local::now()
                .format("%Y-%m-%dT%H:%M:%S%z")
                .to_string(),
        };
        let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
        format!("---\n{}---\n\n{}", yaml, raw_content)
    };

    match fs::write(&full_path, new_fm) {
        Ok(_) => Ok(json!({ "ok": true })),
        Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })),
    }
}

/// P2-2: 查找所有包含 [[title]] 的笔记，返回反向链接列表
#[tauri::command]
pub fn notebook_get_backlinks(path: String) -> Result<Value, String> {
    let full_path = resolve_note_path(&path);
    // Get the title of the target note (from frontmatter or filename)
    let title = if let Ok(raw) = fs::read_to_string(&full_path) {
        if raw.starts_with("---") {
            if let Some(end_idx) = raw[3..].find("---") {
                let fm_body = &raw[3..3 + end_idx];
                if let Ok(fm) = serde_yaml::from_str::<NoteFrontmatter>(fm_body) {
                    if !fm.title.is_empty() {
                        fm.title
                    } else {
                        full_path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default()
                    }
                } else {
                    full_path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default()
                }
            } else {
                full_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            }
        } else {
            full_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        }
    } else {
        return Ok(json!({ "ok": true, "backlinks": [] }));
    };

    if title.is_empty() {
        return Ok(json!({ "ok": true, "backlinks": [] }));
    }

    let pattern = format!("[[{}]]", title);
    let notes_dir = get_notes_dir();
    let mut backlinks = vec![];

    // Scan all .md files in notes directory
    fn scan_dir(
        dir: &std::path::Path,
        notes_dir: &std::path::Path,
        pattern: &str,
        backlinks: &mut Vec<Value>,
    ) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir()
                    && entry_path
                        .file_name()
                        .map(|n| n != "assets")
                        .unwrap_or(true)
                {
                    scan_dir(&entry_path, notes_dir, pattern, backlinks);
                } else if entry_path.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&entry_path) {
                        if content.contains(pattern) {
                            let rel_path = entry_path
                                .strip_prefix(notes_dir)
                                .unwrap_or(&entry_path)
                                .to_string_lossy()
                                .replace('\\', "/");

                            // Extract title from frontmatter
                            let note_title = if content.starts_with("---") {
                                if let Some(end_idx) = content[3..].find("---") {
                                    let fm_body = &content[3..3 + end_idx];
                                    serde_yaml::from_str::<NoteFrontmatter>(fm_body)
                                        .map(|fm| fm.title)
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };

                            // Extract context: line containing the pattern
                            let context = content
                                .lines()
                                .find(|line| line.contains(pattern))
                                .unwrap_or("")
                                .chars()
                                .take(100)
                                .collect::<String>();

                            backlinks.push(json!({
                                "path": rel_path,
                                "title": if note_title.is_empty() {
                                    entry_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
                                } else { note_title },
                                "context": context,
                            }));
                        }
                    }
                }
            }
        }
    }

    scan_dir(&notes_dir, &notes_dir, &pattern, &mut backlinks);

    // Remove self-reference
    backlinks.retain(|b| b["path"].as_str() != Some(&path));

    Ok(json!({ "ok": true, "backlinks": backlinks }))
}

/// P2.5: 合并同义标签 — 将 aliases 替换为 canonical
#[tauri::command]
pub fn notebook_merge_tags(
    canonical: String,
    aliases: Vec<String>,
    db: State<DbState>,
) -> Result<Value, String> {
    let notes_dir = get_notes_dir();
    let mut updated_count = 0;

    // Recursively scan all .md files
    fn process_dir(dir: &std::path::Path, canonical: &str, aliases: &[String], count: &mut usize) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && p.file_name().map(|n| n != "assets").unwrap_or(true) {
                    process_dir(&p, canonical, aliases, count);
                } else if p.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        if content.starts_with("---") {
                            if let Some(end_idx) = content[3..].find("---") {
                                let fm_body = &content[3..3 + end_idx];
                                if let Ok(mut fm) = serde_yaml::from_str::<NoteFrontmatter>(fm_body)
                                {
                                    let mut changed = false;
                                    fm.tags = fm
                                        .tags
                                        .into_iter()
                                        .map(|t| {
                                            if aliases.iter().any(|a| a.eq_ignore_ascii_case(&t)) {
                                                changed = true;
                                                canonical.to_string()
                                            } else {
                                                t
                                            }
                                        })
                                        .collect::<Vec<_>>();
                                    // Deduplicate
                                    fm.tags.sort();
                                    fm.tags.dedup();
                                    if changed {
                                        let rest = &content[3 + end_idx + 3..];
                                        let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
                                        let new_content = format!("---\n{}---{}", yaml, rest);
                                        let _ = std::fs::write(&p, &new_content);
                                        *count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    process_dir(&notes_dir, &canonical, &aliases, &mut updated_count);

    // Merge KG tag nodes
    if let Ok(conn) = db.0.lock() {
        let canonical_id = format!("tag:{}", canonical.to_lowercase());
        let _ = crate::kg::upsert_node(&conn, &canonical_id, &canonical, "tag", "", "notebook", "");
        for alias in &aliases {
            let alias_id = format!("tag:{}", alias.to_lowercase());
            let _ = crate::kg::merge_nodes(&conn, &canonical_id, &alias_id);
        }
    }

    // Write merge history
    let data_dir = get_notes_dir()
        .parent()
        .unwrap_or(&get_notes_dir())
        .to_path_buf();
    let history_path = data_dir.join("tag_merge_history.json");
    let mut history: Vec<Value> = if let Ok(raw) = std::fs::read_to_string(&history_path) {
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        vec![]
    };
    history.push(json!({
        "canonical": canonical,
        "aliases": aliases,
        "merged_at": chrono::Local::now().to_rfc3339(),
        "notes_updated": updated_count
    }));
    let _ = std::fs::write(
        &history_path,
        serde_json::to_string_pretty(&history).unwrap_or_default(),
    );

    Ok(json!({ "ok": true, "updated": updated_count }))
}

/// P2.5: 拒绝标签合并提案 — 写入排除列表
#[tauri::command]
pub fn notebook_reject_tag_merge(tag_a: String, tag_b: String) -> Result<Value, String> {
    let data_dir = get_notes_dir()
        .parent()
        .unwrap_or(&get_notes_dir())
        .to_path_buf();
    let exclusions_path = data_dir.join("tag_merge_exclusions.json");
    let mut exclusions: Vec<(String, String)> =
        if let Ok(raw) = std::fs::read_to_string(&exclusions_path) {
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            vec![]
        };

    // Normalize: always store sorted pair
    let pair = if tag_a < tag_b {
        (tag_a, tag_b)
    } else {
        (tag_b, tag_a)
    };
    if !exclusions.contains(&pair) {
        exclusions.push(pair);
    }
    let _ = std::fs::write(
        &exclusions_path,
        serde_json::to_string_pretty(&exclusions).unwrap_or_default(),
    );

    Ok(json!({ "ok": true }))
}
