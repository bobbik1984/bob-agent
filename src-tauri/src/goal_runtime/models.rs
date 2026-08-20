use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

pub const GOAL_RUNTIME_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalRisk {
    R0,
    R1,
    R2,
    R3,
}

impl GoalRisk {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::R0 => "r0",
            Self::R1 => "r1",
            Self::R2 => "r2",
            Self::R3 => "r3",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "r0" => Some(Self::R0),
            "r1" => Some(Self::R1),
            "r2" => Some(Self::R2),
            "r3" => Some(Self::R3),
            _ => None,
        }
    }

    pub fn requires_approval(self) -> bool {
        matches!(self, Self::R2 | Self::R3)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRuleKind {
    Deterministic,
    Rubric,
    UserAcceptance,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Pending,
    Verified,
    Unverified,
    Rejected,
}

impl VerificationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "verified" => Some(Self::Verified),
            "unverified" => Some(Self::Unverified),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

fn default_required() -> bool {
    true
}

fn default_verification_state() -> VerificationState {
    VerificationState::Pending
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRule {
    pub rule_id: String,
    pub description: String,
    pub kind: EvidenceRuleKind,
    #[serde(default = "default_required")]
    pub required: bool,
    #[serde(default)]
    pub allowed_evidence_types: Vec<String>,
    #[serde(default)]
    pub verifier: Value,
    #[serde(default = "default_verification_state")]
    pub verification_state: VerificationState,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalScope {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub allowed_refs: Vec<String>,
    #[serde(default)]
    pub global_file_access: bool,
}

fn default_runtime_seconds() -> u64 {
    900
}
fn default_slice_seconds() -> u64 {
    120
}
fn default_model_calls() -> u32 {
    8
}
fn default_tool_calls() -> u32 {
    30
}
fn default_repairs() -> u32 {
    2
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalBudget {
    #[serde(default = "default_runtime_seconds")]
    pub max_runtime_seconds: u64,
    #[serde(default = "default_slice_seconds")]
    pub max_slice_seconds: u64,
    #[serde(default = "default_model_calls")]
    pub max_model_calls: u32,
    #[serde(default = "default_tool_calls")]
    pub max_tool_calls: u32,
    #[serde(default = "default_repairs")]
    pub max_repairs: u32,
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

impl Default for GoalBudget {
    fn default() -> Self {
        Self {
            max_runtime_seconds: default_runtime_seconds(),
            max_slice_seconds: default_slice_seconds(),
            max_model_calls: default_model_calls(),
            max_tool_calls: default_tool_calls(),
            max_repairs: default_repairs(),
            max_tokens: None,
        }
    }
}

fn default_auto_risk() -> GoalRisk {
    GoalRisk::R1
}
fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRiskPolicy {
    #[serde(default = "default_auto_risk")]
    pub max_auto_risk: GoalRisk,
    #[serde(default = "default_true")]
    pub trusted_device_required_for_r3: bool,
}

impl Default for GoalRiskPolicy {
    fn default() -> Self {
        Self {
            max_auto_risk: default_auto_risk(),
            trusted_device_required_for_r3: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetExhaustedAction {
    Fail,
    AskUser,
}

fn default_budget_action() -> BudgetExhaustedAction {
    BudgetExhaustedAction::AskUser
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalBlockerPolicy {
    #[serde(default = "default_budget_action")]
    pub on_budget_exhausted: BudgetExhaustedAction,
    #[serde(default = "default_true")]
    pub ask_on_missing_scope: bool,
    #[serde(default = "default_true")]
    pub ask_on_ambiguous_choice: bool,
}

impl Default for GoalBlockerPolicy {
    fn default() -> Self {
        Self {
            on_budget_exhausted: default_budget_action(),
            ask_on_missing_scope: true,
            ask_on_ambiguous_choice: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRecoveryPolicy {
    #[serde(default = "default_true")]
    pub resume_r0_r1_on_startup: bool,
    #[serde(default = "default_true")]
    pub block_unknown_side_effect: bool,
}

impl Default for GoalRecoveryPolicy {
    fn default() -> Self {
        Self {
            resume_r0_r1_on_startup: true,
            block_unknown_side_effect: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalCreatedFrom {
    #[serde(default)]
    pub route_source: String,
    #[serde(default)]
    pub route_confidence: f32,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalContract {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub original_request: String,
    pub outcome: String,
    #[serde(default)]
    pub evidence_rules: Vec<EvidenceRule>,
    #[serde(default)]
    pub scope: GoalScope,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub budget: GoalBudget,
    #[serde(default)]
    pub risk_policy: GoalRiskPolicy,
    #[serde(default)]
    pub blocker_policy: GoalBlockerPolicy,
    #[serde(default)]
    pub recovery_policy: GoalRecoveryPolicy,
    #[serde(default)]
    pub created_from: GoalCreatedFrom,
}

fn schema_version() -> u32 {
    GOAL_RUNTIME_SCHEMA_VERSION
}

fn normalize_text(value: String) -> String {
    value.trim().to_string()
}

fn normalize_list(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(normalize_text)
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}

impl GoalContract {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        if value.get("schemaVersion").is_none() && value.get("outcome").is_some() {
            return Ok(Self::legacy(
                value
                    .get("outcome")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ));
        }
        serde_json::from_value::<Self>(value.clone())
            .map_err(|error| format!("Goal Contract 格式无效: {error}"))?
            .normalize()
    }

    pub fn legacy(outcome: &str) -> Self {
        Self {
            schema_version: GOAL_RUNTIME_SCHEMA_VERSION,
            original_request: outcome.trim().to_string(),
            outcome: outcome.trim().to_string(),
            evidence_rules: vec![],
            scope: GoalScope::default(),
            constraints: vec![],
            budget: GoalBudget::default(),
            risk_policy: GoalRiskPolicy::default(),
            blocker_policy: GoalBlockerPolicy::default(),
            recovery_policy: GoalRecoveryPolicy::default(),
            created_from: GoalCreatedFrom::default(),
        }
    }

    pub fn normalize(mut self) -> Result<Self, String> {
        self.schema_version = GOAL_RUNTIME_SCHEMA_VERSION;
        self.original_request = normalize_text(self.original_request);
        self.outcome = normalize_text(self.outcome);
        self.constraints = normalize_list(self.constraints);
        self.scope.allowed_refs = normalize_list(self.scope.allowed_refs);
        self.scope.project_id = self
            .scope
            .project_id
            .map(normalize_text)
            .filter(|value| !value.is_empty());
        if self.outcome.is_empty() {
            return Err("Goal Contract 缺少 outcome".into());
        }
        if self.outcome.chars().count() > 500 {
            return Err("Goal Contract outcome 不能超过 500 个字符".into());
        }
        if self.budget.max_slice_seconds == 0
            || self.budget.max_runtime_seconds < self.budget.max_slice_seconds
            || self.budget.max_model_calls == 0
            || self.budget.max_tool_calls == 0
        {
            return Err("Goal Contract budget 无效".into());
        }
        let mut seen_rules = HashSet::new();
        for rule in &mut self.evidence_rules {
            rule.rule_id = normalize_text(std::mem::take(&mut rule.rule_id));
            rule.description = normalize_text(std::mem::take(&mut rule.description));
            rule.allowed_evidence_types =
                normalize_list(std::mem::take(&mut rule.allowed_evidence_types));
            if rule.rule_id.is_empty() || rule.description.is_empty() {
                return Err("EvidenceRule 缺少 ruleId 或 description".into());
            }
            if !seen_rules.insert(rule.rule_id.clone()) {
                return Err(format!("EvidenceRule 重复: {}", rule.rule_id));
            }
        }
        Ok(self)
    }

    pub fn ready_for_execution(&self) -> bool {
        self.evidence_rules.iter().any(|rule| rule.required)
    }

    pub fn into_value(self) -> Result<Value, String> {
        serde_json::to_value(self.normalize()?).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStatus {
    Draft,
    Ready,
    Running,
    WaitingUser,
    Blocked,
    Verifying,
    Done,
    Failed,
    Cancelled,
}

impl GoalRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::WaitingUser => "waiting_user",
            Self::Blocked => "blocked",
            Self::Verifying => "verifying",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "ready" => Some(Self::Ready),
            "running" => Some(Self::Running),
            "waiting_user" => Some(Self::WaitingUser),
            "blocked" => Some(Self::Blocked),
            "verifying" => Some(Self::Verifying),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        match self {
            Self::Draft => matches!(
                next,
                Self::Ready | Self::WaitingUser | Self::Failed | Self::Cancelled
            ),
            Self::Ready => matches!(
                next,
                Self::Running
                    | Self::WaitingUser
                    | Self::Blocked
                    | Self::Verifying
                    | Self::Failed
                    | Self::Cancelled
            ),
            Self::Running => matches!(
                next,
                Self::Ready
                    | Self::WaitingUser
                    | Self::Blocked
                    | Self::Verifying
                    | Self::Failed
                    | Self::Cancelled
            ),
            Self::WaitingUser => matches!(
                next,
                Self::Ready | Self::Blocked | Self::Failed | Self::Cancelled
            ),
            Self::Blocked => matches!(
                next,
                Self::Ready | Self::WaitingUser | Self::Failed | Self::Cancelled
            ),
            Self::Verifying => matches!(
                next,
                Self::Ready
                    | Self::Running
                    | Self::Blocked
                    | Self::Done
                    | Self::Failed
                    | Self::Cancelled
            ),
            Self::Done | Self::Failed | Self::Cancelled => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalPhase {
    Compile,
    Observe,
    Plan,
    Act,
    Verify,
    Repair,
    Finish,
}

impl GoalPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compile => "compile",
            Self::Observe => "observe",
            Self::Plan => "plan",
            Self::Act => "act",
            Self::Verify => "verify",
            Self::Repair => "repair",
            Self::Finish => "finish",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "compile" => Some(Self::Compile),
            "observe" => Some(Self::Observe),
            "plan" => Some(Self::Plan),
            "act" => Some(Self::Act),
            "verify" => Some(Self::Verify),
            "repair" => Some(Self::Repair),
            "finish" => Some(Self::Finish),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRun {
    pub run_id: String,
    pub goal_id: String,
    pub project_id: String,
    pub status: GoalRunStatus,
    pub phase: GoalPhase,
    pub verification_state: VerificationState,
    pub risk: GoalRisk,
    pub model_calls_used: u32,
    pub tool_calls_used: u32,
    pub repairs_used: u32,
    pub runtime_seconds_used: u64,
    pub latest_checkpoint_id: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<i64>,
    pub recovery_count: u32,
    pub last_error_code: Option<String>,
    pub last_error_detail: Option<String>,
    pub next_action: Option<String>,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalAttempt {
    pub attempt_id: String,
    pub run_id: String,
    pub phase: GoalPhase,
    pub status: String,
    pub executor: String,
    pub plan_summary: Option<String>,
    pub result_summary: Option<String>,
    pub tool_receipts: Value,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalEvidence {
    pub evidence_id: String,
    pub run_id: String,
    pub rule_id: String,
    pub evidence_type: String,
    pub reference: String,
    pub content_hash: Option<String>,
    pub verification_state: VerificationState,
    pub verifier: String,
    pub detail: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalCheckpoint {
    pub checkpoint_id: String,
    pub run_id: String,
    pub phase: GoalPhase,
    pub checkpoint_type: String,
    pub payload: Value,
    pub safe_to_resume: bool,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalChoiceSemantic {
    Approve,
    Reject,
    Defer,
    Handoff,
    SelectOption,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionChoice {
    pub choice_id: String,
    pub label_key: String,
    pub semantic: ApprovalChoiceSemantic,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Resolved,
    Expired,
}

impl ApprovalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolved => "resolved",
            Self::Expired => "expired",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "resolved" => Some(Self::Resolved),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalApproval {
    pub approval_id: String,
    pub run_id: String,
    pub summary: String,
    pub risk: GoalRisk,
    pub choices: Vec<ActionChoice>,
    pub trusted_device_required: bool,
    pub status: ApprovalStatus,
    pub selected_choice_id: Option<String>,
    pub decided_by: Option<String>,
    pub decided_device_id: Option<String>,
    pub input_modality: Option<String>,
    pub expires_at: Option<i64>,
    pub revision: u64,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecisionInput {
    pub approval_id: String,
    pub choice_id: String,
    pub expected_revision: u64,
    pub actor: String,
    pub device_id: String,
    pub input_modality: String,
    #[serde(default)]
    pub trusted_device: bool,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalEvent {
    pub event_id: String,
    pub run_id: String,
    pub event_type: String,
    pub actor: String,
    pub payload: Value,
    pub idempotency_key: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunInput {
    pub goal_id: String,
    pub project_id: String,
    pub risk: GoalRisk,
    pub initial_status: GoalRunStatus,
    #[serde(default)]
    pub next_action: Option<String>,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionRunInput {
    pub run_id: String,
    pub status: GoalRunStatus,
    pub phase: GoalPhase,
    pub verification_state: VerificationState,
    pub expected_revision: u64,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_detail: Option<String>,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCheckpointInput {
    pub run_id: String,
    pub phase: GoalPhase,
    pub checkpoint_type: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub safe_to_resume: bool,
    pub expected_revision: u64,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEvidenceInput {
    pub run_id: String,
    pub rule_id: String,
    pub evidence_type: String,
    pub reference: String,
    #[serde(default)]
    pub content_hash: Option<String>,
    pub verification_state: VerificationState,
    pub verifier: String,
    #[serde(default)]
    pub detail: Option<String>,
    pub expected_revision: u64,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApprovalInput {
    pub run_id: String,
    pub summary: String,
    pub risk: GoalRisk,
    pub choices: Vec<ActionChoice>,
    #[serde(default)]
    pub trusted_device_required: bool,
    #[serde(default)]
    pub expires_at: Option<i64>,
    pub expected_revision: u64,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquireLeaseInput {
    pub run_id: String,
    pub owner: String,
    pub ttl_seconds: u64,
    pub expected_revision: u64,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAttemptInput {
    pub run_id: String,
    pub phase: GoalPhase,
    pub executor: String,
    #[serde(default)]
    pub plan_summary: Option<String>,
    pub expected_revision: u64,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishAttemptInput {
    pub attempt_id: String,
    pub run_id: String,
    pub status: String,
    #[serde(default)]
    pub result_summary: Option<String>,
    #[serde(default)]
    pub tool_receipts: Value,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_detail: Option<String>,
    #[serde(default)]
    pub tool_calls_used: u32,
    #[serde(default)]
    pub runtime_seconds_used: u64,
    pub expected_revision: u64,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySummary {
    pub recovered_ready: usize,
    pub blocked_unknown_side_effect: usize,
    pub untouched: usize,
}

pub fn default_runtime_choices(risk: GoalRisk) -> Vec<ActionChoice> {
    vec![
        ActionChoice {
            choice_id: "approve".into(),
            label_key: if risk == GoalRisk::R3 {
                "goal.approval_handoff".into()
            } else {
                "goal.approval_approve".into()
            },
            semantic: if risk == GoalRisk::R3 {
                ApprovalChoiceSemantic::Handoff
            } else {
                ApprovalChoiceSemantic::Approve
            },
            payload: json!({}),
        },
        ActionChoice {
            choice_id: "defer".into(),
            label_key: "goal.approval_defer".into(),
            semantic: ApprovalChoiceSemantic::Defer,
            payload: json!({}),
        },
        ActionChoice {
            choice_id: "cancel".into(),
            label_key: "goal.approval_cancel".into(),
            semantic: ApprovalChoiceSemantic::Reject,
            payload: json!({}),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_goal_stays_readable_but_is_not_ready_without_evidence() {
        let contract = GoalContract::from_value(&json!({ "outcome": "完成报告" })).unwrap();
        assert_eq!(contract.outcome, "完成报告");
        assert!(!contract.ready_for_execution());
    }

    #[test]
    fn contract_requires_unique_evidence_rules() {
        let mut contract = GoalContract::legacy("完成报告");
        contract.evidence_rules = vec![
            EvidenceRule {
                rule_id: "result".into(),
                description: "报告文件存在".into(),
                kind: EvidenceRuleKind::Deterministic,
                required: true,
                allowed_evidence_types: vec!["file".into()],
                verifier: json!({}),
                verification_state: VerificationState::Pending,
            },
            EvidenceRule {
                rule_id: "result".into(),
                description: "重复".into(),
                kind: EvidenceRuleKind::Deterministic,
                required: true,
                allowed_evidence_types: vec![],
                verifier: json!({}),
                verification_state: VerificationState::Pending,
            },
        ];
        assert!(contract.normalize().unwrap_err().contains("重复"));
    }

    #[test]
    fn terminal_states_cannot_reopen_implicitly() {
        assert!(!GoalRunStatus::Done.can_transition_to(GoalRunStatus::Ready));
        assert!(!GoalRunStatus::Cancelled.can_transition_to(GoalRunStatus::Running));
        assert!(GoalRunStatus::Running.can_transition_to(GoalRunStatus::Ready));
    }

    #[test]
    fn r3_uses_handoff_instead_of_plain_watch_approval() {
        let choices = default_runtime_choices(GoalRisk::R3);
        assert_eq!(choices[0].semantic, ApprovalChoiceSemantic::Handoff);
        assert!(choices.len() <= 4);
    }
}
