use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const WORK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkObjectKind {
    Responsibility,
    Goal,
    Milestone,
    Task,
    Decision,
    Artifact,
    Evidence,
    Risk,
    Change,
    Commitment,
}

impl WorkObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Responsibility => "responsibility",
            Self::Goal => "goal",
            Self::Milestone => "milestone",
            Self::Task => "task",
            Self::Decision => "decision",
            Self::Artifact => "artifact",
            Self::Evidence => "evidence",
            Self::Risk => "risk",
            Self::Change => "change",
            Self::Commitment => "commitment",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "responsibility" => Some(Self::Responsibility),
            "goal" => Some(Self::Goal),
            "milestone" => Some(Self::Milestone),
            "task" => Some(Self::Task),
            "decision" => Some(Self::Decision),
            "artifact" => Some(Self::Artifact),
            "evidence" => Some(Self::Evidence),
            "risk" => Some(Self::Risk),
            "change" => Some(Self::Change),
            "commitment" => Some(Self::Commitment),
            _ => None,
        }
    }

    pub fn id_prefix(self) -> &'static str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkProject {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub mission: String,
    pub status: String,
    pub current_phase: Option<String>,
    pub summary: Option<String>,
    pub source_ref: Option<String>,
    pub metadata: Value,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkObject {
    pub schema_version: u32,
    pub id: String,
    pub kind: WorkObjectKind,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub status: String,
    pub description: Option<String>,
    pub data: Value,
    pub source_capture_id: Option<String>,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRelation {
    pub id: String,
    pub project_id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation: String,
    pub evidence_ref: Option<String>,
    pub confidence: f64,
    pub created_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkEvent {
    pub id: String,
    pub project_id: String,
    pub object_id: Option<String>,
    pub event_type: String,
    pub actor: String,
    pub payload: Value,
    pub idempotency_key: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAggregate {
    pub project: WorkProject,
    pub responsibilities: Vec<WorkObject>,
    pub goals: Vec<WorkObject>,
    pub milestones: Vec<WorkObject>,
    pub tasks: Vec<WorkObject>,
    pub decisions: Vec<WorkObject>,
    pub artifacts: Vec<WorkObject>,
    pub evidence: Vec<WorkObject>,
    pub risks: Vec<WorkObject>,
    pub changes: Vec<WorkObject>,
    pub commitments: Vec<WorkObject>,
    pub recent_events: Vec<WorkEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    #[serde(default)]
    pub project_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub mission: String,
    #[serde(default)]
    pub current_phase: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub actor: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkObjectInput {
    pub kind: WorkObjectKind,
    pub project_id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub source_capture_id: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkStatusInput {
    pub object_id: String,
    pub status: String,
    pub expected_revision: u64,
    #[serde(default)]
    pub actor: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorkObjectInput {
    pub object_id: String,
    pub expected_revision: u64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRelationInput {
    pub project_id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation: String,
    #[serde(default)]
    pub evidence_ref: Option<String>,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub actor: Option<String>,
    pub idempotency_key: String,
}

fn default_confidence() -> f64 {
    1.0
}

pub fn validate_project_id(id: &str) -> Result<(), String> {
    if id.starts_with("project_") && id.len() > "project_".len() {
        Ok(())
    } else {
        Err("project ID 必须以 project_ 开头".into())
    }
}

pub fn validate_title(title: &str) -> Result<(), String> {
    let length = title.trim().chars().count();
    if length == 0 {
        Err("标题不能为空".into())
    } else if length > 200 {
        Err("标题不能超过 200 个字符".into())
    } else {
        Ok(())
    }
}

pub fn validate_status(status: &str) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "draft",
        "active",
        "pending",
        "ready",
        "running",
        "blocked",
        "needs_review",
        "accepted",
        "done",
        "failed",
        "cancelled",
        "superseded",
        "on_hold",
        "archived",
    ];
    if ALLOWED.contains(&status) {
        Ok(())
    } else {
        Err(format!("不支持的工作状态: {status}"))
    }
}

pub fn validate_object_payload(kind: WorkObjectKind, data: &Value) -> Result<(), String> {
    if !data.is_object() {
        return Err("工作对象 data 必须是 JSON object".into());
    }
    let required = match kind {
        WorkObjectKind::Goal => &["outcome"][..],
        WorkObjectKind::Decision => &["decision", "reason"][..],
        WorkObjectKind::Evidence => &["evidenceType", "reference"][..],
        WorkObjectKind::Commitment => &["owner", "dueAt"][..],
        _ => &[][..],
    };
    let missing = required
        .iter()
        .filter(|key| {
            data.get(**key)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        })
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} 缺少必填字段: {}",
            kind.as_str(),
            missing.join(", ")
        ))
    }
}
