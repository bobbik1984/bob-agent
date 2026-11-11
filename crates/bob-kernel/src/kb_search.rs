//! SQLite FTS5 Knowledge Base Search & Provenance Entity Graph.
//!
//! Provides:
//! - Virtual table `kb_fts` indexed with unicode61 tokenizer.
//! - Entity edges table `kb_entity_edges` connecting docs with `#tags` and `@mentions`.
//! - Safe query sanitization preventing SQLite FTS5 syntax errors.
//! - CJK substring fallback for non-tokenized Chinese text.
//! - Rebuilding and syncing indices from active notes, memories, decisions, captures and schedules.

use crate::app::LocalApp;
use crate::outbox::{Error, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KbSearchHit {
    pub doc_id: String,
    pub doc_kind: String,
    pub title: String,
    pub snippet: String,
    pub tags: Vec<String>,
    pub provenance_id: Option<String>,
    pub related_entities: Vec<String>,
    pub updated_at: i64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityEdge {
    pub id: String,
    pub source_doc_id: String,
    pub entity_tag: String,
    pub relation: String,
    pub created_at: i64,
}

pub fn extract_entity_tags(text: &str) -> (Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut entities = Vec::new();

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '#' || chars[i] == '@' {
            let is_tag = chars[i] == '#';
            let start = i + 1;
            let mut end = start;
            while end < len
                && (chars[end].is_alphanumeric() || chars[end] == '_' || chars[end] == '-')
            {
                end += 1;
            }
            if end > start {
                let token: String = chars[start..end].iter().collect();
                let clean_token = token.trim_matches('-');
                if !clean_token.is_empty() {
                    if is_tag {
                        let formatted = format!("#{clean_token}");
                        if !tags.contains(&formatted) {
                            tags.push(formatted);
                        }
                    } else {
                        let formatted = format!("@{clean_token}");
                        if !entities.contains(&formatted) {
                            entities.push(formatted);
                        }
                    }
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }

    (tags, entities)
}

pub fn sanitize_fts5_query(raw: &str) -> String {
    let mut tokens = Vec::new();
    let chars: Vec<char> = raw.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        while i < len
            && (chars[i].is_whitespace()
                || matches!(
                    chars[i],
                    '"' | '*'
                        | ':'
                        | '('
                        | ')'
                        | '-'
                        | '+'
                        | '^'
                        | '{'
                        | '}'
                        | '['
                        | ']'
                        | '~'
                        | '&'
                        | '|'
                        | '!'
                        | '?'
                        | ','
                        | '.'
                        | ';'
                        | '/'
                        | '\\'
                        | '%'
                        | '\''
                ))
        {
            i += 1;
        }
        if i >= len {
            break;
        }
        let start = i;
        while i < len
            && !chars[i].is_whitespace()
            && !matches!(
                chars[i],
                '"' | '*'
                    | ':'
                    | '('
                    | ')'
                    | '-'
                    | '+'
                    | '^'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '~'
                    | '&'
                    | '|'
                    | '!'
                    | '?'
                    | ','
                    | '.'
                    | ';'
                    | '/'
                    | '\\'
                    | '%'
                    | '\''
            )
        {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            tokens.push(format!("\"{trimmed}\""));
        }
    }

    tokens.join(" ")
}

fn make_snippet(body: &str, query: &str) -> String {
    let clean_body = body.trim();
    if clean_body.is_empty() {
        return String::new();
    }
    let lower_body = clean_body.to_lowercase();
    let lower_q = query.trim().to_lowercase();

    if !lower_q.is_empty() {
        if let Some(pos) = lower_body.find(&lower_q) {
            let start = pos.saturating_sub(40);
            let end = (pos + lower_q.len() + 100).min(clean_body.len());
            let snippet = &clean_body[start..end];
            let mut res = String::new();
            if start > 0 {
                res.push_str("...");
            }
            res.push_str(snippet);
            if end < clean_body.len() {
                res.push_str("...");
            }
            return res;
        }
    }

    let limit = clean_body.chars().take(150).collect::<String>();
    if clean_body.len() > limit.len() {
        format!("{limit}...")
    } else {
        limit
    }
}

impl LocalApp {
    pub fn ensure_kb_search_schema(&self) -> Result<()> {
        let sql = r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS kb_fts USING fts5(
                doc_id UNINDEXED,
                doc_kind UNINDEXED,
                title,
                body,
                tags,
                provenance_id UNINDEXED,
                updated_at UNINDEXED,
                tokenize='unicode61'
            );
            CREATE TABLE IF NOT EXISTS kb_entity_edges (
                id TEXT PRIMARY KEY,
                source_doc_id TEXT NOT NULL,
                entity_tag TEXT NOT NULL,
                relation TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_kb_entity_edges_doc ON kb_entity_edges(source_doc_id);
            CREATE INDEX IF NOT EXISTS idx_kb_entity_edges_tag ON kb_entity_edges(entity_tag);
        "#;
        self.store.db.execute_batch(sql)?;
        Ok(())
    }

    pub fn rebuild_or_sync_kb_index(&self) -> Result<usize> {
        self.ensure_kb_search_schema()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let tx = self
            .store
            .db
            .unchecked_transaction()
            .map_err(|e| Error::Database(e))?;

        tx.execute("DELETE FROM kb_fts", [])?;
        tx.execute("DELETE FROM kb_entity_edges", [])?;

        let mut total_indexed = 0usize;

        // 1. Notes
        {
            let mut stmt = tx.prepare(
                "SELECT id, title, body_markdown, tags_json, conversation_id, updated_at FROM notes ORDER BY rowid ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })?;

            let mut notes = Vec::new();
            for r in rows {
                notes.push(r?);
            }
            drop(stmt);

            for (id, title, body, tags_json, conv_id, updated_at) in notes {
                let tags_vec: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                let tags_str = tags_vec.join(" ");

                tx.execute(
                    "INSERT INTO kb_fts(doc_id, doc_kind, title, body, tags, provenance_id, updated_at)
                     VALUES(?1, 'note', ?2, ?3, ?4, ?5, ?6)",
                    params![id, title, body, tags_str, conv_id, updated_at],
                )?;

                let combined_text = format!("{title} {body} {tags_str}");
                let (mut tags, entities) = extract_entity_tags(&combined_text);
                for t in tags_vec {
                    let formatted = if t.starts_with('#') { t } else { format!("#{t}") };
                    if !tags.contains(&formatted) {
                        tags.push(formatted);
                    }
                }

                for t in tags {
                    let edge_id = format!("edge:{id}:{t}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'tag', ?4)",
                        params![edge_id, id, t, now],
                    );
                }
                for e in entities {
                    let edge_id = format!("edge:{id}:{e}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'mention', ?4)",
                        params![edge_id, id, e, now],
                    );
                }
                total_indexed += 1;
            }
        }

        // 2. Active Memories
        {
            let mut stmt = tx.prepare(
                "SELECT id, body, source_message_id, source_capture_id, version FROM memories WHERE state='active' ORDER BY rowid ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?;

            let mut mems = Vec::new();
            for r in rows {
                mems.push(r?);
            }
            drop(stmt);

            for (id, body, src_msg, src_cap, version) in mems {
                let prov = src_msg.or(src_cap);
                tx.execute(
                    "INSERT INTO kb_fts(doc_id, doc_kind, title, body, tags, provenance_id, updated_at)
                     VALUES(?1, 'memory', 'Memory', ?2, '', ?3, ?4)",
                    params![id, body, prov, version],
                )?;

                let (tags, entities) = extract_entity_tags(&body);
                for t in tags {
                    let edge_id = format!("edge:{id}:{t}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'tag', ?4)",
                        params![edge_id, id, t, now],
                    );
                }
                for e in entities {
                    let edge_id = format!("edge:{id}:{e}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'mention', ?4)",
                        params![edge_id, id, e, now],
                    );
                }
                total_indexed += 1;
            }
        }

        // 3. Decisions
        {
            let mut stmt = tx.prepare(
                "SELECT id, body, rationale_json, source_message_id, status FROM decisions ORDER BY rowid ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?;

            let mut decs = Vec::new();
            for r in rows {
                decs.push(r?);
            }
            drop(stmt);

            for (id, body, rationale_json, src_msg, status) in decs {
                let title = format!("Decision: {status}");
                let full_body = format!("{body} {rationale_json}");
                tx.execute(
                    "INSERT INTO kb_fts(doc_id, doc_kind, title, body, tags, provenance_id, updated_at)
                     VALUES(?1, 'decision', ?2, ?3, ?4, ?5, ?6)",
                    params![id, title, full_body, status, src_msg, now],
                )?;

                let (tags, entities) = extract_entity_tags(&full_body);
                for t in tags {
                    let edge_id = format!("edge:{id}:{t}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'tag', ?4)",
                        params![edge_id, id, t, now],
                    );
                }
                for e in entities {
                    let edge_id = format!("edge:{id}:{e}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'mention', ?4)",
                        params![edge_id, id, e, now],
                    );
                }
                total_indexed += 1;
            }
        }

        // 4. Captures
        {
            let mut stmt = tx.prepare(
                "SELECT id, content_ref, source_message_id, created_at FROM captures ORDER BY rowid ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?;

            let mut caps = Vec::new();
            for r in rows {
                caps.push(r?);
            }
            drop(stmt);

            for (id, content_ref, src_msg, created_at) in caps {
                tx.execute(
                    "INSERT INTO kb_fts(doc_id, doc_kind, title, body, tags, provenance_id, updated_at)
                     VALUES(?1, 'capture', 'Capture', ?2, '', ?3, ?4)",
                    params![id, content_ref, src_msg, created_at],
                )?;

                let (tags, entities) = extract_entity_tags(&content_ref);
                for t in tags {
                    let edge_id = format!("edge:{id}:{t}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'tag', ?4)",
                        params![edge_id, id, t, now],
                    );
                }
                for e in entities {
                    let edge_id = format!("edge:{id}:{e}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'mention', ?4)",
                        params![edge_id, id, e, now],
                    );
                }
                total_indexed += 1;
            }
        }

        // 5. Schedules
        {
            let mut stmt = tx.prepare(
                "SELECT id, title, notes, task_id, kind, status, updated_at FROM schedule_items ORDER BY rowid ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })?;

            let mut scheds = Vec::new();
            for r in rows {
                scheds.push(r?);
            }
            drop(stmt);

            for (id, title, notes, task_id, kind, status, updated_at) in scheds {
                let tags = format!("{kind} {status}");
                tx.execute(
                    "INSERT INTO kb_fts(doc_id, doc_kind, title, body, tags, provenance_id, updated_at)
                     VALUES(?1, 'schedule', ?2, ?3, ?4, ?5, ?6)",
                    params![id, title, notes, tags, task_id, updated_at],
                )?;

                let combined_text = format!("{title} {notes}");
                let (mut tags_vec, entities) = extract_entity_tags(&combined_text);
                if !tags_vec.contains(&format!("#{kind}")) {
                    tags_vec.push(format!("#{kind}"));
                }
                for t in tags_vec {
                    let edge_id = format!("edge:{id}:{t}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'tag', ?4)",
                        params![edge_id, id, t, now],
                    );
                }
                for e in entities {
                    let edge_id = format!("edge:{id}:{e}");
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO kb_entity_edges(id, source_doc_id, entity_tag, relation, created_at)
                         VALUES(?1, ?2, ?3, 'mention', ?4)",
                        params![edge_id, id, e, now],
                    );
                }
                total_indexed += 1;
            }
        }

        tx.commit()?;
        Ok(total_indexed)
    }

    pub fn kb_search(&self, query: &str, limit: usize) -> Result<Vec<KbSearchHit>> {
        self.ensure_kb_search_schema()?;
        let _ = self.rebuild_or_sync_kb_index();
        let trimmed_query = query.trim();
        if trimmed_query.is_empty() {
            return Ok(Vec::new());
        }

        let max_limit = limit.max(1).min(100);
        let sanitized = sanitize_fts5_query(trimmed_query);

        let mut hits: Vec<KbSearchHit> = Vec::new();

        // 1. FTS5 MATCH query if sanitized string is not empty
        if !sanitized.is_empty() {
            let sql = "SELECT doc_id, doc_kind, title, body, tags, provenance_id, updated_at, bm25(kb_fts)
                       FROM kb_fts WHERE kb_fts MATCH ?1 ORDER BY bm25(kb_fts) ASC LIMIT ?2";
            if let Ok(mut stmt) = self.store.db.prepare(sql) {
                let rows = stmt
                    .query_map(params![sanitized, max_limit as i64], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, Option<i64>>(6)?,
                            r.get::<_, f64>(7)?,
                        ))
                    })
                    .ok();

                if let Some(iter) = rows {
                    for row in iter.flatten() {
                        let (id, kind, title, body, tags_str, prov, updated_at, bm25) = row;
                        let snippet = make_snippet(&body, trimmed_query);
                        let tags: Vec<String> = tags_str
                            .split_whitespace()
                            .map(str::to_owned)
                            .collect();
                        // BM25 is negative in SQLite; invert so higher is better
                        let score = -bm25;

                        hits.push(KbSearchHit {
                            doc_id: id,
                            doc_kind: kind,
                            title,
                            snippet,
                            tags,
                            provenance_id: prov,
                            related_entities: Vec::new(),
                            updated_at: updated_at.unwrap_or(0),
                            score,
                        });
                    }
                }
            }
        }

        // 2. CJK / Substring LIKE Fallback
        // Always attempt fallback if hits are empty or if query contains non-ASCII characters
        let contains_cjk = trimmed_query.chars().any(|c| c as u32 > 0x2e80);
        if hits.is_empty() || contains_cjk {
            let pattern = format!("%{trimmed_query}%");
            let sql = "SELECT doc_id, doc_kind, title, body, tags, provenance_id, updated_at
                       FROM kb_fts WHERE title LIKE ?1 OR body LIKE ?1 OR tags LIKE ?1 LIMIT ?2";
            if let Ok(mut stmt) = self.store.db.prepare(sql) {
                let rows = stmt
                    .query_map(params![pattern, max_limit as i64], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, Option<i64>>(6)?,
                        ))
                    })
                    .ok();

                if let Some(iter) = rows {
                    for row in iter.flatten() {
                        let (id, kind, title, body, tags_str, prov, updated_at) = row;
                        if !hits.iter().any(|h| h.doc_id == id) {
                            let snippet = make_snippet(&body, trimmed_query);
                            let tags: Vec<String> = tags_str
                                .split_whitespace()
                                .map(str::to_owned)
                                .collect();
                            hits.push(KbSearchHit {
                                doc_id: id,
                                doc_kind: kind,
                                title,
                                snippet,
                                tags,
                                provenance_id: prov,
                                related_entities: Vec::new(),
                                updated_at: updated_at.unwrap_or(0),
                                score: 1.0,
                            });
                        }
                    }
                }
            }
        }

        // 3. Populate related entities from kb_entity_edges
        for hit in &mut hits {
            if let Ok(mut stmt) = self.store.db.prepare(
                "SELECT entity_tag FROM kb_entity_edges WHERE source_doc_id = ?1 ORDER BY created_at ASC",
            ) {
                let edges = stmt
                    .query_map([&hit.doc_id], |r| r.get::<_, String>(0))
                    .ok();
                if let Some(iter) = edges {
                    for tag in iter.flatten() {
                        if !hit.related_entities.contains(&tag) {
                            hit.related_entities.push(tag);
                        }
                    }
                }
            }
        }

        // Sort descending by score
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        if hits.len() > max_limit {
            hits.truncate(max_limit);
        }

        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_sanitize_fts5_query_punctuation() {
        let raw = r#"note: "search" -foo +bar *punctuation* (group) [bracket] {brace} & | ! ? , . ; / \ % ' "#;
        let sanitized = sanitize_fts5_query(raw);
        assert!(!sanitized.contains(':'));
        assert!(!sanitized.contains('*'));
        assert!(!sanitized.contains('('));
        assert!(!sanitized.contains(')'));
        assert!(!sanitized.contains('-'));
        assert!(!sanitized.contains('+'));
        assert!(sanitized.contains("\"note\""));
        assert!(sanitized.contains("\"search\""));
        assert!(sanitized.contains("\"foo\""));
        assert!(sanitized.contains("\"bar\""));
        assert!(sanitized.contains("\"punctuation\""));
    }

    #[test]
    fn test_extract_entity_tags_bilingual() {
        let text = "Meeting with @alice and @张三 regarding #roadmap-v2 and #项目规划 for 2026";
        let (tags, entities) = extract_entity_tags(text);
        assert_eq!(tags, vec!["#roadmap-v2", "#项目规划"]);
        assert_eq!(entities, vec!["@alice", "@张三"]);
    }

    #[test]
    fn test_kb_search_across_kinds_and_languages() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("kernel.sqlite");
        let mut app = LocalApp::open(&db_path).unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let session = app.default_session().unwrap();
        app.accept("req-1", "msg-1", &session, "用户指示".into(), now).unwrap();
        let proj = app.create_project("proj-main", "Main Project", now).unwrap();

        // 1. Note with English and tags
        app.upsert_note(
            "note-1",
            None,
            Some(&session),
            "Project Roadmap",
            "Sprint 1 deliverables and #feature-x with @alice",
            &["planning".to_string()],
            now,
        )
        .unwrap();

        // 2. Note with Chinese
        app.upsert_note(
            "note-2",
            None,
            Some(&session),
            "北京出差会议纪要",
            "与@张三 讨论#预算2026 方案和架构落地",
            &["会议".to_string(), "北京".to_string()],
            now + 1,
        )
        .unwrap();

        // 3. Active Memory
        let mem = app
            .propose_memory(
                "mem-1",
                None,
                Some("msg-1"),
                "用户偏好使用简洁的中文回答，关注#效率",
                None,
            )
            .unwrap();
        app.update_memory_state(&mem.id, "active").unwrap();

        // 4. Decision
        app.record_decision(
            "dec-1",
            &proj.id,
            None,
            "msg-1",
            "采纳 QuickJS 作为沙箱执行引擎",
            "{\"reason\":\"lightweight\"}",
            "accepted",
            None,
        )
        .unwrap();

        // 5. Capture
        app.create_capture(
            "cap-1",
            Some("msg-1"),
            "需要调研 #WebPush 在 iOS 16.4 上的表现 @team",
            now + 2,
        )
        .unwrap();

        // 6. Schedule item
        app.create_schedule_item_with_kind(
            "sched-1",
            None,
            None,
            "团队周会",
            "event",
            now + 1000,
            Some(now + 2000),
            None,
            "讨论季度规划与复盘",
            now + 3,
        )
        .unwrap();

        // Sync index
        let count = app.rebuild_or_sync_kb_index().unwrap();
        assert_eq!(count, 6);

        // Search English
        let hits = app.kb_search("Roadmap", 5).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].doc_id, "note-1");
        assert_eq!(hits[0].doc_kind, "note");
        assert!(hits[0].related_entities.contains(&"#feature-x".to_string()));
        assert!(hits[0].related_entities.contains(&"@alice".to_string()));

        // Search with punctuation and colons
        let hits_punct = app.kb_search("planning: #feature-x* (roadmap)", 5).unwrap();
        assert!(!hits_punct.is_empty());
        assert_eq!(hits_punct[0].doc_id, "note-1");

        // Search Chinese note
        let hits_cn = app.kb_search("北京", 5).unwrap();
        assert!(!hits_cn.is_empty());
        assert_eq!(hits_cn[0].doc_id, "note-2");
        assert!(hits_cn[0].related_entities.contains(&"@张三".to_string()));
        assert!(hits_cn[0].related_entities.contains(&"#预算2026".to_string()));

        // Search decision
        let hits_dec = app.kb_search("QuickJS", 5).unwrap();
        assert!(!hits_dec.is_empty());
        assert_eq!(hits_dec[0].doc_id, "dec-1");
        assert_eq!(hits_dec[0].doc_kind, "decision");

        // Search active memory
        let hits_mem = app.kb_search("简洁", 5).unwrap();
        assert!(!hits_mem.is_empty());
        assert_eq!(hits_mem[0].doc_id, "mem-1");
        assert_eq!(hits_mem[0].doc_kind, "memory");

        // Search capture
        let hits_cap = app.kb_search("WebPush", 5).unwrap();
        assert!(!hits_cap.is_empty());
        assert_eq!(hits_cap[0].doc_id, "cap-1");
        assert_eq!(hits_cap[0].doc_kind, "capture");

        // Search schedule
        let hits_sched = app.kb_search("周会", 5).unwrap();
        assert!(!hits_sched.is_empty());
        assert_eq!(hits_sched[0].doc_id, "sched-1");
        assert_eq!(hits_sched[0].doc_kind, "schedule");
    }
}

