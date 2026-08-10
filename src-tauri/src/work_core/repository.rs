use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde_json::{json, Value};

use super::models::{
    validate_object_payload, validate_project_id, validate_status, validate_title,
    CreateProjectInput, CreateRelationInput, CreateWorkObjectInput, DeleteWorkObjectInput,
    ProjectAggregate, UpdateWorkStatusInput, WorkEvent, WorkObject, WorkObjectKind, WorkProject,
    WorkRelation, WORK_SCHEMA_VERSION,
};

const PROJECT_OPERATION: &str = "work.project.create";
const OBJECT_OPERATION: &str = "work.object.create";
const STATUS_OPERATION: &str = "work.object.status";
const DELETE_OPERATION: &str = "work.object.delete";
const RELATION_OPERATION: &str = "work.relation.create";

pub fn init_work_core_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS work_projects (
            id TEXT PRIMARY KEY,
            schema_version INTEGER NOT NULL,
            title TEXT NOT NULL,
            mission TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'active',
            current_phase TEXT,
            summary TEXT,
            source_ref TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            revision INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_work_projects_status
            ON work_projects(status, updated_at);

        CREATE TABLE IF NOT EXISTS work_objects (
            id TEXT PRIMARY KEY,
            schema_version INTEGER NOT NULL,
            kind TEXT NOT NULL,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            title TEXT NOT NULL,
            status TEXT NOT NULL,
            description TEXT,
            data_json TEXT NOT NULL DEFAULT '{}',
            source_capture_id TEXT,
            revision INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER,
            FOREIGN KEY (project_id) REFERENCES work_projects(id),
            FOREIGN KEY (parent_id) REFERENCES work_objects(id)
        );
        CREATE INDEX IF NOT EXISTS idx_work_objects_project
            ON work_objects(project_id, kind, status, updated_at);
        CREATE INDEX IF NOT EXISTS idx_work_objects_parent
            ON work_objects(parent_id);
        CREATE INDEX IF NOT EXISTS idx_work_objects_capture
            ON work_objects(source_capture_id);

        CREATE TABLE IF NOT EXISTS work_relations (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            source_id TEXT NOT NULL,
            target_id TEXT NOT NULL,
            relation TEXT NOT NULL,
            evidence_ref TEXT,
            confidence REAL NOT NULL DEFAULT 1.0,
            created_at INTEGER NOT NULL,
            deleted_at INTEGER,
            FOREIGN KEY (project_id) REFERENCES work_projects(id),
            UNIQUE(source_id, target_id, relation)
        );
        CREATE INDEX IF NOT EXISTS idx_work_relations_project
            ON work_relations(project_id, relation);

        CREATE TABLE IF NOT EXISTS work_events (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            object_id TEXT,
            event_type TEXT NOT NULL,
            actor TEXT NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            idempotency_key TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (project_id) REFERENCES work_projects(id)
        );
        CREATE INDEX IF NOT EXISTS idx_work_events_project
            ON work_events(project_id, created_at);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_work_events_idempotency
            ON work_events(idempotency_key) WHERE idempotency_key IS NOT NULL;

        CREATE TABLE IF NOT EXISTS work_idempotency (
            idempotency_key TEXT PRIMARY KEY,
            operation TEXT NOT NULL,
            result_object_id TEXT NOT NULL,
            response_json TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        ",
    )
    .map_err(|error| format!("初始化 Work Core 数据表失败: {error}"))?;
    Ok(())
}

fn now_ms() -> i64 {
    crate::now_ms() as i64
}

fn actor_or_bob(actor: Option<&str>) -> String {
    let actor = actor.map(str::trim).unwrap_or("");
    if actor.is_empty() {
        "bob".into()
    } else {
        actor.chars().take(100).collect()
    }
}

fn require_idempotency_key(key: &str) -> Result<(), String> {
    let length = key.trim().chars().count();
    if length == 0 {
        Err("idempotencyKey 不能为空".into())
    } else if length > 200 {
        Err("idempotencyKey 不能超过 200 个字符".into())
    } else {
        Ok(())
    }
}

fn deserialize_json(raw: String, label: &str) -> Result<Value, rusqlite::Error> {
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            raw.len(),
            rusqlite::types::Type::Text,
            format!("{label} JSON 无效: {error}").into(),
        )
    })
}

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<WorkProject> {
    let metadata_raw: String = row.get(8)?;
    Ok(WorkProject {
        id: row.get(0)?,
        schema_version: row.get::<_, i64>(1)? as u32,
        title: row.get(2)?,
        mission: row.get(3)?,
        status: row.get(4)?,
        current_phase: row.get(5)?,
        summary: row.get(6)?,
        source_ref: row.get(7)?,
        metadata: deserialize_json(metadata_raw, "project metadata")?,
        revision: row.get::<_, i64>(9)? as u64,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        deleted_at: row.get(12)?,
    })
}

fn object_from_row(row: &Row<'_>) -> rusqlite::Result<WorkObject> {
    let kind_raw: String = row.get(2)?;
    let kind = WorkObjectKind::parse(&kind_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            kind_raw.len(),
            rusqlite::types::Type::Text,
            format!("未知工作对象类型: {kind_raw}").into(),
        )
    })?;
    let data_raw: String = row.get(8)?;
    Ok(WorkObject {
        id: row.get(0)?,
        schema_version: row.get::<_, i64>(1)? as u32,
        kind,
        project_id: row.get(3)?,
        parent_id: row.get(4)?,
        title: row.get(5)?,
        status: row.get(6)?,
        description: row.get(7)?,
        data: deserialize_json(data_raw, "work object data")?,
        source_capture_id: row.get(9)?,
        revision: row.get::<_, i64>(10)? as u64,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        deleted_at: row.get(13)?,
    })
}

fn read_receipt<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    key: &str,
    operation: &str,
) -> Result<Option<T>, String> {
    let receipt = conn
        .query_row(
            "SELECT operation, response_json FROM work_idempotency WHERE idempotency_key = ?1",
            params![key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match receipt {
        None => Ok(None),
        Some((stored_operation, _)) if stored_operation != operation => Err(format!(
            "idempotencyKey 已被其他操作使用: {stored_operation}"
        )),
        Some((_, response)) => serde_json::from_str(&response)
            .map(Some)
            .map_err(|error| format!("幂等回执损坏: {error}")),
    }
}

fn save_receipt<T: serde::Serialize>(
    tx: &Transaction<'_>,
    key: &str,
    operation: &str,
    object_id: &str,
    response: &T,
    created_at: i64,
) -> Result<(), String> {
    let raw = serde_json::to_string(response).map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO work_idempotency (idempotency_key, operation, result_object_id, response_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![key, operation, object_id, raw, created_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn append_event(
    tx: &Transaction<'_>,
    project_id: &str,
    object_id: Option<&str>,
    event_type: &str,
    actor: &str,
    payload: &Value,
    idempotency_key: Option<&str>,
    created_at: i64,
) -> Result<(), String> {
    let event_id = format!("work_event_{}", ulid::Ulid::new());
    let raw = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO work_events (id, project_id, object_id, event_type, actor, payload_json, idempotency_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![event_id, project_id, object_id, event_type, actor, raw, idempotency_key, created_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn touch_project(
    tx: &Transaction<'_>,
    project_id: &str,
    now: i64,
) -> Result<(), String> {
    let changed = tx
        .execute(
            "UPDATE work_projects SET revision = revision + 1, updated_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            params![project_id, now],
        )
        .map_err(|error| error.to_string())?;
    if changed == 1 {
        Ok(())
    } else {
        Err(format!("活动项目不存在: {project_id}"))
    }
}

fn normalize_object(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        json!({})
    }
}

fn default_status(kind: WorkObjectKind) -> &'static str {
    match kind {
        WorkObjectKind::Decision | WorkObjectKind::Evidence => "accepted",
        WorkObjectKind::Milestone | WorkObjectKind::Task | WorkObjectKind::Change => "pending",
        _ => "active",
    }
}

pub fn get_project(conn: &Connection, project_id: &str) -> Result<Option<WorkProject>, String> {
    conn.query_row(
        "SELECT id, schema_version, title, mission, status, current_phase, summary, source_ref, metadata_json, revision, created_at, updated_at, deleted_at FROM work_projects WHERE id = ?1",
        params![project_id],
        project_from_row,
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn list_projects(conn: &Connection) -> Result<Vec<WorkProject>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, schema_version, title, mission, status, current_phase, summary, source_ref, metadata_json, revision, created_at, updated_at, deleted_at FROM work_projects WHERE deleted_at IS NULL ORDER BY updated_at DESC, id DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], project_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn get_object(conn: &Connection, object_id: &str) -> Result<Option<WorkObject>, String> {
    conn.query_row(
        "SELECT id, schema_version, kind, project_id, parent_id, title, status, description, data_json, source_capture_id, revision, created_at, updated_at, deleted_at FROM work_objects WHERE id = ?1",
        params![object_id],
        object_from_row,
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn create_project(
    conn: &mut Connection,
    input: CreateProjectInput,
) -> Result<WorkProject, String> {
    validate_title(&input.title)?;
    require_idempotency_key(&input.idempotency_key)?;
    if let Some(existing) = read_receipt(conn, &input.idempotency_key, PROJECT_OPERATION)? {
        return Ok(existing);
    }
    let project_id = input
        .project_id
        .unwrap_or_else(|| format!("project_{}", ulid::Ulid::new()));
    validate_project_id(&project_id)?;
    let metadata = normalize_object(input.metadata);
    let now = now_ms();
    let project = WorkProject {
        schema_version: WORK_SCHEMA_VERSION,
        id: project_id.clone(),
        title: input.title.trim().to_string(),
        mission: input.mission.trim().to_string(),
        status: "active".into(),
        current_phase: input.current_phase,
        summary: input.summary,
        source_ref: input.source_ref,
        metadata,
        revision: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    let actor = actor_or_bob(input.actor.as_deref());
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO work_projects (id, schema_version, title, mission, status, current_phase, summary, source_ref, metadata_json, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            &project.id,
            project.schema_version,
            &project.title,
            &project.mission,
            &project.status,
            &project.current_phase,
            &project.summary,
            &project.source_ref,
            serde_json::to_string(&project.metadata).map_err(|error| error.to_string())?,
            project.revision as i64,
            project.created_at,
            project.updated_at,
        ],
    )
    .map_err(|error| error.to_string())?;
    append_event(
        &tx,
        &project.id,
        Some(&project.id),
        "project.created",
        &actor,
        &json!({ "title": project.title, "revision": project.revision }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        PROJECT_OPERATION,
        &project.id,
        &project,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(project)
}

pub fn create_object(
    conn: &mut Connection,
    input: CreateWorkObjectInput,
) -> Result<WorkObject, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let object = create_object_in_tx(&tx, input)?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(object)
}

pub(crate) fn create_object_in_tx(
    tx: &Transaction<'_>,
    input: CreateWorkObjectInput,
) -> Result<WorkObject, String> {
    validate_project_id(&input.project_id)?;
    validate_title(&input.title)?;
    require_idempotency_key(&input.idempotency_key)?;
    let data = normalize_object(input.data);
    validate_object_payload(input.kind, &data)?;
    let status = input
        .status
        .unwrap_or_else(|| default_status(input.kind).to_string());
    validate_status(&status)?;
    if let Some(existing) = read_receipt(tx, &input.idempotency_key, OBJECT_OPERATION)? {
        return Ok(existing);
    }
    let project = get_project(tx, &input.project_id)?
        .ok_or_else(|| format!("项目不存在: {}", input.project_id))?;
    if project.deleted_at.is_some() {
        return Err("不能向已删除项目添加工作对象".into());
    }
    if let Some(parent_id) = input.parent_id.as_deref() {
        let parent =
            get_object(tx, parent_id)?.ok_or_else(|| format!("父对象不存在: {parent_id}"))?;
        if parent.project_id != input.project_id || parent.deleted_at.is_some() {
            return Err("父对象必须属于同一活动项目".into());
        }
    }
    let now = now_ms();
    let object = WorkObject {
        schema_version: WORK_SCHEMA_VERSION,
        id: format!("{}_{}", input.kind.id_prefix(), ulid::Ulid::new()),
        kind: input.kind,
        project_id: input.project_id,
        parent_id: input.parent_id,
        title: input.title.trim().to_string(),
        status,
        description: input.description,
        data,
        source_capture_id: input.source_capture_id,
        revision: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    let actor = actor_or_bob(input.actor.as_deref());
    tx.execute(
        "INSERT INTO work_objects (id, schema_version, kind, project_id, parent_id, title, status, description, data_json, source_capture_id, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            &object.id,
            object.schema_version,
            object.kind.as_str(),
            &object.project_id,
            &object.parent_id,
            &object.title,
            &object.status,
            &object.description,
            serde_json::to_string(&object.data).map_err(|error| error.to_string())?,
            &object.source_capture_id,
            object.revision as i64,
            object.created_at,
            object.updated_at,
        ],
    )
    .map_err(|error| error.to_string())?;
    touch_project(tx, &object.project_id, now)?;
    append_event(
        tx,
        &object.project_id,
        Some(&object.id),
        &format!("{}.created", object.kind.as_str()),
        &actor,
        &json!({ "title": object.title, "status": object.status, "revision": object.revision }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        tx,
        &input.idempotency_key,
        OBJECT_OPERATION,
        &object.id,
        &object,
        now,
    )?;
    Ok(object)
}

fn transition_allowed(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    match from {
        "draft" => matches!(to, "active" | "pending" | "cancelled"),
        "active" => matches!(
            to,
            "on_hold" | "done" | "archived" | "cancelled" | "superseded"
        ),
        "pending" => matches!(to, "ready" | "running" | "blocked" | "done" | "cancelled"),
        "ready" => matches!(to, "running" | "blocked" | "cancelled"),
        "running" => matches!(
            to,
            "blocked" | "needs_review" | "done" | "failed" | "cancelled"
        ),
        "blocked" => matches!(to, "ready" | "running" | "failed" | "cancelled"),
        "needs_review" => matches!(to, "accepted" | "running" | "failed"),
        "accepted" => matches!(to, "superseded" | "archived"),
        "failed" => matches!(to, "ready" | "running" | "cancelled"),
        "on_hold" => matches!(to, "active" | "archived" | "cancelled"),
        "done" => matches!(to, "archived" | "superseded"),
        "archived" | "cancelled" | "superseded" => false,
        _ => false,
    }
}

pub fn update_object_status(
    conn: &mut Connection,
    input: UpdateWorkStatusInput,
) -> Result<WorkObject, String> {
    validate_status(&input.status)?;
    require_idempotency_key(&input.idempotency_key)?;
    if let Some(existing) = read_receipt(conn, &input.idempotency_key, STATUS_OPERATION)? {
        return Ok(existing);
    }
    let current = get_object(conn, &input.object_id)?
        .ok_or_else(|| format!("工作对象不存在: {}", input.object_id))?;
    if current.deleted_at.is_some() {
        return Err("不能更新已删除工作对象".into());
    }
    if current.revision != input.expected_revision {
        return Err(format!(
            "revision 冲突：期望 {}，当前 {}",
            input.expected_revision, current.revision
        ));
    }
    if !transition_allowed(&current.status, &input.status) {
        return Err(format!(
            "不允许的状态转换: {} -> {}",
            current.status, input.status
        ));
    }
    let now = now_ms();
    let mut updated = current.clone();
    updated.status = input.status;
    updated.revision += 1;
    updated.updated_at = now;
    let actor = actor_or_bob(input.actor.as_deref());
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let changed = tx
        .execute(
            "UPDATE work_objects SET status = ?2, revision = ?3, updated_at = ?4 WHERE id = ?1 AND revision = ?5 AND deleted_at IS NULL",
            params![
                &updated.id,
                &updated.status,
                updated.revision as i64,
                now,
                current.revision as i64
            ],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("revision 冲突：对象已被其他操作更新".into());
    }
    touch_project(&tx, &updated.project_id, now)?;
    append_event(
        &tx,
        &updated.project_id,
        Some(&updated.id),
        &format!("{}.status_changed", updated.kind.as_str()),
        &actor,
        &json!({ "from": current.status, "to": updated.status, "revision": updated.revision }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        STATUS_OPERATION,
        &updated.id,
        &updated,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(updated)
}

pub fn delete_object(
    conn: &mut Connection,
    input: DeleteWorkObjectInput,
) -> Result<WorkObject, String> {
    require_idempotency_key(&input.idempotency_key)?;
    if let Some(existing) = read_receipt(conn, &input.idempotency_key, DELETE_OPERATION)? {
        return Ok(existing);
    }
    let current = get_object(conn, &input.object_id)?
        .ok_or_else(|| format!("工作对象不存在: {}", input.object_id))?;
    if current.deleted_at.is_some() {
        return Err("工作对象已经删除".into());
    }
    if current.revision != input.expected_revision {
        return Err(format!(
            "revision 冲突：期望 {}，当前 {}",
            input.expected_revision, current.revision
        ));
    }

    let now = now_ms();
    let mut deleted = current.clone();
    deleted.deleted_at = Some(now);
    deleted.updated_at = now;
    deleted.revision += 1;
    let actor = actor_or_bob(input.actor.as_deref());
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let changed = tx
        .execute(
            "UPDATE work_objects SET deleted_at = ?2, updated_at = ?2, revision = ?3 WHERE id = ?1 AND revision = ?4 AND deleted_at IS NULL",
            params![
                &deleted.id,
                now,
                deleted.revision as i64,
                current.revision as i64
            ],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("revision 冲突：对象已被其他操作更新".into());
    }
    touch_project(&tx, &deleted.project_id, now)?;
    append_event(
        &tx,
        &deleted.project_id,
        Some(&deleted.id),
        &format!("{}.deleted", deleted.kind.as_str()),
        &actor,
        &json!({ "reason": input.reason, "revision": deleted.revision }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        DELETE_OPERATION,
        &deleted.id,
        &deleted,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(deleted)
}

pub fn list_project_events(
    conn: &Connection,
    project_id: &str,
    limit: usize,
) -> Result<Vec<WorkEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, object_id, event_type, actor, payload_json, idempotency_key, created_at FROM work_events WHERE project_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![project_id, limit.clamp(1, 200) as i64], |row| {
            let payload_raw: String = row.get(5)?;
            Ok(WorkEvent {
                id: row.get(0)?,
                project_id: row.get(1)?,
                object_id: row.get(2)?,
                event_type: row.get(3)?,
                actor: row.get(4)?,
                payload: deserialize_json(payload_raw, "work event payload")?,
                idempotency_key: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn list_project_objects(conn: &Connection, project_id: &str) -> Result<Vec<WorkObject>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, schema_version, kind, project_id, parent_id, title, status, description, data_json, source_capture_id, revision, created_at, updated_at, deleted_at FROM work_objects WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY updated_at DESC, id DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![project_id], object_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn get_project_aggregate(
    conn: &Connection,
    project_id: &str,
) -> Result<ProjectAggregate, String> {
    let project = get_project(conn, project_id)?
        .filter(|project| project.deleted_at.is_none())
        .ok_or_else(|| format!("活动项目不存在: {project_id}"))?;
    let mut aggregate = ProjectAggregate {
        project,
        responsibilities: Vec::new(),
        goals: Vec::new(),
        milestones: Vec::new(),
        tasks: Vec::new(),
        decisions: Vec::new(),
        artifacts: Vec::new(),
        evidence: Vec::new(),
        risks: Vec::new(),
        changes: Vec::new(),
        commitments: Vec::new(),
        recent_events: list_project_events(conn, project_id, 50)?,
    };
    for object in list_project_objects(conn, project_id)? {
        match object.kind {
            WorkObjectKind::Responsibility => aggregate.responsibilities.push(object),
            WorkObjectKind::Goal => aggregate.goals.push(object),
            WorkObjectKind::Milestone => aggregate.milestones.push(object),
            WorkObjectKind::Task => aggregate.tasks.push(object),
            WorkObjectKind::Decision => aggregate.decisions.push(object),
            WorkObjectKind::Artifact => aggregate.artifacts.push(object),
            WorkObjectKind::Evidence => aggregate.evidence.push(object),
            WorkObjectKind::Risk => aggregate.risks.push(object),
            WorkObjectKind::Change => aggregate.changes.push(object),
            WorkObjectKind::Commitment => aggregate.commitments.push(object),
        }
    }
    Ok(aggregate)
}

fn relation_allowed(value: &str) -> bool {
    matches!(
        value,
        "belongs_to"
            | "owned_by"
            | "depends_on"
            | "blocks"
            | "supports"
            | "contradicts"
            | "supersedes"
            | "derived_from"
            | "decided_in"
            | "affected_by"
            | "due_at"
            | "assigned_to"
            | "produced_by"
            | "related_to"
    )
}

fn entity_belongs_to_project(
    conn: &Connection,
    entity_id: &str,
    project_id: &str,
) -> Result<bool, String> {
    if entity_id == project_id {
        return Ok(get_project(conn, project_id)?.is_some());
    }
    Ok(get_object(conn, entity_id)?
        .filter(|object| object.deleted_at.is_none())
        .map(|object| object.project_id == project_id)
        .unwrap_or(false))
}

pub fn create_relation(
    conn: &mut Connection,
    input: CreateRelationInput,
) -> Result<WorkRelation, String> {
    validate_project_id(&input.project_id)?;
    require_idempotency_key(&input.idempotency_key)?;
    if !relation_allowed(input.relation.trim()) {
        return Err(format!("不支持的工作关系: {}", input.relation));
    }
    if !(0.0..=1.0).contains(&input.confidence) {
        return Err("关系 confidence 必须在 0 到 1 之间".into());
    }
    if let Some(existing) = read_receipt(conn, &input.idempotency_key, RELATION_OPERATION)? {
        return Ok(existing);
    }
    if !entity_belongs_to_project(conn, &input.source_id, &input.project_id)?
        || !entity_belongs_to_project(conn, &input.target_id, &input.project_id)?
    {
        return Err("关系两端必须属于同一活动项目".into());
    }
    let now = now_ms();
    let relation = WorkRelation {
        id: format!("relation_{}", ulid::Ulid::new()),
        project_id: input.project_id,
        source_id: input.source_id,
        target_id: input.target_id,
        relation: input.relation.trim().to_string(),
        evidence_ref: input.evidence_ref,
        confidence: input.confidence,
        created_at: now,
        deleted_at: None,
    };
    let actor = actor_or_bob(input.actor.as_deref());
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO work_relations (id, project_id, source_id, target_id, relation, evidence_ref, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            &relation.id,
            &relation.project_id,
            &relation.source_id,
            &relation.target_id,
            &relation.relation,
            &relation.evidence_ref,
            relation.confidence,
            relation.created_at,
        ],
    )
    .map_err(|error| error.to_string())?;
    touch_project(&tx, &relation.project_id, now)?;
    append_event(
        &tx,
        &relation.project_id,
        Some(&relation.source_id),
        "relation.created",
        &actor,
        &json!({
            "relationId": relation.id,
            "sourceId": relation.source_id,
            "targetId": relation.target_id,
            "relation": relation.relation,
            "confidence": relation.confidence
        }),
        Some(&input.idempotency_key),
        now,
    )?;
    save_receipt(
        &tx,
        &input.idempotency_key,
        RELATION_OPERATION,
        &relation.id,
        &relation,
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(relation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_core::models::{CreateProjectInput, CreateWorkObjectInput};

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        init_work_core_tables(&conn).unwrap();
        conn
    }

    fn project_input(key: &str) -> CreateProjectInput {
        CreateProjectInput {
            project_id: None,
            title: "商场改造".into(),
            mission: "完成开业前改造".into(),
            current_phase: Some("方案确认".into()),
            summary: None,
            source_ref: None,
            metadata: json!({}),
            actor: Some("user".into()),
            idempotency_key: key.into(),
        }
    }

    fn object_input(
        kind: WorkObjectKind,
        project_id: &str,
        key: &str,
        data: Value,
    ) -> CreateWorkObjectInput {
        CreateWorkObjectInput {
            kind,
            project_id: project_id.into(),
            parent_id: None,
            title: format!("{} 示例", kind.as_str()),
            status: None,
            description: None,
            data,
            source_capture_id: None,
            actor: Some("user".into()),
            idempotency_key: key.into(),
        }
    }

    #[test]
    fn project_creation_is_idempotent_and_writes_one_event() {
        let mut conn = database();
        let first = create_project(&mut conn, project_input("project-key")).unwrap();
        let second = create_project(&mut conn, project_input("project-key")).unwrap();
        assert_eq!(first, second);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM work_projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(list_project_events(&conn, &first.id, 10).unwrap().len(), 1);
    }

    #[test]
    fn idempotency_key_cannot_be_reused_for_another_operation() {
        let mut conn = database();
        let project = create_project(&mut conn, project_input("shared-key")).unwrap();
        let result = create_object(
            &mut conn,
            object_input(WorkObjectKind::Task, &project.id, "shared-key", json!({})),
        );
        assert!(result.unwrap_err().contains("其他操作"));
    }

    #[test]
    fn decision_requires_reason_before_transaction_starts() {
        let mut conn = database();
        let project = create_project(&mut conn, project_input("project-key")).unwrap();
        let result = create_object(
            &mut conn,
            object_input(
                WorkObjectKind::Decision,
                &project.id,
                "decision-key",
                json!({ "decision": "采用方案 A" }),
            ),
        );
        assert!(result.unwrap_err().contains("reason"));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM work_objects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn object_parent_must_belong_to_same_project() {
        let mut conn = database();
        let first = create_project(&mut conn, project_input("p1")).unwrap();
        let mut second_input = project_input("p2");
        second_input.title = "另一个项目".into();
        let second = create_project(&mut conn, second_input).unwrap();
        let parent = create_object(
            &mut conn,
            object_input(
                WorkObjectKind::Goal,
                &first.id,
                "goal",
                json!({ "outcome": "开业" }),
            ),
        )
        .unwrap();
        let mut child = object_input(WorkObjectKind::Task, &second.id, "task", json!({}));
        child.parent_id = Some(parent.id);
        assert!(create_object(&mut conn, child)
            .unwrap_err()
            .contains("同一活动项目"));
    }

    #[test]
    fn status_update_uses_revision_and_is_idempotent() {
        let mut conn = database();
        let project = create_project(&mut conn, project_input("project-key")).unwrap();
        let task = create_object(
            &mut conn,
            object_input(WorkObjectKind::Task, &project.id, "task-key", json!({})),
        )
        .unwrap();
        let input = UpdateWorkStatusInput {
            object_id: task.id.clone(),
            status: "ready".into(),
            expected_revision: 1,
            actor: Some("bob".into()),
            idempotency_key: "status-key".into(),
        };
        let first = update_object_status(&mut conn, input.clone()).unwrap();
        let second = update_object_status(&mut conn, input).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.revision, 2);
        assert_eq!(first.status, "ready");
        assert_eq!(
            list_project_events(&conn, &project.id, 10).unwrap().len(),
            3
        );
    }

    #[test]
    fn stale_revision_cannot_overwrite_newer_state() {
        let mut conn = database();
        let project = create_project(&mut conn, project_input("project-key")).unwrap();
        let task = create_object(
            &mut conn,
            object_input(WorkObjectKind::Task, &project.id, "task-key", json!({})),
        )
        .unwrap();
        update_object_status(
            &mut conn,
            UpdateWorkStatusInput {
                object_id: task.id.clone(),
                status: "ready".into(),
                expected_revision: 1,
                actor: None,
                idempotency_key: "first-update".into(),
            },
        )
        .unwrap();
        let error = update_object_status(
            &mut conn,
            UpdateWorkStatusInput {
                object_id: task.id,
                status: "cancelled".into(),
                expected_revision: 1,
                actor: None,
                idempotency_key: "stale-update".into(),
            },
        )
        .unwrap_err();
        assert!(error.contains("revision 冲突"));
    }

    #[test]
    fn aggregate_groups_objects_without_chat_context() {
        let mut conn = database();
        let project = create_project(&mut conn, project_input("project-key")).unwrap();
        create_object(
            &mut conn,
            object_input(
                WorkObjectKind::Goal,
                &project.id,
                "goal-key",
                json!({ "outcome": "完成改造" }),
            ),
        )
        .unwrap();
        create_object(
            &mut conn,
            object_input(
                WorkObjectKind::Decision,
                &project.id,
                "decision-key",
                json!({ "decision": "保留入口", "reason": "降低风险" }),
            ),
        )
        .unwrap();
        let aggregate = get_project_aggregate(&conn, &project.id).unwrap();
        assert_eq!(aggregate.project.id, project.id);
        assert_eq!(aggregate.project.revision, 3);
        assert_eq!(aggregate.goals.len(), 1);
        assert_eq!(aggregate.decisions.len(), 1);
        assert_eq!(aggregate.recent_events.len(), 3);
    }

    #[test]
    fn relation_is_idempotent_and_cannot_cross_projects() {
        let mut conn = database();
        let first = create_project(&mut conn, project_input("p1")).unwrap();
        let mut second_input = project_input("p2");
        second_input.title = "另一个项目".into();
        let second = create_project(&mut conn, second_input).unwrap();
        let goal = create_object(
            &mut conn,
            object_input(
                WorkObjectKind::Goal,
                &first.id,
                "goal",
                json!({ "outcome": "开业" }),
            ),
        )
        .unwrap();
        let task = create_object(
            &mut conn,
            object_input(WorkObjectKind::Task, &first.id, "task", json!({})),
        )
        .unwrap();
        let input = CreateRelationInput {
            project_id: first.id.clone(),
            source_id: task.id.clone(),
            target_id: goal.id,
            relation: "belongs_to".into(),
            evidence_ref: None,
            confidence: 1.0,
            actor: None,
            idempotency_key: "relation-key".into(),
        };
        let created = create_relation(&mut conn, input.clone()).unwrap();
        assert_eq!(created, create_relation(&mut conn, input).unwrap());

        let cross_project = CreateRelationInput {
            project_id: second.id.clone(),
            source_id: task.id,
            target_id: second.id,
            relation: "belongs_to".into(),
            evidence_ref: None,
            confidence: 1.0,
            actor: None,
            idempotency_key: "cross-project".into(),
        };
        assert!(create_relation(&mut conn, cross_project)
            .unwrap_err()
            .contains("同一活动项目"));
    }

    #[test]
    fn projects_can_be_listed_without_chat_history() {
        let mut conn = database();
        let first = create_project(&mut conn, project_input("first-project")).unwrap();
        let mut second_input = project_input("second-project");
        second_input.title = "第二个项目".into();
        let second = create_project(&mut conn, second_input).unwrap();

        let projects = list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects.iter().any(|project| project.id == first.id));
        assert!(projects.iter().any(|project| project.id == second.id));
    }

    #[test]
    fn soft_delete_is_revision_safe_idempotent_and_auditable() {
        let mut conn = database();
        let project = create_project(&mut conn, project_input("project-key")).unwrap();
        let task = create_object(
            &mut conn,
            object_input(WorkObjectKind::Task, &project.id, "task-key", json!({})),
        )
        .unwrap();
        let input = DeleteWorkObjectInput {
            object_id: task.id.clone(),
            expected_revision: task.revision,
            reason: Some("不再需要".into()),
            actor: Some("user".into()),
            idempotency_key: "delete-key".into(),
        };

        let deleted = delete_object(&mut conn, input.clone()).unwrap();
        assert!(deleted.deleted_at.is_some());
        assert_eq!(delete_object(&mut conn, input).unwrap(), deleted);
        assert!(get_project_aggregate(&conn, &project.id)
            .unwrap()
            .tasks
            .is_empty());
        assert!(list_project_events(&conn, &project.id, 10)
            .unwrap()
            .iter()
            .any(|event| event.event_type == "task.deleted"));
    }

    #[test]
    fn project_state_survives_database_reopen_without_chat_context() {
        let root = std::env::temp_dir().join(format!("bob-work-reopen-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("work.db");
        let project_id = {
            let mut conn = Connection::open(&path).unwrap();
            init_work_core_tables(&conn).unwrap();
            let project = create_project(&mut conn, project_input("persist-project")).unwrap();
            create_object(
                &mut conn,
                object_input(
                    WorkObjectKind::Decision,
                    &project.id,
                    "persist-decision",
                    json!({ "decision": "继续现有架构", "reason": "降低迁移风险" }),
                ),
            )
            .unwrap();
            project.id
        };

        let conn = Connection::open(&path).unwrap();
        init_work_core_tables(&conn).unwrap();
        let aggregate = get_project_aggregate(&conn, &project_id).unwrap();
        assert_eq!(aggregate.decisions.len(), 1);
        assert_eq!(aggregate.decisions[0].data["reason"], "降低迁移风险");
        drop(conn);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_markdown_project_can_keep_its_stable_identity() {
        let mut conn = database();
        let mut input = project_input("markdown-project");
        input.project_id = Some("project_existing_001".into());
        input.source_ref = Some("notes/projects/existing/README.md".into());

        let project = create_project(&mut conn, input).unwrap();
        assert_eq!(project.id, "project_existing_001");
        assert_eq!(
            project.source_ref.as_deref(),
            Some("notes/projects/existing/README.md")
        );
    }
}
