use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::path::Path;

const SCHEMA: &str = include_str!("../migrations/0001.sql");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("persistent database path required")]
    PersistentPathRequired,
    #[error("filesystem error")]
    Filesystem(#[from] std::io::Error),
    #[error("database owner already active")]
    OwnershipUnavailable,
    #[error("database or lock is a symbolic link or reparse point")]
    LinkedDatabase,
    #[error("database error")]
    Database(#[from] rusqlite::Error),
    #[error("serialization error")]
    Json(#[from] serde_json::Error),
    #[error("request identity conflict")]
    IdentityConflict,
    #[error("invalid transition or stale authority")]
    InvalidTransition,
    #[error("schema checksum mismatch")]
    SchemaMismatch,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Immutable identity scope; payload must contain user content, never credentials.
#[derive(Clone, Serialize, Deserialize)]
pub struct AppendMessage {
    pub request_id: String,
    pub session_id: String,
    pub message_id: String,
    pub origin_device_id: String,
    pub home_device_id: String,
    pub home_epoch: i64,
    pub content: Value,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct LocalAccepted {
    pub request_id: String,
    pub message_id: String,
    pub payload_sha256: String,
}

/// A dispatcher reports this only for the exact attempt it owns. Generic I/O
/// errors must use PossiblySent; timeout is never evidence of zero bytes.
pub enum AttemptOutcome {
    ConfirmedZeroBytes,
    PossiblySent,
}

/// Trusted adapter input, not an authentication implementation. The adapter
/// must authenticate the Home before passing this receipt to the kernel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HomeReceipt {
    pub request_id: String,
    pub home_device_id: String,
    pub home_epoch: i64,
    pub payload_sha256: String,
    pub accepted: bool,
}

// Field order matters: close SQLite before dropping the ownership lock.
pub struct Store {
    pub(crate) db: Connection,
    _owner: File,
}

fn reject_link(path: &Path) -> Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if meta.file_type().is_symlink() {
        return Err(Error::LinkedDatabase);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(Error::LinkedDatabase);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            return Err(Error::LinkedDatabase);
        }
    }
    Ok(())
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Store {
    pub fn open(path: &Path, now: i64) -> Result<Self> {
        let name = path.to_string_lossy();
        if name.is_empty() || name == ":memory:" || name.starts_with("file:") {
            return Err(Error::PersistentPathRequired);
        }
        // Canonical parent makes relative and directory-alias paths share a lock.
        // Caller must provision a private application data directory. This is
        // process coordination, not protection from a hostile directory owner.
        let file_name = path.file_name().ok_or(Error::PersistentPathRequired)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let path = parent.canonicalize()?.join(file_name);
        reject_link(&path)?;
        let path = if path.exists() {
            path.canonicalize()?
        } else {
            path
        };
        let mut lock_name = path
            .file_name()
            .ok_or(Error::PersistentPathRequired)?
            .to_os_string();
        lock_name.push(".owner-lock");
        let lock_path = path.with_file_name(lock_name);
        reject_link(&lock_path)?;
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        owner.try_lock().map_err(|_| Error::OwnershipUnavailable)?;
        // The lock file stays on disk; deleting it could create two lock inodes.
        reject_link(&path)?;
        let mut db = Connection::open(&path)?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let checksum = digest(SCHEMA.replace("\r\n", "\n").as_bytes());
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='schema_migrations')",
            [],
            |r| r.get(0),
        )?;
        if exists {
            let stored: Option<String> = tx
                .query_row(
                    "SELECT checksum FROM schema_migrations WHERE version=1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if stored.as_deref() != Some(checksum.as_str()) {
                return Err(Error::SchemaMismatch);
            }
        } else {
            tx.execute_batch(SCHEMA)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(1,?1,?2)",
                params![checksum, now],
            )?;
        }
        let grant_schema = include_str!("../migrations/0003.sql");
        let grant_hash = digest(grant_schema.replace("\r\n", "\n").as_bytes());
        let previous: Option<String> = tx
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version=3",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match previous {
            Some(v) if v != grant_hash => return Err(Error::SchemaMismatch),
            Some(_) => {}
            None => {
                tx.execute_batch(grant_schema)?;
                tx.execute(
                    "INSERT INTO schema_migrations VALUES(3,?1,?2)",
                    params![grant_hash, now],
                )?;
            }
        }
        tx.commit()?;
        let mut store = Self { db, _owner: owner };
        store.recover_interrupted()?;
        Ok(store)
    }

    /// Explicit local provisioning seam. Pairing and trust verification belong
    /// to a future trusted adapter; this does not itself pair a remote device.
    pub fn initialize_local_identity(
        &mut self,
        origin: &str,
        home: &str,
        epoch: i64,
        now: i64,
    ) -> Result<()> {
        if epoch < 1 {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for id in [origin, home] {
            tx.execute("INSERT OR IGNORE INTO devices(id,label,trust_state,created_at) VALUES(?1,?1,'unpaired',?2)", params![id, now])?;
        }
        tx.execute(
            "INSERT OR IGNORE INTO home_authority VALUES(1,?1,?2)",
            params![home, epoch],
        )?;
        let matches: bool = tx.query_row(
            "SELECT device_id=?1 AND epoch=?2 FROM home_authority WHERE singleton=1",
            params![home, epoch],
            |r| r.get(0),
        )?;
        if !matches {
            return Err(Error::InvalidTransition);
        }
        tx.commit()?;
        Ok(())
    }

    pub fn accept_message(&mut self, request: &AppendMessage, now: i64) -> Result<LocalAccepted> {
        if [
            &request.request_id,
            &request.session_id,
            &request.message_id,
            &request.origin_device_id,
            &request.home_device_id,
        ]
        .iter()
        .any(|id| id.trim().is_empty())
        {
            return Err(Error::InvalidTransition);
        }
        // serde_json's default map is sorted, giving stable nested object order.
        let hash = digest(&serde_json::to_vec(request)?);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior: Option<String> = tx
            .query_row(
                "SELECT payload_sha256 FROM intent_outbox WHERE request_id=?1",
                [&request.request_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(prior) = prior {
            if prior != hash {
                return Err(Error::IdentityConflict);
            }
        } else {
            let authority: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM home_authority WHERE device_id=?1 AND epoch=?2)",
                params![request.home_device_id, request.home_epoch],
                |r| r.get(0),
            )?;
            if !authority {
                return Err(Error::InvalidTransition);
            }
            tx.execute("INSERT OR IGNORE INTO conversations(id,title,acceptance,version,created_at) VALUES(?1,'','local',1,?2)", params![request.session_id, now])?;
            tx.execute("INSERT INTO messages(id,conversation_id,origin_device_id,request_id,role,content_json,acceptance,created_at) VALUES(?1,?2,?3,?4,'user',?5,'local',?6)", params![request.message_id,request.session_id,request.origin_device_id,request.request_id,serde_json::to_string(&request.content)?,now])?;
            tx.execute("INSERT INTO intent_outbox(request_id,session_id,message_id,origin_device_id,target_device_id,home_epoch,kind,payload_json,payload_sha256,send_state,created_at,transport_evidence) VALUES(?1,?2,?3,?4,?5,?6,'append_message',?7,?8,'local_saved',?9,'none')", params![request.request_id,request.session_id,request.message_id,request.origin_device_id,request.home_device_id,request.home_epoch,serde_json::to_string(&request.content)?,hash,now])?;
        }
        tx.commit()?;
        Ok(LocalAccepted {
            request_id: request.request_id.clone(),
            message_id: request.message_id.clone(),
            payload_sha256: hash,
        })
    }

    /// Persist the ambiguity boundary BEFORE handing any bytes to transport.
    /// Returns the monotonically increasing attempt number for completion CAS.
    pub fn begin_attempt(&mut self, id: &str, now: i64) -> Result<i64> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute("UPDATE intent_outbox SET send_state='dispatching',transport_evidence='started_or_possible',attempts=attempts+1,dispatched_at=?2 WHERE request_id=?1 AND send_state IN ('local_saved','queued_for_wakeup','not_dispatched') AND EXISTS(SELECT 1 FROM home_authority h WHERE h.device_id=intent_outbox.target_device_id AND h.epoch=intent_outbox.home_epoch) AND (lane='control' OR NOT EXISTS(SELECT 1 FROM intent_outbox blocker WHERE blocker.session_id=intent_outbox.session_id AND blocker.request_id!=intent_outbox.request_id AND blocker.lane='normal' AND blocker.send_state IN ('dispatching','dispatched','unknown_send')))", params![id, now])?;
        if changed != 1 {
            return Err(Error::InvalidTransition);
        }
        let attempt = tx.query_row(
            "SELECT attempts FROM intent_outbox WHERE request_id=?1",
            [id],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(attempt)
    }

    pub fn finish_attempt(
        &mut self,
        id: &str,
        attempt: i64,
        outcome: AttemptOutcome,
    ) -> Result<()> {
        let (state, evidence) = match outcome {
            AttemptOutcome::ConfirmedZeroBytes => ("not_dispatched", "confirmed_zero"),
            AttemptOutcome::PossiblySent => ("unknown_send", "started_or_possible"),
        };
        if self.db.execute("UPDATE intent_outbox SET send_state=?3,transport_evidence=?4 WHERE request_id=?1 AND attempts=?2 AND send_state='dispatching'", params![id,attempt,state,evidence])? != 1 { return Err(Error::InvalidTransition); }
        Ok(())
    }

    /// Store owns an exclusive process lock for its entire lifetime. A second
    /// Store cannot open this database or recover another live writer.
    fn recover_interrupted(&mut self) -> Result<usize> {
        Ok(self.db.execute("UPDATE intent_outbox SET send_state='unknown_send',transport_evidence='started_or_possible',lease_owner=NULL,lease_until=NULL WHERE send_state IN ('dispatching','dispatched')", [])?)
    }

    pub fn apply_receipt(&mut self, receipt: &HomeReceipt, now: i64) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = if receipt.accepted {
            "peer_accepted"
        } else {
            "rejected"
        };
        let changed = tx.execute("UPDATE intent_outbox SET send_state=?5,transport_evidence='peer_receipt',acked_at=?6 WHERE request_id=?1 AND target_device_id=?2 AND home_epoch=?3 AND payload_sha256=?4 AND send_state IN ('dispatching','dispatched','unknown_send') AND EXISTS(SELECT 1 FROM home_authority h WHERE h.device_id=?2 AND h.epoch=?3)", params![receipt.request_id,receipt.home_device_id,receipt.home_epoch,receipt.payload_sha256,state,now])?;
        if changed != 1 {
            let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM intent_outbox WHERE request_id=?1 AND target_device_id=?2 AND home_epoch=?3 AND payload_sha256=?4 AND send_state=?5 AND transport_evidence='peer_receipt' AND EXISTS(SELECT 1 FROM home_authority h WHERE h.device_id=?2 AND h.epoch=?3))", params![receipt.request_id,receipt.home_device_id,receipt.home_epoch,receipt.payload_sha256,state], |r| r.get(0))?;
            if !duplicate {
                return Err(Error::InvalidTransition);
            }
            tx.commit()?;
            return Ok(());
        }
        tx.execute(
            "UPDATE messages SET acceptance=?2 WHERE request_id=?1",
            params![
                receipt.request_id,
                if receipt.accepted { "home" } else { "rejected" }
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn state(&self, id: &str) -> Result<String> {
        Ok(self.db.query_row(
            "SELECT send_state FROM intent_outbox WHERE request_id=?1",
            [id],
            |r| r.get(0),
        )?)
    }
}
