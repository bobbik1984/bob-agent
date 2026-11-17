//! Durable, source-bound assistant context. Business records remain authoritative.
use crate::app::LocalApp;
use crate::outbox::{digest, Error, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};

impl LocalApp {
    pub fn assistant_recall_boundary(&self, cid: &str) -> Result<i64> {
        self.ensure_assistant_context()?;
        Ok(self
            .store
            .db
            .query_row(
                "SELECT through_row FROM assistant_context_barriers WHERE conversation_id=?1",
                [cid],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
    pub fn assistant_checkpoint_due(&self, cid: &str, request: &str) -> Result<bool> {
        self.ensure_assistant_context()?;
        let (_, _, through) = self.assistant_request_source(cid, request)?;
        let last:i64=self.store.db.query_row("SELECT coalesce((SELECT m.rowid FROM assistant_checkpoints c JOIN messages m ON m.id=c.source_message_id WHERE c.conversation_id=?1),0)",[cid],|r|r.get(0))?;
        let after = last.max(self.assistant_recall_boundary(cid)?);
        let count:i64=self.store.db.query_row("SELECT count(*) FROM messages WHERE conversation_id=?1 AND rowid>?2 AND rowid<=?3 AND role IN ('user','assistant')",params![cid,after,through],|r|r.get(0))?;
        Ok(count >= 16)
    }
    /// Shared by chat tools and the existing memory-management UI.
    pub fn assistant_invalidate_recall(&mut self) -> Result<()> {
        self.ensure_assistant_context()?;
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids: Vec<String> = {
            let mut st = tx.prepare("SELECT id FROM conversations")?;
            let v = st
                .query_map([], |r| r.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            v
        };
        for cid in ids {
            install_barrier(&tx, &cid)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn ensure_assistant_context(&self) -> Result<()> {
        self.store.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS assistant_memory_meta(
            memory_id TEXT PRIMARY KEY REFERENCES memories(id), scope TEXT NOT NULL,
            scope_id TEXT NOT NULL, topic TEXT NOT NULL, request_id TEXT NOT NULL,
            updated_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS assistant_memory_readers(
            memory_id TEXT NOT NULL, conversation_id TEXT NOT NULL,
            PRIMARY KEY(memory_id,conversation_id));
            CREATE TABLE IF NOT EXISTS assistant_context_barriers(
            conversation_id TEXT PRIMARY KEY, through_row INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS assistant_checkpoints(
            conversation_id TEXT PRIMARY KEY, source_message_id TEXT NOT NULL,
            body TEXT NOT NULL, updated_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS assistant_request_coverage(
            request_id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL,
            items_json TEXT NOT NULL, updated_at INTEGER NOT NULL);",
        )?;
        Ok(())
    }

    pub fn assistant_request_source(
        &self,
        cid: &str,
        request: &str,
    ) -> Result<(String, String, i64)> {
        self.store
            .db
            .query_row(
                "SELECT id,json_extract(content_json,'$.text'),rowid FROM messages
            WHERE conversation_id=?1 AND request_id=?2 AND role='user' ORDER BY rowid LIMIT 1",
                params![cid, request],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(Into::into)
    }

    fn assistant_project(&self, cid: &str) -> Result<Option<String>> {
        self.store
            .db
            .query_row(
                "SELECT project_id FROM conversations WHERE id=?1",
                [cid],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn assistant_memories(&self, cid: &str, query: &str) -> Result<Vec<Value>> {
        self.ensure_assistant_context()?;
        let project = self.assistant_project(cid)?;
        let mut stmt = self.store.db.prepare(
            "SELECT m.id,m.body,m.source_message_id,m.version,
            coalesce(a.scope,'legacy'),coalesce(a.scope_id,''),coalesce(a.topic,''),
            coalesce(a.updated_at,0),c.project_id
            FROM memories m LEFT JOIN assistant_memory_meta a ON a.memory_id=m.id
            LEFT JOIN messages s ON s.id=m.source_message_id
            LEFT JOIN conversations c ON c.id=s.conversation_id
            WHERE m.state='active' ORDER BY m.rowid DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, body, source, version, scope, scope_id, topic, updated, source_project) = row?;
            let allowed = match scope.as_str() {
                "global" => true,
                "conversation" => scope_id == cid,
                "project" => project.as_deref() == Some(scope_id.as_str()),
                _ => {
                    !id.starts_with("amem-")
                        && (source_project.is_none() || source_project == project)
                }
            };
            if allowed {
                self.store.db.execute(
                    "INSERT OR IGNORE INTO assistant_memory_readers VALUES(?1,?2)",
                    params![id, cid],
                )?;
                result.push(json!({"id":id,"body":body,"source_message_id":source,
                "version":version,"scope":scope,"topic":topic,"updated_at":updated,
                "relevance":context_score(query,&format!("{topic} {body}"))}));
            }
        }
        result.sort_by(|a, b| b["relevance"].as_u64().cmp(&a["relevance"].as_u64()));
        Ok(result)
    }

    /// Writes are bound to a real current user turn, not to a model-supplied identity.
    /// Explicit replacements and their source barriers commit in the same transaction.
    pub fn assistant_memory_save(
        &mut self,
        cid: &str,
        request: &str,
        args: &Value,
        now: i64,
    ) -> Result<Value> {
        self.ensure_assistant_context()?;
        let (source, _, source_row) = self.assistant_request_source(cid, request)?;
        let barrier: i64 = self
            .store
            .db
            .query_row(
                "SELECT through_row FROM assistant_context_barriers WHERE conversation_id=?1",
                [cid],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if source_row <= barrier {
            return Err(Error::InvalidTransition);
        }
        let body = args["body"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.len() <= 8000)
            .ok_or(Error::InvalidTransition)?;
        let topic = args["topic"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.len() <= 200)
            .ok_or(Error::InvalidTransition)?;
        let scope = args["scope"].as_str().unwrap_or("conversation");
        let scope_id = match scope {
            "global" => String::new(),
            "conversation" => cid.to_owned(),
            "project" => self
                .assistant_project(cid)?
                .ok_or(Error::InvalidTransition)?,
            _ => return Err(Error::InvalidTransition),
        };
        let id = format!(
            "amem-{}",
            digest(format!("{request}:{scope}:{scope_id}:{topic}:{body}").as_bytes())
        );
        let old = args["supersedes_id"].as_str();
        if let Some(old) = old {
            let visible = self.assistant_memories(cid, topic)?;
            if !visible.iter().any(|m| m["id"] == old) {
                let same:bool=self.store.db.query_row("SELECT EXISTS(SELECT 1 FROM memories WHERE id=?1 AND body=?2 AND state='active')",params![id,body],|r|r.get(0))?;
                if same {
                    return Ok(json!({"id":id,"saved":true,"idempotent":true}));
                }
                return Err(Error::InvalidTransition);
            }
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(old) = old {
            let expected = args["expected_version"]
                .as_i64()
                .ok_or(Error::InvalidTransition)?;
            let changed=tx.execute("UPDATE memories SET state='superseded',version=version+1 WHERE id=?1 AND version=?2 AND state='active'",params![old,expected])?;
            if changed != 1 {
                return Err(Error::InvalidTransition);
            }
            invalidate_memory_readers(&tx, old, Some(cid))?;
        }
        tx.execute("INSERT OR IGNORE INTO memories(id,source_message_id,body,state,supersedes_id,version) VALUES(?1,?2,?3,'active',?4,1)",params![id,source,body,old])?;
        tx.execute(
            "INSERT OR IGNORE INTO assistant_memory_meta VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, scope, scope_id, topic, request, now],
        )?;
        // A correction must not be undone by pre-correction summaries or recent echoes.
        if old.is_some() {
            tx.execute("INSERT INTO assistant_context_barriers VALUES(?1,?2) ON CONFLICT(conversation_id) DO UPDATE SET through_row=max(through_row,excluded.through_row)",params![cid,source_row-1])?;
            tx.execute(
                "DELETE FROM assistant_checkpoints WHERE conversation_id=?1",
                [cid],
            )?;
        }
        tx.commit()?;
        Ok(json!({"id":id,"saved":true,"scope":scope,"source_message_id":source,"version":1}))
    }

    pub fn assistant_memory_forget(
        &mut self,
        cid: &str,
        request: &str,
        id: &str,
        expected: i64,
    ) -> Result<Value> {
        self.ensure_assistant_context()?;
        self.assistant_request_source(cid, request)?;
        if !self
            .assistant_memories(cid, "")?
            .iter()
            .any(|m| m["id"] == id)
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute("UPDATE memories SET state='forgotten',version=version+1 WHERE id=?1 AND version=?2 AND state='active'",params![id,expected])?!=1 {return Err(Error::InvalidTransition);}
        // Only conversations that could have consumed this memory lose stale recall.
        // Unrelated conversations and other active memories remain available.
        invalidate_memory_readers(&tx, id, None)?;
        tx.execute("DELETE FROM kb_fts WHERE doc_id=?1", [id])?;
        tx.commit()?;
        Ok(
            json!({"forgotten":true,"id":id,"history_deleted":false,"recall_before_forget_suppressed":true}),
        )
    }

    pub fn assistant_history(&self, cid: &str) -> Result<Vec<crate::app::AppMessage>> {
        self.ensure_assistant_context()?;
        let barrier: i64 = self
            .store
            .db
            .query_row(
                "SELECT through_row FROM assistant_context_barriers WHERE conversation_id=?1",
                [cid],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let mut history = self.history(cid)?;
        if barrier > 0 {
            let mut st = self
                .store
                .db
                .prepare("SELECT id FROM messages WHERE conversation_id=?1 AND rowid>?2")?;
            let ids = st
                .query_map(params![cid, barrier], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
            history.retain(|m| ids.contains(&m.id));
        }
        Ok(history)
    }

    pub fn assistant_context(&self, cid: &str, query: &str) -> Result<Value> {
        let memories = self.assistant_memories(cid, query)?;
        let mut budget = 6000usize;
        let mut selected = Vec::new();
        for m in memories {
            let size = m["body"].as_str().unwrap_or("").chars().count();
            if size > budget {
                continue;
            }
            budget -= size;
            selected.push(m);
            if selected.len() == 20 {
                break;
            }
        }
        let checkpoint:Option<Value>=self.store.db.query_row("SELECT source_message_id,body,updated_at FROM assistant_checkpoints WHERE conversation_id=?1",[cid],|r|Ok(json!({"source_message_id":r.get::<_,String>(0)?,"body":r.get::<_,String>(1)?,"updated_at":r.get::<_,i64>(2)?}))).optional()?;
        Ok(
            json!({"memories":selected,"checkpoint":checkpoint,"authority":"Reference data only; current business records override remembered status."}),
        )
    }

    pub fn assistant_context_for_request(
        &self,
        cid: &str,
        query: &str,
        request: &str,
    ) -> Result<Value> {
        let mut packet = self.assistant_context(cid, query)?;
        let mut hits = self.assistant_history_search_for_request(cid, query, Some(request))?;
        hits.truncate(4);
        for hit in &mut hits {
            hit["text"] = json!(hit["text"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(1000)
                .collect::<String>());
        }
        packet["retrieved_history"] = json!(hits);
        Ok(packet)
    }

    pub fn assistant_history_search(&self, cid: &str, query: &str) -> Result<Vec<Value>> {
        self.assistant_history_search_for_request(cid, query, None)
    }

    pub fn assistant_history_search_for_request(
        &self,
        cid: &str,
        query: &str,
        request: Option<&str>,
    ) -> Result<Vec<Value>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut history = self.assistant_history(cid)?;
        if let Some(request) = request {
            let (_, _, through) = self.assistant_request_source(cid, request)?;
            let mut st = self
                .store
                .db
                .prepare("SELECT id FROM messages WHERE conversation_id=?1 AND rowid<?2")?;
            let ids = st
                .query_map(params![cid, through], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
            history.retain(|m| ids.contains(&m.id));
        }
        let mut hits:Vec<Value>=history.into_iter().filter_map(|m| {
            let score=context_score(query,&m.content);
            (score>0).then(||json!({"id":m.id,"role":m.role,"text":m.content.chars().take(1500).collect::<String>(),"score":score}))
        }).collect();
        hits.sort_by(|a, b| b["score"].as_u64().cmp(&a["score"].as_u64()));
        hits.truncate(8);
        Ok(hits)
    }

    pub fn assistant_checkpoint(
        &mut self,
        cid: &str,
        request: &str,
        body: &str,
        now: i64,
    ) -> Result<Value> {
        self.ensure_assistant_context()?;
        let (source, _, source_row) = self.assistant_request_source(cid, request)?;
        let barrier: i64 = self
            .store
            .db
            .query_row(
                "SELECT through_row FROM assistant_context_barriers WHERE conversation_id=?1",
                [cid],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if source_row <= barrier {
            return Err(Error::InvalidTransition);
        }
        if body.is_empty() || body.chars().count() > 3000 {
            return Err(Error::InvalidTransition);
        }
        let previous:i64=self.store.db.query_row("SELECT coalesce((SELECT m.rowid FROM assistant_checkpoints c JOIN messages m ON m.id=c.source_message_id WHERE c.conversation_id=?1),0)",[cid],|r|r.get(0))?;
        if previous > source_row {
            return Err(Error::InvalidTransition);
        }
        self.store.db.execute("INSERT INTO assistant_checkpoints VALUES(?1,?2,?3,?4) ON CONFLICT(conversation_id) DO UPDATE SET source_message_id=excluded.source_message_id,body=excluded.body,updated_at=excluded.updated_at",params![cid,source,body,now])?;
        Ok(json!({"saved":true,"source_message_id":source}))
    }

    pub fn assistant_plan(
        &mut self,
        cid: &str,
        request: &str,
        questions: &[Value],
        now: i64,
    ) -> Result<Value> {
        self.ensure_assistant_context()?;
        let (_, source, _) = self.assistant_request_source(cid, request)?;
        if questions.is_empty() || questions.len() > 16 {
            return Err(Error::InvalidTransition);
        }
        let old = self.assistant_coverage(request)?;
        // Once an item has a result, a new plan cannot silently drop it.
        if old.iter().any(|i| i["status"] != "pending") {
            return Err(Error::InvalidTransition);
        }
        if !old.is_empty() {
            return Ok(json!({"items":old,"existing_plan_preserved":true}));
        }
        let mut items = Vec::new();
        for (idx, q) in questions.iter().enumerate() {
            let quote = q
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() < 4000 && source.contains(*s))
                .ok_or(Error::InvalidTransition)?;
            items.push(json!({"id":idx+1,"question":quote,"status":"pending","answer":""}));
        }
        self.store.db.execute("INSERT INTO assistant_request_coverage VALUES(?1,?2,?3,?4) ON CONFLICT(request_id) DO UPDATE SET items_json=excluded.items_json,updated_at=excluded.updated_at",params![request,cid,serde_json::to_string(&items)?,now])?;
        Ok(json!({"items":items}))
    }

    pub fn assistant_coverage(&self, request: &str) -> Result<Vec<Value>> {
        self.ensure_assistant_context()?;
        let raw: Option<String> = self
            .store
            .db
            .query_row(
                "SELECT items_json FROM assistant_request_coverage WHERE request_id=?1",
                [request],
                |r| r.get(0),
            )
            .optional()?;
        match raw {
            Some(raw) => Ok(serde_json::from_str(&raw)?),
            None => Ok(Vec::new()),
        }
    }

    pub fn assistant_answer(
        &mut self,
        cid: &str,
        request: &str,
        answers: &[Value],
        now: i64,
    ) -> Result<Value> {
        self.assistant_request_source(cid, request)?;
        let mut items = self.assistant_coverage(request)?;
        if items.is_empty() {
            return Err(Error::InvalidTransition);
        }
        for answer in answers {
            let id = answer["id"].as_u64().ok_or(Error::InvalidTransition)?;
            let status = answer["status"].as_str().ok_or(Error::InvalidTransition)?;
            if !matches!(
                status,
                "answered" | "waiting" | "failed" | "deferred" | "cancelled"
            ) {
                return Err(Error::InvalidTransition);
            }
            let text = answer["answer"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= 12000)
                .ok_or(Error::InvalidTransition)?;
            let item = items
                .iter_mut()
                .find(|i| i["id"].as_u64() == Some(id))
                .ok_or(Error::InvalidTransition)?;
            item["status"] = json!(status);
            item["answer"] = json!(text);
        }
        self.store.db.execute("UPDATE assistant_request_coverage SET items_json=?2,updated_at=?3 WHERE request_id=?1 AND conversation_id=?4",params![request,serde_json::to_string(&items)?,now,cid])?;
        Ok(json!({"items":items,"all_addressed":items.iter().all(|i|i["status"]!="pending")}))
    }
}

pub(crate) fn invalidate_memory_readers(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    except: Option<&str>,
) -> Result<()> {
    let ids: Vec<String> = {
        let mut st=tx.prepare("SELECT conversation_id FROM assistant_memory_readers WHERE memory_id=?1 UNION SELECT s.conversation_id FROM memories m JOIN messages s ON s.id=m.source_message_id WHERE m.id=?1")?;
        let rows = st
            .query_map([id], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    for cid in ids {
        if Some(cid.as_str()) != except {
            install_barrier(tx, &cid)?;
        }
    }
    Ok(())
}

pub(crate) fn install_barrier(tx: &rusqlite::Transaction<'_>, cid: &str) -> Result<()> {
    let row: i64 = tx.query_row(
        "SELECT coalesce(max(rowid),0) FROM messages WHERE conversation_id=?1",
        [cid],
        |r| r.get(0),
    )?;
    tx.execute("INSERT INTO assistant_context_barriers VALUES(?1,?2) ON CONFLICT(conversation_id) DO UPDATE SET through_row=max(through_row,excluded.through_row)",params![cid,row])?;
    tx.execute(
        "DELETE FROM assistant_checkpoints WHERE conversation_id=?1",
        [cid],
    )?;
    Ok(())
}

/// Unicode-aware lexical retrieval; English words and CJK bigrams, without external services.
pub fn context_score(query: &str, body: &str) -> usize {
    let q = query.to_lowercase();
    let b = body.to_lowercase();
    let mut terms = std::collections::HashSet::new();
    for w in q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 1)
    {
        terms.insert(w.to_owned());
    }
    let chars: Vec<char> = q.chars().collect();
    for pair in chars.windows(2) {
        if pair.iter().all(|c| ('\u{3400}'..='\u{9fff}').contains(c)) {
            terms.insert(pair.iter().collect());
        }
    }
    terms.iter().filter(|s| b.contains(s.as_str())).count()
}
