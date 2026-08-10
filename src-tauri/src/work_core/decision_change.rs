use std::collections::{HashMap, HashSet};

use md5::{Digest, Md5};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::models::{DecisionData, WorkObject, WorkObjectKind};
use super::repository;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeReview {
    pub id: String,
    pub project_id: String,
    pub change_id: String,
    pub change_title: String,
    pub target_object_id: String,
    pub target_kind: String,
    pub target_title: String,
    pub relation_source_id: String,
    pub relation_target_id: String,
    pub proposed_relation: String,
    pub reason_code: String,
    pub explanation: Option<String>,
    pub evidence_refs: Vec<String>,
    pub confidence: f64,
    pub status: String,
    pub resolution_note: Option<String>,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeReviewActionInput {
    pub review_id: String,
    pub action: String,
    pub expected_revision: u64,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeReviewActionOutcome {
    pub review: ChangeReview,
    pub relation_created: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ChangeAnalysisInput<'a> {
    pub project_id: &'a str,
    pub change: &'a WorkObject,
    pub new_artifact: Option<&'a WorkObject>,
    pub previous_artifact_id: Option<&'a str>,
    pub external_refs: Vec<String>,
    pub explicit_affected_object_ids: Vec<String>,
    pub explicit_impacts: Vec<ExplicitImpact>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExplicitImpact {
    pub object_id: String,
    pub relation: String,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default = "default_impact_confidence")]
    pub confidence: f64,
}

fn default_impact_confidence() -> f64 {
    0.5
}

pub fn init_decision_change_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS work_change_reviews (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            change_id TEXT NOT NULL,
            target_object_id TEXT NOT NULL,
            target_kind TEXT NOT NULL,
            relation_source_id TEXT NOT NULL,
            relation_target_id TEXT NOT NULL,
            proposed_relation TEXT NOT NULL,
            reason_code TEXT NOT NULL,
            explanation TEXT,
            evidence_refs_json TEXT NOT NULL DEFAULT '[]',
            confidence REAL NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'pending',
            resolution_note TEXT,
            revision INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            resolved_at INTEGER,
            UNIQUE(change_id, target_object_id, proposed_relation),
            FOREIGN KEY(project_id) REFERENCES work_projects(id),
            FOREIGN KEY(change_id) REFERENCES work_objects(id)
        );
        CREATE INDEX IF NOT EXISTS idx_work_change_reviews_status
            ON work_change_reviews(status, updated_at);
        CREATE INDEX IF NOT EXISTS idx_work_change_reviews_project
            ON work_change_reviews(project_id, status, updated_at);
        CREATE INDEX IF NOT EXISTS idx_work_change_reviews_change
            ON work_change_reviews(change_id, status);
        ",
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn review_from_row(row: &Row<'_>) -> rusqlite::Result<ChangeReview> {
    let evidence_raw: String = row.get(10)?;
    Ok(ChangeReview {
        id: row.get(0)?,
        project_id: row.get(1)?,
        change_id: row.get(2)?,
        change_title: String::new(),
        target_object_id: row.get(3)?,
        target_kind: row.get(4)?,
        target_title: String::new(),
        relation_source_id: row.get(5)?,
        relation_target_id: row.get(6)?,
        proposed_relation: row.get(7)?,
        reason_code: row.get(8)?,
        explanation: row.get(9)?,
        evidence_refs: serde_json::from_str(&evidence_raw).unwrap_or_default(),
        confidence: row.get(11)?,
        status: row.get(12)?,
        resolution_note: row.get(13)?,
        revision: row.get::<_, i64>(14)?.max(0) as u64,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        resolved_at: row.get(17)?,
    })
}

const REVIEW_SELECT: &str = "SELECT id, project_id, change_id, target_object_id, target_kind, relation_source_id, relation_target_id, proposed_relation, reason_code, explanation, evidence_refs_json, confidence, status, resolution_note, revision, created_at, updated_at, resolved_at FROM work_change_reviews";

pub fn get_review(conn: &Connection, review_id: &str) -> Result<ChangeReview, String> {
    let mut review = conn
        .query_row(
            &format!("{REVIEW_SELECT} WHERE id=?1"),
            params![review_id],
            review_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "CHANGE_REVIEW_NOT_FOUND".to_string())?;
    hydrate_review(conn, &mut review)?;
    Ok(review)
}

fn hydrate_review(conn: &Connection, review: &mut ChangeReview) -> Result<(), String> {
    review.change_title = repository::get_object(conn, &review.change_id)?
        .map(|object| object.title)
        .unwrap_or_else(|| review.change_id.clone());
    review.target_title = if review.target_object_id == review.project_id {
        repository::get_project(conn, &review.project_id)?
            .map(|project| project.title)
            .unwrap_or_else(|| review.project_id.clone())
    } else {
        repository::get_object(conn, &review.target_object_id)?
            .map(|object| object.title)
            .unwrap_or_else(|| review.target_object_id.clone())
    };
    Ok(())
}

pub fn list_reviews(
    conn: &Connection,
    project_id: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<ChangeReview>, String> {
    let project = project_id.map(str::trim).filter(|value| !value.is_empty());
    let state = status.map(str::trim).filter(|value| !value.is_empty());
    let limit = limit.clamp(1, 100) as i64;
    let mut reviews = match (project, state) {
        (Some(project), Some(state)) => {
            let mut statement = conn
                .prepare(&format!("{REVIEW_SELECT} WHERE project_id=?1 AND status=?2 ORDER BY updated_at DESC LIMIT ?3"))
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params![project, state, limit], review_from_row)
                .map_err(|error| error.to_string())?;
            let reviews = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            reviews
        }
        (Some(project), None) => {
            let mut statement = conn
                .prepare(&format!(
                    "{REVIEW_SELECT} WHERE project_id=?1 ORDER BY updated_at DESC LIMIT ?2"
                ))
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params![project, limit], review_from_row)
                .map_err(|error| error.to_string())?;
            let reviews = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            reviews
        }
        (None, Some(state)) => {
            let mut statement = conn
                .prepare(&format!(
                    "{REVIEW_SELECT} WHERE status=?1 ORDER BY updated_at DESC LIMIT ?2"
                ))
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params![state, limit], review_from_row)
                .map_err(|error| error.to_string())?;
            let reviews = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            reviews
        }
        (None, None) => {
            let mut statement = conn
                .prepare(&format!(
                    "{REVIEW_SELECT} ORDER BY updated_at DESC LIMIT ?1"
                ))
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params![limit], review_from_row)
                .map_err(|error| error.to_string())?;
            let reviews = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            reviews
        }
    };
    for review in &mut reviews {
        hydrate_review(conn, review)?;
    }
    Ok(reviews)
}

fn allowed_target_kind(kind: WorkObjectKind) -> bool {
    matches!(
        kind,
        WorkObjectKind::Decision
            | WorkObjectKind::Goal
            | WorkObjectKind::Task
            | WorkObjectKind::Artifact
            | WorkObjectKind::Risk
    )
}

fn unique_refs(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}

struct ReviewProposal {
    target_object_id: String,
    target_kind: String,
    relation_source_id: String,
    relation_target_id: String,
    proposed_relation: String,
    reason_code: String,
    explanation: Option<String>,
    evidence_refs: Vec<String>,
    confidence: f64,
}

fn insert_review_in_tx(
    tx: &Transaction<'_>,
    project_id: &str,
    change_id: &str,
    proposal: ReviewProposal,
) -> Result<ChangeReview, String> {
    let key = format!(
        "{change_id}:{}:{}",
        proposal.target_object_id, proposal.proposed_relation
    );
    let id = format!("change_review_{:x}", Md5::digest(key.as_bytes()));
    let now = now_ms();
    tx.execute(
        "INSERT INTO work_change_reviews (id, project_id, change_id, target_object_id, target_kind, relation_source_id, relation_target_id, proposed_relation, reason_code, explanation, evidence_refs_json, confidence, status, revision, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'pending', 1, ?13, ?13)
         ON CONFLICT(change_id, target_object_id, proposed_relation) DO UPDATE SET explanation=COALESCE(work_change_reviews.explanation, excluded.explanation), evidence_refs_json=excluded.evidence_refs_json, confidence=MAX(work_change_reviews.confidence, excluded.confidence), updated_at=excluded.updated_at",
        params![id, project_id, change_id, proposal.target_object_id, proposal.target_kind, proposal.relation_source_id, proposal.relation_target_id, proposal.proposed_relation, proposal.reason_code, proposal.explanation, serde_json::to_string(&unique_refs(proposal.evidence_refs)).map_err(|error| error.to_string())?, proposal.confidence.clamp(0.0, 1.0), now],
    )
    .map_err(|error| error.to_string())?;
    get_review(tx, &id)
}

fn object_map(conn: &Connection, project_id: &str) -> Result<HashMap<String, WorkObject>, String> {
    let aggregate = repository::get_project_aggregate(conn, project_id)?;
    let mut values = HashMap::new();
    for object in aggregate
        .goals
        .into_iter()
        .chain(aggregate.tasks)
        .chain(aggregate.decisions)
        .chain(aggregate.artifacts)
        .chain(aggregate.risks)
    {
        values.insert(object.id.clone(), object);
    }
    Ok(values)
}

pub(crate) fn create_change_reviews_in_tx(
    tx: &Transaction<'_>,
    input: ChangeAnalysisInput<'_>,
) -> Result<Vec<ChangeReview>, String> {
    if input.change.kind != WorkObjectKind::Change || input.change.project_id != input.project_id {
        return Err("CHANGE_ANALYSIS_PROJECT_MISMATCH".into());
    }
    let objects = object_map(tx, input.project_id)?;
    let mut proposals = Vec::new();
    let external_refs = unique_refs(input.external_refs);

    if let (Some(new_artifact), Some(previous_id)) =
        (input.new_artifact, input.previous_artifact_id)
    {
        if new_artifact.project_id == input.project_id
            && objects
                .get(previous_id)
                .is_some_and(|object| object.kind == WorkObjectKind::Artifact)
        {
            proposals.push(ReviewProposal {
                target_object_id: previous_id.into(),
                target_kind: "artifact".into(),
                relation_source_id: new_artifact.id.clone(),
                relation_target_id: previous_id.into(),
                proposed_relation: "supersedes".into(),
                reason_code: "file_revision".into(),
                explanation: None,
                evidence_refs: external_refs.clone(),
                confidence: 1.0,
            });
        }
    }

    let mut affected_ids = input.explicit_affected_object_ids;
    if let Some(previous_id) = input.previous_artifact_id {
        let mut statement = tx
            .prepare(
                "SELECT source_id, target_id, id FROM work_relations WHERE project_id=?1 AND deleted_at IS NULL AND (source_id=?2 OR target_id=?2)",
            )
            .map_err(|error| error.to_string())?;
        let related = statement
            .query_map(params![input.project_id, previous_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        for (source, target, relation_id) in related {
            let other = if source == previous_id {
                target
            } else {
                source
            };
            if objects
                .get(&other)
                .is_some_and(|object| allowed_target_kind(object.kind))
            {
                affected_ids.push(other.clone());
                proposals.push(ReviewProposal {
                    target_object_id: other.clone(),
                    target_kind: objects[&other].kind.as_str().into(),
                    relation_source_id: other,
                    relation_target_id: input.change.id.clone(),
                    proposed_relation: "affected_by".into(),
                    reason_code: "explicit_relation".into(),
                    explanation: None,
                    evidence_refs: vec![format!("work_relation:{relation_id}")],
                    confidence: 1.0,
                });
            }
        }
    }

    let mut evidence_needles = external_refs.iter().cloned().collect::<HashSet<_>>();
    if let Some(previous_id) = input.previous_artifact_id {
        evidence_needles.insert(previous_id.into());
    }
    for object in objects
        .values()
        .filter(|object| object.kind == WorkObjectKind::Decision)
    {
        let Ok(decision) = DecisionData::from_value(&object.data) else {
            continue;
        };
        let matched = decision
            .evidence
            .iter()
            .filter(|reference| evidence_needles.contains(*reference))
            .cloned()
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            proposals.push(ReviewProposal {
                target_object_id: object.id.clone(),
                target_kind: "decision".into(),
                relation_source_id: object.id.clone(),
                relation_target_id: input.change.id.clone(),
                proposed_relation: "affected_by".into(),
                reason_code: "decision_evidence_changed".into(),
                explanation: None,
                evidence_refs: matched,
                confidence: 1.0,
            });
        }
    }

    for object_id in unique_refs(affected_ids) {
        let Some(object) = objects.get(&object_id) else {
            continue;
        };
        if !allowed_target_kind(object.kind) {
            continue;
        }
        proposals.push(ReviewProposal {
            target_object_id: object.id.clone(),
            target_kind: object.kind.as_str().into(),
            relation_source_id: object.id.clone(),
            relation_target_id: input.change.id.clone(),
            proposed_relation: "affected_by".into(),
            reason_code: "explicit_impact".into(),
            explanation: None,
            evidence_refs: external_refs.clone(),
            confidence: 1.0,
        });
    }

    for impact in input.explicit_impacts {
        let object_id = impact.object_id.trim();
        let relation = impact.relation.trim();
        let Some(object) = objects.get(object_id) else {
            continue;
        };
        if !allowed_target_kind(object.kind)
            || !matches!(relation, "affected_by" | "contradicts" | "supersedes")
        {
            continue;
        }
        let (source_id, target_id) = if relation == "affected_by" {
            (object.id.clone(), input.change.id.clone())
        } else {
            (input.change.id.clone(), object.id.clone())
        };
        proposals.push(ReviewProposal {
            target_object_id: object.id.clone(),
            target_kind: object.kind.as_str().into(),
            relation_source_id: source_id,
            relation_target_id: target_id,
            proposed_relation: relation.into(),
            reason_code: format!("explicit_{relation}"),
            explanation: impact
                .explanation
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            evidence_refs: unique_refs(
                external_refs
                    .iter()
                    .cloned()
                    .chain(impact.evidence_refs.into_iter()),
            ),
            confidence: impact.confidence.clamp(0.0, 1.0),
        });
    }

    if proposals.is_empty() {
        proposals.push(ReviewProposal {
            target_object_id: input.project_id.into(),
            target_kind: "project".into(),
            relation_source_id: input.project_id.into(),
            relation_target_id: input.change.id.clone(),
            proposed_relation: "affected_by".into(),
            reason_code: "impact_scope_unknown".into(),
            explanation: None,
            evidence_refs: external_refs,
            confidence: 0.0,
        });
    }

    let mut dedup = HashSet::new();
    let mut reviews = Vec::new();
    for proposal in proposals {
        let key = format!(
            "{}:{}",
            proposal.target_object_id, proposal.proposed_relation
        );
        if dedup.insert(key) {
            reviews.push(insert_review_in_tx(
                tx,
                input.project_id,
                &input.change.id,
                proposal,
            )?);
        }
    }
    Ok(reviews)
}

fn entity_in_project(conn: &Connection, project_id: &str, entity_id: &str) -> Result<bool, String> {
    if entity_id == project_id {
        return Ok(repository::get_project(conn, project_id)?.is_some());
    }
    Ok(repository::get_object(conn, entity_id)?
        .is_some_and(|object| object.project_id == project_id && object.deleted_at.is_none()))
}

fn create_review_relation_in_tx(
    tx: &Transaction<'_>,
    review: &ChangeReview,
) -> Result<bool, String> {
    if !entity_in_project(tx, &review.project_id, &review.relation_source_id)?
        || !entity_in_project(tx, &review.project_id, &review.relation_target_id)?
    {
        return Err("CHANGE_REVIEW_RELATION_PROJECT_MISMATCH".into());
    }
    let existing: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM work_relations WHERE project_id=?1 AND source_id=?2 AND target_id=?3 AND relation=?4 AND deleted_at IS NULL",
            params![review.project_id, review.relation_source_id, review.relation_target_id, review.proposed_relation],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if existing > 0 {
        return Ok(false);
    }
    let relation_id = format!("relation_change_{:x}", Md5::digest(review.id.as_bytes()));
    tx.execute(
        "INSERT INTO work_relations (id, project_id, source_id, target_id, relation, evidence_ref, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![relation_id, review.project_id, review.relation_source_id, review.relation_target_id, review.proposed_relation, format!("change_review:{}", review.id), review.confidence, now_ms()],
    )
    .map_err(|error| error.to_string())?;
    Ok(true)
}

pub fn apply_review_action(
    conn: &mut Connection,
    input: ChangeReviewActionInput,
) -> Result<ChangeReviewActionOutcome, String> {
    let action = input.action.trim();
    let target_status = match action {
        "accept" => "accepted",
        "reject" => "rejected",
        "defer" => "deferred",
        "reopen" => "pending",
        _ => return Err("CHANGE_REVIEW_ACTION_INVALID".into()),
    };
    let current = get_review(conn, &input.review_id)?;
    if current.status == target_status {
        return Ok(ChangeReviewActionOutcome {
            review: current,
            relation_created: false,
        });
    }
    let allowed = match action {
        "accept" | "reject" | "defer" => matches!(current.status.as_str(), "pending" | "deferred"),
        "reopen" => current.status == "deferred",
        _ => false,
    };
    if !allowed {
        return Err("CHANGE_REVIEW_TRANSITION_INVALID".into());
    }
    if current.revision != input.expected_revision {
        return Err("CHANGE_REVIEW_REVISION_CONFLICT".into());
    }
    let note = input
        .note
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let now = now_ms();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let relation_created = if action == "accept" {
        create_review_relation_in_tx(&tx, &current)?
    } else {
        false
    };
    let changed = tx
        .execute(
            "UPDATE work_change_reviews SET status=?2, resolution_note=?3, revision=revision+1, updated_at=?4, resolved_at=CASE WHEN ?2 IN ('accepted','rejected') THEN ?4 ELSE NULL END WHERE id=?1 AND revision=?5",
            params![input.review_id, target_status, note, now, input.expected_revision as i64],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("CHANGE_REVIEW_REVISION_CONFLICT".into());
    }
    repository::touch_project(&tx, &current.project_id, now)?;
    repository::append_event(
        &tx,
        &current.project_id,
        Some(&current.change_id),
        &format!("change_review.{target_status}"),
        "user",
        &json!({
            "reviewId": current.id,
            "targetObjectId": current.target_object_id,
            "proposedRelation": current.proposed_relation,
            "note": note,
            "relationCreated": relation_created
        }),
        Some(&format!(
            "change-review:{}:{target_status}:{}",
            current.id, current.revision
        )),
        now,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(ChangeReviewActionOutcome {
        review: get_review(conn, &input.review_id)?,
        relation_created,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_core::models::{CreateProjectInput, CreateWorkObjectInput};

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::work_core::init_work_core_tables(&conn).unwrap();
        conn
    }

    fn project(conn: &mut Connection) {
        repository::create_project(
            conn,
            CreateProjectInput {
                project_id: Some("project_phase3".into()),
                title: "Phase 3".into(),
                mission: String::new(),
                current_phase: None,
                summary: None,
                source_ref: None,
                metadata: json!({}),
                actor: None,
                idempotency_key: "project-phase3".into(),
            },
        )
        .unwrap();
    }

    fn object(
        conn: &mut Connection,
        kind: WorkObjectKind,
        idempotency_key: &str,
        data: serde_json::Value,
    ) -> WorkObject {
        repository::create_object(
            conn,
            CreateWorkObjectInput {
                kind,
                project_id: "project_phase3".into(),
                parent_id: None,
                title: idempotency_key.into(),
                status: None,
                description: None,
                data,
                source_capture_id: None,
                actor: None,
                idempotency_key: idempotency_key.into(),
            },
        )
        .unwrap()
    }

    #[test]
    fn evidence_reference_creates_review_and_accept_adds_relation() {
        let mut conn = database();
        project(&mut conn);
        let old = object(&mut conn, WorkObjectKind::Artifact, "old", json!({}));
        let new = object(&mut conn, WorkObjectKind::Artifact, "new", json!({}));
        let decision = object(
            &mut conn,
            WorkObjectKind::Decision,
            "decision",
            json!({
                "decision": "采用该方案",
                "reason": "报告支持",
                "evidence": [old.id]
            }),
        );
        let change = object(
            &mut conn,
            WorkObjectKind::Change,
            "change",
            json!({"changeType": "file_content_changed"}),
        );
        let tx = conn.transaction().unwrap();
        let reviews = create_change_reviews_in_tx(
            &tx,
            ChangeAnalysisInput {
                project_id: "project_phase3",
                change: &change,
                new_artifact: Some(&new),
                previous_artifact_id: Some(&old.id),
                external_refs: vec!["C:/report.md".into()],
                explicit_affected_object_ids: vec![],
                explicit_impacts: vec![],
            },
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(reviews.len(), 2);
        let decision_review = reviews
            .into_iter()
            .find(|review| review.target_object_id == decision.id)
            .unwrap();
        let accepted = apply_review_action(
            &mut conn,
            ChangeReviewActionInput {
                review_id: decision_review.id,
                action: "accept".into(),
                expected_revision: 1,
                note: Some("确认报告变化会影响决定".into()),
            },
        )
        .unwrap();
        assert_eq!(accepted.review.status, "accepted");
        assert!(accepted.relation_created);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM work_relations WHERE source_id=?1 AND target_id=?2 AND relation='affected_by'",
                params![decision.id, change.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn unknown_scope_is_visible_and_defer_can_reopen() {
        let mut conn = database();
        project(&mut conn);
        let change = object(
            &mut conn,
            WorkObjectKind::Change,
            "change-unknown",
            json!({"changeType": "new_information"}),
        );
        let tx = conn.transaction().unwrap();
        let review = create_change_reviews_in_tx(
            &tx,
            ChangeAnalysisInput {
                project_id: "project_phase3",
                change: &change,
                new_artifact: None,
                previous_artifact_id: None,
                external_refs: vec![],
                explicit_affected_object_ids: vec![],
                explicit_impacts: vec![],
            },
        )
        .unwrap()
        .remove(0);
        tx.commit().unwrap();
        assert_eq!(review.reason_code, "impact_scope_unknown");
        let deferred = apply_review_action(
            &mut conn,
            ChangeReviewActionInput {
                review_id: review.id.clone(),
                action: "defer".into(),
                expected_revision: 1,
                note: None,
            },
        )
        .unwrap();
        assert_eq!(deferred.review.status, "deferred");
        let reopened = apply_review_action(
            &mut conn,
            ChangeReviewActionInput {
                review_id: review.id,
                action: "reopen".into(),
                expected_revision: 2,
                note: None,
            },
        )
        .unwrap();
        assert_eq!(reopened.review.status, "pending");
    }

    #[test]
    fn reject_does_not_create_relation_and_revision_conflict_is_reported() {
        let mut conn = database();
        project(&mut conn);
        let task = object(&mut conn, WorkObjectKind::Task, "task", json!({}));
        let change = object(
            &mut conn,
            WorkObjectKind::Change,
            "change-task",
            json!({"changeType": "scope_changed"}),
        );
        let tx = conn.transaction().unwrap();
        let review = create_change_reviews_in_tx(
            &tx,
            ChangeAnalysisInput {
                project_id: "project_phase3",
                change: &change,
                new_artifact: None,
                previous_artifact_id: None,
                external_refs: vec![],
                explicit_affected_object_ids: vec![task.id.clone()],
                explicit_impacts: vec![],
            },
        )
        .unwrap()
        .remove(0);
        tx.commit().unwrap();
        let error = apply_review_action(
            &mut conn,
            ChangeReviewActionInput {
                review_id: review.id.clone(),
                action: "reject".into(),
                expected_revision: 9,
                note: None,
            },
        )
        .unwrap_err();
        assert_eq!(error, "CHANGE_REVIEW_REVISION_CONFLICT");
        let rejected = apply_review_action(
            &mut conn,
            ChangeReviewActionInput {
                review_id: review.id,
                action: "reject".into(),
                expected_revision: 1,
                note: Some("与该任务无关".into()),
            },
        )
        .unwrap();
        assert_eq!(rejected.review.status, "rejected");
        assert!(!rejected.relation_created);
    }

    #[test]
    fn explicit_contradiction_requires_acceptance_before_relation_exists() {
        let mut conn = database();
        project(&mut conn);
        let decision = object(
            &mut conn,
            WorkObjectKind::Decision,
            "decision-contradicted",
            json!({"decision": "采用旧方案", "reason": "当时成本最低"}),
        );
        let change = object(
            &mut conn,
            WorkObjectKind::Change,
            "change-contradiction",
            json!({"changeType": "new_evidence"}),
        );
        let tx = conn.transaction().unwrap();
        let review = create_change_reviews_in_tx(
            &tx,
            ChangeAnalysisInput {
                project_id: "project_phase3",
                change: &change,
                new_artifact: None,
                previous_artifact_id: None,
                external_refs: vec!["source:new-report".into()],
                explicit_affected_object_ids: vec![],
                explicit_impacts: vec![ExplicitImpact {
                    object_id: decision.id.clone(),
                    relation: "contradicts".into(),
                    explanation: Some("新版报告给出相反结论".into()),
                    evidence_refs: vec!["source:new-report".into()],
                    confidence: 0.9,
                }],
            },
        )
        .unwrap()
        .remove(0);
        tx.commit().unwrap();
        assert_eq!(review.proposed_relation, "contradicts");
        let before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM work_relations WHERE relation='contradicts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(before, 0);
        apply_review_action(
            &mut conn,
            ChangeReviewActionInput {
                review_id: review.id,
                action: "accept".into(),
                expected_revision: 1,
                note: None,
            },
        )
        .unwrap();
        let after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM work_relations WHERE source_id=?1 AND target_id=?2 AND relation='contradicts'",
                params![change.id, decision.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after, 1);
    }
}
