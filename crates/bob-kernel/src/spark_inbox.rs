//! BOB-D-014 One Inbox, sorted once a day: the unified spark inbox contract.
//!
//! A spark is an existing `captures` row (immutable content). Its triage state lives in
//! `capture_triage`: no row means `unsorted`. Sorting never deletes the capture; a sorted
//! spark keeps a destination link (`project_todo` | `calendar` | `note` | `merged`).
//! Triage changes are versioned, idempotent on retry and replicated to Home through the
//! existing personal-ops outbox (`entity_kind = "capture_triage"`).
use crate::{
    app::LocalApp,
    outbox::{digest, Error, Result},
    personal_ops::CaptureItem,
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

const SCHEMA: &str = include_str!("../migrations/0011.sql");
const SCHEMA_0012: &str = include_str!("../migrations/0012.sql");

pub const TRIAGE_UNSORTED: &str = "unsorted";
pub const TRIAGE_SORTED: &str = "sorted";
pub const DESTINATION_KINDS: [&str; 4] = ["project_todo", "calendar", "note", "merged"];

const MAX_REF_LEN: usize = 512;
const MAX_LABEL_LEN: usize = 256;
const MAX_BATCH_LEN: usize = 128;
const MAX_BATCH_PAYLOAD: usize = 512 * 1024;

fn apply_versioned(tx: &Transaction<'_>, version: i64, sql: &str, now: i64) -> Result<()> {
    let hash = digest(sql.replace("\r\n", "\n").as_bytes());
    let old: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=?1",
            [version],
            |r| r.get(0),
        )
        .optional()?;
    match old {
        Some(v) if v != hash => return Err(Error::SchemaMismatch),
        Some(_) => (),
        None => {
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(?1,?2,?3)",
                params![version, hash, now],
            )?;
        }
    }
    Ok(())
}

pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    apply_versioned(tx, 11, SCHEMA, now)?;
    apply_versioned(tx, 12, SCHEMA_0012, now)
}

/// A persisted Daily Review draft (BOB-D-014 Step 2). `payload_json` is produced and
/// validated by the runtime; the kernel only guarantees identity, JSON shape and lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriageBatch {
    pub id: String,
    pub status: String,
    pub payload_json: String,
    pub model_ref: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn read_batch(db: &rusqlite::Connection, sql_where: &str, arg: &str) -> Result<Option<TriageBatch>> {
    db.query_row(
        &format!(
            "SELECT id,status,payload_json,model_ref,created_at,updated_at FROM triage_batches {sql_where}"
        ),
        [arg],
        |r| {
            Ok(TriageBatch {
                id: r.get(0)?,
                status: r.get(1)?,
                payload_json: r.get(2)?,
                model_ref: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

/// Where a sorted spark went. `ref_id` points at the created object (schedule item id,
/// note id, pending todo write id, or the surviving spark for `merged`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriageDestination {
    pub kind: String,
    pub ref_id: Option<String>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureTriage {
    pub capture_id: String,
    pub triage_status: String,
    pub destination_kind: Option<String>,
    pub destination_ref_id: Option<String>,
    pub destination_label: Option<String>,
    pub triage_batch_id: Option<String>,
    pub version: i64,
    pub updated_at: i64,
}

impl CaptureTriage {
    fn same_state(&self, other: &CaptureTriage) -> bool {
        self.triage_status == other.triage_status
            && self.destination_kind == other.destination_kind
            && self.destination_ref_id == other.destination_ref_id
            && self.destination_label == other.destination_label
            && self.triage_batch_id == other.triage_batch_id
    }

    /// Shape and vocabulary check shared by local writes, Home receive and bundle merge.
    pub(crate) fn validate(&self) -> Result<()> {
        let bounded = |v: &Option<String>, max: usize| {
            v.as_ref()
                .map_or(true, |s| !s.trim().is_empty() && s.len() <= max && !s.contains('\0'))
        };
        let status_ok = match self.triage_status.as_str() {
            TRIAGE_SORTED => self
                .destination_kind
                .as_deref()
                .is_some_and(|k| DESTINATION_KINDS.contains(&k)),
            TRIAGE_UNSORTED => {
                self.destination_kind.is_none()
                    && self.destination_ref_id.is_none()
                    && self.destination_label.is_none()
            }
            _ => false,
        };
        if !status_ok
            || self.capture_id.trim().is_empty()
            || self.capture_id.len() > 128
            || self.version < 1
            || !bounded(&self.destination_ref_id, MAX_REF_LEN)
            || !bounded(&self.destination_label, MAX_LABEL_LEN)
            || !bounded(&self.triage_batch_id, MAX_BATCH_LEN)
        {
            return Err(Error::InvalidTransition);
        }
        Ok(())
    }
}

/// One inbox row: the immutable capture plus its triage state.
/// `triage_version == 0` means the spark was never triaged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InboxItem {
    #[serde(flatten)]
    pub capture: CaptureItem,
    pub triage_status: String,
    pub destination_kind: Option<String>,
    pub destination_ref_id: Option<String>,
    pub destination_label: Option<String>,
    pub triage_batch_id: Option<String>,
    pub triage_version: i64,
}

pub(crate) fn read_triage(db: &rusqlite::Connection, capture_id: &str) -> Result<Option<CaptureTriage>> {
    db.query_row(
        "SELECT capture_id,triage_status,destination_kind,destination_ref_id,destination_label,
                triage_batch_id,version,updated_at
         FROM capture_triage WHERE capture_id=?1",
        [capture_id],
        |r| {
            Ok(CaptureTriage {
                capture_id: r.get(0)?,
                triage_status: r.get(1)?,
                destination_kind: r.get(2)?,
                destination_ref_id: r.get(3)?,
                destination_label: r.get(4)?,
                triage_batch_id: r.get(5)?,
                version: r.get(6)?,
                updated_at: r.get(7)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub(crate) fn list_all_triage(db: &rusqlite::Connection) -> Result<Vec<CaptureTriage>> {
    let mut stmt = db.prepare(
        "SELECT capture_id,triage_status,destination_kind,destination_ref_id,destination_label,
                triage_batch_id,version,updated_at
         FROM capture_triage ORDER BY capture_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(CaptureTriage {
            capture_id: r.get(0)?,
            triage_status: r.get(1)?,
            destination_kind: r.get(2)?,
            destination_ref_id: r.get(3)?,
            destination_label: r.get(4)?,
            triage_batch_id: r.get(5)?,
            version: r.get(6)?,
            updated_at: r.get(7)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn write_triage(tx: &Transaction<'_>, t: &CaptureTriage) -> Result<()> {
    tx.execute(
        "INSERT INTO capture_triage(capture_id,triage_status,destination_kind,destination_ref_id,
                                    destination_label,triage_batch_id,version,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(capture_id) DO UPDATE SET
           triage_status=excluded.triage_status, destination_kind=excluded.destination_kind,
           destination_ref_id=excluded.destination_ref_id, destination_label=excluded.destination_label,
           triage_batch_id=excluded.triage_batch_id, version=excluded.version, updated_at=excluded.updated_at",
        params![
            t.capture_id,
            t.triage_status,
            t.destination_kind,
            t.destination_ref_id,
            t.destination_label,
            t.triage_batch_id,
            t.version,
            t.updated_at
        ],
    )?;
    Ok(())
}

fn capture_exists(tx: &Transaction<'_>, id: &str) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM captures WHERE id=?1)",
        [id],
        |r| r.get(0),
    )?)
}

/// Home side of a Replica `capture_triage` op. Same rules as a local write: an identical
/// state is an idempotent retry; otherwise `expected_version` must equal Home's version.
pub(crate) fn receive_replica_triage(
    tx: &Transaction<'_>,
    incoming: &CaptureTriage,
    expected_version: i64,
    now: i64,
) -> Result<()> {
    incoming.validate()?;
    if !capture_exists(tx, &incoming.capture_id)? {
        return Err(Error::InvalidTransition);
    }
    let current = read_triage(tx, &incoming.capture_id)?;
    if current.as_ref().is_some_and(|c| c.same_state(incoming)) {
        return Ok(());
    }
    let current_version = current.as_ref().map_or(0, |c| c.version);
    if expected_version != current_version {
        return Err(Error::InvalidTransition);
    }
    let applied = CaptureTriage {
        version: current_version + 1,
        updated_at: now,
        ..incoming.clone()
    };
    write_triage(tx, &applied)
}

/// Snapshot merge: a strictly newer version wins; equal versions keep local state.
/// Rows for captures not (yet) present locally are skipped, never invented.
pub(crate) fn merge_triage(tx: &Transaction<'_>, incoming: &[CaptureTriage]) -> Result<()> {
    for t in incoming {
        t.validate()?;
        if !capture_exists(tx, &t.capture_id)? {
            continue;
        }
        let local_version = read_triage(tx, &t.capture_id)?.map_or(0, |c| c.version);
        if t.version > local_version {
            write_triage(tx, t)?;
        }
    }
    Ok(())
}

impl LocalApp {
    /// Inbox listing, newest first. `status` filters to `unsorted` or `sorted`.
    pub fn list_inbox(&self, status: Option<&str>) -> Result<Vec<InboxItem>> {
        if let Some(s) = status {
            if s != TRIAGE_UNSORTED && s != TRIAGE_SORTED {
                return Err(Error::InvalidTransition);
            }
        }
        let mut stmt = self.store.db.prepare(
            "SELECT c.id,c.origin_device_id,c.source_message_id,c.content_ref,c.content_sha256,c.created_at,
                    COALESCE(t.triage_status,'unsorted'),t.destination_kind,t.destination_ref_id,
                    t.destination_label,t.triage_batch_id,COALESCE(t.version,0)
             FROM captures c LEFT JOIN capture_triage t ON t.capture_id=c.id
             WHERE ?1 IS NULL OR COALESCE(t.triage_status,'unsorted')=?1
             ORDER BY c.created_at DESC, c.rowid DESC",
        )?;
        let rows = stmt.query_map([status], |r| {
            Ok(InboxItem {
                capture: CaptureItem {
                    id: r.get(0)?,
                    origin_device_id: r.get(1)?,
                    source_message_id: r.get(2)?,
                    content_ref: r.get(3)?,
                    content_sha256: r.get(4)?,
                    created_at: r.get(5)?,
                },
                triage_status: r.get(6)?,
                destination_kind: r.get(7)?,
                destination_ref_id: r.get(8)?,
                destination_label: r.get(9)?,
                triage_batch_id: r.get(10)?,
                triage_version: r.get(11)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn capture_triage(&self, capture_id: &str) -> Result<Option<CaptureTriage>> {
        read_triage(&self.store.db, capture_id)
    }

    /// Mark a spark `sorted` (with a destination) or back to `unsorted`.
    ///
    /// - `expected_version` is the triage version the caller saw (0 = never triaged).
    /// - Retrying an identical request after a lost receipt returns the current state.
    /// - A different state against a stale version is rejected (`InvalidTransition`).
    /// - The capture itself is never modified or deleted.
    pub fn set_capture_triage(
        &mut self,
        capture_id: &str,
        destination: Option<&TriageDestination>,
        batch_id: Option<&str>,
        expected_version: i64,
        now: i64,
    ) -> Result<CaptureTriage> {
        let trimmed = |v: Option<&str>| v.map(str::trim).map(str::to_owned);
        let requested = CaptureTriage {
            capture_id: capture_id.trim().to_owned(),
            triage_status: if destination.is_some() { TRIAGE_SORTED } else { TRIAGE_UNSORTED }.to_owned(),
            destination_kind: destination.map(|d| d.kind.trim().to_owned()),
            destination_ref_id: destination.and_then(|d| trimmed(d.ref_id.as_deref())),
            destination_label: destination.and_then(|d| trimmed(d.label.as_deref())),
            triage_batch_id: trimmed(batch_id),
            version: expected_version.max(0) + 1,
            updated_at: now,
        };
        requested.validate()?;
        let home = self.paired_home()?;
        let origin = self.origin.clone();
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !capture_exists(&tx, &requested.capture_id)? {
            return Err(Error::InvalidTransition);
        }
        let current = read_triage(&tx, &requested.capture_id)?;
        if let Some(cur) = current.as_ref().filter(|c| c.same_state(&requested)) {
            return Ok(cur.clone());
        }
        let current_version = current.as_ref().map_or(0, |c| c.version);
        if current.is_none() && requested.triage_status == TRIAGE_UNSORTED {
            // Never triaged and asked to stay unsorted: nothing to record.
            if expected_version != 0 {
                return Err(Error::InvalidTransition);
            }
            return Ok(CaptureTriage { version: 0, ..requested });
        }
        if expected_version != current_version {
            return Err(Error::InvalidTransition);
        }
        let applied = CaptureTriage { version: current_version + 1, ..requested };
        write_triage(&tx, &applied)?;
        if let Some((home_id, home_epoch, _)) = home {
            crate::personal_ops::insert_personal_sync(
                &tx,
                &origin,
                (&home_id, home_epoch),
                ("capture_triage", &applied.capture_id, "upsert"),
                current_version,
                &serde_json::to_string(&applied)?,
                now,
            )?;
        }
        tx.commit()?;
        Ok(applied)
    }
}

impl LocalApp {
    /// Store a new Daily Review draft. The newest draft supersedes older open drafts
    /// (they become `dismissed`). Saving the same id with the same payload is idempotent.
    pub fn save_triage_draft(
        &mut self,
        id: &str,
        payload_json: &str,
        model_ref: Option<&str>,
        now: i64,
    ) -> Result<TriageBatch> {
        let id = id.trim();
        if id.is_empty()
            || id.len() > MAX_BATCH_LEN
            || payload_json.len() > MAX_BATCH_PAYLOAD
            || serde_json::from_str::<serde_json::Value>(payload_json).is_err()
            || model_ref.is_some_and(|m| m.len() > MAX_LABEL_LEN)
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = read_batch(&tx, "WHERE id=?1", id)? {
            if existing.payload_json != payload_json {
                return Err(Error::IdentityConflict);
            }
            return Ok(existing);
        }
        tx.execute(
            "UPDATE triage_batches SET status='dismissed', updated_at=?1 WHERE status='draft'",
            [now],
        )?;
        tx.execute(
            "INSERT INTO triage_batches(id,status,payload_json,model_ref,created_at,updated_at)
             VALUES(?1,'draft',?2,?3,?4,?4)",
            params![id, payload_json, model_ref, now],
        )?;
        let saved = read_batch(&tx, "WHERE id=?1", id)?.ok_or(Error::InvalidTransition)?;
        tx.commit()?;
        Ok(saved)
    }

    /// The open draft the Inbox should show, if any.
    pub fn latest_triage_draft(&self) -> Result<Option<TriageBatch>> {
        read_batch(
            &self.store.db,
            "WHERE status=?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
            "draft",
        )
    }

    pub fn triage_batch(&self, id: &str) -> Result<Option<TriageBatch>> {
        read_batch(&self.store.db, "WHERE id=?1", id)
    }

    /// Close a draft: `applied` after the user confirmed it, `dismissed` when skipped.
    /// Only open drafts can close; repeating the same close is idempotent.
    pub fn close_triage_batch(&mut self, id: &str, status: &str, now: i64) -> Result<TriageBatch> {
        if status != "applied" && status != "dismissed" {
            return Err(Error::InvalidTransition);
        }
        let current = self.triage_batch(id)?.ok_or(Error::InvalidTransition)?;
        if current.status == status {
            return Ok(current);
        }
        if current.status != "draft" {
            return Err(Error::InvalidTransition);
        }
        self.store.db.execute(
            "UPDATE triage_batches SET status=?2, updated_at=?3 WHERE id=?1 AND status='draft'",
            params![id, status, now],
        )?;
        self.triage_batch(id)?.ok_or(Error::InvalidTransition)
    }
}
