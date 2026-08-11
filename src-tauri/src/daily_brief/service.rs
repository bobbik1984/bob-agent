use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use super::models::{
    BriefStatus, DailyBriefSnapshot, DateContext, SourceState, DAILY_BRIEF_SCHEMA_VERSION,
};
use super::{ranker, sources};

pub fn init_daily_brief_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS daily_brief_cache (
            local_date TEXT PRIMARY KEY,
            revision INTEGER NOT NULL,
            source_fingerprint TEXT NOT NULL,
            snapshot_id TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            generated_at INTEGER NOT NULL,
            status TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS daily_brief_seen (
            device_id TEXT NOT NULL,
            local_date TEXT NOT NULL,
            last_seen_revision INTEGER NOT NULL DEFAULT 0,
            seen_items_json TEXT NOT NULL DEFAULT '{}',
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (device_id, local_date)
         );",
    )
    .map_err(|error| format!("ERR-BRIEF-INIT: {error}"))?;
    Ok(())
}

fn fingerprint(collection: &super::models::SourceCollection) -> String {
    let mut health = collection
        .health
        .iter()
        .map(|item| {
            format!(
                "{}:{:?}:{}:{}",
                item.source.as_str(),
                item.state,
                item.revision,
                item.error_code.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>();
    health.sort();
    let mut items = collection
        .items
        .iter()
        .map(|item| item.content_revision_key())
        .collect::<Vec<_>>();
    items.sort();
    format!("{}||{}", health.join("|"), items.join("|"))
}

fn cached_snapshot(
    conn: &Connection,
    local_date: &str,
) -> Result<Option<(u64, String, DailyBriefSnapshot)>, String> {
    conn.query_row(
        "SELECT revision, source_fingerprint, payload_json
         FROM daily_brief_cache WHERE local_date = ?1",
        params![local_date],
        |row| {
            let revision = row.get::<_, i64>(0)? as u64;
            let fingerprint = row.get::<_, String>(1)?;
            let payload = row.get::<_, String>(2)?;
            Ok((revision, fingerprint, payload))
        },
    )
    .optional()
    .map_err(|error| format!("ERR-BRIEF-CACHE-READ: {error}"))?
    .map(|(revision, fingerprint, payload)| {
        serde_json::from_str::<DailyBriefSnapshot>(&payload)
            .map(|snapshot| (revision, fingerprint, snapshot))
            .map_err(|error| format!("ERR-BRIEF-CACHE-JSON: {error}"))
    })
    .transpose()
}

fn seen_items(
    conn: &Connection,
    device_id: &str,
    local_date: &str,
) -> Result<(u64, BTreeMap<String, String>), String> {
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT last_seen_revision, seen_items_json
             FROM daily_brief_seen WHERE device_id = ?1 AND local_date = ?2",
            params![device_id, local_date],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("ERR-BRIEF-SEEN-READ: {error}"))?;
    match row {
        Some((revision, raw)) => {
            let items = serde_json::from_str::<BTreeMap<String, String>>(&raw)
                .map_err(|error| format!("ERR-BRIEF-SEEN-JSON: {error}"))?;
            Ok((revision.max(0) as u64, items))
        }
        None => Ok((0, BTreeMap::new())),
    }
}

fn apply_seen_delta(
    snapshot: &mut DailyBriefSnapshot,
    seen_revision: u64,
    seen: &BTreeMap<String, String>,
) {
    if seen_revision >= snapshot.revision {
        snapshot.changed_since_last_seen.clear();
        return;
    }
    snapshot.changed_since_last_seen = snapshot
        .all_items()
        .filter(|item| seen.get(&item.item_id) != Some(&item.source_revision))
        .map(|item| item.item_id.clone())
        .collect();
}

fn save_cache(
    conn: &Connection,
    fingerprint: &str,
    snapshot: &DailyBriefSnapshot,
) -> Result<(), String> {
    let payload = serde_json::to_string(snapshot)
        .map_err(|error| format!("ERR-BRIEF-CACHE-SERIALIZE: {error}"))?;
    conn.execute(
        "INSERT INTO daily_brief_cache (
            local_date, revision, source_fingerprint, snapshot_id, payload_json, generated_at, status
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(local_date) DO UPDATE SET
            revision=excluded.revision,
            source_fingerprint=excluded.source_fingerprint,
            snapshot_id=excluded.snapshot_id,
            payload_json=excluded.payload_json,
            generated_at=excluded.generated_at,
            status=excluded.status",
        params![
            &snapshot.local_date,
            snapshot.revision as i64,
            fingerprint,
            &snapshot.snapshot_id,
            payload,
            snapshot.generated_at,
            match snapshot.status {
                BriefStatus::Fresh => "fresh",
                BriefStatus::Partial => "partial",
                BriefStatus::Stale => "stale",
            }
        ],
    )
    .map_err(|error| format!("ERR-BRIEF-CACHE-WRITE: {error}"))?;
    Ok(())
}

pub fn get_snapshot(
    conn: &Connection,
    data_dir: &Path,
    context: &DateContext,
    device_id: &str,
    force_refresh: bool,
) -> Result<DailyBriefSnapshot, String> {
    context.validate()?;
    init_daily_brief_tables(conn)?;

    let collection = sources::collect(conn, data_dir, context);
    let current_fingerprint = fingerprint(&collection);
    let cached = match cached_snapshot(conn, &context.local_date) {
        Ok(value) => value,
        Err(error) if error.starts_with("ERR-BRIEF-CACHE-JSON") => {
            conn.execute(
                "DELETE FROM daily_brief_cache WHERE local_date = ?1",
                params![&context.local_date],
            )
            .map_err(|delete_error| format!("{error}; {delete_error}"))?;
            None
        }
        Err(error) => return Err(error),
    };
    let (seen_revision, seen) = seen_items(conn, device_id, &context.local_date)?;

    if !force_refresh {
        if let Some((_, cached_fingerprint, mut snapshot)) = cached.clone() {
            if cached_fingerprint == current_fingerprint {
                apply_seen_delta(&mut snapshot, seen_revision, &seen);
                return Ok(snapshot);
            }
        }
    }

    let previous_revision = cached.as_ref().map(|value| value.0).unwrap_or(0);
    let fingerprint_changed = cached
        .as_ref()
        .map(|value| value.1 != current_fingerprint)
        .unwrap_or(true);
    let revision = if fingerprint_changed {
        previous_revision.saturating_add(1).max(1)
    } else {
        previous_revision.max(1)
    };
    let ranked = ranker::rank(collection.items);
    let has_error = collection
        .health
        .iter()
        .any(|item| item.state == SourceState::Error);
    let warnings = collection
        .health
        .iter()
        .filter_map(|item| item.error_code.clone())
        .collect::<Vec<_>>();
    let generated_at = crate::now_ms() as i64;
    let mut snapshot = DailyBriefSnapshot {
        schema_version: DAILY_BRIEF_SCHEMA_VERSION,
        snapshot_id: format!("daily:{}:{revision}", context.local_date),
        local_date: context.local_date.clone(),
        revision,
        generated_at,
        status: if has_error {
            BriefStatus::Partial
        } else {
            BriefStatus::Fresh
        },
        focus_item: ranked.focus_item,
        attention_items: ranked.attention_items,
        detail_items: ranked.detail_items,
        section_counts: ranked.section_counts,
        actionable_count: ranked.actionable_count,
        changed_since_last_seen: Vec::new(),
        source_health: collection.health,
        warnings,
    };
    apply_seen_delta(&mut snapshot, seen_revision, &seen);
    save_cache(conn, &current_fingerprint, &snapshot)?;
    Ok(snapshot)
}

pub fn mark_seen(
    conn: &Connection,
    device_id: &str,
    snapshot_id: &str,
    revision: u64,
) -> Result<bool, String> {
    init_daily_brief_tables(conn)?;
    let row: Option<(String, i64, String)> = conn
        .query_row(
            "SELECT local_date, revision, payload_json
             FROM daily_brief_cache WHERE snapshot_id = ?1",
            params![snapshot_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| format!("ERR-BRIEF-MARK-READ: {error}"))?;
    let Some((local_date, stored_revision, payload)) = row else {
        return Err("ERR-BRIEF-SNAPSHOT-NOT-FOUND".into());
    };
    if stored_revision.max(0) as u64 != revision {
        return Err("ERR-BRIEF-REVISION-CONFLICT".into());
    }
    let snapshot: DailyBriefSnapshot =
        serde_json::from_str(&payload).map_err(|error| format!("ERR-BRIEF-MARK-JSON: {error}"))?;
    let items: BTreeMap<String, String> = snapshot
        .all_items()
        .map(|item| (item.item_id.clone(), item.source_revision.clone()))
        .collect();
    let items_json = serde_json::to_string(&items)
        .map_err(|error| format!("ERR-BRIEF-MARK-SERIALIZE: {error}"))?;
    let now = crate::now_ms() as i64;
    conn.execute(
        "INSERT INTO daily_brief_seen (
            device_id, local_date, last_seen_revision, seen_items_json, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(device_id, local_date) DO UPDATE SET
            last_seen_revision = CASE
                WHEN excluded.last_seen_revision >= daily_brief_seen.last_seen_revision
                THEN excluded.last_seen_revision ELSE daily_brief_seen.last_seen_revision END,
            seen_items_json = CASE
                WHEN excluded.last_seen_revision >= daily_brief_seen.last_seen_revision
                THEN excluded.seen_items_json ELSE daily_brief_seen.seen_items_json END,
            updated_at = excluded.updated_at",
        params![device_id, local_date, revision as i64, items_json, now],
    )
    .map_err(|error| format!("ERR-BRIEF-MARK-WRITE: {error}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn context() -> DateContext {
        DateContext {
            local_date: "2026-08-11".into(),
            utc_offset_minutes: 480,
        }
    }

    fn temp_dir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("bob-daily-brief-{}", ulid::Ulid::new()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn seed_todo(conn: &Connection) {
        crate::calendar::init_events_table(conn);
        conn.execute(
            "INSERT INTO events (
                id, title, type, status, date, description, created_at, updated_at
             ) VALUES (?1, ?2, 'todo', 'pending', ?3, ?4, ?5, ?5)",
            params![
                "todo-brief-test",
                "Prepare the daily review",
                "2026-08-11",
                "A deterministic test item",
                1_786_400_000_000_i64
            ],
        )
        .unwrap();
    }

    #[test]
    fn cache_revision_is_stable_without_source_changes() {
        let conn = Connection::open_in_memory().unwrap();
        let dir = temp_dir();
        let first = get_snapshot(&conn, &dir, &context(), "pc", false).unwrap();
        let second = get_snapshot(&conn, &dir, &context(), "pc", false).unwrap();
        assert_eq!(first.revision, second.revision);
        assert_eq!(first.snapshot_id, second.snapshot_id);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn seen_state_is_device_local_and_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        seed_todo(&conn);
        let dir = temp_dir();
        let snapshot = get_snapshot(&conn, &dir, &context(), "pc", false).unwrap();
        assert!(!snapshot.changed_since_last_seen.is_empty());
        assert!(mark_seen(&conn, "pc", &snapshot.snapshot_id, snapshot.revision).unwrap());
        assert!(mark_seen(&conn, "pc", &snapshot.snapshot_id, snapshot.revision).unwrap());
        let pc = get_snapshot(&conn, &dir, &context(), "pc", false).unwrap();
        let mobile = get_snapshot(&conn, &dir, &context(), "mobile", false).unwrap();
        assert!(pc.changed_since_last_seen.is_empty());
        assert_eq!(
            mobile.changed_since_last_seen.len(),
            mobile.all_items().count()
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn cache_and_seen_state_survive_database_reopen() {
        let dir = temp_dir();
        let db_path = dir.join("daily-brief-test.db");
        let (snapshot_id, revision) = {
            let conn = Connection::open(&db_path).unwrap();
            seed_todo(&conn);
            let snapshot = get_snapshot(&conn, &dir, &context(), "pc", false).unwrap();
            assert!(!snapshot.changed_since_last_seen.is_empty());
            mark_seen(&conn, "pc", &snapshot.snapshot_id, snapshot.revision).unwrap();
            (snapshot.snapshot_id, snapshot.revision)
        };

        let reopened = Connection::open(&db_path).unwrap();
        let snapshot = get_snapshot(&reopened, &dir, &context(), "pc", false).unwrap();
        assert_eq!(snapshot.snapshot_id, snapshot_id);
        assert_eq!(snapshot.revision, revision);
        assert!(snapshot.changed_since_last_seen.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn old_revision_cannot_mark_new_snapshot_seen() {
        let conn = Connection::open_in_memory().unwrap();
        let dir = temp_dir();
        let snapshot = get_snapshot(&conn, &dir, &context(), "pc", false).unwrap();
        let error =
            mark_seen(&conn, "pc", &snapshot.snapshot_id, snapshot.revision + 1).unwrap_err();
        assert_eq!(error, "ERR-BRIEF-REVISION-CONFLICT");
        let _ = fs::remove_dir_all(dir);
    }
}
