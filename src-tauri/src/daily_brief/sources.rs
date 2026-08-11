use std::fs;
use std::path::Path;

use chrono::{FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use rusqlite::Connection;
use serde_json::{json, Value};

use super::models::{
    BriefActionKind, BriefItemKind, BriefSource, DailyBriefAction, DailyBriefItem, DateContext,
    SourceCollection, SourceHealth,
};

fn source_revision(items: &[DailyBriefItem], source: BriefSource) -> String {
    items
        .iter()
        .filter(|item| item.source == source)
        .filter_map(|item| item.occurred_at)
        .max()
        .unwrap_or(0)
        .to_string()
}

fn stable_error_code(error: &str) -> String {
    error.split(':').next().unwrap_or(error).trim().to_string()
}

fn local_epoch_ms(date: &str, time: Option<&str>, utc_offset_minutes: i32) -> Option<i64> {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let time = time
        .and_then(|value| {
            NaiveTime::parse_from_str(value, "%H:%M")
                .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M:%S"))
                .ok()
        })
        .unwrap_or(NaiveTime::MIN);
    let offset = FixedOffset::east_opt(utc_offset_minutes * 60)?;
    offset
        .from_local_datetime(&NaiveDateTime::new(date, time))
        .single()
        .map(|value| value.timestamp_millis())
}

fn action(
    kind: BriefActionKind,
    target_type: &str,
    target_id: &str,
    payload: Value,
) -> DailyBriefAction {
    DailyBriefAction {
        kind,
        target_type: Some(target_type.into()),
        target_id: Some(target_id.into()),
        payload,
    }
}

fn collect_calendar_and_todos(
    conn: &Connection,
    context: &DateContext,
) -> Result<Vec<DailyBriefItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, type, status, date, start_time, description,
                    CASE WHEN COALESCE(updated_at, 0) > 0 THEN updated_at ELSE created_at END
             FROM events
             WHERE status NOT IN ('completed', 'done', 'cancelled', 'deleted')
             ORDER BY COALESCE(updated_at, created_at) DESC
             LIMIT 200",
        )
        .map_err(|error| format!("ERR-BRIEF-CALENDAR-QUERY: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })
        .map_err(|error| format!("ERR-BRIEF-CALENDAR-READ: {error}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (id, title, event_type, status, date, start_time, description, updated_at) =
            row.map_err(|error| format!("ERR-BRIEF-CALENDAR-ROW: {error}"))?;
        let is_todo = event_type.eq_ignore_ascii_case("todo");
        if !is_todo && date.as_deref() != Some(context.local_date.as_str()) {
            continue;
        }
        let overdue = is_todo
            && date
                .as_deref()
                .map(|value| value < context.local_date.as_str())
                .unwrap_or(false);
        let due_today = date.as_deref() == Some(context.local_date.as_str());
        let requires_attention = is_todo && (overdue || due_today || status == "blocked");
        let priority = if overdue {
            940
        } else if status == "blocked" {
            920
        } else if is_todo && due_today {
            820
        } else if is_todo {
            430
        } else {
            650
        };
        let source = if is_todo {
            BriefSource::Todo
        } else {
            BriefSource::Calendar
        };
        let kind = if is_todo {
            if requires_attention {
                BriefItemKind::Due
            } else {
                BriefItemKind::Progress
            }
        } else {
            BriefItemKind::Schedule
        };
        let action_kind = if is_todo {
            BriefActionKind::OpenTodo
        } else {
            BriefActionKind::OpenCalendar
        };
        let mut reason_codes = Vec::new();
        if overdue {
            reason_codes.push("brief.reason.overdue".into());
        } else if due_today {
            reason_codes.push("brief.reason.today".into());
        }
        if status == "blocked" {
            reason_codes.push("brief.reason.blocked".into());
        }
        items.push(DailyBriefItem {
            item_id: DailyBriefItem::stable_id(source, &id),
            canonical_ref: format!("event:{id}"),
            source,
            source_id: id.clone(),
            source_revision: updated_at.to_string(),
            kind,
            title: Some(title),
            title_key: None,
            summary: description.filter(|value| !value.trim().is_empty()),
            summary_key: None,
            message_args: Value::Null,
            priority,
            requires_attention,
            occurred_at: Some(updated_at),
            due_at: date.as_deref().and_then(|value| {
                local_epoch_ms(value, start_time.as_deref(), context.utc_offset_minutes)
            }),
            action: action(
                action_kind,
                if is_todo { "todo" } else { "calendar_event" },
                &id,
                Value::Null,
            ),
            reason_codes,
            evidence_refs: vec![format!("event:{id}@{updated_at}")],
        });
    }
    Ok(items)
}

fn collect_work(conn: &Connection) -> Result<Vec<DailyBriefItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, project_id, title, status, description, data_json, revision, updated_at
             FROM work_objects
             WHERE deleted_at IS NULL
               AND status NOT IN ('done', 'completed', 'cancelled', 'rejected', 'archived')
               AND kind IN ('goal', 'task', 'risk', 'change', 'commitment')
             ORDER BY updated_at DESC
             LIMIT 150",
        )
        .map_err(|error| format!("ERR-BRIEF-WORK-QUERY: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })
        .map_err(|error| format!("ERR-BRIEF-WORK-READ: {error}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (id, kind_raw, project_id, title, status, description, data_raw, revision, updated_at) =
            row.map_err(|error| format!("ERR-BRIEF-WORK-ROW: {error}"))?;
        let data: Value = serde_json::from_str(&data_raw).unwrap_or(Value::Null);
        let severity = data
            .get("severity")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let blocked = status == "blocked";
        let (item_kind, priority, requires_attention, reason_code) = match kind_raw.as_str() {
            "change" => (
                BriefItemKind::Change,
                880,
                true,
                "brief.reason.change_review",
            ),
            "risk" if blocked || matches!(severity.as_str(), "high" | "critical") => {
                (BriefItemKind::Risk, 860, true, "brief.reason.risk")
            }
            "risk" => (BriefItemKind::Risk, 610, false, "brief.reason.risk"),
            _ if blocked => (BriefItemKind::Risk, 850, true, "brief.reason.blocked"),
            _ => (
                BriefItemKind::Progress,
                500,
                false,
                "brief.reason.in_progress",
            ),
        };
        let canonical_ref = if kind_raw == "goal" {
            format!("goal:{id}")
        } else {
            format!("work:{id}")
        };
        items.push(DailyBriefItem {
            item_id: DailyBriefItem::stable_id(BriefSource::WorkCore, &id),
            canonical_ref,
            source: BriefSource::WorkCore,
            source_id: id.clone(),
            source_revision: format!("{revision}:{updated_at}"),
            kind: item_kind,
            title: Some(title),
            title_key: None,
            summary: description.filter(|value| !value.trim().is_empty()),
            summary_key: None,
            message_args: json!({ "projectId": project_id, "status": status }),
            priority,
            requires_attention,
            occurred_at: Some(updated_at),
            due_at: None,
            action: action(
                if kind_raw == "goal" {
                    BriefActionKind::OpenGoal
                } else {
                    BriefActionKind::OpenWorkObject
                },
                &kind_raw,
                &id,
                json!({ "projectId": project_id }),
            ),
            reason_codes: vec![reason_code.into()],
            evidence_refs: vec![format!("work_object:{id}@{revision}")],
        });
    }
    Ok(items)
}

fn collect_goal_runtime(conn: &Connection) -> Result<Vec<DailyBriefItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT g.run_id, g.goal_id, g.project_id, g.status, g.phase, g.next_action,
                    g.last_error_code, g.revision, g.updated_at, COALESCE(o.title, ''),
                    (SELECT a.approval_id FROM goal_approvals a
                     WHERE a.run_id = g.run_id AND a.status = 'pending'
                     ORDER BY a.created_at DESC LIMIT 1),
                    (SELECT a.summary FROM goal_approvals a
                     WHERE a.run_id = g.run_id AND a.status = 'pending'
                     ORDER BY a.created_at DESC LIMIT 1)
             FROM goal_runs g
             LEFT JOIN work_objects o ON o.id = g.goal_id
             WHERE g.status NOT IN ('done', 'cancelled')
             ORDER BY g.updated_at DESC
             LIMIT 100",
        )
        .map_err(|error| format!("ERR-BRIEF-GOAL-QUERY: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
            ))
        })
        .map_err(|error| format!("ERR-BRIEF-GOAL-READ: {error}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (
            run_id,
            goal_id,
            project_id,
            status,
            phase,
            next_action,
            last_error_code,
            revision,
            updated_at,
            title,
            approval_id,
            approval_summary,
        ) = row.map_err(|error| format!("ERR-BRIEF-GOAL-ROW: {error}"))?;
        let (kind, priority, requires_attention, reason_code) = match status.as_str() {
            "waiting_user" => (
                BriefItemKind::Approval,
                1000,
                true,
                "brief.reason.waiting_user",
            ),
            "blocked" => (BriefItemKind::Risk, 950, true, "brief.reason.blocked"),
            "failed" => (BriefItemKind::Risk, 930, true, "brief.reason.failed"),
            _ => (
                BriefItemKind::Progress,
                640,
                false,
                "brief.reason.in_progress",
            ),
        };
        let action = match approval_id.as_deref() {
            Some(approval_id) => action(
                BriefActionKind::RespondApproval,
                "goal_approval",
                approval_id,
                json!({ "runId": run_id, "goalId": goal_id }),
            ),
            None => action(
                BriefActionKind::OpenGoal,
                "goal",
                &goal_id,
                json!({ "runId": run_id, "projectId": project_id }),
            ),
        };
        items.push(DailyBriefItem {
            item_id: DailyBriefItem::stable_id(BriefSource::GoalRuntime, &run_id),
            canonical_ref: format!("goal:{goal_id}"),
            source: BriefSource::GoalRuntime,
            source_id: run_id.clone(),
            source_revision: format!("{revision}:{updated_at}"),
            kind,
            title: if title.trim().is_empty() {
                None
            } else {
                Some(title)
            },
            title_key: if approval_id.is_some() {
                Some("brief.item.goal_approval".into())
            } else {
                Some("brief.item.goal".into())
            },
            summary: approval_summary.or(next_action),
            summary_key: None,
            message_args: json!({
                "status": status,
                "phase": phase,
                "errorCode": last_error_code,
            }),
            priority,
            requires_attention,
            occurred_at: Some(updated_at),
            due_at: None,
            action,
            reason_codes: vec![reason_code.into()],
            evidence_refs: vec![format!("goal_run:{run_id}@{revision}")],
        });
    }
    Ok(items)
}

fn collect_sessions(data_dir: &Path) -> Result<Vec<DailyBriefItem>, String> {
    let session_dir = data_dir.join("memory").join("sessions");
    if !session_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = fs::read_dir(&session_dir)
        .map_err(|error| format!("ERR-BRIEF-SESSION-DIR: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    files.sort_by_key(|entry| {
        std::cmp::Reverse(entry.metadata().and_then(|meta| meta.modified()).ok())
    });

    let cutoff = crate::now_ms() as i64 - 7 * 24 * 60 * 60 * 1000;
    let mut items = Vec::new();
    for entry in files.into_iter().take(20) {
        let path = entry.path();
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("ERR-BRIEF-SESSION-READ: {error}"))?;
        let value: Value = serde_json::from_str(&raw)
            .map_err(|error| format!("ERR-BRIEF-SESSION-JSON: {error}"))?;
        let timestamp = value.get("timestamp").and_then(Value::as_i64).unwrap_or(0);
        if timestamp < cutoff {
            continue;
        }
        let conversation_id = value
            .get("conversationId")
            .and_then(Value::as_str)
            .or_else(|| path.file_stem().and_then(|value| value.to_str()))
            .unwrap_or("")
            .trim();
        if conversation_id.is_empty() {
            continue;
        }
        let title = value
            .get("userTopics")
            .and_then(Value::as_array)
            .and_then(|topics| topics.first())
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let summary = value
            .get("assistantHighlight")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        items.push(DailyBriefItem {
            item_id: DailyBriefItem::stable_id(BriefSource::Conversation, conversation_id),
            canonical_ref: format!("conversation:{conversation_id}"),
            source: BriefSource::Conversation,
            source_id: conversation_id.into(),
            source_revision: timestamp.to_string(),
            kind: BriefItemKind::ContinueConversation,
            title,
            title_key: Some("brief.item.continue_conversation".into()),
            summary,
            summary_key: None,
            message_args: Value::Null,
            priority: 390,
            requires_attention: false,
            occurred_at: Some(timestamp),
            due_at: None,
            action: action(
                BriefActionKind::ContinueConversation,
                "conversation",
                conversation_id,
                Value::Null,
            ),
            reason_codes: vec!["brief.reason.recent_conversation".into()],
            evidence_refs: vec![path.to_string_lossy().to_string()],
        });
    }
    Ok(items)
}

fn collect_dream(data_dir: &Path) -> Result<Vec<DailyBriefItem>, String> {
    let path = data_dir.join("memory").join("dream_report.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw =
        fs::read_to_string(&path).map_err(|error| format!("ERR-BRIEF-DREAM-READ: {error}"))?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|error| format!("ERR-BRIEF-DREAM-JSON: {error}"))?;
    if value
        .get("dismissed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(Vec::new());
    }
    let generated_at = value
        .get("generatedAt")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let stats = value.get("stats").cloned().unwrap_or_else(|| json!({}));
    let digest_notes = stats
        .get("digest_notes")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let digest_entities = stats
        .get("digest_entities")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let merged = stats.get("merged").and_then(Value::as_i64).unwrap_or(0);
    let corrected = stats.get("corrected").and_then(Value::as_i64).unwrap_or(0);
    if digest_notes == 0 && merged == 0 && corrected == 0 {
        return Ok(Vec::new());
    }
    Ok(vec![DailyBriefItem {
        item_id: DailyBriefItem::stable_id(BriefSource::Dream, "latest"),
        canonical_ref: "dream:latest".into(),
        source: BriefSource::Dream,
        source_id: "latest".into(),
        source_revision: generated_at.to_string(),
        kind: BriefItemKind::Insight,
        title: None,
        title_key: Some("brief.item.dream_insight".into()),
        summary: None,
        summary_key: Some("brief.summary.dream_maintenance".into()),
        message_args: json!({
            "digestNotes": digest_notes,
            "digestEntities": digest_entities,
            "merged": merged,
            "corrected": corrected,
        }),
        priority: 180,
        requires_attention: false,
        occurred_at: Some(generated_at),
        due_at: None,
        action: DailyBriefAction::none(),
        reason_codes: vec!["brief.reason.dream_insight".into()],
        evidence_refs: vec![path.to_string_lossy().to_string()],
    }])
}

fn extend_database_source(
    collection: &mut SourceCollection,
    source: BriefSource,
    result: Result<Vec<DailyBriefItem>, String>,
) {
    match result {
        Ok(items) => {
            let revision = source_revision(&items, source);
            collection.items.extend(items);
            collection.health.push(SourceHealth::ok(source, revision));
        }
        Err(error) => collection
            .health
            .push(SourceHealth::error(source, stable_error_code(&error))),
    }
}

pub fn collect(conn: &Connection, data_dir: &Path, context: &DateContext) -> SourceCollection {
    let mut collection = SourceCollection::default();
    match collect_calendar_and_todos(conn, context) {
        Ok(items) => {
            let calendar_revision = source_revision(&items, BriefSource::Calendar);
            let todo_revision = source_revision(&items, BriefSource::Todo);
            collection
                .health
                .push(SourceHealth::ok(BriefSource::Calendar, calendar_revision));
            collection
                .health
                .push(SourceHealth::ok(BriefSource::Todo, todo_revision));
            collection.items.extend(items);
        }
        Err(error) => {
            collection.health.push(SourceHealth::error(
                BriefSource::Calendar,
                stable_error_code(&error),
            ));
            collection.health.push(SourceHealth::error(
                BriefSource::Todo,
                stable_error_code(&error),
            ));
        }
    }
    extend_database_source(&mut collection, BriefSource::WorkCore, collect_work(conn));
    extend_database_source(
        &mut collection,
        BriefSource::GoalRuntime,
        collect_goal_runtime(conn),
    );

    match collect_sessions(data_dir) {
        Ok(items) => {
            let revision = source_revision(&items, BriefSource::Conversation);
            collection.items.extend(items);
            collection
                .health
                .push(SourceHealth::ok(BriefSource::Conversation, revision));
        }
        Err(error) => collection.health.push(SourceHealth::error(
            BriefSource::Conversation,
            stable_error_code(&error),
        )),
    }
    match collect_dream(data_dir) {
        Ok(items) => {
            let revision = source_revision(&items, BriefSource::Dream);
            collection.items.extend(items);
            collection
                .health
                .push(SourceHealth::ok(BriefSource::Dream, revision));
        }
        Err(error) => collection.health.push(SourceHealth::error(
            BriefSource::Dream,
            stable_error_code(&error),
        )),
    }
    collection
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> DateContext {
        DateContext {
            local_date: "2026-08-11".into(),
            utc_offset_minutes: 480,
        }
    }

    #[test]
    fn missing_tables_are_reported_as_source_errors_not_empty_success() {
        let conn = Connection::open_in_memory().unwrap();
        let temp = std::env::temp_dir().join(format!("bob-brief-{}", ulid::Ulid::new()));
        fs::create_dir_all(&temp).unwrap();
        let collection = collect(&conn, &temp, &context());
        assert!(collection.health.iter().any(|health| {
            health.source == BriefSource::Calendar
                && health.state == super::super::models::SourceState::Error
        }));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn calendar_and_todo_are_distinct_and_only_today_events_are_included() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE events (
                id TEXT PRIMARY KEY, title TEXT, type TEXT, status TEXT, date TEXT,
                start_time TEXT, description TEXT, created_at INTEGER, updated_at INTEGER
             );
             INSERT INTO events VALUES
               ('event_today','Meeting','event','pending','2026-08-11','09:00','',1,1),
               ('event_later','Later','event','pending','2026-08-12','09:00','',1,1),
               ('todo_due','Submit','todo','pending','2026-08-11',NULL,'',1,2);",
        )
        .unwrap();
        let items = collect_calendar_and_todos(&conn, &context()).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items
            .iter()
            .any(|item| item.source == BriefSource::Calendar));
        let todo = items
            .iter()
            .find(|item| item.source == BriefSource::Todo)
            .unwrap();
        assert!(todo.requires_attention);
        assert_eq!(todo.kind, BriefItemKind::Due);
    }
}
