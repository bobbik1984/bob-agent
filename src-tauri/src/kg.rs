//! M17: 知识图谱引擎 (Knowledge Graph Engine)
//!
//! SQLite 原生图存储 + BFS 子图查询，替代 Python NetworkX。
//! 表结构: kg_nodes (实体) + kg_edges (关系)，在 db.rs init_db() 中创建。

use rusqlite::params;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use tauri::State;

use crate::db::DbState;

// ── Node/Edge CRUD ──────────────────────────────────────────

/// 插入或更新一个知识节点 (upsert)
pub fn upsert_node(
    conn: &rusqlite::Connection,
    id: &str,
    label: &str,
    node_type: &str,
    summary: &str,
    source: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO kg_nodes (id, label, node_type, summary, source)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            label = COALESCE(NULLIF(?2, ''), kg_nodes.label),
            node_type = ?3,
            summary = CASE WHEN LENGTH(?4) > LENGTH(kg_nodes.summary) THEN ?4 ELSE kg_nodes.summary END,
            source = COALESCE(NULLIF(?5, ''), kg_nodes.source)",
        params![id, label, node_type, summary, source],
    ).map_err(|e| format!("upsert_node failed: {}", e))?;
    Ok(())
}

/// 根据实体名称和类型，解析出应该使用的 node_id
/// 能够自动处理已被合并的 alias 映射
pub fn resolve_node_id(conn: &rusqlite::Connection, name: &str, etype: &str) -> String {
    let query = "
        SELECT id FROM kg_nodes 
        WHERE label = ?1 
           OR EXISTS (SELECT 1 FROM json_each(kg_nodes.metadata, '$.aliases') WHERE value = ?1)
        LIMIT 1
    ";
    if let Ok(mut stmt) = conn.prepare(query) {
        if let Ok(mut rows) = stmt.query(params![name]) {
            if let Ok(Some(row)) = rows.next() {
                if let Ok(id) = row.get::<_, String>(0) {
                    return id;
                }
            }
        }
    }
    // 默认回退：生成基于类型和名称的标准化 ID
    format!("{}_{}", etype, name.to_lowercase().replace(' ', "_").replace('.', "_").chars().take(60).collect::<String>())
}

/// 插入一条关系边 (忽略重复)
pub fn insert_edge(
    conn: &rusqlite::Connection,
    source_id: &str,
    target_id: &str,
    relation: &str,
    confidence: f64,
) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO kg_edges (source_id, target_id, relation, confidence)
         VALUES (?1, ?2, ?3, ?4)",
        params![source_id, target_id, relation, confidence],
    ).map_err(|e| format!("insert_edge failed: {}", e))?;
    Ok(())
}

/// 删除一个节点及其所有关联边
pub fn delete_node(conn: &rusqlite::Connection, id: &str) -> Result<usize, String> {
    conn.execute("DELETE FROM kg_edges WHERE source_id = ?1 OR target_id = ?1", params![id])
        .map_err(|e| format!("delete edges failed: {}", e))?;
    let deleted = conn.execute("DELETE FROM kg_nodes WHERE id = ?1", params![id])
        .map_err(|e| format!("delete node failed: {}", e))?;
    Ok(deleted)
}

/// 合并两个节点 (将 alias_id 合并到 primary_id)
pub fn merge_nodes(conn: &rusqlite::Connection, primary_id: &str, alias_id: &str) -> Result<(), String> {
    if primary_id == alias_id {
        return Ok(());
    }

    // 1. 将所有 source_id 为 alias_id 的边更新为 primary_id
    conn.execute(
        "UPDATE OR IGNORE kg_edges SET source_id = ?1 WHERE source_id = ?2",
        params![primary_id, alias_id],
    ).map_err(|e| format!("update source edges failed: {}", e))?;

    // 2. 将所有 target_id 为 alias_id 的边更新为 primary_id
    conn.execute(
        "UPDATE OR IGNORE kg_edges SET target_id = ?1 WHERE target_id = ?2",
        params![primary_id, alias_id],
    ).map_err(|e| format!("update target edges failed: {}", e))?;

    // 3. 删除残留冲突边
    conn.execute(
        "DELETE FROM kg_edges WHERE source_id = ?1 OR target_id = ?1",
        params![alias_id],
    ).map_err(|e| format!("delete residual edges failed: {}", e))?;

    // 4. 将 alias_id 的 label 加入 primary_id 的 metadata.aliases
    let alias_label: String = conn.query_row(
        "SELECT label FROM kg_nodes WHERE id = ?1",
        params![alias_id],
        |row| row.get(0),
    ).map_err(|e| format!("get alias label failed: {}", e))?;

    let primary_meta: String = conn.query_row(
        "SELECT metadata FROM kg_nodes WHERE id = ?1",
        params![primary_id],
        |row| row.get(0),
    ).unwrap_or_else(|_| "{}".to_string());

    let mut meta_val: Value = serde_json::from_str(&primary_meta).unwrap_or(json!({}));
    if let Some(obj) = meta_val.as_object_mut() {
        let aliases = obj.entry("aliases").or_insert(json!([]));
        if let Some(arr) = aliases.as_array_mut() {
            let label_val = json!(alias_label);
            if !arr.contains(&label_val) {
                arr.push(label_val);
            }
        }
    }
    
    conn.execute(
        "UPDATE kg_nodes SET metadata = ?1 WHERE id = ?2",
        params![meta_val.to_string(), primary_id],
    ).map_err(|e| format!("update primary metadata failed: {}", e))?;

    // 5. 删除 alias 节点
    conn.execute("DELETE FROM kg_nodes WHERE id = ?1", params![alias_id])
        .map_err(|e| format!("delete alias node failed: {}", e))?;

    Ok(())
}

/// 删除一条边
pub fn delete_edge(conn: &rusqlite::Connection, source_id: &str, target_id: &str, relation: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM kg_edges WHERE source_id = ?1 AND target_id = ?2 AND relation = ?3",
        params![source_id, target_id, relation],
    ).map_err(|e| format!("delete edge failed: {}", e))
}

// ── BFS 子图查询 ────────────────────────────────────────────

/// BFS 从匹配 `term` 的节点出发，向外扩展 `max_hops` 跳，返回子图 JSON
pub fn query_subgraph(conn: &rusqlite::Connection, term: &str, max_hops: usize) -> Value {
    // Step 1: 找到匹配的种子节点 (label 或 id 模糊匹配)
    let like_term = format!("%{}%", term);
    let mut stmt = match conn.prepare(
        "SELECT id, label, node_type, summary, source FROM kg_nodes
         WHERE id LIKE ?1 OR label LIKE ?1 LIMIT 20"
    ) {
        Ok(s) => s,
        Err(e) => return json!({"error": format!("query failed: {}", e)}),
    };

    let seeds: Vec<(String, String, String, String, String)> = match stmt
        .query_map(params![like_term], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    };

    if seeds.is_empty() {
        return json!({"nodes": [], "edges": [], "seed_count": 0});
    }

    // Step 2: BFS 扩展
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut result_nodes: HashMap<String, Value> = HashMap::new();
    let mut result_edges: Vec<Value> = Vec::new();

    for (id, label, node_type, summary, source) in &seeds {
        visited.insert(id.clone());
        queue.push_back((id.clone(), 0));
        result_nodes.insert(id.clone(), json!({
            "id": id, "label": label, "type": node_type,
            "summary": summary, "source": source, "is_seed": true
        }));
    }

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_hops {
            continue;
        }

        // 查询从 node_id 出发的边 (双向)
        if let Ok(mut edge_stmt) = conn.prepare(
            "SELECT source_id, target_id, relation, confidence FROM kg_edges
             WHERE source_id = ?1 OR target_id = ?1"
        ) {
            let edges: Vec<(String, String, String, f64)> = match edge_stmt
                .query_map(params![node_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f64>(3)?,
                    ))
                }) {
                Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
                Err(_) => vec![],
            };

            for (src, tgt, rel, conf) in edges {
                result_edges.push(json!({
                    "source": src, "target": tgt, "relation": rel, "confidence": conf
                }));

                // 确定邻居节点
                let neighbor = if src == node_id { &tgt } else { &src };
                if !visited.contains(neighbor) {
                    visited.insert(neighbor.clone());
                    queue.push_back((neighbor.clone(), depth + 1));

                    // 加载邻居节点信息
                    if let Ok(mut n_stmt) = conn.prepare(
                        "SELECT id, label, node_type, summary, source FROM kg_nodes WHERE id = ?1"
                    ) {
                        if let Ok(Some(row)) = n_stmt.query_row(params![neighbor], |row| {
                            Ok(Some(json!({
                                "id": row.get::<_, String>(0)?,
                                "label": row.get::<_, String>(1)?,
                                "type": row.get::<_, String>(2)?,
                                "summary": row.get::<_, String>(3)?,
                                "source": row.get::<_, String>(4)?,
                                "is_seed": false
                            })))
                        }) {
                            result_nodes.insert(neighbor.clone(), row);
                        }
                    }
                }
            }
        }
    }

    // 去重边
    let mut seen_edges: HashSet<String> = HashSet::new();
    let unique_edges: Vec<Value> = result_edges.into_iter().filter(|e| {
        let key = format!("{}->{}:{}", e["source"], e["target"], e["relation"]);
        seen_edges.insert(key)
    }).collect();

    json!({
        "nodes": result_nodes.values().collect::<Vec<_>>(),
        "edges": unique_edges,
        "seed_count": seeds.len()
    })
}

// ── 图统计 ──────────────────────────────────────────────────

pub fn get_stats(conn: &rusqlite::Connection) -> Value {
    let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM kg_nodes", [], |r| r.get(0)).unwrap_or(0);
    let edge_count: i64 = conn.query_row("SELECT COUNT(*) FROM kg_edges", [], |r| r.get(0)).unwrap_or(0);

    // 类型分布
    let mut type_dist: Vec<Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT node_type, COUNT(*) FROM kg_nodes GROUP BY node_type ORDER BY COUNT(*) DESC") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(json!({"type": row.get::<_, String>(0)?, "count": row.get::<_, i64>(1)?}))
        }) {
            type_dist = rows.filter_map(|r| r.ok()).collect();
        }
    }

    // 关系类型分布
    let mut rel_dist: Vec<Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT relation, COUNT(*) FROM kg_edges GROUP BY relation ORDER BY COUNT(*) DESC") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(json!({"relation": row.get::<_, String>(0)?, "count": row.get::<_, i64>(1)?}))
        }) {
            rel_dist = rows.filter_map(|r| r.ok()).collect();
        }
    }

    // Top 5 高连接度节点
    let mut top_nodes: Vec<Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT n.id, n.label, n.node_type,
                (SELECT COUNT(*) FROM kg_edges WHERE source_id = n.id OR target_id = n.id) as degree
         FROM kg_nodes n ORDER BY degree DESC LIMIT 5"
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "label": row.get::<_, String>(1)?,
                "type": row.get::<_, String>(2)?,
                "degree": row.get::<_, i64>(3)?
            }))
        }) {
            top_nodes = rows.filter_map(|r| r.ok()).collect();
        }
    }

    json!({
        "node_count": node_count,
        "edge_count": edge_count,
        "type_distribution": type_dist,
        "relation_distribution": rel_dist,
        "top_connected_nodes": top_nodes
    })
}

/// 获取完整图谱 (节点 + 边)，用于前端 vis.js 渲染
pub fn get_full_graph(conn: &rusqlite::Connection) -> Value {
    let mut nodes: Vec<Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, label, node_type, summary, source FROM kg_nodes") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "label": row.get::<_, String>(1)?,
                "type": row.get::<_, String>(2)?,
                "summary": row.get::<_, String>(3)?,
                "source": row.get::<_, String>(4)?
            }))
        }) {
            nodes = rows.filter_map(|r| r.ok()).collect();
        }
    }

    let mut edges: Vec<Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT source_id, target_id, relation, confidence FROM kg_edges") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(json!({
                "source": row.get::<_, String>(0)?,
                "target": row.get::<_, String>(1)?,
                "relation": row.get::<_, String>(2)?,
                "confidence": row.get::<_, f64>(3)?
            }))
        }) {
            edges = rows.filter_map(|r| r.ok()).collect();
        }
    }

    json!({ "nodes": nodes, "edges": edges })
}

// ── Tauri Commands ──────────────────────────────────────────

#[tauri::command]
pub fn kg_get_full_graph(db: State<DbState>) -> Value {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return json!({"error": "DB lock failed"}),
    };
    get_full_graph(&conn)
}

#[tauri::command]
pub fn kg_query(db: State<DbState>, term: String, max_hops: Option<usize>) -> Value {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return json!({"error": "DB lock failed"}),
    };
    query_subgraph(&conn, &term, max_hops.unwrap_or(2))
}

#[tauri::command]
pub fn kg_stats(db: State<DbState>) -> Value {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return json!({"error": "DB lock failed"}),
    };
    get_stats(&conn)
}

#[tauri::command]
pub fn kg_delete_node_cmd(db: State<DbState>, node_id: String) -> Value {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return json!({"error": "DB lock failed"}),
    };
    match delete_node(&conn, &node_id) {
        Ok(n) => json!({"ok": true, "deleted": n}),
        Err(e) => json!({"error": e}),
    }
}

/// 回填: 从现有 wiki_fts 数据生成图谱节点 (一次性迁移工具)
#[tauri::command]
pub fn kg_backfill(db: State<DbState>) -> Value {
    let conn = match db.0.lock() {
        Ok(c) => c,
        Err(_) => return json!({"error": "DB lock failed"}),
    };

    // 读取 wiki_fts 中所有条目
    let mut stmt = match conn.prepare(
        "SELECT file_name, source_path, summary, keywords, category FROM wiki_fts"
    ) {
        Ok(s) => s,
        Err(e) => return json!({"error": format!("query failed: {}", e)}),
    };

    let entries: Vec<(String, String, String, String, String)> = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0).unwrap_or_default(),
            row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, String>(2).unwrap_or_default(),
            row.get::<_, String>(3).unwrap_or_default(),
            row.get::<_, String>(4).unwrap_or_default(),
        ))
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    };

    let mut node_count = 0usize;
    let mut edge_count = 0usize;

    for (file_name, source_path, summary, keywords, category) in &entries {
        // 根据 category 决定 node_type
        let node_type = match category.as_str() {
            "pdf" | "docx" | "doc" | "xlsx" | "xls" | "pptx" => "file",
            "learned_project" => "project",
            "learned_user" => "concept",
            "learned_feedback" => "concept",
            "learned_reference" => "concept",
            _ => "file",
        };

        // 生成节点 ID
        let node_id = format!("{}_{}", node_type,
            file_name.to_lowercase()
                .replace(' ', "_")
                .replace('.', "_")
                .chars().take(60).collect::<String>()
        );

        let source = if source_path.is_empty() { file_name } else { source_path };
        let _ = upsert_node(&conn, &node_id, file_name, node_type, summary, source);
        node_count += 1;

        // 从 keywords 创建 tag 节点 + 边
        if !keywords.is_empty() {
            for kw in keywords.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                let tag_id = format!("tag_{}", kw.to_lowercase().replace(' ', "_"));
                let _ = upsert_node(&conn, &tag_id, kw, "tag", "", "");
                let _ = insert_edge(&conn, &node_id, &tag_id, "tagged_as", 0.7);
                edge_count += 1;
            }
        }
    }

    // 同类型 keyword 节点之间创建 related_to 边 (共现关系)
    // 简单策略: 同一文件的关键词互相连接
    for (file_name, _, _, keywords, _) in &entries {
        if keywords.is_empty() { continue; }
        let kws: Vec<String> = keywords.split(',')
            .map(|s| format!("tag_{}", s.trim().to_lowercase().replace(' ', "_")))
            .filter(|s| s.len() > 4)
            .collect();
        for i in 0..kws.len().min(3) {
            for j in (i+1)..kws.len().min(4) {
                let _ = insert_edge(&conn, &kws[i], &kws[j], "co_occurs_in", 0.5);
                edge_count += 1;
            }
        }
        // file_name node -> first few keywords (already done above)
        let _ = file_name; // suppress warning
    }

    json!({
        "ok": true,
        "nodes_created": node_count,
        "edges_created": edge_count,
        "source_entries": entries.len()
    })
}

#[derive(serde::Deserialize)]
pub struct MergeNodesPayload {
    pub primary_id: String,
    pub alias_id: String,
}

#[tauri::command]
pub fn kg_merge_nodes(
    state: State<'_, DbState>,
    payload: MergeNodesPayload,
) -> Result<Value, String> {
    let conn = match state.0.lock() {
        Ok(c) => c,
        Err(_) => return Ok(json!({ "ok": false, "error": "DB lock failed" })),
    };
    if let Err(e) = merge_nodes(&conn, &payload.primary_id, &payload.alias_id) {
        return Ok(json!({ "ok": false, "error": e }));
    }
    Ok(json!({ "ok": true }))
}
