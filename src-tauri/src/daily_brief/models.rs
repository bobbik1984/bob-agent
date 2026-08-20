use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DAILY_BRIEF_SCHEMA_VERSION: u32 = 1;
pub const DAILY_BRIEF_DETAIL_LIMIT: usize = 50;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefStatus {
    Fresh,
    Partial,
    Stale,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefSource {
    WorkCore,
    GoalRuntime,
    Calendar,
    Todo,
    Conversation,
    Dream,
}

impl BriefSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkCore => "work_core",
            Self::GoalRuntime => "goal_runtime",
            Self::Calendar => "calendar",
            Self::Todo => "todo",
            Self::Conversation => "conversation",
            Self::Dream => "dream",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Ok,
    Unavailable,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefItemKind {
    Approval,
    Risk,
    Due,
    Schedule,
    Progress,
    Change,
    ContinueConversation,
    Insight,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefActionKind {
    OpenGoal,
    OpenWorkObject,
    OpenCalendar,
    OpenTodo,
    ContinueConversation,
    RespondApproval,
    OpenDetails,
    None,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyBriefAction {
    pub kind: BriefActionKind,
    #[serde(default)]
    pub target_type: Option<String>,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

impl DailyBriefAction {
    pub fn none() -> Self {
        Self {
            kind: BriefActionKind::None,
            target_type: None,
            target_id: None,
            payload: Value::Null,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyBriefItem {
    pub item_id: String,
    pub canonical_ref: String,
    pub source: BriefSource,
    pub source_id: String,
    pub source_revision: String,
    pub kind: BriefItemKind,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_key: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub summary_key: Option<String>,
    #[serde(default)]
    pub message_args: Value,
    pub priority: i32,
    pub requires_attention: bool,
    #[serde(default)]
    pub occurred_at: Option<i64>,
    #[serde(default)]
    pub due_at: Option<i64>,
    pub action: DailyBriefAction,
    #[serde(default)]
    pub reason_codes: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

impl DailyBriefItem {
    pub fn stable_id(source: BriefSource, source_id: &str) -> String {
        format!("brief:{}:{}", source.as_str(), source_id.trim())
    }

    pub fn content_revision_key(&self) -> String {
        format!("{}@{}", self.item_id, self.source_revision)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefSectionCounts {
    pub attention: usize,
    pub today: usize,
    pub in_progress: usize,
    pub changes: usize,
    pub insights: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceHealth {
    pub source: BriefSource,
    pub state: SourceState,
    pub revision: String,
    #[serde(default)]
    pub error_code: Option<String>,
}

impl SourceHealth {
    pub fn ok(source: BriefSource, revision: impl Into<String>) -> Self {
        Self {
            source,
            state: SourceState::Ok,
            revision: revision.into(),
            error_code: None,
        }
    }

    pub fn error(source: BriefSource, error_code: impl Into<String>) -> Self {
        Self {
            source,
            state: SourceState::Error,
            revision: "0".into(),
            error_code: Some(error_code.into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateContext {
    pub local_date: String,
    pub utc_offset_minutes: i32,
}

impl DateContext {
    pub fn validate(&self) -> Result<(), String> {
        chrono::NaiveDate::parse_from_str(&self.local_date, "%Y-%m-%d")
            .map_err(|_| "ERR-BRIEF-DATE".to_string())?;
        if !(-14 * 60..=14 * 60).contains(&self.utc_offset_minutes) {
            return Err("ERR-BRIEF-OFFSET".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyBriefSnapshot {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub local_date: String,
    pub revision: u64,
    pub generated_at: i64,
    pub status: BriefStatus,
    #[serde(default)]
    pub focus_item: Option<DailyBriefItem>,
    #[serde(default)]
    pub attention_items: Vec<DailyBriefItem>,
    #[serde(default)]
    pub detail_items: Vec<DailyBriefItem>,
    pub section_counts: BriefSectionCounts,
    pub actionable_count: usize,
    #[serde(default)]
    pub changed_since_last_seen: Vec<String>,
    #[serde(default)]
    pub source_health: Vec<SourceHealth>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl DailyBriefSnapshot {
    pub fn all_items(&self) -> impl Iterator<Item = &DailyBriefItem> {
        self.focus_item
            .iter()
            .chain(self.attention_items.iter())
            .chain(self.detail_items.iter())
    }
}

#[derive(Clone, Debug, Default)]
pub struct SourceCollection {
    pub items: Vec<DailyBriefItem>,
    pub health: Vec<SourceHealth>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_context_rejects_invalid_values() {
        assert!(DateContext {
            local_date: "2026-08-11".into(),
            utc_offset_minutes: 480,
        }
        .validate()
        .is_ok());
        assert!(DateContext {
            local_date: "08/11/2026".into(),
            utc_offset_minutes: 480,
        }
        .validate()
        .is_err());
        assert!(DateContext {
            local_date: "2026-08-11".into(),
            utc_offset_minutes: 900,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn item_ids_are_stable_and_source_scoped() {
        assert_eq!(
            DailyBriefItem::stable_id(BriefSource::GoalRuntime, "run_1"),
            "brief:goal_runtime:run_1"
        );
        assert_ne!(
            DailyBriefItem::stable_id(BriefSource::GoalRuntime, "1"),
            DailyBriefItem::stable_id(BriefSource::Todo, "1")
        );
    }
}
