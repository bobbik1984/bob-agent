use std::fs::File;
use std::io::Read;
use std::path::Path;

use md5::{Digest, Md5};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::capture::CaptureEnvelope;

use super::decision_change::{self, ChangeAnalysisInput, ExplicitImpact};
use super::models::{
    CreateWorkObjectInput, DecisionData, RejectedAlternative, WorkObject, WorkObjectKind,
    WorkProject,
};
use super::{repository, snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLinkIntent {
    WorkTask,
    Decision,
    Todo,
    Event,
    Note,
    Source,
    Knowledge,
    Artifact,
    Meeting,
    Change,
    Commitment,
}

impl ProjectLinkIntent {
    fn as_str(self) -> &'static str {
        match self {
            Self::WorkTask => "work_task",
            Self::Decision => "decision",
            Self::Todo => "todo",
            Self::Event => "event",
            Self::Note => "note",
            Self::Source => "source",
            Self::Knowledge => "knowledge",
            Self::Artifact => "artifact",
            Self::Meeting => "meeting",
            Self::Change => "change",
            Self::Commitment => "commitment",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "work_task" => Self::WorkTask,
            "decision" => Self::Decision,
            "todo" => Self::Todo,
            "event" => Self::Event,
            "note" => Self::Note,
            "source" => Self::Source,
            "knowledge" => Self::Knowledge,
            "artifact" => Self::Artifact,
            "meeting" => Self::Meeting,
            "change" => Self::Change,
            "commitment" => Self::Commitment,
            _ => return None,
        })
    }

    fn is_work_only(self) -> bool {
        matches!(
            self,
            Self::WorkTask
                | Self::Decision
                | Self::Artifact
                | Self::Meeting
                | Self::Change
                | Self::Commitment
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLinkProposal {
    pub intent: ProjectLinkIntent,
    pub title: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_hint: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub external_kind: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

fn default_confidence() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLinkCandidate {
    pub id: String,
    pub capture_id: String,
    pub intent: ProjectLinkIntent,
    pub title: String,
    pub proposal: ProjectLinkProposal,
    pub project_hint: Option<String>,
    pub candidate_project_ids: Vec<String>,
    pub selected_project_id: Option<String>,
    pub status: String,
    pub reason_code: String,
    pub confidence: f64,
    pub resolved_object_id: Option<String>,
    pub last_error: Option<String>,
    pub retry_count: u64,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveProjectLinkInput {
    pub candidate_id: String,
    pub project_id: String,
    pub expected_revision: u64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DismissProjectLinkInput {
    pub candidate_id: String,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLinkOutcome {
    pub candidate: ProjectLinkCandidate,
    pub object: Option<WorkObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLink {
    pub id: String,
    pub project_id: String,
    pub object_id: Option<String>,
    pub external_kind: String,
    pub external_id: String,
    pub relation: String,
    pub source_capture_id: Option<String>,
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFingerprint {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub modified_at: Option<i64>,
}

pub fn init_project_link_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS project_link_candidates (
            id TEXT PRIMARY KEY,
            capture_id TEXT NOT NULL UNIQUE,
            intent TEXT NOT NULL,
            title TEXT NOT NULL,
            proposal_json TEXT NOT NULL,
            project_hint TEXT,
            candidate_project_ids_json TEXT NOT NULL DEFAULT '[]',
            selected_project_id TEXT,
            status TEXT NOT NULL,
            reason_code TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 1.0,
            resolved_object_id TEXT,
            last_error TEXT,
            retry_count INTEGER NOT NULL DEFAULT 0,
            revision INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            resolved_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_project_link_candidates_status
            ON project_link_candidates(status, updated_at);

        CREATE TABLE IF NOT EXISTS work_external_links (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            object_id TEXT,
            external_kind TEXT NOT NULL,
            external_id TEXT NOT NULL,
            relation TEXT NOT NULL,
            source_capture_id TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER,
            UNIQUE(project_id, external_kind, external_id, relation),
            FOREIGN KEY(project_id) REFERENCES work_projects(id),
            FOREIGN KEY(object_id) REFERENCES work_objects(id)
        );
        CREATE INDEX IF NOT EXISTS idx_work_external_links_external
            ON work_external_links(external_kind, external_id);
        CREATE INDEX IF NOT EXISTS idx_work_external_links_project
            ON work_external_links(project_id, relation, updated_at);
        ",
    )
    .map_err(|error| format!("初始化项目关联表失败: {error}"))?;
    Ok(())
}

fn now_ms() -> i64 {
    crate::now_ms() as i64
}

fn normalize_project_title(value: &str) -> String {
    let mut normalized = value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(ch, '-' | '_'))
        .collect::<String>();
    for suffix in ["项目", "project"] {
        if normalized.ends_with(suffix) && normalized.len() > suffix.len() {
            normalized.truncate(normalized.len() - suffix.len());
        }
    }
    normalized
}

fn active_projects(conn: &Connection) -> Result<Vec<WorkProject>, String> {
    Ok(repository::list_projects(conn)?
        .into_iter()
        .filter(|project| !matches!(project.status.as_str(), "archived" | "cancelled"))
        .collect())
}

fn resolve_project(
    conn: &Connection,
    proposal: &ProjectLinkProposal,
) -> Result<(Option<String>, Vec<String>, String), String> {
    let projects = active_projects(conn)?;
    if let Some(project_id) = proposal
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let available = projects.iter().any(|project| project.id == project_id);
        return Ok(if available {
            (
                Some(project_id.to_string()),
                vec![project_id.to_string()],
                "explicit_project_id".into(),
            )
        } else {
            (None, vec![], "project_unavailable".into())
        });
    }
    let Some(hint) = proposal
        .project_hint
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Ok((None, vec![], "project_hint_missing".into()));
    };
    let wanted = normalize_project_title(hint);
    let matches = projects
        .iter()
        .filter(|project| normalize_project_title(&project.title) == wanted)
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    Ok(match matches.as_slice() {
        [project_id] => (
            Some(project_id.clone()),
            matches,
            "unique_exact_title".into(),
        ),
        [] => (None, matches, "project_not_found".into()),
        _ => (None, matches, "ambiguous_project".into()),
    })
}

pub fn resolve_unique_project_id(
    conn: &Connection,
    proposal: &ProjectLinkProposal,
) -> Result<Option<String>, String> {
    let (project_id, _, _) = resolve_project(conn, proposal)?;
    Ok(project_id)
}

fn missing_field(proposal: &ProjectLinkProposal) -> Option<&'static str> {
    if proposal.title.trim().is_empty() {
        return Some("missing_title");
    }
    match proposal.intent {
        ProjectLinkIntent::Decision
            if proposal
                .reason
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
                && proposal
                    .metadata
                    .get("decisionData")
                    .or_else(|| proposal.metadata.get("decision"))
                    .and_then(|value| value.get("reason"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty() =>
        {
            Some("missing_decision_reason")
        }
        ProjectLinkIntent::Commitment
            if proposal
                .owner
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty() =>
        {
            Some("missing_commitment_owner")
        }
        ProjectLinkIntent::Commitment
            if proposal
                .due_at
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty() =>
        {
            Some("missing_commitment_due_at")
        }
        ProjectLinkIntent::Meeting
            if proposal
                .metadata
                .get("items")
                .and_then(Value::as_array)
                .map(|items| items.is_empty())
                .unwrap_or(true) =>
        {
            Some("missing_meeting_outcomes")
        }
        _ => None,
    }
}

fn candidate_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectLinkCandidate> {
    let intent_raw: String = row.get(2)?;
    let proposal_raw: String = row.get(4)?;
    let projects_raw: String = row.get(6)?;
    Ok(ProjectLinkCandidate {
        id: row.get(0)?,
        capture_id: row.get(1)?,
        intent: ProjectLinkIntent::parse(&intent_raw)
            .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
        title: row.get(3)?,
        proposal: serde_json::from_str(&proposal_raw).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                proposal_raw.len(),
                rusqlite::types::Type::Text,
                e.into(),
            )
        })?,
        project_hint: row.get(5)?,
        candidate_project_ids: serde_json::from_str(&projects_raw).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                projects_raw.len(),
                rusqlite::types::Type::Text,
                e.into(),
            )
        })?,
        selected_project_id: row.get(7)?,
        status: row.get(8)?,
        reason_code: row.get(9)?,
        confidence: row.get(10)?,
        resolved_object_id: row.get(11)?,
        last_error: row.get(12)?,
        retry_count: row.get::<_, i64>(13)? as u64,
        revision: row.get::<_, i64>(14)? as u64,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        resolved_at: row.get(17)?,
    })
}

const CANDIDATE_SELECT: &str = "SELECT id, capture_id, intent, title, proposal_json, project_hint, candidate_project_ids_json, selected_project_id, status, reason_code, confidence, resolved_object_id, last_error, retry_count, revision, created_at, updated_at, resolved_at FROM project_link_candidates";

pub fn get_candidate(conn: &Connection, id: &str) -> Result<ProjectLinkCandidate, String> {
    conn.query_row(
        &format!("{CANDIDATE_SELECT} WHERE id = ?1"),
        params![id],
        candidate_from_row,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "项目关联候选不存在".into())
}

pub fn list_pending(conn: &Connection, limit: usize) -> Result<Vec<ProjectLinkCandidate>, String> {
    let mut statement = conn
        .prepare(&format!(
            "{CANDIDATE_SELECT} WHERE status = 'pending' ORDER BY updated_at DESC LIMIT ?1"
        ))
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params![limit.clamp(1, 100) as i64], candidate_from_row)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn upsert_candidate(
    conn: &Connection,
    capture_id: &str,
    proposal: &ProjectLinkProposal,
    candidates: &[String],
    selected: Option<&str>,
    status: &str,
    reason_code: &str,
    object_id: Option<&str>,
    last_error: Option<&str>,
) -> Result<ProjectLinkCandidate, String> {
    let id = format!("project_link_{capture_id}");
    let now = now_ms();
    conn.execute(
        "INSERT INTO project_link_candidates (id, capture_id, intent, title, proposal_json, project_hint, candidate_project_ids_json, selected_project_id, status, reason_code, confidence, resolved_object_id, last_error, retry_count, revision, created_at, updated_at, resolved_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, CASE WHEN ?13 IS NULL THEN 0 ELSE 1 END, 1, ?14, ?14, CASE WHEN ?9 = 'resolved' THEN ?14 ELSE NULL END)
         ON CONFLICT(capture_id) DO UPDATE SET proposal_json=excluded.proposal_json, project_hint=excluded.project_hint, candidate_project_ids_json=excluded.candidate_project_ids_json, selected_project_id=excluded.selected_project_id, status=excluded.status, reason_code=excluded.reason_code, confidence=excluded.confidence, resolved_object_id=COALESCE(excluded.resolved_object_id, project_link_candidates.resolved_object_id), last_error=excluded.last_error, retry_count=project_link_candidates.retry_count + CASE WHEN excluded.last_error IS NULL THEN 0 ELSE 1 END, revision=project_link_candidates.revision + 1, updated_at=excluded.updated_at, resolved_at=excluded.resolved_at",
        params![id, capture_id, proposal.intent.as_str(), proposal.title.trim(), serde_json::to_string(proposal).map_err(|e| e.to_string())?, proposal.project_hint, serde_json::to_string(candidates).map_err(|e| e.to_string())?, selected, status, reason_code, proposal.confidence, object_id, last_error, now],
    ).map_err(|e| e.to_string())?;
    get_candidate(conn, &id)
}

fn create_external_link_in_tx(
    tx: &Transaction<'_>,
    project_id: &str,
    object_id: Option<&str>,
    proposal: &ProjectLinkProposal,
    capture_id: &str,
) -> Result<Option<ExternalLink>, String> {
    let Some(kind) = proposal
        .external_kind
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };
    let Some(external_id) = proposal
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };
    let relation = match proposal.intent {
        ProjectLinkIntent::Todo | ProjectLinkIntent::Event => "execution_source",
        ProjectLinkIntent::Note => "owned_note",
        ProjectLinkIntent::Source | ProjectLinkIntent::Knowledge => "knowledge_reference",
        ProjectLinkIntent::Artifact => "artifact_source",
        _ => "derived_from",
    };
    if relation == "owned_note" {
        let existing_owner: Option<String> = tx
            .query_row(
                "SELECT project_id FROM work_external_links WHERE external_kind=?1 AND external_id=?2 AND relation='owned_note' AND deleted_at IS NULL LIMIT 1",
                params![kind, external_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if existing_owner
            .as_deref()
            .is_some_and(|owner| owner != project_id)
        {
            return Err("note_already_owned_by_another_project".into());
        }
    }
    let id = format!(
        "external_link_{:x}",
        Md5::digest(format!("{project_id}:{kind}:{external_id}:{relation}"))
    );
    let now = now_ms();
    tx.execute(
        "INSERT INTO work_external_links (id, project_id, object_id, external_kind, external_id, relation, source_capture_id, metadata_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
         ON CONFLICT(project_id, external_kind, external_id, relation) DO UPDATE SET object_id=COALESCE(excluded.object_id, work_external_links.object_id), metadata_json=excluded.metadata_json, updated_at=excluded.updated_at, deleted_at=NULL",
        params![id, project_id, object_id, kind, external_id, relation, capture_id, serde_json::to_string(&proposal.metadata).map_err(|e| e.to_string())?, now],
    ).map_err(|e| e.to_string())?;
    let event_key = if relation == "artifact_source" {
        let fingerprint = proposal
            .metadata
            .get("hash")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        format!("external-link:{id}:{fingerprint}")
    } else {
        format!("external-link:{id}")
    };
    let event_exists: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM work_events WHERE idempotency_key=?1",
            params![event_key],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if event_exists == 0 {
        repository::touch_project(tx, project_id, now)?;
        repository::append_event(
            tx,
            project_id,
            object_id,
            "external_link.recorded",
            "bob",
            &json!({"externalKind": kind, "externalId": external_id, "relation": relation}),
            Some(&event_key),
            now,
        )?;
    }
    Ok(Some(ExternalLink {
        id,
        project_id: project_id.into(),
        object_id: object_id.map(str::to_string),
        external_kind: kind.into(),
        external_id: external_id.into(),
        relation: relation.into(),
        source_capture_id: Some(capture_id.into()),
        metadata: proposal.metadata.clone(),
        created_at: now,
        updated_at: now,
    }))
}

fn decision_data(proposal: &ProjectLinkProposal) -> Result<Value, String> {
    let source = proposal
        .metadata
        .get("decisionData")
        .or_else(|| proposal.metadata.get("decision"))
        .filter(|value| value.is_object());
    let mut value = source.cloned().unwrap_or_else(|| json!({}));
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Decision 数据必须是 object".to_string())?;
    object
        .entry("decision")
        .or_insert_with(|| json!(proposal.title));
    if let Some(reason) = proposal.reason.as_deref() {
        object.insert("reason".into(), json!(reason));
    }
    object.entry("reason").or_insert(Value::Null);
    DecisionData::from_value(&value)?.into_value()
}

fn object_input(
    project_id: &str,
    capture_id: &str,
    proposal: &ProjectLinkProposal,
) -> Result<Option<CreateWorkObjectInput>, String> {
    let (kind, data) = match proposal.intent {
        ProjectLinkIntent::WorkTask | ProjectLinkIntent::Todo => (WorkObjectKind::Task, json!({})),
        ProjectLinkIntent::Event => (WorkObjectKind::Milestone, json!({})),
        ProjectLinkIntent::Decision => (WorkObjectKind::Decision, decision_data(proposal)?),
        ProjectLinkIntent::Note => (
            WorkObjectKind::Evidence,
            json!({"evidenceType": "note", "reference": proposal.external_id}),
        ),
        ProjectLinkIntent::Artifact => (WorkObjectKind::Artifact, proposal.metadata.clone()),
        ProjectLinkIntent::Change => (WorkObjectKind::Change, proposal.metadata.clone()),
        ProjectLinkIntent::Commitment => (
            WorkObjectKind::Commitment,
            json!({"owner": proposal.owner, "dueAt": proposal.due_at}),
        ),
        ProjectLinkIntent::Source | ProjectLinkIntent::Knowledge | ProjectLinkIntent::Meeting => {
            return Ok(None)
        }
    };
    let idempotency_key = if proposal.intent == ProjectLinkIntent::Artifact {
        let path = proposal.external_id.as_deref().unwrap_or(&proposal.title);
        let hash = proposal
            .metadata
            .get("hash")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        format!("file-artifact:{:x}:{hash}", Md5::digest(path.as_bytes()))
    } else {
        format!("capture-work:{capture_id}:{}", proposal.intent.as_str())
    };
    Ok(Some(CreateWorkObjectInput {
        kind,
        project_id: project_id.into(),
        parent_id: None,
        title: proposal.title.clone(),
        status: (proposal.intent == ProjectLinkIntent::Change).then(|| "needs_review".to_string()),
        description: proposal.description.clone(),
        data,
        source_capture_id: Some(capture_id.into()),
        actor: Some("bob".into()),
        idempotency_key,
    }))
}

fn meeting_inputs(
    project_id: &str,
    capture_id: &str,
    proposal: &ProjectLinkProposal,
) -> Result<Vec<CreateWorkObjectInput>, String> {
    let Some(items) = proposal.metadata.get("items").and_then(Value::as_array) else {
        return Ok(vec![]);
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let kind = match item.get("kind").and_then(Value::as_str).unwrap_or("") {
                "decision" => WorkObjectKind::Decision,
                "task" => WorkObjectKind::Task,
                "commitment" => WorkObjectKind::Commitment,
                _ => return Err("meeting_item_kind_invalid".to_string()),
            };
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let data = match kind {
                WorkObjectKind::Decision => DecisionData {
                    decision: title.clone(),
                    reason: item
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    alternatives: item
                        .get("alternatives")
                        .cloned()
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default(),
                    rejected_alternatives: item
                        .get("rejectedAlternatives")
                        .cloned()
                        .and_then(|value| {
                            serde_json::from_value::<Vec<RejectedAlternative>>(value).ok()
                        })
                        .unwrap_or_default(),
                    participants: item
                        .get("participants")
                        .cloned()
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default(),
                    owner: item
                        .get("owner")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    evidence: item
                        .get("evidence")
                        .cloned()
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default(),
                    revisit_condition: item
                        .get("revisitCondition")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                }
                .into_value()?,
                WorkObjectKind::Commitment => {
                    json!({ "owner": item.get("owner"), "dueAt": item.get("dueAt") })
                }
                _ => json!({}),
            };
            Ok(CreateWorkObjectInput {
                kind,
                project_id: project_id.into(),
                parent_id: None,
                title,
                status: None,
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                data,
                source_capture_id: Some(capture_id.into()),
                actor: Some("bob".into()),
                idempotency_key: format!("capture-work:{capture_id}:meeting:{index}"),
            })
        })
        .collect()
}

fn merged_refs(
    capture: &CaptureEnvelope,
    candidate_id: &str,
    object: Option<&WorkObject>,
    link: Option<&ExternalLink>,
) -> Vec<String> {
    let mut refs = capture.derived_refs.clone();
    for value in [
        Some(format!("project_link:{candidate_id}")),
        object.map(|v| format!("work:{}", v.id)),
        link.map(|v| format!("external_link:{}", v.id)),
    ]
    .into_iter()
    .flatten()
    {
        if !refs.contains(&value) {
            refs.push(value);
        }
    }
    refs
}

fn current_file_fingerprint(metadata: &Value) -> Value {
    let mut value = serde_json::Map::new();
    for key in ["path", "hash", "size", "modifiedAt"] {
        if let Some(item) = metadata.get(key) {
            value.insert(key.into(), item.clone());
        }
    }
    Value::Object(value)
}

fn file_name_key(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
}

fn mark_file_change(
    proposal: &mut ProjectLinkProposal,
    previous_artifact_id: Option<String>,
    previous_metadata: Value,
    match_method: &str,
) -> bool {
    let current_hash = proposal
        .metadata
        .get("hash")
        .and_then(Value::as_str)
        .unwrap_or("");
    let previous_hash = previous_metadata
        .get("hash")
        .and_then(Value::as_str)
        .unwrap_or("");
    if previous_hash.is_empty() || current_hash.is_empty() || previous_hash == current_hash {
        return false;
    }
    if let Some(object) = proposal.metadata.as_object_mut() {
        object.insert("previousHash".into(), json!(previous_hash));
        object.insert("previousFingerprint".into(), previous_metadata);
        object.insert("versionMatchMethod".into(), json!(match_method));
        if let Some(previous_artifact_id) = previous_artifact_id {
            object.insert("previousArtifactId".into(), json!(previous_artifact_id));
        }
        object.insert("changeType".into(), json!("file_content_changed"));
    }
    proposal.intent = ProjectLinkIntent::Change;
    true
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn explicit_impacts(value: Option<&Value>) -> Vec<ExplicitImpact> {
    value
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn apply_file_change_in_tx(
    tx: &Transaction<'_>,
    capture: &CaptureEnvelope,
    proposal: &ProjectLinkProposal,
    project_id: &str,
) -> Result<(WorkObject, WorkObject, Option<ExternalLink>, Vec<String>), String> {
    let mut artifact_proposal = proposal.clone();
    artifact_proposal.intent = ProjectLinkIntent::Artifact;
    artifact_proposal.metadata = current_file_fingerprint(&proposal.metadata);
    let artifact_input = object_input(project_id, &capture.capture_id, &artifact_proposal)?
        .ok_or_else(|| "FILE_CHANGE_ARTIFACT_INPUT_MISSING".to_string())?;
    let new_artifact = repository::create_object_in_tx(tx, artifact_input)?;

    let previous_artifact_id = proposal
        .metadata
        .get("previousArtifactId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let change_data = json!({
        "changeType": proposal.metadata.get("changeType").and_then(Value::as_str).unwrap_or("file_content_changed"),
        "externalKind": "file",
        "externalId": proposal.external_id.clone(),
        "previousFingerprint": proposal.metadata.get("previousFingerprint").cloned().unwrap_or(Value::Null),
        "currentFingerprint": artifact_proposal.metadata.clone(),
        "previousArtifactId": previous_artifact_id,
        "newArtifactId": new_artifact.id.clone(),
        "affectedObjectIds": proposal.metadata.get("affectedObjectIds").cloned().unwrap_or_else(|| json!([])),
        "observedAt": now_ms()
    });
    let change = repository::create_object_in_tx(
        tx,
        CreateWorkObjectInput {
            kind: WorkObjectKind::Change,
            project_id: project_id.into(),
            parent_id: None,
            title: proposal.title.clone(),
            status: Some("needs_review".into()),
            description: proposal.description.clone(),
            data: change_data,
            source_capture_id: Some(capture.capture_id.clone()),
            actor: Some("bob".into()),
            idempotency_key: format!("capture-work:{}:file-change", capture.capture_id),
        },
    )?;
    let link = create_external_link_in_tx(
        tx,
        project_id,
        Some(&new_artifact.id),
        &artifact_proposal,
        &capture.capture_id,
    )?;
    let mut external_refs = vec![proposal.external_id.clone().unwrap_or_default()];
    if let Some(link) = &link {
        external_refs.push(format!("external_link:{}", link.id));
    }
    let reviews = decision_change::create_change_reviews_in_tx(
        tx,
        ChangeAnalysisInput {
            project_id,
            change: &change,
            new_artifact: Some(&new_artifact),
            previous_artifact_id,
            external_refs,
            explicit_affected_object_ids: string_array(proposal.metadata.get("affectedObjectIds")),
            explicit_impacts: explicit_impacts(proposal.metadata.get("impacts")),
        },
    )?;
    Ok((
        change,
        new_artifact,
        link,
        reviews.into_iter().map(|review| review.id).collect(),
    ))
}

fn apply_resolved(
    conn: &mut Connection,
    capture: &CaptureEnvelope,
    proposal: &ProjectLinkProposal,
    project_id: &str,
    candidates: &[String],
    reason_code: &str,
) -> Result<ProjectLinkOutcome, String> {
    let candidate_id = format!("project_link_{}", capture.capture_id);
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let existing = get_candidate(&tx, &candidate_id).ok();
    if existing.as_ref().map(|v| v.status.as_str()) == Some("dismissed") {
        return Err("项目关联已被忽略".into());
    }
    let mut objects = Vec::new();
    let mut review_ids = Vec::new();
    let file_change = proposal.intent == ProjectLinkIntent::Change
        && proposal.external_kind.as_deref() == Some("file")
        && proposal.metadata.get("previousHash").is_some();
    let link = if file_change {
        let (change, artifact, link, reviews) =
            apply_file_change_in_tx(&tx, capture, proposal, project_id)?;
        objects.push(change);
        objects.push(artifact);
        review_ids = reviews;
        link
    } else {
        if proposal.intent == ProjectLinkIntent::Meeting {
            for input in meeting_inputs(project_id, &capture.capture_id, proposal)? {
                objects.push(repository::create_object_in_tx(&tx, input)?);
            }
        } else if let Some(input) = object_input(project_id, &capture.capture_id, proposal)? {
            objects.push(repository::create_object_in_tx(&tx, input)?);
        }
        let link = create_external_link_in_tx(
            &tx,
            project_id,
            objects.first().map(|value| value.id.as_str()),
            proposal,
            &capture.capture_id,
        )?;
        if proposal.intent == ProjectLinkIntent::Change {
            if let Some(change) = objects.first() {
                let mut external_refs = vec![proposal.external_id.clone().unwrap_or_default()];
                if let Some(link) = &link {
                    external_refs.push(format!("external_link:{}", link.id));
                }
                review_ids = decision_change::create_change_reviews_in_tx(
                    &tx,
                    ChangeAnalysisInput {
                        project_id,
                        change,
                        new_artifact: None,
                        previous_artifact_id: proposal
                            .metadata
                            .get("previousArtifactId")
                            .and_then(Value::as_str),
                        external_refs,
                        explicit_affected_object_ids: string_array(
                            proposal.metadata.get("affectedObjectIds"),
                        ),
                        explicit_impacts: explicit_impacts(proposal.metadata.get("impacts")),
                    },
                )?
                .into_iter()
                .map(|review| review.id)
                .collect();
            }
        }
        link
    };
    let object = objects.first().cloned();
    let candidate = upsert_candidate(
        &tx,
        &capture.capture_id,
        proposal,
        candidates,
        Some(project_id),
        "resolved",
        reason_code,
        object.as_ref().map(|v| v.id.as_str()),
        None,
    )?;
    let latest_capture = crate::capture::get_capture(&tx, &capture.capture_id)?;
    let mut refs = merged_refs(
        &latest_capture,
        &candidate.id,
        object.as_ref(),
        link.as_ref(),
    );
    for value in objects
        .iter()
        .skip(1)
        .map(|object| format!("work:{}", object.id))
        .chain(
            review_ids
                .iter()
                .map(|review_id| format!("change_review:{review_id}")),
        )
    {
        if !refs.contains(&value) {
            refs.push(value);
        }
    }
    crate::capture::mark_routed_committed(&tx, &latest_capture, &refs)?;
    tx.execute("UPDATE capture_enrichment SET stage='committed', last_error=NULL, updated_at=?2 WHERE capture_id=?1", params![capture.capture_id, now_ms()]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ProjectLinkOutcome { candidate, object })
}

fn persist_pending(
    conn: &mut Connection,
    capture: &CaptureEnvelope,
    proposal: &ProjectLinkProposal,
    candidates: &[String],
    selected_project_id: Option<&str>,
    reason_code: &str,
    last_error: Option<&str>,
) -> Result<ProjectLinkOutcome, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let candidate = upsert_candidate(
        &tx,
        &capture.capture_id,
        proposal,
        candidates,
        selected_project_id,
        "pending",
        reason_code,
        None,
        last_error,
    )?;
    if proposal.intent.is_work_only() {
        crate::capture::mark_project_assignment_pending(&tx, capture, &candidate.id, reason_code)?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ProjectLinkOutcome {
        candidate,
        object: None,
    })
}

pub fn apply_proposal(
    conn: &mut Connection,
    capture: &CaptureEnvelope,
    mut proposal: ProjectLinkProposal,
) -> Result<Option<ProjectLinkOutcome>, String> {
    if proposal
        .project_id
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
        && proposal
            .project_hint
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Ok(None);
    }
    let (project_id, candidates, mut reason_code) = resolve_project(conn, &proposal)?;
    if proposal.intent == ProjectLinkIntent::Change {
        reason_code = "change_requires_review".into();
    }
    if proposal.intent == ProjectLinkIntent::Artifact {
        if let (Some(project_id), Some(path)) =
            (project_id.as_deref(), proposal.external_id.as_deref())
        {
            let explicit_previous = proposal
                .metadata
                .get("previousArtifactId")
                .and_then(Value::as_str)
                .and_then(|object_id| repository::get_object(conn, object_id).ok().flatten())
                .filter(|object| {
                    object.project_id == project_id && object.kind == WorkObjectKind::Artifact
                })
                .map(|object| (Some(object.id), object.data, "explicit"));
            let exact_previous = conn
                .query_row(
                    "SELECT object_id, metadata_json FROM work_external_links WHERE project_id=?1 AND external_kind='file' AND external_id=?2 AND relation='artifact_source' AND deleted_at IS NULL",
                    params![project_id, path],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .map(|(object_id, raw)| {
                    (
                        object_id,
                        serde_json::from_str(&raw).unwrap_or_else(|_| json!({})),
                        "same_path",
                    )
                });
            let file_name = file_name_key(path);
            let same_name_previous = if explicit_previous.is_none()
                && exact_previous.is_none()
                && file_name.is_some()
            {
                let mut statement = conn
                    .prepare("SELECT object_id, external_id, metadata_json FROM work_external_links WHERE project_id=?1 AND external_kind='file' AND relation='artifact_source' AND deleted_at IS NULL AND external_id != ?2")
                    .map_err(|e| e.to_string())?;
                let matches = statement
                    .query_map(params![project_id, path], |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })
                    .map_err(|e| e.to_string())?
                    .filter_map(Result::ok)
                    .filter(|(_, candidate_path, _)| file_name_key(candidate_path) == file_name)
                    .collect::<Vec<_>>();
                match matches.as_slice() {
                    [(object_id, _, raw)] => Some((
                        object_id.clone(),
                        serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
                        "unique_file_name",
                    )),
                    _ => None,
                }
            } else {
                None
            };
            if let Some((previous_artifact_id, previous_metadata, method)) =
                explicit_previous.or(exact_previous).or(same_name_previous)
            {
                if mark_file_change(
                    &mut proposal,
                    previous_artifact_id,
                    previous_metadata,
                    method,
                ) {
                    reason_code = "change_requires_review".into();
                }
            }
        }
    }
    if let Some(missing) = missing_field(&proposal) {
        reason_code = missing.into();
    }
    if let Some(project_id) = project_id
        .as_deref()
        .filter(|_| missing_field(&proposal).is_none() && reason_code != "change_requires_review")
    {
        return match apply_resolved(
            conn,
            capture,
            &proposal,
            project_id,
            &candidates,
            &reason_code,
        ) {
            Ok(outcome) => Ok(Some(outcome)),
            Err(error) => {
                let outcome = persist_pending(
                    conn,
                    capture,
                    &proposal,
                    &candidates,
                    Some(project_id),
                    "apply_failed",
                    Some(&error),
                )?;
                Ok(Some(outcome))
            }
        };
    }
    Ok(Some(persist_pending(
        conn,
        capture,
        &proposal,
        &candidates,
        project_id.as_deref(),
        &reason_code,
        None,
    )?))
}

pub fn resolve_candidate(
    conn: &mut Connection,
    input: ResolveProjectLinkInput,
) -> Result<ProjectLinkOutcome, String> {
    let current = get_candidate(conn, &input.candidate_id)?;
    if current.status == "resolved" {
        let object = current
            .resolved_object_id
            .as_deref()
            .map(|id| repository::get_object(conn, id))
            .transpose()?
            .flatten();
        return Ok(ProjectLinkOutcome {
            candidate: current,
            object,
        });
    }
    if current.status == "dismissed" {
        return Err("项目关联已被忽略".into());
    }
    if current.revision != input.expected_revision {
        return Err("PROJECT_LINK_REVISION_CONFLICT".into());
    }
    let project = repository::get_project(conn, &input.project_id)?
        .ok_or_else(|| "项目不存在".to_string())?;
    if project.deleted_at.is_some() || matches!(project.status.as_str(), "archived" | "cancelled") {
        return Err("PROJECT_UNAVAILABLE".into());
    }
    let mut proposal = current.proposal.clone();
    proposal.project_id = Some(input.project_id.clone());
    proposal.reason = input.reason.or(proposal.reason);
    proposal.owner = input.owner.or(proposal.owner);
    proposal.due_at = input.due_at.or(proposal.due_at);
    if let Some(missing) = missing_field(&proposal) {
        return Err(missing.into());
    }
    let capture = crate::capture::get_capture(conn, &current.capture_id)?;
    apply_resolved(
        conn,
        &capture,
        &proposal,
        &input.project_id,
        &[input.project_id.clone()],
        "user_resolved",
    )
}

pub fn dismiss_candidate(
    conn: &mut Connection,
    input: DismissProjectLinkInput,
) -> Result<ProjectLinkCandidate, String> {
    let current = get_candidate(conn, &input.candidate_id)?;
    if current.status == "dismissed" {
        return Ok(current);
    }
    if current.status == "resolved" {
        return Err("已完成的项目关联不能忽略".into());
    }
    if current.revision != input.expected_revision {
        return Err("PROJECT_LINK_REVISION_CONFLICT".into());
    }
    let capture = crate::capture::get_capture(conn, &current.capture_id)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let changed = tx.execute("UPDATE project_link_candidates SET status='dismissed', reason_code='user_dismissed', revision=revision+1, updated_at=?2 WHERE id=?1 AND revision=?3 AND status='pending'", params![input.candidate_id, now_ms(), input.expected_revision as i64]).map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("PROJECT_LINK_REVISION_CONFLICT".into());
    }
    crate::capture::mark_project_link_dismissed(&tx, &capture, &input.candidate_id)?;
    tx.commit().map_err(|e| e.to_string())?;
    get_candidate(conn, &input.candidate_id)
}

pub fn list_external_links(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<ExternalLink>, String> {
    let mut statement = conn.prepare("SELECT id, project_id, object_id, external_kind, external_id, relation, source_capture_id, metadata_json, created_at, updated_at FROM work_external_links WHERE project_id=?1 AND deleted_at IS NULL ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params![project_id], |row| {
            let raw: String = row.get(7)?;
            Ok(ExternalLink {
                id: row.get(0)?,
                project_id: row.get(1)?,
                object_id: row.get(2)?,
                external_kind: row.get(3)?,
                external_id: row.get(4)?,
                relation: row.get(5)?,
                source_capture_id: row.get(6)?,
                metadata: serde_json::from_str(&raw).unwrap_or_else(|_| json!({})),
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn record_external_state_change(
    conn: &mut Connection,
    external_kind: &str,
    external_id: &str,
    state: &str,
    payload: Value,
) -> Result<usize, String> {
    let links = {
        let mut statement = conn
            .prepare(
                "SELECT project_id, object_id FROM work_external_links WHERE external_kind=?1 AND external_id=?2 AND deleted_at IS NULL",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params![external_kind, external_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    if links.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let now = now_ms();
    for (project_id, object_id) in &links {
        repository::touch_project(&tx, project_id, now)?;
        repository::append_event(
            &tx,
            project_id,
            object_id.as_deref(),
            &format!("external.{state}"),
            "user",
            &payload,
            Some(&format!(
                "external-state:{external_kind}:{external_id}:{state}:{now}"
            )),
            now,
        )?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(links.len())
}

pub fn fingerprint_file(path: &Path) -> Result<FileFingerprint, String> {
    if !path.is_file() {
        return Err("只支持引用单个现有文件".into());
    }
    let absolute = path
        .canonicalize()
        .map_err(|e| format!("解析文件路径失败: {e}"))?;
    let metadata = absolute
        .metadata()
        .map_err(|e| format!("读取文件属性失败: {e}"))?;
    let mut file = File::open(&absolute).map_err(|e| format!("打开文件失败: {e}"))?;
    let mut hasher = Md5::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("读取文件失败: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|v| v.as_millis() as i64);
    Ok(FileFingerprint {
        path: absolute.to_string_lossy().to_string(),
        hash: format!("{:x}", hasher.finalize()),
        size: metadata.len(),
        modified_at,
    })
}

pub fn refresh_project_snapshot(conn: &Connection, project_id: &str) {
    if let Ok(aggregate) = repository::get_project_aggregate(conn, project_id) {
        if let Err(error) = snapshot::write_project_snapshot(&aggregate) {
            log::warn!("Work snapshot refresh failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{build_envelope_for_test, init_capture_tables, CaptureInput};
    use crate::work_core::models::CreateProjectInput;

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_capture_tables(&conn);
        crate::capture_router::init_capture_router_tables(&conn);
        crate::work_core::init_work_core_tables(&conn).unwrap();
        conn
    }

    fn capture(text: &str) -> CaptureEnvelope {
        build_envelope_for_test(CaptureInput {
            entry_point: "test".into(),
            content: Some(text.into()),
            source_url: None,
            file_path: None,
            explicit_intent: None,
            language: None,
            privacy_scope: None,
            sync_scope: None,
            source_device: Some("test".into()),
            idempotency_key: Some(format!("test:{text}")),
        })
        .unwrap()
    }

    fn insert_capture(conn: &Connection, capture: &CaptureEnvelope) {
        conn.execute("INSERT INTO capture_journal (capture_id,schema_version,entry_point,source_device,original_content,content_hash,idempotency_key,language,privacy_scope,sync_scope,status,derived_refs,created_at,updated_at) VALUES (?1,1,'test','test',?2,?3,?4,'auto','private','paired_devices','received','[]',?5,?5)", params![capture.capture_id,capture.content,capture.content_hash,capture.idempotency_key,now_ms()]).unwrap();
    }

    fn project(conn: &mut Connection, id: &str, title: &str) {
        repository::create_project(
            conn,
            CreateProjectInput {
                project_id: Some(id.into()),
                title: title.into(),
                mission: "".into(),
                current_phase: None,
                summary: None,
                source_ref: None,
                metadata: json!({}),
                actor: None,
                idempotency_key: format!("create:{id}"),
            },
        )
        .unwrap();
    }

    fn proposal(intent: ProjectLinkIntent, hint: &str) -> ProjectLinkProposal {
        ProjectLinkProposal {
            intent,
            title: "完成同步回放".into(),
            project_id: None,
            project_hint: Some(hint.into()),
            description: None,
            reason: None,
            owner: None,
            due_at: None,
            external_kind: None,
            external_id: None,
            metadata: json!({}),
            confidence: 0.98,
            reason_codes: vec![],
        }
    }

    #[test]
    fn unique_project_creates_one_idempotent_task() {
        let mut conn = database();
        project(&mut conn, "project_bob", "Bob 项目");
        let capture = capture("在 Bob 项目中新增任务");
        insert_capture(&conn, &capture);
        let first = apply_proposal(
            &mut conn,
            &capture,
            proposal(ProjectLinkIntent::WorkTask, "Bob"),
        )
        .unwrap()
        .unwrap();
        let second = apply_proposal(
            &mut conn,
            &capture,
            proposal(ProjectLinkIntent::WorkTask, "Bob"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            first.object.as_ref().unwrap().id,
            second.object.as_ref().unwrap().id
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM work_objects", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn duplicate_title_waits_without_creating_object() {
        let mut conn = database();
        project(&mut conn, "project_a", "Bob");
        project(&mut conn, "project_b", "Bob 项目");
        let capture = capture("Bob 项目任务");
        insert_capture(&conn, &capture);
        let outcome = apply_proposal(
            &mut conn,
            &capture,
            proposal(ProjectLinkIntent::WorkTask, "Bob"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(outcome.candidate.status, "pending");
        assert_eq!(outcome.candidate.reason_code, "ambiguous_project");
        assert!(outcome.object.is_none());
    }

    #[test]
    fn decision_requires_reason_then_resolves_once() {
        let mut conn = database();
        project(&mut conn, "project_bob", "Bob");
        let capture = capture("Bob 项目决定保留架构");
        insert_capture(&conn, &capture);
        let pending = apply_proposal(
            &mut conn,
            &capture,
            proposal(ProjectLinkIntent::Decision, "Bob"),
        )
        .unwrap()
        .unwrap()
        .candidate;
        assert_eq!(pending.reason_code, "missing_decision_reason");
        let resolved = resolve_candidate(
            &mut conn,
            ResolveProjectLinkInput {
                candidate_id: pending.id.clone(),
                project_id: "project_bob".into(),
                expected_revision: pending.revision,
                reason: Some("迁移风险更低".into()),
                owner: None,
                due_at: None,
            },
        )
        .unwrap();
        let repeated = resolve_candidate(
            &mut conn,
            ResolveProjectLinkInput {
                candidate_id: pending.id,
                project_id: "project_bob".into(),
                expected_revision: 0,
                reason: None,
                owner: None,
                due_at: None,
            },
        )
        .unwrap();
        assert_eq!(resolved.object.unwrap().id, repeated.object.unwrap().id);
    }

    #[test]
    fn todo_external_link_keeps_calendar_as_truth_source() {
        let mut conn = database();
        project(&mut conn, "project_bob", "Bob");
        let capture = capture("Bob 项目明天交报告");
        insert_capture(&conn, &capture);
        let mut value = proposal(ProjectLinkIntent::Todo, "Bob");
        value.external_kind = Some("calendar_event".into());
        value.external_id = Some("event_1".into());
        let outcome = apply_proposal(&mut conn, &capture, value).unwrap().unwrap();
        assert_eq!(outcome.object.unwrap().kind, WorkObjectKind::Task);
        assert_eq!(
            list_external_links(&conn, "project_bob").unwrap()[0].external_id,
            "event_1"
        );
    }

    #[test]
    fn one_source_can_be_referenced_by_multiple_projects_without_copying_content() {
        let mut conn = database();
        project(&mut conn, "project_a", "A");
        project(&mut conn, "project_b", "B");
        for (suffix, project_id) in [("a", "project_a"), ("b", "project_b")] {
            let capture = capture(&format!("source-{suffix}"));
            insert_capture(&conn, &capture);
            let mut value = proposal(ProjectLinkIntent::Source, "unused");
            value.project_id = Some(project_id.into());
            value.project_hint = None;
            value.external_kind = Some("knowledge_source".into());
            value.external_id = Some("source_shared".into());
            let outcome = apply_proposal(&mut conn, &capture, value).unwrap().unwrap();
            assert!(outcome.object.is_none());
        }
        assert_eq!(list_external_links(&conn, "project_a").unwrap().len(), 1);
        assert_eq!(list_external_links(&conn, "project_b").unwrap().len(), 1);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM work_objects", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn changed_file_waits_for_review_instead_of_rewriting_artifact() {
        let mut conn = database();
        project(&mut conn, "project_bob", "Bob");
        let first_capture = capture("first-file");
        insert_capture(&conn, &first_capture);
        let mut first = proposal(ProjectLinkIntent::Artifact, "Bob");
        first.external_kind = Some("file".into());
        first.external_id = Some("C:/docs/report.md".into());
        first.metadata = json!({ "hash": "aaa", "size": 10 });
        assert_eq!(
            apply_proposal(&mut conn, &first_capture, first)
                .unwrap()
                .unwrap()
                .candidate
                .status,
            "resolved"
        );

        let changed_capture = capture("changed-file");
        insert_capture(&conn, &changed_capture);
        let mut changed = proposal(ProjectLinkIntent::Artifact, "Bob");
        changed.external_kind = Some("file".into());
        changed.external_id = Some("C:/docs/report.md".into());
        changed.metadata = json!({ "hash": "bbb", "size": 11 });
        let outcome = apply_proposal(&mut conn, &changed_capture, changed)
            .unwrap()
            .unwrap();
        assert_eq!(outcome.candidate.status, "pending");
        assert_eq!(outcome.candidate.reason_code, "change_requires_review");
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM work_objects", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn confirming_changed_file_preserves_old_artifact_and_creates_change_reviews() {
        let mut conn = database();
        project(&mut conn, "project_bob", "Bob");
        let first_capture = capture("file-v1");
        insert_capture(&conn, &first_capture);
        let mut first = proposal(ProjectLinkIntent::Artifact, "Bob");
        first.external_kind = Some("file".into());
        first.external_id = Some("C:/docs/plan.md".into());
        first.metadata = json!({ "path": "C:/docs/plan.md", "hash": "v1", "size": 10 });
        let old_artifact = apply_proposal(&mut conn, &first_capture, first)
            .unwrap()
            .unwrap()
            .object
            .unwrap();

        let changed_capture = capture("file-v2");
        insert_capture(&conn, &changed_capture);
        let mut changed = proposal(ProjectLinkIntent::Artifact, "Bob");
        changed.external_kind = Some("file".into());
        changed.external_id = Some("C:/docs/plan.md".into());
        changed.metadata = json!({ "path": "C:/docs/plan.md", "hash": "v2", "size": 12 });
        let pending = apply_proposal(&mut conn, &changed_capture, changed)
            .unwrap()
            .unwrap()
            .candidate;
        let resolved = resolve_candidate(
            &mut conn,
            ResolveProjectLinkInput {
                candidate_id: pending.id,
                project_id: "project_bob".into(),
                expected_revision: pending.revision,
                reason: None,
                owner: None,
                due_at: None,
            },
        )
        .unwrap();
        assert_eq!(
            resolved.object.as_ref().unwrap().kind,
            WorkObjectKind::Change
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM work_objects WHERE kind='artifact'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM work_change_reviews WHERE status='pending'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let link = list_external_links(&conn, "project_bob").unwrap().remove(0);
        assert_ne!(link.object_id.as_deref(), Some(old_artifact.id.as_str()));
        assert_eq!(
            link.metadata.get("hash").and_then(Value::as_str),
            Some("v2")
        );
    }

    #[test]
    fn unique_same_file_name_becomes_review_candidate_not_automatic_version() {
        let mut conn = database();
        project(&mut conn, "project_bob", "Bob");
        let first_capture = capture("named-file-v1");
        insert_capture(&conn, &first_capture);
        let mut first = proposal(ProjectLinkIntent::Artifact, "Bob");
        first.external_kind = Some("file".into());
        first.external_id = Some("C:/archive/report.md".into());
        first.metadata = json!({ "path": "C:/archive/report.md", "hash": "old" });
        apply_proposal(&mut conn, &first_capture, first)
            .unwrap()
            .unwrap();

        let second_capture = capture("named-file-v2");
        insert_capture(&conn, &second_capture);
        let mut second = proposal(ProjectLinkIntent::Artifact, "Bob");
        second.external_kind = Some("file".into());
        second.external_id = Some("D:/incoming/report.md".into());
        second.metadata = json!({ "path": "D:/incoming/report.md", "hash": "new" });
        let candidate = apply_proposal(&mut conn, &second_capture, second)
            .unwrap()
            .unwrap()
            .candidate;
        assert_eq!(candidate.status, "pending");
        assert_eq!(candidate.reason_code, "change_requires_review");
        assert_eq!(
            candidate
                .proposal
                .metadata
                .get("versionMatchMethod")
                .and_then(Value::as_str),
            Some("unique_file_name")
        );
    }

    #[test]
    fn invalid_meeting_rolls_back_every_derived_object() {
        let mut conn = database();
        project(&mut conn, "project_bob", "Bob");
        let capture = capture("meeting");
        insert_capture(&conn, &capture);
        let mut value = proposal(ProjectLinkIntent::Meeting, "Bob");
        value.metadata = json!({ "items": [
            { "kind": "task", "title": "Prepare replay" },
            { "kind": "decision", "title": "Keep architecture" }
        ] });
        let outcome = apply_proposal(&mut conn, &capture, value).unwrap().unwrap();
        assert_eq!(outcome.candidate.status, "pending");
        assert_eq!(outcome.candidate.reason_code, "apply_failed");
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM work_objects", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn external_state_change_appends_project_event_without_copying_status() {
        let mut conn = database();
        project(&mut conn, "project_bob", "Bob");
        let capture = capture("todo-link");
        insert_capture(&conn, &capture);
        let mut value = proposal(ProjectLinkIntent::Todo, "Bob");
        value.external_kind = Some("calendar_event".into());
        value.external_id = Some("event_2".into());
        apply_proposal(&mut conn, &capture, value).unwrap();
        assert_eq!(
            record_external_state_change(
                &mut conn,
                "calendar_event",
                "event_2",
                "completed",
                json!({"status":"done"})
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM work_events WHERE event_type='external.completed'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}
