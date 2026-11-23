//! Local immutable capture originals. Bytes are deliberately excluded from sync snapshots.
use crate::{
    app::LocalApp,
    outbox::{digest, Error, Result},
    personal_ops::CaptureItem,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const SQL: &str = include_str!("../migrations/0008.sql");
pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    let hash = digest(SQL.replace("\r\n", "\n").as_bytes());
    let old: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=8",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match old {
        Some(v) if v != hash => return Err(Error::SchemaMismatch),
        Some(_) => {}
        None => {
            tx.execute_batch(SQL)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(8,?1,?2)",
                params![hash, now],
            )?;
        }
    }
    Ok(())
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureMedia {
    pub id: String,
    pub mime: String,
    pub base64: String,
}
impl LocalApp {
    pub fn put_capture_media(&mut self, mime: &str, encoded: &str, now: i64) -> Result<String> {
        if encoded.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
            return Err(Error::InvalidTransition);
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| Error::InvalidTransition)?;
        let valid = match mime {
            "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "image/jpeg" => bytes.starts_with(&[255, 216, 255]),
            "image/webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            _ => false,
        };
        if !valid || bytes.len() > MAX_IMAGE_BYTES {
            return Err(Error::InvalidTransition);
        }
        let id = digest(&bytes);
        self.store.db.execute(
            "INSERT OR IGNORE INTO capture_media VALUES(?1,?2,?3,?4)",
            params![id, mime, bytes, now],
        )?;
        Ok(id)
    }
    pub fn capture_media(&self, id: &str) -> Result<CaptureMedia> {
        self.store
            .db
            .query_row(
                "SELECT id,mime,bytes FROM capture_media WHERE id=?1",
                [id],
                |r| {
                    let bytes: Vec<u8> = r.get(2)?;
                    Ok(CaptureMedia {
                        id: r.get(0)?,
                        mime: r.get(1)?,
                        base64: STANDARD.encode(bytes),
                    })
                },
            )
            .map_err(Into::into)
    }
    pub fn capture_image_id(&self, id: &str) -> Result<Option<String>> {
        self.store
            .db
            .query_row(
                "SELECT media_id FROM capture_media_links WHERE capture_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn capture_with_image(
        &mut self,
        id: &str,
        source: Option<&str>,
        content: &str,
        project: Option<&str>,
        title: Option<&str>,
        media: Option<&str>,
        now: i64,
    ) -> Result<CaptureItem> {
        self.store.db.execute_batch("SAVEPOINT capture_image")?;
        let result = (|| {
            if self.list_captures()?.iter().any(|c| c.id == id)
                && self.capture_image_id(id)?.as_deref() != media
            {
                return Err(Error::IdentityConflict);
            }
            if let Some(mid) = media {
                self.capture_media(mid)?;
            }
            let item = self.capture_with_project(id, source, content, project, title, now)?;
            if let Some(mid) = media {
                self.store.db.execute(
                    "INSERT OR IGNORE INTO capture_media_links VALUES(?1,?2)",
                    params![id, mid],
                )?;
            }
            Ok(item)
        })();
        match result {
            Ok(v) => {
                self.store.db.execute_batch("RELEASE capture_image")?;
                Ok(v)
            }
            Err(e) => {
                self.store
                    .db
                    .execute_batch("ROLLBACK TO capture_image; RELEASE capture_image")?;
                Err(e)
            }
        }
    }
}
