//! F3 Personal Assistant domain: Captures, Memories (with provenance and
//! supersede/forget lifecycle), Markdown Notes, Schedule/Calendar items (with
//! `kind`: `'event' | 'todo'`), Decision records, and Replica → Home personal
//! data synchronization (`personal_ops_sync_outbox`).
use crate::{
    app::LocalApp,
    outbox::{digest, Error, Result, Store},
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const MIGRATION_0007: &str = include_str!("../migrations/0007.sql");

pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    let hash = digest(MIGRATION_0007.replace("\r\n", "\n").as_bytes());
    let previous: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=7",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match previous {
        Some(v) if v != hash => return Err(Error::SchemaMismatch),
        Some(_) => {}
        None => {
            tx.execute_batch(MIGRATION_0007)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(7,?1,?2)",
                params![hash, now],
            )?;
        }
    }
    Ok(())
}

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
    pub kind: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalOpSyncEntry {
    pub sync_id: String,
    pub entity_kind: String,
    pub entity_id: String,
    pub op_kind: String,
    pub origin_device_id: String,
    pub home_device_id: String,
    pub home_epoch: i64,
    pub expected_version: i64,
    pub payload_json: String,
    pub payload_sha256: String,
    pub sync_state: String,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub synced_at: Option<i64>,
}

impl LocalApp {
    fn enqueue_personal_sync(
        &self,
        entity_kind: &str,
        entity_id: &str,
        op_kind: &str,
        expected_version: i64,
        payload_json: &str,
        now: i64,
    ) -> Result<()> {
        let Some((home_id, home_epoch, _)) = self.paired_home()? else {
            return Ok(());
        };
        let sha = digest(payload_json.as_bytes());
        let sync_id = format!("psync-{entity_kind}-{entity_id}-{expected_version}-{}", &sha[..8]);
        self.store.db.execute(
            "INSERT OR REPLACE INTO personal_ops_sync_outbox(
               sync_id,entity_kind,entity_id,op_kind,origin_device_id,home_device_id,home_epoch,
               expected_version,payload_json,payload_sha256,sync_state,last_error,created_at,synced_at
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending',NULL,?11,NULL)",
            params![
                sync_id,
                entity_kind,
                entity_id,
                op_kind,
                self.origin,
                home_id,
                home_epoch,
                expected_version,
                payload_json,
                sha,
                now
            ],
        )?;
        Ok(())
    }

    pub fn pending_personal_op_syncs(&self) -> Result<Vec<PersonalOpSyncEntry>> {
        let mut stmt = self.store.db.prepare(
            "SELECT sync_id,entity_kind,entity_id,op_kind,origin_device_id,home_device_id,home_epoch,
                    expected_version,payload_json,payload_sha256,sync_state,last_error,created_at,synced_at
             FROM personal_ops_sync_outbox
             WHERE sync_state='pending'
             ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PersonalOpSyncEntry {
                sync_id: r.get(0)?,
                entity_kind: r.get(1)?,
                entity_id: r.get(2)?,
                op_kind: r.get(3)?,
                origin_device_id: r.get(4)?,
                home_device_id: r.get(5)?,
                home_epoch: r.get(6)?,
                expected_version: r.get(7)?,
                payload_json: r.get(8)?,
                payload_sha256: r.get(9)?,
                sync_state: r.get(10)?,
                last_error: r.get(11)?,
                created_at: r.get(12)?,
                synced_at: r.get(13)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn ack_replica_personal_op(&mut self, sync_id: &str, now: i64) -> Result<()> {
        let row: Option<(String, String)> = self
            .store
            .db
            .query_row(
                "SELECT entity_kind, entity_id FROM personal_ops_sync_outbox WHERE sync_id=?1",
                [sync_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((entity_kind, entity_id)) = row else {
            return Err(Error::InvalidTransition);
        };
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE personal_ops_sync_outbox SET sync_state='synced', synced_at=?2, last_error=NULL WHERE sync_id=?1",
            params![sync_id, now],
        )?;
        match entity_kind.as_str() {
            "note" => {
                tx.execute(
                    "UPDATE notes SET acceptance='home' WHERE id=?1",
                    [&entity_id],
                )?;
            }
            "schedule" => {
                tx.execute(
                    "UPDATE schedule_items SET acceptance='home' WHERE id=?1",
                    [&entity_id],
                )?;
            }
            _ => {}
        }
        tx.commit()?;
        Ok(())
    }

    pub fn host_receive_replica_personal_op(
        &mut self,
        ctx: & crate::home_auth::HomeMessageContext,
        entry: &PersonalOpSyncEntry,
        now: i64,
    ) -> Result<crate::outbox::HomeReceipt> {
        self.store.receive_replica_personal_op_with_receipt(ctx, entry, now)
    }

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
        let item = CaptureItem {
            id: id.to_owned(),
            origin_device_id: self.origin.clone(),
            source_message_id: source_message_id.map(str::to_owned),
            content_ref: content.to_owned(),
            content_sha256: hash,
            created_at: now,
        };
        let payload = serde_json::to_string(&item)?;
        self.enqueue_personal_sync("capture", &item.id, "upsert", 0, &payload, now)?;
        Ok(item)
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
        let item = MemoryItem {
            id: id.to_owned(),
            source_capture_id: source_capture_id.map(str::to_owned),
            source_message_id: source_message_id.map(str::to_owned),
            body: body.to_owned(),
            state: "proposed".to_owned(),
            supersedes_id: supersedes_id.map(str::to_owned),
            version: 1,
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let payload = serde_json::to_string(&item)?;
        self.enqueue_personal_sync("memory", &item.id, "upsert", 0, &payload, now)?;
        Ok(item)
    }

    pub fn update_memory_state(&mut self, id: &str, new_state: &str) -> Result<MemoryItem> {
        if !matches!(new_state, "active" | "superseded" | "forgotten") {
            return Err(Error::InvalidTransition);
        }
        let prev_ver: Option<i64> = self
            .store
            .db
            .query_row(
                "SELECT version FROM memories WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(expected_version) = prev_ver else {
            return Err(Error::InvalidTransition);
        };
        let changed = self.store.db.execute(
            "UPDATE memories SET state=?2, version=version+1 WHERE id=?1",
            params![id, new_state],
        )?;
        if changed != 1 {
            return Err(Error::InvalidTransition);
        }
        let item = self.store.db.query_row(
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
        )?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let payload = serde_json::to_string(&item)?;
        self.enqueue_personal_sync(
            "memory",
            &item.id,
            "status_change",
            expected_version,
            &payload,
            now,
        )?;
        Ok(item)
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

    #[allow(clippy::too_many_arguments)]
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
        let existing_version: i64 = self
            .store
            .db
            .query_row("SELECT version FROM notes WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .unwrap_or(0);
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
               acceptance=excluded.acceptance,
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
        let item = self.store.db.query_row(
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
        )?;
        let payload = serde_json::to_string(&item)?;
        self.enqueue_personal_sync("note", &item.id, "upsert", existing_version, &payload, now)?;
        Ok(item)
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
        self.create_schedule_item_with_kind(
            id,
            project_id,
            task_id,
            title,
            "event",
            start_at,
            end_at,
            reminder_at,
            notes,
            now,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_schedule_item_with_kind(
        &mut self,
        id: &str,
        project_id: Option<&str>,
        task_id: Option<&str>,
        title: &str,
        kind: &str,
        start_at: i64,
        end_at: Option<i64>,
        reminder_at: Option<i64>,
        notes: &str,
        now: i64,
    ) -> Result<ScheduleItem> {
        let id = id.trim();
        let title = title.trim();
        let kind = kind.trim();
        if id.is_empty()
            || id.len() > 128
            || title.is_empty()
            || title.len() > 256
            || !matches!(kind, "event" | "todo")
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
            "INSERT INTO schedule_items(id,project_id,task_id,origin_device_id,title,kind,start_at,end_at,reminder_at,status,notes,acceptance,version,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'scheduled',?10,?11,1,?12,?12)",
            params![
                id,
                project_id,
                task_id,
                self.origin,
                title,
                kind,
                start_at,
                end_at,
                reminder_at,
                notes.trim(),
                acceptance,
                now
            ],
        )?;
        let item = ScheduleItem {
            id: id.to_owned(),
            project_id: project_id.map(str::to_owned),
            task_id: task_id.map(str::to_owned),
            origin_device_id: self.origin.clone(),
            title: title.to_owned(),
            kind: kind.to_owned(),
            start_at,
            end_at,
            reminder_at,
            status: "scheduled".to_owned(),
            notes: notes.trim().to_owned(),
            acceptance: acceptance.to_owned(),
            version: 1,
            created_at: now,
            updated_at: now,
        };
        let payload = serde_json::to_string(&item)?;
        self.enqueue_personal_sync("schedule", &item.id, "upsert", 0, &payload, now)?;
        Ok(item)
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
        let prev_ver: Option<i64> = self
            .store
            .db
            .query_row(
                "SELECT version FROM schedule_items WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(expected_version) = prev_ver else {
            return Err(Error::InvalidTransition);
        };
        let acceptance = if self.paired_home()?.is_some() {
            "local"
        } else {
            "home"
        };
        let changed = self.store.db.execute(
            "UPDATE schedule_items SET status=?2, acceptance=?3, version=version+1, updated_at=?4 WHERE id=?1",
            params![id, status, acceptance, now],
        )?;
        if changed != 1 {
            return Err(Error::InvalidTransition);
        }
        let item = self.store.db.query_row(
            "SELECT id,project_id,task_id,origin_device_id,title,kind,start_at,end_at,reminder_at,status,notes,acceptance,version,created_at,updated_at
             FROM schedule_items WHERE id=?1",
            [id],
            |r| {
                Ok(ScheduleItem {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    task_id: r.get(2)?,
                    origin_device_id: r.get(3)?,
                    title: r.get(4)?,
                    kind: r.get(5)?,
                    start_at: r.get(6)?,
                    end_at: r.get(7)?,
                    reminder_at: r.get(8)?,
                    status: r.get(9)?,
                    notes: r.get(10)?,
                    acceptance: r.get(11)?,
                    version: r.get(12)?,
                    created_at: r.get(13)?,
                    updated_at: r.get(14)?,
                })
            },
        )?;
        let payload = serde_json::to_string(&item)?;
        self.enqueue_personal_sync(
            "schedule",
            &item.id,
            "status_change",
            expected_version,
            &payload,
            now,
        )?;
        Ok(item)
    }

    pub fn list_schedule_items(&self) -> Result<Vec<ScheduleItem>> {
        let mut stmt = self.store.db.prepare(
            "SELECT id,project_id,task_id,origin_device_id,title,kind,start_at,end_at,reminder_at,status,notes,acceptance,version,created_at,updated_at
             FROM schedule_items ORDER BY start_at ASC, rowid ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ScheduleItem {
                id: r.get(0)?,
                project_id: r.get(1)?,
                task_id: r.get(2)?,
                origin_device_id: r.get(3)?,
                title: r.get(4)?,
                kind: r.get(5)?,
                start_at: r.get(6)?,
                end_at: r.get(7)?,
                reminder_at: r.get(8)?,
                status: r.get(9)?,
                notes: r.get(10)?,
                acceptance: r.get(11)?,
                version: r.get(12)?,
                created_at: r.get(13)?,
                updated_at: r.get(14)?,
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
        let item = DecisionItem {
            id: id.to_owned(),
            project_id: project_id.to_owned(),
            task_id: task_id.map(str::to_owned),
            source_message_id: source_message_id.to_owned(),
            body: body.to_owned(),
            rationale_json: rationale_json.to_owned(),
            status: status.to_owned(),
            supersedes_id: supersedes_id.map(str::to_owned),
            version: 1,
        };
        let payload = serde_json::to_string(&item)?;
        self.enqueue_personal_sync("decision", &item.id, "upsert", 0, &payload, 0)?;
        Ok(item)
    }

    pub fn update_decision_status(&mut self, id: &str, status: &str) -> Result<DecisionItem> {
        if !matches!(status, "proposed" | "accepted" | "superseded" | "rejected") {
            return Err(Error::InvalidTransition);
        }
        let changed = self.store.db.execute(
            "UPDATE decisions SET status=?2, version=version+1 WHERE id=?1",
            params![id, status],
        )?;
        if changed != 1 {
            return Err(Error::InvalidTransition);
        }
        self.store
            .db
            .query_row(
                "SELECT id,project_id,task_id,source_message_id,body,rationale_json,status,supersedes_id,version
                 FROM decisions WHERE id=?1",
                [id],
                |r| {
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
                },
            )
            .map_err(Into::into)
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

    pub fn list_all_decisions(&self) -> Result<Vec<DecisionItem>> {
        let mut stmt = self.store.db.prepare(
            "SELECT id,project_id,task_id,source_message_id,body,rationale_json,status,supersedes_id,version
             FROM decisions ORDER BY rowid DESC",
        )?;
        let rows = stmt.query_map([], |r| {
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

impl Store {
    /// Home-side idempotent convergence for Replica personal operations (`RV-13`).
    /// - New append items (`expected_version == 0`): idempotent if same ID + same content digest,
    ///   rejected on content conflict.
    /// - Updates (`expected_version > 0`): applied only when Home's current `version == expected_version`,
    ///   otherwise rejected as stale version conflict.
    pub fn receive_replica_personal_op(
        &mut self,
        entry: &PersonalOpSyncEntry,
        now: i64,
    ) -> Result<()> {
        let computed_sha = digest(entry.payload_json.as_bytes());
        if computed_sha != entry.payload_sha256 {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO devices(id,label,trust_state,created_at) VALUES(?1,?1,'paired',?2)",
            params![entry.origin_device_id, now],
        )?;
        match entry.entity_kind.as_str() {
            "capture" => {
                let cap: CaptureItem = serde_json::from_str(&entry.payload_json)?;
                let existing: Option<String> = tx
                    .query_row(
                        "SELECT content_sha256 FROM captures WHERE id=?1",
                        [&cap.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                match existing {
                    Some(existing_sha) if existing_sha == cap.content_sha256 => {}
                    Some(_) => return Err(Error::InvalidTransition),
                    None => {
                        tx.execute(
                            "INSERT INTO captures(id,origin_device_id,source_message_id,content_ref,content_sha256,created_at)
                             VALUES(?1,?2,?3,?4,?5,?6)",
                            params![
                                cap.id,
                                cap.origin_device_id,
                                cap.source_message_id,
                                cap.content_ref,
                                cap.content_sha256,
                                cap.created_at
                            ],
                        )?;
                    }
                }
            }
            "note" => {
                let note: NoteItem = serde_json::from_str(&entry.payload_json)?;
                let existing: Option<(i64, String, String)> = tx
                    .query_row(
                        "SELECT version, title, body_markdown FROM notes WHERE id=?1",
                        [&note.id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                let tags_json = serde_json::to_string(&note.tags)?;
                match existing {
                    None => {
                        if entry.expected_version != 0 {
                            return Err(Error::InvalidTransition);
                        }
                        tx.execute(
                            "INSERT INTO notes(id,project_id,conversation_id,origin_device_id,title,body_markdown,tags_json,acceptance,version,created_at,updated_at)
                             VALUES(?1,?2,?3,?4,?5,?6,?7,'home',?8,?9,?10)",
                            params![
                                note.id,
                                note.project_id,
                                note.conversation_id,
                                note.origin_device_id,
                                note.title,
                                note.body_markdown,
                                tags_json,
                                note.version,
                                note.created_at,
                                note.updated_at
                            ],
                        )?;
                    }
                    Some((cur_ver, cur_title, cur_body)) => {
                        if entry.expected_version == 0
                            && cur_ver == note.version
                            && cur_title == note.title
                            && cur_body == note.body_markdown
                        {
                            // Idempotent retry of initial creation
                        } else if cur_ver != entry.expected_version {
                            return Err(Error::InvalidTransition);
                        } else {
                            tx.execute(
                                "UPDATE notes SET title=?2, body_markdown=?3, tags_json=?4, acceptance='home', version=version+1, updated_at=?5 WHERE id=?1",
                                params![note.id, note.title, note.body_markdown, tags_json, now],
                            )?;
                        }
                    }
                }
            }
            "schedule" => {
                let sched: ScheduleItem = serde_json::from_str(&entry.payload_json)?;
                let existing: Option<(i64, String, String)> = tx
                    .query_row(
                        "SELECT version, title, status FROM schedule_items WHERE id=?1",
                        [&sched.id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                match existing {
                    None => {
                        if entry.expected_version != 0 {
                            return Err(Error::InvalidTransition);
                        }
                        tx.execute(
                            "INSERT INTO schedule_items(id,project_id,task_id,origin_device_id,title,kind,start_at,end_at,reminder_at,status,notes,acceptance,version,created_at,updated_at)
                             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'home',?12,?13,?14)",
                            params![
                                sched.id,
                                sched.project_id,
                                sched.task_id,
                                sched.origin_device_id,
                                sched.title,
                                sched.kind,
                                sched.start_at,
                                sched.end_at,
                                sched.reminder_at,
                                sched.status,
                                sched.notes,
                                sched.version,
                                sched.created_at,
                                sched.updated_at
                            ],
                        )?;
                    }
                    Some((cur_ver, cur_title, cur_status)) => {
                        if cur_ver == sched.version
                            && cur_title == sched.title
                            && cur_status == sched.status
                        {
                            // Idempotent retry
                        } else if cur_ver != entry.expected_version {
                            return Err(Error::InvalidTransition);
                        } else {
                            tx.execute(
                                "UPDATE schedule_items SET status=?2, notes=?3, acceptance='home', version=version+1, updated_at=?4 WHERE id=?1",
                                params![sched.id, sched.status, sched.notes, now],
                            )?;
                        }
                    }
                }
            }
            "memory" => {
                let mem: MemoryItem = serde_json::from_str(&entry.payload_json)?;
                let existing: Option<(i64, String, String)> = tx
                    .query_row(
                        "SELECT version, body, state FROM memories WHERE id=?1",
                        [&mem.id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                match existing {
                    None => {
                        if entry.expected_version != 0 {
                            return Err(Error::InvalidTransition);
                        }
                        tx.execute(
                            "INSERT INTO memories(id,source_capture_id,source_message_id,body,state,supersedes_id,version)
                             VALUES(?1,?2,?3,?4,?5,?6,?7)",
                            params![
                                mem.id,
                                mem.source_capture_id,
                                mem.source_message_id,
                                mem.body,
                                mem.state,
                                mem.supersedes_id,
                                mem.version
                            ],
                        )?;
                    }
                    Some((cur_ver, cur_body, cur_state)) => {
                        if cur_ver == mem.version && cur_body == mem.body && cur_state == mem.state
                        {
                            // Idempotent retry
                        } else if cur_ver != entry.expected_version || cur_body != mem.body {
                            return Err(Error::InvalidTransition);
                        } else {
                            tx.execute(
                                "UPDATE memories SET state=?2, version=version+1 WHERE id=?1",
                                params![mem.id, mem.state],
                            )?;
                        }
                    }
                }
            }
            _ => return Err(Error::InvalidTransition),
        }
        tx.commit()?;
        Ok(())
    }

    pub fn receive_replica_personal_op_with_receipt(
        &mut self,
        ctx: &crate::home_auth::HomeMessageContext,
        entry: &PersonalOpSyncEntry,
        now: i64,
    ) -> Result<crate::outbox::HomeReceipt> {
        if ctx.origin_device_id() != entry.origin_device_id {
            return Err(Error::InvalidTransition);
        }
        self.receive_replica_personal_op(entry, now)?;
        Ok(crate::outbox::HomeReceipt {
            request_id: entry.sync_id.clone(),
            home_device_id: entry.home_device_id.clone(),
            home_epoch: entry.home_epoch,
            payload_sha256: entry.payload_sha256.clone(),
            accepted: true,
        })
    }
}
