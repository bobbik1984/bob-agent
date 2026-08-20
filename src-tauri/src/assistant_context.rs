use crate::complexity_router::RouteMode;
use crate::work_core::models::{ProjectAggregate, WorkObject, WorkObjectKind, WorkProject};
use crate::work_core::repository;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const HIGH_CONFIDENCE: f32 = 0.85;
const AMBIGUITY_MARGIN: f32 = 0.20;
const MIN_AMBIGUOUS_SCORE: f32 = 0.60;
const MAX_FACT_SUMMARY_CHARS: usize = 500;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PurposeFrame {
    pub raw_intent: String,
    pub desired_outcome: String,
    pub explicit_constraints: Vec<String>,
    pub candidate_refs: Vec<String>,
    pub requested_capability_hints: Vec<String>,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContextFactKind {
    Project,
    Goal,
    Task,
    Decision,
    Artifact,
    Evidence,
    Risk,
    Change,
    Commitment,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextFact {
    pub kind: ContextFactKind,
    pub object_id: String,
    pub title: String,
    pub status: String,
    pub summary: String,
    pub source_ref: String,
    pub source_revision: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextObjectRef {
    pub object_id: String,
    pub object_kind: String,
    pub project_id: String,
    pub title: String,
    pub source_ref: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextConflict {
    pub candidate_ids: Vec<String>,
    pub reason_code: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssistantContext {
    pub active_object: Option<ContextObjectRef>,
    pub relevant_facts: Vec<ContextFact>,
    pub conflicts: Vec<ContextConflict>,
    pub confidence: f32,
    pub reason_codes: Vec<String>,
    pub generated_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContextBudget {
    pub max_facts: usize,
    pub max_chars: usize,
}

impl ContextBudget {
    pub(crate) fn for_route(route: RouteMode) -> Self {
        match route {
            RouteMode::Direct => Self {
                max_facts: 6,
                max_chars: 3_000,
            },
            RouteMode::Deep => Self {
                max_facts: 12,
                max_chars: 8_000,
            },
            RouteMode::Advanced => Self {
                max_facts: 20,
                max_chars: 16_000,
            },
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ContextSourceSnapshot {
    pub projects: Vec<WorkProject>,
    pub aggregates: Vec<ProjectAggregate>,
    pub focus_project_ids: Vec<String>,
}

impl ContextSourceSnapshot {
    pub(crate) fn load(conn: &Connection) -> Result<Self, String> {
        let projects = repository::list_projects(conn)?
            .into_iter()
            .filter(|project| is_active_status(&project.status) && project.deleted_at.is_none())
            .collect::<Vec<_>>();
        let mut aggregates = Vec::with_capacity(projects.len());
        for project in &projects {
            aggregates.push(repository::get_project_aggregate(conn, &project.id)?);
        }
        Ok(Self {
            projects,
            aggregates,
            focus_project_ids: Vec::new(),
        })
    }
}

pub(crate) fn resolve_context_from_work_core(
    conn: &Connection,
    raw_intent: &str,
    route: RouteMode,
    generated_at: i64,
) -> Result<(PurposeFrame, ContextSourceSnapshot, AssistantContext), String> {
    let purpose = compile_purpose(raw_intent);
    let snapshot = ContextSourceSnapshot::load(conn)?;
    let context = resolve_context(&purpose, &snapshot, route, generated_at);
    Ok((purpose, snapshot, context))
}

#[derive(Clone, Debug)]
struct ScoredCandidate {
    project: WorkProject,
    score: f32,
    reason_codes: Vec<String>,
}

pub(crate) fn compile_purpose(raw_intent: &str) -> PurposeFrame {
    let desired_outcome = raw_intent.trim().to_string();
    let normalized_intent = desired_outcome.to_lowercase();
    let mut capability_hints = Vec::new();
    for (patterns, hint) in [
        (&["powershell", "pwsh"][..], "powershell"),
        (&["浏览器", "browser", "网页点击"][..], "browser"),
        (&["桌面", "desktop"][..], "desktop_file"),
        (&["手机沙盒", "mobile sandbox"][..], "mobile_sandbox"),
    ] {
        if patterns
            .iter()
            .any(|pattern| normalized_intent.contains(pattern))
        {
            capability_hints.push(hint.to_string());
        }
    }

    let constraint_markers = [
        "不要", "别", "不能", "只要", "仅", "必须", "do not", "don't", "only", "must",
    ];
    let explicit_constraints = desired_outcome
        .split(|character| {
            matches!(
                character,
                '。' | '！' | '!' | '；' | ';' | '，' | ',' | '\n'
            )
        })
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter(|part| {
            let normalized = part.to_lowercase();
            constraint_markers
                .iter()
                .any(|marker| normalized.contains(marker))
        })
        .map(|part| truncate_chars(part, 300))
        .collect::<Vec<_>>();

    let mut candidate_refs = desired_outcome
        .split(|character: char| {
            !(character.is_alphanumeric() || character == '_' || character == '-')
        })
        .filter(|token| {
            let token = token.to_ascii_lowercase();
            ["project_", "goal_", "task_", "work_", "artifact_"]
                .iter()
                .any(|prefix| token.starts_with(prefix))
        })
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    candidate_refs.sort();
    candidate_refs.dedup();
    capability_hints.sort();
    capability_hints.dedup();

    PurposeFrame {
        raw_intent: desired_outcome.clone(),
        desired_outcome,
        explicit_constraints,
        candidate_refs,
        requested_capability_hints: capability_hints,
        confidence: if raw_intent.trim().is_empty() {
            0.0
        } else {
            1.0
        },
    }
}

pub(crate) fn resolve_context(
    purpose: &PurposeFrame,
    snapshot: &ContextSourceSnapshot,
    route: RouteMode,
    generated_at: i64,
) -> AssistantContext {
    let mut candidates = snapshot
        .projects
        .iter()
        .filter(|project| is_active_status(&project.status) && project.deleted_at.is_none())
        .filter_map(|project| score_project(purpose, project, snapshot))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.project.id.cmp(&right.project.id))
    });

    let mut context = AssistantContext {
        active_object: None,
        relevant_facts: Vec::new(),
        conflicts: Vec::new(),
        confidence: candidates.first().map(|item| item.score).unwrap_or(0.0),
        reason_codes: Vec::new(),
        generated_at,
    };

    let Some(top) = candidates.first() else {
        context.reason_codes.push("context.no_candidate".into());
        return context;
    };

    if let Some(second) = candidates.get(1) {
        if second.score >= MIN_AMBIGUOUS_SCORE && top.score - second.score < AMBIGUITY_MARGIN {
            context.conflicts.push(ContextConflict {
                candidate_ids: candidates
                    .iter()
                    .take_while(|candidate| top.score - candidate.score < AMBIGUITY_MARGIN)
                    .map(|candidate| candidate.project.id.clone())
                    .collect(),
                reason_code: "context.ambiguous_candidates".into(),
            });
            context
                .reason_codes
                .push("context.ambiguous_candidates".into());
            return context;
        }
    }

    if top.score < HIGH_CONFIDENCE {
        context.reason_codes.push("context.low_confidence".into());
        context.reason_codes.extend(top.reason_codes.clone());
        normalize_reason_codes(&mut context.reason_codes);
        return context;
    }

    context.active_object = Some(ContextObjectRef {
        object_id: top.project.id.clone(),
        object_kind: "project".into(),
        project_id: top.project.id.clone(),
        title: top.project.title.clone(),
        source_ref: format!("work_project:{}@{}", top.project.id, top.project.revision),
    });
    context.reason_codes.extend(top.reason_codes.clone());
    if let Some(aggregate) = snapshot
        .aggregates
        .iter()
        .find(|aggregate| aggregate.project.id == top.project.id)
    {
        context.relevant_facts = collect_facts(aggregate, route);
    } else {
        context
            .reason_codes
            .push("context.source_unavailable".into());
    }
    apply_budget(&mut context, ContextBudget::for_route(route));
    normalize_reason_codes(&mut context.reason_codes);
    context
}

pub(crate) fn render_context_packet(purpose: &PurposeFrame, context: &AssistantContext) -> String {
    let Some(active) = context.active_object.as_ref() else {
        return String::new();
    };

    let mut output = String::from("[Bob Assistant Context]\n");
    output.push_str(
        "Use only the sourced facts below. Explicit user constraints override history.\n",
    );
    output.push_str(&format!(
        "purpose: {}\nactive_object: project | {} | {} | {}\n",
        single_line(&purpose.desired_outcome),
        active.object_id,
        single_line(&active.title),
        active.source_ref
    ));
    if !purpose.explicit_constraints.is_empty() {
        output.push_str("explicit_constraints:\n");
        for constraint in &purpose.explicit_constraints {
            output.push_str(&format!("- {}\n", single_line(constraint)));
        }
    }
    if !context.relevant_facts.is_empty() {
        output.push_str("facts:\n");
        for fact in &context.relevant_facts {
            output.push_str(&format!(
                "- {:?} | {} | {} | {} | {}\n",
                fact.kind,
                fact.source_ref,
                single_line(&fact.title),
                fact.status,
                single_line(&fact.summary)
            ));
        }
    }
    output
}

fn score_project(
    purpose: &PurposeFrame,
    project: &WorkProject,
    snapshot: &ContextSourceSnapshot,
) -> Option<ScoredCandidate> {
    let raw = purpose.raw_intent.to_lowercase();
    let normalized_raw = normalize_for_match(&raw);
    let normalized_title = normalize_for_match(&project.title);
    let generic_project_ref = contains_any(
        &raw,
        &[
            "这个项目",
            "本项目",
            "该项目",
            "项目",
            "this project",
            "the project",
        ],
    );
    let mut score = 0.0_f32;
    let mut reason_codes = Vec::new();

    if raw.contains(&project.id.to_lowercase())
        || purpose
            .candidate_refs
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&project.id))
    {
        score = 1.0;
        reason_codes.push("context.explicit_ref".into());
    }

    if normalized_title.chars().count() >= 2 && normalized_raw.contains(&normalized_title) {
        score = score.max(0.92);
        reason_codes.push("context.exact_title".into());
    }

    if let Some(aggregate) = snapshot
        .aggregates
        .iter()
        .find(|aggregate| aggregate.project.id == project.id)
    {
        for object in aggregate_objects(aggregate) {
            let normalized_object_title = normalize_for_match(&object.title);
            if raw.contains(&object.id.to_lowercase()) {
                score = score.max(0.96);
                reason_codes.push("context.explicit_ref".into());
            } else if normalized_object_title.chars().count() >= 2
                && normalized_raw.contains(&normalized_object_title)
            {
                score = score.max(0.88);
                reason_codes.push("context.exact_title".into());
            }
        }
    }

    if generic_project_ref && snapshot.focus_project_ids.contains(&project.id) {
        score = score.max(0.88);
        reason_codes.push("context.active_focus".into());
    } else if generic_project_ref {
        let active_count = snapshot
            .projects
            .iter()
            .filter(|candidate| {
                is_active_status(&candidate.status) && candidate.deleted_at.is_none()
            })
            .count();
        score = score.max(if active_count == 1 { 0.86 } else { 0.60 });
        reason_codes.push("context.active_focus".into());
    }

    if score == 0.0 {
        return None;
    }
    normalize_reason_codes(&mut reason_codes);
    Some(ScoredCandidate {
        project: project.clone(),
        score,
        reason_codes,
    })
}

fn collect_facts(aggregate: &ProjectAggregate, route: RouteMode) -> Vec<ContextFact> {
    let mut facts = vec![ContextFact {
        kind: ContextFactKind::Project,
        object_id: aggregate.project.id.clone(),
        title: aggregate.project.title.clone(),
        status: aggregate.project.status.clone(),
        summary: project_summary(&aggregate.project),
        source_ref: format!(
            "work_project:{}@{}",
            aggregate.project.id, aggregate.project.revision
        ),
        source_revision: aggregate.project.revision.to_string(),
        updated_at: aggregate.project.updated_at,
    }];

    for object in aggregate
        .goals
        .iter()
        .chain(aggregate.tasks.iter())
        .chain(aggregate.commitments.iter())
        .chain(aggregate.decisions.iter())
    {
        if is_active_status(&object.status) || object.kind == WorkObjectKind::Decision {
            facts.push(object_fact(object));
        }
    }
    if matches!(route, RouteMode::Deep | RouteMode::Advanced) {
        for object in aggregate
            .risks
            .iter()
            .chain(aggregate.changes.iter())
            .chain(aggregate.artifacts.iter())
            .chain(aggregate.evidence.iter())
        {
            if is_active_status(&object.status)
                || matches!(
                    object.kind,
                    WorkObjectKind::Artifact | WorkObjectKind::Evidence
                )
            {
                facts.push(object_fact(object));
            }
        }
    }
    facts.sort_by(|left, right| {
        fact_priority(left.kind)
            .cmp(&fact_priority(right.kind))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.object_id.cmp(&right.object_id))
    });
    facts
}

fn object_fact(object: &WorkObject) -> ContextFact {
    ContextFact {
        kind: match object.kind {
            WorkObjectKind::Goal => ContextFactKind::Goal,
            WorkObjectKind::Task | WorkObjectKind::Milestone => ContextFactKind::Task,
            WorkObjectKind::Decision => ContextFactKind::Decision,
            WorkObjectKind::Artifact => ContextFactKind::Artifact,
            WorkObjectKind::Evidence => ContextFactKind::Evidence,
            WorkObjectKind::Risk => ContextFactKind::Risk,
            WorkObjectKind::Change => ContextFactKind::Change,
            WorkObjectKind::Commitment | WorkObjectKind::Responsibility => {
                ContextFactKind::Commitment
            }
        },
        object_id: object.id.clone(),
        title: object.title.clone(),
        status: object.status.clone(),
        summary: object_summary(object),
        source_ref: format!("work_object:{}@{}", object.id, object.revision),
        source_revision: object.revision.to_string(),
        updated_at: object.updated_at,
    }
}

fn apply_budget(context: &mut AssistantContext, budget: ContextBudget) {
    let original_count = context.relevant_facts.len();
    context.relevant_facts.truncate(budget.max_facts);
    for fact in &mut context.relevant_facts {
        fact.summary = truncate_chars(&fact.summary, MAX_FACT_SUMMARY_CHARS);
    }
    while context.relevant_facts.len() > 1 && serialized_chars(context) > budget.max_chars {
        context.relevant_facts.pop();
    }
    if context.relevant_facts.len() < original_count || serialized_chars(context) > budget.max_chars
    {
        context.reason_codes.push("context.budget_truncated".into());
    }
}

fn serialized_chars(context: &AssistantContext) -> usize {
    serde_json::to_string(context)
        .unwrap_or_default()
        .chars()
        .count()
}

fn project_summary(project: &WorkProject) -> String {
    [
        Some(project.mission.as_str()),
        project.current_phase.as_deref(),
        project.summary.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" | ")
}

fn object_summary(object: &WorkObject) -> String {
    if let Some(description) = object.description.as_deref() {
        if !description.trim().is_empty() {
            return description.trim().to_string();
        }
    }
    for key in ["outcome", "reason", "reference", "owner"] {
        if let Some(value) = object.data.get(key).and_then(|value| value.as_str()) {
            if !value.trim().is_empty() {
                return value.trim().to_string();
            }
        }
    }
    String::new()
}

fn aggregate_objects(aggregate: &ProjectAggregate) -> Vec<&WorkObject> {
    aggregate
        .goals
        .iter()
        .chain(aggregate.milestones.iter())
        .chain(aggregate.tasks.iter())
        .chain(aggregate.decisions.iter())
        .chain(aggregate.artifacts.iter())
        .chain(aggregate.evidence.iter())
        .chain(aggregate.risks.iter())
        .chain(aggregate.changes.iter())
        .chain(aggregate.commitments.iter())
        .collect()
}

fn fact_priority(kind: ContextFactKind) -> u8 {
    match kind {
        ContextFactKind::Project => 0,
        ContextFactKind::Risk | ContextFactKind::Change => 1,
        ContextFactKind::Commitment => 2,
        ContextFactKind::Goal | ContextFactKind::Task => 3,
        ContextFactKind::Decision => 4,
        ContextFactKind::Evidence | ContextFactKind::Artifact => 5,
    }
}

fn normalize_for_match(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_active_status(status: &str) -> bool {
    !matches!(
        status.to_ascii_lowercase().as_str(),
        "done" | "completed" | "cancelled" | "rejected" | "archived" | "deleted"
    )
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| text.contains(pattern))
}

fn normalize_reason_codes(reason_codes: &mut Vec<String>) {
    reason_codes.sort();
    reason_codes.dedup();
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_core::models::CreateProjectInput;
    use crate::work_core::repository::{create_project, init_work_core_tables};
    use rusqlite::Connection;
    use serde_json::{json, Value};

    fn project(id: &str, title: &str, status: &str, updated_at: i64) -> WorkProject {
        WorkProject {
            schema_version: 1,
            id: id.into(),
            title: title.into(),
            mission: format!("推进{title}"),
            status: status.into(),
            current_phase: Some("评审准备".into()),
            summary: Some(format!("{title}当前摘要")),
            source_ref: None,
            metadata: Value::Null,
            revision: 2,
            created_at: 1,
            updated_at,
            deleted_at: None,
        }
    }

    fn object(
        id: &str,
        kind: WorkObjectKind,
        project_id: &str,
        title: &str,
        status: &str,
        updated_at: i64,
    ) -> WorkObject {
        WorkObject {
            schema_version: 1,
            id: id.into(),
            kind,
            project_id: project_id.into(),
            parent_id: None,
            title: title.into(),
            status: status.into(),
            description: Some(format!("{title}的必要事实")),
            data: json!({}),
            source_capture_id: None,
            revision: 3,
            created_at: 1,
            updated_at,
            deleted_at: None,
        }
    }

    fn aggregate(project: WorkProject) -> ProjectAggregate {
        let project_id = project.id.clone();
        ProjectAggregate {
            project,
            responsibilities: Vec::new(),
            goals: vec![object(
                "goal_review",
                WorkObjectKind::Goal,
                &project_id,
                "进入可评审状态",
                "active",
                30,
            )],
            milestones: Vec::new(),
            tasks: vec![object(
                "task_gap",
                WorkObjectKind::Task,
                &project_id,
                "补齐评审材料",
                "pending",
                31,
            )],
            decisions: vec![object(
                "decision_scope",
                WorkObjectKind::Decision,
                &project_id,
                "不修改正式数据",
                "accepted",
                20,
            )],
            artifacts: Vec::new(),
            evidence: Vec::new(),
            risks: Vec::new(),
            changes: Vec::new(),
            commitments: Vec::new(),
            recent_events: Vec::new(),
        }
    }

    fn snapshot(projects: Vec<WorkProject>) -> ContextSourceSnapshot {
        ContextSourceSnapshot {
            aggregates: projects.iter().cloned().map(aggregate).collect(),
            projects,
            focus_project_ids: Vec::new(),
        }
    }

    #[test]
    fn purpose_preserves_explicit_constraints_and_capability_hints() {
        let frame = compile_purpose("把桌面周报整理好，别动正式数据，必须用 PowerShell 验证");
        assert_eq!(frame.desired_outcome, frame.raw_intent);
        assert_eq!(
            frame.requested_capability_hints,
            vec!["desktop_file", "powershell"]
        );
        assert_eq!(
            frame.explicit_constraints,
            vec!["别动正式数据", "必须用 PowerShell 验证"]
        );
    }

    #[test]
    fn pc_purpose_driven_request_resolves_the_only_active_project() {
        let current = project("project_bob", "Bob 助手", "active", 20);
        let context = resolve_context(
            &compile_purpose("把这个项目推进到可评审状态"),
            &snapshot(vec![current]),
            RouteMode::Advanced,
            100,
        );
        assert_eq!(context.active_object.unwrap().object_id, "project_bob");
        assert!(context.conflicts.is_empty());
        assert!(context.confidence >= HIGH_CONFIDENCE);
    }

    #[test]
    fn mobile_reminder_does_not_bind_an_unmentioned_project() {
        let context = resolve_context(
            &compile_purpose("明天提醒我跟王总确认方案"),
            &snapshot(vec![project("project_bob", "Bob 助手", "active", 20)]),
            RouteMode::Direct,
            100,
        );
        assert!(context.active_object.is_none());
        assert_eq!(context.reason_codes, vec!["context.no_candidate"]);
    }

    #[test]
    fn desktop_report_keeps_a_capability_hint_without_inventing_access() {
        let frame = compile_purpose("把桌面那份周报整理成正式版本");
        let context = resolve_context(
            &frame,
            &ContextSourceSnapshot::default(),
            RouteMode::Deep,
            100,
        );
        assert_eq!(frame.requested_capability_hints, vec!["desktop_file"]);
        assert!(context.active_object.is_none());
    }

    #[test]
    fn two_active_projects_return_a_conflict_instead_of_guessing() {
        let context = resolve_context(
            &compile_purpose("把招商项目继续推进"),
            &snapshot(vec![
                project("project_north", "华北招商项目", "active", 30),
                project("project_south", "华南招商项目", "active", 20),
            ]),
            RouteMode::Advanced,
            100,
        );
        assert!(context.active_object.is_none());
        assert_eq!(context.conflicts.len(), 1);
        assert_eq!(context.reason_codes, vec!["context.ambiguous_candidates"]);
    }

    #[test]
    fn powershell_request_records_a_need_not_an_available_capability() {
        let frame = compile_purpose("用 PowerShell 检查这个项目");
        assert_eq!(frame.requested_capability_hints, vec!["powershell"]);
        let json = serde_json::to_value(frame).unwrap();
        assert!(json.get("availableCapabilities").is_none());
    }

    #[test]
    fn explicit_project_id_wins_over_a_generic_reference() {
        let context = resolve_context(
            &compile_purpose("继续 project_south 这个项目"),
            &snapshot(vec![
                project("project_north", "华北招商项目", "active", 30),
                project("project_south", "华南招商项目", "active", 20),
            ]),
            RouteMode::Advanced,
            100,
        );
        assert_eq!(context.active_object.unwrap().object_id, "project_south");
        assert!(context
            .reason_codes
            .contains(&"context.explicit_ref".into()));
    }

    #[test]
    fn archived_project_is_never_selected() {
        let context = resolve_context(
            &compile_purpose("继续 project_old 这个项目"),
            &snapshot(vec![project("project_old", "旧项目", "archived", 50)]),
            RouteMode::Direct,
            100,
        );
        assert!(context.active_object.is_none());
    }

    #[test]
    fn context_packet_contains_sources_and_excludes_other_projects() {
        let context = resolve_context(
            &compile_purpose("推进 Bob 助手"),
            &snapshot(vec![
                project("project_bob", "Bob 助手", "active", 30),
                project("project_other", "其他项目", "active", 20),
            ]),
            RouteMode::Deep,
            100,
        );
        let packet = render_context_packet(&compile_purpose("推进 Bob 助手"), &context);
        assert!(packet.contains("work_project:project_bob@2"));
        assert!(packet.contains("work_object:goal_review@3"));
        assert!(!packet.contains("project_other"));
    }

    #[test]
    fn direct_budget_is_stricter_than_advanced_budget() {
        let current = project("project_bob", "Bob 助手", "active", 30);
        let mut current_aggregate = aggregate(current.clone());
        for index in 0..30 {
            current_aggregate.tasks.push(object(
                &format!("task_{index}"),
                WorkObjectKind::Task,
                &current.id,
                &format!("任务 {index}"),
                "pending",
                index,
            ));
        }
        let sources = ContextSourceSnapshot {
            projects: vec![current],
            aggregates: vec![current_aggregate],
            focus_project_ids: Vec::new(),
        };
        let direct = resolve_context(
            &compile_purpose("推进这个项目"),
            &sources,
            RouteMode::Direct,
            100,
        );
        let advanced = resolve_context(
            &compile_purpose("推进这个项目"),
            &sources,
            RouteMode::Advanced,
            100,
        );
        assert!(
            direct.relevant_facts.len() <= ContextBudget::for_route(RouteMode::Direct).max_facts
        );
        assert!(advanced.relevant_facts.len() > direct.relevant_facts.len());
        assert!(direct
            .reason_codes
            .contains(&"context.budget_truncated".into()));
    }

    #[test]
    fn source_snapshot_loads_from_work_core_without_chat_history() {
        let mut connection = Connection::open_in_memory().unwrap();
        init_work_core_tables(&connection).unwrap();
        create_project(
            &mut connection,
            CreateProjectInput {
                project_id: Some("project_bob".into()),
                title: "Bob 助手".into(),
                mission: "让个人工作不断线".into(),
                current_phase: Some("Phase 5.5".into()),
                summary: None,
                source_ref: None,
                metadata: json!({}),
                actor: Some("test".into()),
                idempotency_key: "create-project-bob".into(),
            },
        )
        .unwrap();

        let snapshot = ContextSourceSnapshot::load(&connection).unwrap();
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(snapshot.aggregates.len(), 1);
        assert_eq!(snapshot.aggregates[0].project.id, "project_bob");
    }
}
