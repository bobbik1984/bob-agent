use std::time::Duration;
use tauri::{AppHandle, Manager};
use serde_json::Value;

pub fn start_ghost_merger_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            
            if let Err(e) = process_conflicts(&app).await {
                log::error!("[Ghost Merger] Failed to process conflicts: {}", e);
            }
        }
    });
}

async fn process_conflicts(app: &AppHandle) -> Result<(), String> {
    let mut conflicts: Vec<(String, String, String, String)> = Vec::new();
    
    // 1. Fetch pending conflicts
    {
        let db = app.state::<crate::db::DbState>();
        let conn = db.0.lock().map_err(|_| "DB lock failed")?;
        
        let mut stmt = conn.prepare(
            "SELECT id, table_name, local_id, remote_id FROM sync_conflicts WHERE status = 'pending' LIMIT 5"
        ).map_err(|e| e.to_string())?;
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // table_name
                row.get::<_, String>(2)?, // local_id
                row.get::<_, String>(3)?  // remote_id
            ))
        }).map_err(|e| e.to_string())?;
        
        for r in rows {
            if let Ok(c) = r {
                conflicts.push(c);
            }
        }
    }
    
    if conflicts.is_empty() {
        return Ok(());
    }
    
    log::info!("[Ghost Merger] Found {} pending conflicts to resolve", conflicts.len());
    
    // 2. Process each conflict
    for (id, table_name, local_id, remote_id) in conflicts {
        log::info!("[Ghost Merger] Resolving conflict for {}: {}", table_name, local_id);
        
        let db = app.state::<crate::db::DbState>();
        
        // Mark as processing
        {
            let conn = db.0.lock().map_err(|_| "DB lock failed")?;
            let _ = conn.execute("UPDATE sync_conflicts SET status = 'processing' WHERE id = ?1", rusqlite::params![&id]);
        }
        
        // Currently we only implement AI merge for kg_nodes (notes/knowledge)
        if table_name == "kg_nodes" {
            let (local_content, remote_content, local_title, remote_title) = {
                let conn = db.0.lock().map_err(|_| "DB lock failed")?;
                let mut get_node = |nid: &str| -> Result<(String, String), String> {
                    conn.query_row(
                        "SELECT label, summary FROM kg_nodes WHERE id = ?1",
                        rusqlite::params![nid],
                        |row| Ok((row.get(0)?, row.get(1)?))
                    ).map_err(|e| e.to_string())
                };
                
                let local = get_node(&local_id).unwrap_or_default();
                let remote = get_node(&remote_id).unwrap_or_default();
                (local.1, remote.1, local.0, remote.0)
            };
            
            if !local_content.is_empty() && !remote_content.is_empty() {
                // Call LLM to merge
                let prompt = format!(
                    "【版本A (PC端)】:\n{}\n\n【版本B (手机端)】:\n{}",
                    local_content, remote_content
                );
                
                if let Some(merged_content) = crate::llm::call_clerk_oneshot(
                    "你是一个智能同步仲裁助手。这里有同一个文档的两个离线修改版本，请分析它们的增量内容，将两者的修改进行无缝融合成一个最终版本。直接输出合并后的正文内容，不要有任何解释语。",
                    &prompt,
                    4096
                ).await {
                        let conn = db.0.lock().map_err(|_| "DB lock failed")?;
                        // Update local with merged content
                        let _ = conn.execute(
                            "UPDATE kg_nodes SET summary = ?1, updated_at = ?2 WHERE id = ?3",
                            rusqlite::params![&merged_content, crate::now_ms(), &local_id]
                        );
                        // Delete remote conflict copy
                        let _ = crate::kg::delete_node(&conn, &remote_id);
                        
                        // Mark resolved
                        let _ = conn.execute("UPDATE sync_conflicts SET status = 'resolved' WHERE id = ?1", rusqlite::params![&id]);
                        
                        log::info!("[Ghost Merger] Successfully merged {} using AI", local_id);
                } else {
                        log::error!("[Ghost Merger] LLM merge failed");
                        let conn = db.0.lock().map_err(|_| "DB lock failed")?;
                        let _ = conn.execute("UPDATE sync_conflicts SET status = 'pending' WHERE id = ?1", rusqlite::params![&id]);
                }
            } else {
                // One side is empty, just keep the non-empty one
                let conn = db.0.lock().map_err(|_| "DB lock failed")?;
                let _ = conn.execute("UPDATE sync_conflicts SET status = 'resolved' WHERE id = ?1", rusqlite::params![&id]);
            }
        } else {
            // Non text-heavy tables (like events, conversations), just LWW (keep local or remote based on something, for now just keep local since remote is already forked)
            // Or we just resolve it and let user manually check the fork.
            let conn = db.0.lock().map_err(|_| "DB lock failed")?;
            let _ = conn.execute("UPDATE sync_conflicts SET status = 'ignored' WHERE id = ?1", rusqlite::params![&id]);
        }
    }
    
    Ok(())
}
