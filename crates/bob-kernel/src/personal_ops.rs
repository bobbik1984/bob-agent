//! F3 Personal Assistant domain: Captures, Memories (with provenance and
//! supersede/forget lifecycle), Markdown Notes, Schedule/Calendar items, and
//! Decision records.
use crate::{
    app::LocalApp,
    outbox::{digest, Error, Result},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureItem {
    pub id: String,
    pub origin_device_id: String,
    pub source_message_id: Option<String>,
    pub content_ref: String,
    pub content_sha256: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub source_capture_id: Option<String>,
    pub source_message_id: Option<String>,
    pub body: String,
    pub state: String,
    pub supersedes_id: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteItem {
    pub id: String,
    pub project_id: Option<String>,
    pub conversation_id: Option<String>,
    pub origin_device_id: String,
    pub title: String,
    pub body_markdown: String,
    pub tags: Vec<String>,
    pub acceptance: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleItem {
    pub id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub origin_device_id: String,
    pub title: String,
    pub start_at: i64,
    pub end_at: Option<i64>,
    pub reminder_at: Option<i64>,
    pub status: String,
    pub notes: String,
    pub acceptance: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionItem {
    pub id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub source_message_id: String,
    pub body: String,
    pub rationale_json: String,
    pub status: String,
    pub supersedes_id: Option<String>,
    pub version: i64,
}

impl LocalApp {
    pub fn create_capture(
        &mut self,
        id: &str,
        source_message_id: Option<&str>,
        content_ref: &str,
        now: i64,
    ) -> Result<CaptureItem> {
        let id = id.trim();
        let content = content_ref.trim();
        if id.is_empty() || id.len() > 128 || content.is_empty() || content.len() > 256 * 1024 {
            return Err(Error::InvalidTransition);
        }
        let hash = digest(content.as_bytes());
        self.store.db.execute(
            "INSERT INTO captures(id,origin_device_id,source_message_id,content_ref,content_sha256,created_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, self.origin, source_message_id, content, hash, now],
        )?;
        Ok(CaptureItem {
            id: id.to_owned(),
            origin_device_id: self.origin.clone(),
            source_message_id: source_message_id.map(str::to_owned),
            content_ref: content.to_owned(),
            content_sha256: hash,
            created_at: now,
        })
    }

    pub fn list_captures(&self) -> Result<Vec<CaptureItem>> {
        let mut stmt = self.store.db.prepare(
            "SELECT id,origin_device_id,source_message_id,content_ref,content_sha256,created_at
             FROM captures ORDER BY created_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(CaptureItem {
                id: r.get(0)?,
                origin_device_id: r.get(1)?,
                source_message_id: r.get(2)?,
                content_ref: r.get(3)?,
                content_sha256: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn propose_memory(
        &mut self,
        id: &str,
        source_capture_id: Option<&str>,
        source_message_id: Option<&str>,
        body: &str,
        supersedes_id: Option<&str>,
    ) -> Result<MemoryItem> {
        let id = id.trim();
        let body = body.trim();
        if id.is_empty()
            || id.len() > 128
            || body.is_empty()
            || body.len() > 64 * 1024
            || (source_capture_id.is_none() && source_message_id.is_none())
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(prev_id) = supersedes_id {
            let changed = tx.execute(
                "UPDATE memories SET state='superseded', version=version+1 WHERE id=?1 AND state IN ('proposed','active')",
                [prev_id],
            )?;
            if changed != 1 {
                return Err(Error::InvalidTransition);
            }
        }
        tx.execute(
            "INSERT INTO memories(id,source_capture_id,source_message_id,body,state,supersedes_id,version)
             VALUES(?1,?2,?3,?4,'proposed',?5,1)",
            params![id, source_capture_id, source_message_id, body, supersedes_id],
        )?;
        tx.commit()?;
        Ok(MemoryItem {
            id: id.to_owned(),
            source_capture_id: source_capture_id.map(str::to_owned),
            source_message_id: source_message_id.map(str::to_owned),
            body: body.to_owned(),
            state: "proposed".to_owned(),
            supersedes_id: supersedes_id.map(str::to_owned),
            version: 1,
        })
    }

    pub fn update_memory_state(&mut self, id: &str, new_state: &str) -> Result<MemoryItem> {
        if !matches!(new_state, "active" | "superseded" | "forgotten") {
            return Err(Error::InvalidTransition);
        }
        let changed = self.store.db.execute(
            "UPDATE memories SET state=?2, version=version+1 WHERE id=?1",
            params![id, new_state],
        )?;
        if changed != 1 {
            return Err(Error::InvalidTransition);
        }
        self.store
            .db
            .query_row(
                "SELECT id,source_capture_id,source_message_id,body,state,supersedes_id,version
                 FROM memories WHERE id=?1",
                [id],
                |r| {
                    Ok(MemoryItem {
                        id: r.get(0)?,
                        source_capture_id: r.get(1)?,
                        source_message_id: r.get(2)?,
                        body: r.get(3)?,
                        state: r.get(4)?,
                        supersedes_id: r.get(5)?,
                        version: r.get(6)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn list_memories(&self, active_only: bool) -> Result<Vec<MemoryItem>> {
        let sql = if active_only {
            "SELECT id,source_capture_id,source_message_id,body,state,supersedes_id,version FROM memories WHERE state='active' ORDER BY rowid DESC"
        } else {
            "SELECT id,source_capture_id,source_message_id,body,state,supersedes_id,version FROM memories ORDER BY rowid DESC"
        };
        let mut stmt = self.store.db.prepare(sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(MemoryItem {
                id: r.get(0)?,
                source_capture_id: r.get(1)?,
                source_message_id: r.get(2)?,
                body: r.get(3)?,
                state: r.get(4)?,
                supersedes_id: r.get(5)?,
                version: r.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn upsert_note(
        &mut self,
        id: &str,
        project_id: Option<&str>,
        conversation_id: Option<&str>,
        title: &str,
        body_markdown: &str,
        tags: &[String],
        now: i64,
    ) -> Result<NoteItem> {
        let id = id.trim();
        let title = title.trim();
        if id.is_empty()
            || id.len() > 128
            || title.is_empty()
            || title.len() > 256
            || body_markdown.len() > 512 * 1024
        {
            return Err(Error::InvalidTransition);
        }
        let acceptance = if self.paired_home()?.is_some() {
            "local"
        } else {
            "home"
        };
        let tags_json = serde_json::to_string(tags)?;
        self.store.db.execute(
            "INSERT INTO notes(id,project_id,conversation_id,origin_device_id,title,body_markdown,tags_json,acceptance,version,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?9)
             ON CONFLICT(id) DO UPDATE SET
               project_id=excluded.project_id,
               conversation_id=excluded.conversation_id,
               title=excluded.title,
               body_markdown=excluded.body_markdown,
               tags_json=excluded.tags_json,
               version=notes.version+1,
               updated_at=excluded.updated_at",
            params![
                id,
                project_id,
                conversation_id,
                self.origin,
                title,
                body_markdown,
                tags_json,
                acceptance,
                now
            ],
        )?;
        self.store
            .db
            .query_row(
                "SELECT id,project_id,conversation_id,origin_device_id,title,body_markdown,tags_json,acceptance,version,created_at,updated_at
                 FROM notes WHERE id=?1",
                [id],
                |r| {
                    let raw_tags: String = r.get(6)?;
                    let parsed_tags: Vec<String> =
                        serde_json::from_str(&raw_tags).unwrap_or_default();
                    Ok(NoteItem {
                        id: r.get(0)?,
                        project_id: r.get(1)?,
                        conversation_id: r.get(2)?,
                        origin_device_id: r.get(3)?,
                        title: r.get(4)?,
                        body_markdown: r.get(5)?,
                        tags: parsed_tags,
                        acceptance: r.get(7)?,
                        version: r.get(8)?,
                        created_at: r.get(9)?,
                        updated_at: r.get(10)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn list_notes(&self, project_id: Option<&str>) -> Result<Vec<NoteItem>> {
        let mut stmt = self.store.db.prepare(
            "SELECT id,project_id,conversation_id,origin_device_id,title,body_markdown,tags_json,acceptance,version,created_at,updated_at
             FROM notes
             WHERE (?1 IS NULL OR project_id = ?1)
             ORDER BY updated_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map([project_id], |r| {
            let raw_tags: String = r.get(6)?;
            let parsed_tags: Vec<String> = serde_json::from_str(&raw_tags).unwrap_or_default();
            Ok(NoteItem {
                id: r.get(0)?,
                project_id: r.get(1)?,
                conversation_id: r.get(2)?,
                origin_device_id: r.get(3)?,
                title: r.get(4)?,
                body_markdown: r.get(5)?,
                tags: parsed_tags,
                acceptance: r.get(7)?,
                version: r.get(8)?,
                created_at: r.get(9)?,
                updated_at: r.get(10)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_schedule_item(
        &mut self,
        id: &str,
        project_id: Option<&str>,
        task_id: Option<&str>,
        title: &str,
        start_at: i64,
        end_at: Option<i64>,
        reminder_at: Option<i64>,
        notes: &str,
        now: i64,
    ) -> Result<ScheduleItem> {
        let id = id.trim();
        let title = title.trim();
        if id.is_empty()
            || id.len() > 128
            || title.is_empty()
            || title.len() > 256
            || end_at.is_some_and(|e| e < start_at)
        {
            return Err(Error::InvalidTransition);
        }
        let acceptance = if self.paired_home()?.is_some() {
            "local"
        } else {
            "home"
        };
        self.store.db.execute(
            "INSERT INTO schedule_items(id,project_id,task_id,origin_device_id,title,start_at,end_at,reminder_at,status,notes,acceptance,version,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'scheduled',?9,?10,1,?11,?11)",
            params![
                id,
                project_id,
                task_id,
                self.origin,
                title,
                start_at,
                end_at,
                reminder_at,
                notes.trim(),
                acceptance,
                now
            ],
        )?;
        Ok(ScheduleItem {
            id: id.to_owned(),
            project_id: project_id.map(str::to_owned),
            task_id: task_id.map(str::to_owned),
            origin_device_id: self.origin.clone(),
            title: title.to_owned(),
            start_at,
            end_at,
            reminder_at,
            status: "scheduled".to_owned(),
            notes: notes.trim().to_owned(),
            acceptance: acceptance.to_owned(),
            version: 1,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update_schedule_status(
        &mut self,
        id: &str,
        status: &str,
        now: i64,
    ) -> Result<ScheduleItem> {
        if !matches!(status, "scheduled" | "completed" | "cancelled") {
            return Err(Error::InvalidTransition);
        }
        let changed = self.store.db.execute(
            "UPDATE schedule_items SET status=?2, version=version+1, updated_at=?3 WHERE id=?1",
            params![id, status, now],
        )?;
        if changed != 1 {
            return Err(Error::InvalidTransition);
        }
        self.store
            .db
            .query_row(
                "SELECT id,project_id,task_id,origin_device_id,title,start_at,end_at,reminder_at,status,notes,acceptance,version,created_at,updated_at
                 FROM schedule_items WHERE id=?1",
                [id],
                |r| {
                    Ok(ScheduleItem {
                        id: r.get(0)?,
                        project_id: r.get(1)?,
                        task_id: r.get(2)?,
                        origin_device_id: r.get(3)?,
                        title: r.get(4)?,
                        start_at: r.get(5)?,
                        end_at: r.get(6)?,
                        reminder_at: r.get(7)?,
                        status: r.get(8)?,
                        notes: r.get(9)?,
                        acceptance: r.get(10)?,
                        version: r.get(11)?,
                        created_at: r.get(12)?,
                        updated_at: r.get(13)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn list_schedule_items(&self) -> Result<Vec<ScheduleItem>> {
        let mut stmt = self.store.db.prepare(
            "SELECT id,project_id,task_id,origin_device_id,title,start_at,end_at,reminder_at,status,notes,acceptance,version,created_at,updated_at
             FROM schedule_items ORDER BY start_at ASC, rowid ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ScheduleItem {
                id: r.get(0)?,
                project_id: r.get(1)?,
                task_id: r.get(2)?,
                origin_device_id: r.get(3)?,
                title: r.get(4)?,
                start_at: r.get(5)?,
                end_at: r.get(6)?,
                reminder_at: r.get(7)?,
                status: r.get(8)?,
                notes: r.get(9)?,
                acceptance: r.get(10)?,
                version: r.get(11)?,
                created_at: r.get(12)?,
                updated_at: r.get(13)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_decision(
        &mut self,
        id: &str,
        project_id: &str,
        task_id: Option<&str>,
        source_message_id: &str,
        body: &str,
        rationale_json: &str,
        status: &str,
        supersedes_id: Option<&str>,
    ) -> Result<DecisionItem> {
        let id = id.trim();
        let body = body.trim();
        if id.is_empty()
            || body.is_empty()
            || !matches!(status, "proposed" | "accepted" | "superseded" | "rejected")
        {
            return Err(Error::InvalidTransition);
        }
        let _: serde_json::Value = serde_json::from_str(rationale_json)?;
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(prev_id) = supersedes_id {
            let exists: Option<i64> = tx
                .query_row(
                    "SELECT version FROM decisions WHERE id=?1 AND project_id=?2",
                    params![prev_id, project_id],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(Error::InvalidTransition);
            }
            tx.execute(
                "UPDATE decisions SET status='superseded', version=version+1 WHERE id=?1",
                [prev_id],
            )?;
        }
        tx.execute(
            "INSERT INTO decisions(id,project_id,task_id,source_message_id,body,rationale_json,status,supersedes_id,version)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1)",
            params![
                id,
                project_id,
                task_id,
                source_message_id,
                body,
                rationale_json,
                status,
                supersedes_id
            ],
        )?;
        tx.commit()?;
        Ok(DecisionItem {
            id: id.to_owned(),
            project_id: project_id.to_owned(),
            task_id: task_id.map(str::to_owned),
            source_message_id: source_message_id.to_owned(),
            body: body.to_owned(),
            rationale_json: rationale_json.to_owned(),
            status: status.to_owned(),
            supersedes_id: supersedes_id.map(str::to_owned),
            version: 1,
        })
    }

    pub fn list_decisions(&self, project_id: &str) -> Result<Vec<DecisionItem>> {
        let mut stmt = self.store.db.prepare(
            "SELECT id,project_id,task_id,source_message_id,body,rationale_json,status,supersedes_id,version
             FROM decisions WHERE project_id=?1 ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map([project_id], |r| {
            Ok(DecisionItem {
                id: r.get(0)?,
                project_id: r.get(1)?,
                task_id: r.get(2)?,
                source_message_id: r.get(3)?,
                body: r.get(4)?,
                rationale_json: r.get(5)?,
                status: r.get(6)?,
                supersedes_id: r.get(7)?,
                version: r.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}
