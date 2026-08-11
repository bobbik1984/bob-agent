use serde::Deserialize;
use serde_json::{json, Value};

use crate::complexity_router::{RouteDecision, RouteRisk, RouteSource, RouteTaskKind};

use super::models::{
    EvidenceRule, EvidenceRuleKind, GoalBlockerPolicy, GoalBudget, GoalContract, GoalCreatedFrom,
    GoalRecoveryPolicy, GoalRisk, GoalRiskPolicy, GoalScope, VerificationState,
    GOAL_RUNTIME_SCHEMA_VERSION,
};

#[derive(Clone, Debug)]
pub struct CompilationOutcome {
    pub contract: GoalContract,
    pub used_clerk: bool,
    pub warning_code: Option<String>,
    pub needs_user_input: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompilerCandidate {
    outcome: String,
    #[serde(default)]
    evidence_rules: Vec<CompilerEvidenceRule>,
    #[serde(default)]
    constraints: Vec<String>,
    #[serde(default)]
    allowed_refs: Vec<String>,
    #[serde(default)]
    missing_critical_information: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompilerEvidenceRule {
    rule_id: String,
    description: String,
    kind: EvidenceRuleKind,
    #[serde(default)]
    allowed_evidence_types: Vec<String>,
    #[serde(default)]
    verifier: Value,
}

fn risk_from_route(risk: RouteRisk, task_kind: RouteTaskKind) -> GoalRisk {
    match risk {
        RouteRisk::R0 => GoalRisk::R0,
        RouteRisk::R1 => GoalRisk::R1,
        RouteRisk::R2 => GoalRisk::R2,
        RouteRisk::R3 => GoalRisk::R3,
        RouteRisk::Unknown => {
            if task_kind == RouteTaskKind::Answer {
                GoalRisk::R0
            } else {
                GoalRisk::R1
            }
        }
    }
}

fn route_source(source: RouteSource) -> String {
    match source {
        RouteSource::Override => "override",
        RouteSource::Deterministic => "deterministic",
        RouteSource::Clerk => "clerk",
        RouteSource::ConservativeFallback => "conservative_fallback",
    }
    .into()
}

fn compact_outcome(request: &str) -> String {
    let cleaned = request.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned.chars().take(300).collect()
}

fn baseline_rule(task_kind: RouteTaskKind) -> EvidenceRule {
    match task_kind {
        RouteTaskKind::Action => EvidenceRule {
            rule_id: "tool_result".into(),
            description: "至少一个实际工具回执证明请求的动作已成功完成".into(),
            kind: EvidenceRuleKind::Deterministic,
            required: true,
            allowed_evidence_types: vec!["tool_receipt".into()],
            verifier: json!({ "kind": "tool_receipt", "requireSuccess": true }),
            verification_state: VerificationState::Pending,
        },
        RouteTaskKind::Answer => EvidenceRule {
            rule_id: "user_acceptance".into(),
            description: "用户确认持续工作的阶段结果满足目标".into(),
            kind: EvidenceRuleKind::UserAcceptance,
            required: true,
            allowed_evidence_types: vec!["user_acceptance".into()],
            verifier: json!({ "kind": "user_acceptance" }),
            verification_state: VerificationState::Pending,
        },
    }
}

pub fn baseline_contract(
    request: &str,
    decision: &RouteDecision,
    project_id: &str,
    conversation_id: Option<String>,
) -> Result<GoalContract, String> {
    let request = request.trim();
    if request.is_empty() {
        return Err("Goal 原始请求不能为空".into());
    }
    GoalContract {
        schema_version: GOAL_RUNTIME_SCHEMA_VERSION,
        original_request: request.into(),
        outcome: compact_outcome(request),
        evidence_rules: vec![baseline_rule(decision.task_kind)],
        scope: GoalScope {
            project_id: Some(project_id.into()),
            allowed_refs: vec![],
            global_file_access: false,
        },
        constraints: vec![
            "不得绕过 R0–R3 Policy Engine".into(),
            "证据不足时不得宣称完成".into(),
        ],
        budget: GoalBudget::default(),
        risk_policy: GoalRiskPolicy {
            max_auto_risk: risk_from_route(decision.risk, decision.task_kind),
            trusted_device_required_for_r3: true,
        },
        blocker_policy: GoalBlockerPolicy::default(),
        recovery_policy: GoalRecoveryPolicy::default(),
        created_from: GoalCreatedFrom {
            route_source: route_source(decision.source),
            route_confidence: decision.confidence,
            conversation_id,
        },
    }
    .normalize()
}

fn compiler_prompt(request: &str, baseline: &GoalContract) -> String {
    format!(
        "Convert the request into a verifiable Goal Contract candidate. Return one JSON object and no markdown.\n\
         Schema: {{\"outcome\":\"<=500 chars\",\"evidenceRules\":[{{\"ruleId\":\"stable_id\",\"description\":\"observable result\",\"kind\":\"deterministic|rubric|user_acceptance\",\"required\":true,\"allowedEvidenceTypes\":[\"tool_receipt|file|database|user_acceptance|artifact\"],\"verifier\":{{}}}}],\"constraints\":[],\"allowedRefs\":[],\"missingCriticalInformation\":false}}.\n\
         Do not grant permissions, choose a risk level, change budgets, call tools, or claim completion. Prefer deterministic evidence. Keep 1-4 evidence rules.\n\
         Baseline outcome: {}\nUser request: {}",
        baseline.outcome, request
    )
}

fn parse_candidate(raw: &str) -> Option<CompilerCandidate> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&raw[start..=end]).ok()
}

fn merge_candidate(
    mut baseline: GoalContract,
    candidate: CompilerCandidate,
) -> Result<(GoalContract, bool), String> {
    let outcome = candidate.outcome.trim();
    if !outcome.is_empty() {
        baseline.outcome = outcome.chars().take(500).collect();
    }
    if !candidate.evidence_rules.is_empty() {
        let baseline_rule_id = baseline
            .evidence_rules
            .first()
            .map(|rule| rule.rule_id.clone());
        let mut proposed = candidate
            .evidence_rules
            .into_iter()
            .take(3)
            .map(|rule| EvidenceRule {
                rule_id: rule.rule_id,
                description: rule.description,
                kind: rule.kind,
                // Phase 5 can only close the deterministic baseline or explicit user
                // acceptance. Candidate rules remain informative until a supported
                // verifier is registered; the Clerk cannot invent a completion gate.
                required: false,
                allowed_evidence_types: rule.allowed_evidence_types,
                verifier: rule.verifier,
                verification_state: VerificationState::Pending,
            })
            .filter(|rule| Some(&rule.rule_id) != baseline_rule_id.as_ref())
            .collect::<Vec<_>>();
        baseline.evidence_rules.append(&mut proposed);
    }
    baseline.constraints.extend(candidate.constraints);
    baseline.scope.allowed_refs.extend(candidate.allowed_refs);
    Ok((
        baseline.normalize()?,
        candidate.missing_critical_information,
    ))
}

pub async fn compile_contract(
    request: &str,
    decision: &RouteDecision,
    project_id: &str,
    conversation_id: Option<String>,
) -> CompilationOutcome {
    let baseline = match baseline_contract(request, decision, project_id, conversation_id) {
        Ok(contract) => contract,
        Err(error) => {
            return CompilationOutcome {
                contract: GoalContract::legacy(request),
                used_clerk: false,
                warning_code: Some(format!("GOAL-CONTRACT-INVALID:{error}")),
                needs_user_input: true,
            }
        }
    };
    let prompt = compiler_prompt(request, &baseline);
    let Some(raw) = crate::llm::call_clerk_oneshot_with_timeout(
        "You compile candidate Goal Contracts. You cannot execute tools or grant permissions. Output strict JSON only.",
        &prompt, 900, 8,
    ).await else {
        return CompilationOutcome { contract: baseline, used_clerk: false, warning_code: Some("GOAL-COMPILER-OFFLINE".into()), needs_user_input: false };
    };
    let Some(candidate) = parse_candidate(&raw) else {
        return CompilationOutcome {
            contract: baseline,
            used_clerk: false,
            warning_code: Some("GOAL-COMPILER-INVALID-JSON".into()),
            needs_user_input: false,
        };
    };
    match merge_candidate(baseline.clone(), candidate) {
        Ok((contract, missing)) => CompilationOutcome {
            contract,
            used_clerk: true,
            warning_code: None,
            needs_user_input: missing,
        },
        Err(_) => CompilationOutcome {
            contract: baseline,
            used_clerk: false,
            warning_code: Some("GOAL-COMPILER-INVALID-CONTRACT".into()),
            needs_user_input: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complexity_router::{route_text, RouteMode};

    #[test]
    fn baseline_never_lets_the_compiler_change_risk_or_budget() {
        let decision = route_text("持续推进并修改这个文件直到完成", false, "auto");
        assert_eq!(decision.mode, RouteMode::Advanced);
        let baseline = baseline_contract(
            "持续推进并修改这个文件直到完成",
            &decision,
            "project_personal_inbox",
            None,
        )
        .unwrap();
        let original_budget = baseline.budget.clone();
        let original_risk = baseline.risk_policy.clone();
        let candidate = parse_candidate(r#"{"outcome":"完成更新","evidenceRules":[{"ruleId":"file","description":"文件存在","kind":"deterministic","required":true,"allowedEvidenceTypes":["file"],"verifier":{"kind":"file_exists"}}],"constraints":[],"allowedRefs":[],"missingCriticalInformation":false}"#).unwrap();
        let (merged, _) = merge_candidate(baseline, candidate).unwrap();
        assert_eq!(merged.budget, original_budget);
        assert_eq!(merged.risk_policy, original_risk);
    }

    #[test]
    fn action_and_answer_receive_different_completion_evidence() {
        let action = route_text("持续推进并修改这个文件直到完成", false, "auto");
        let answer = route_text("未来几周持续跟进这个行业", false, "auto");
        let action_contract = baseline_contract(
            "持续推进并修改这个文件直到完成",
            &action,
            "project_personal_inbox",
            None,
        )
        .unwrap();
        let answer_contract = baseline_contract(
            "未来几周持续跟进这个行业",
            &answer,
            "project_personal_inbox",
            None,
        )
        .unwrap();
        assert_eq!(
            action_contract.evidence_rules[0].allowed_evidence_types,
            vec!["tool_receipt"]
        );
        assert_eq!(
            answer_contract.evidence_rules[0].kind,
            EvidenceRuleKind::UserAcceptance
        );
    }
}
