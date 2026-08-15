use serde::{Deserialize, Serialize};

use crate::assistant_context::{AssistantContext, PurposeFrame};
use crate::capability::{CapabilitySnapshot, CapabilityState};
use crate::complexity_router::{RouteDecision, RouteTaskKind};
use crate::tools::ToolRisk;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActionKind {
    LocalExecute,
    PcHandoff,
    Ask,
    Defer,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionDecision {
    pub kind: ActionKind,
    pub reason_code: String,
    pub required_capability: Option<String>,
}

impl ActionDecision {
    pub(crate) fn render_prompt(&self) -> String {
        format!(
            "\n## Deterministic action decision\nDecision: {:?}\nReason: {}\nDo not override this decision. Ask at most one question that changes the execution path. A handoff is not completion.\n",
            self.kind, self.reason_code
        )
    }
}

pub(crate) fn select_action(
    purpose: &PurposeFrame,
    context: Option<&AssistantContext>,
    capabilities: &CapabilitySnapshot,
    route: &RouteDecision,
    required_tool_risk: Option<ToolRisk>,
) -> ActionDecision {
    if context.is_some_and(|value| !value.conflicts.is_empty()) {
        return decision(ActionKind::Ask, "action.context_ambiguous", None);
    }
    if route.task_kind == RouteTaskKind::Answer {
        return decision(ActionKind::LocalExecute, "action.answer_locally", None);
    }
    if matches!(required_tool_risk, Some(ToolRisk::R2 | ToolRisk::R3)) {
        return decision(ActionKind::Ask, "action.approval_required", None);
    }

    let required = purpose
        .requested_capability_hints
        .iter()
        .find_map(|hint| match hint.as_str() {
            "powershell" => Some("powershell"),
            "browser" => Some("desktop_browser"),
            "desktop_file" => Some("sandbox_files"),
            "mobile_sandbox" => Some("sandbox_files"),
            _ => None,
        });

    if capabilities.is_mobile_runtime()
        && purpose
            .requested_capability_hints
            .iter()
            .any(|hint| matches!(hint.as_str(), "powershell" | "browser" | "desktop_file"))
    {
        return if capabilities.connected_pc {
            decision(
                ActionKind::PcHandoff,
                "action.desktop_work_handoff",
                required,
            )
        } else {
            decision(ActionKind::Defer, "action.pc_unavailable", required)
        };
    }

    if let Some(required) = required {
        let available = capabilities
            .capability(required)
            .is_some_and(|item| item.state == CapabilityState::Available);
        if !available {
            return decision(
                ActionKind::Defer,
                "action.capability_unavailable",
                Some(required),
            );
        }
    }

    decision(
        ActionKind::LocalExecute,
        "action.local_adapter_ready",
        required,
    )
}

fn decision(kind: ActionKind, reason_code: &str, required: Option<&str>) -> ActionDecision {
    ActionDecision {
        kind,
        reason_code: reason_code.into(),
        required_capability: required.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{Capability, CapabilitySnapshot};
    use crate::complexity_router::{RouteDuration, RouteMode, RouteRisk, RouteSource};

    fn route(task_kind: RouteTaskKind) -> RouteDecision {
        RouteDecision {
            mode: RouteMode::Direct,
            task_kind,
            confidence: 1.0,
            risk: RouteRisk::R0,
            duration: RouteDuration::Instant,
            source: RouteSource::Deterministic,
            reason_codes: Vec::new(),
            requires_project_state: false,
            semantic_fallback_recommended: false,
        }
    }

    fn snapshot(platform: &str, connected_pc: bool, available: &[&str]) -> CapabilitySnapshot {
        CapabilitySnapshot {
            platform: platform.into(),
            request_channel: "local_ui".into(),
            file_scope: "sandbox_and_authorized_folders".into(),
            connected_pc,
            capabilities: available
                .iter()
                .map(|id| Capability {
                    id: (*id).into(),
                    state: CapabilityState::Available,
                    reason_code: "capability.adapter_ready".into(),
                })
                .collect(),
        }
    }

    fn purpose(hints: &[&str]) -> PurposeFrame {
        PurposeFrame {
            raw_intent: "test".into(),
            desired_outcome: "test".into(),
            explicit_constraints: Vec::new(),
            candidate_refs: Vec::new(),
            requested_capability_hints: hints.iter().map(|value| (*value).into()).collect(),
            confidence: 1.0,
        }
    }

    #[test]
    fn simple_answer_stays_local() {
        assert_eq!(
            select_action(
                &purpose(&[]),
                None,
                &snapshot("windows", false, &[]),
                &route(RouteTaskKind::Answer),
                None,
            )
            .kind,
            ActionKind::LocalExecute
        );
    }

    #[test]
    fn mobile_desktop_task_handoffs_only_to_a_connected_pc() {
        let decision = select_action(
            &purpose(&["desktop_file"]),
            None,
            &snapshot("android", true, &["sandbox_files"]),
            &route(RouteTaskKind::Action),
            None,
        );
        assert_eq!(decision.kind, ActionKind::PcHandoff);
    }

    #[test]
    fn missing_pc_defers_instead_of_claiming_execution() {
        let decision = select_action(
            &purpose(&["desktop_file"]),
            None,
            &snapshot("android", false, &["sandbox_files"]),
            &route(RouteTaskKind::Action),
            None,
        );
        assert_eq!(decision.kind, ActionKind::Defer);
    }

    #[test]
    fn detected_powershell_without_adapter_defers() {
        let mut capabilities = snapshot("windows", false, &[]);
        capabilities.capabilities.push(Capability {
            id: "powershell".into(),
            state: CapabilityState::Degraded,
            reason_code: "capability.adapter_missing".into(),
        });
        assert_eq!(
            select_action(
                &purpose(&["powershell"]),
                None,
                &capabilities,
                &route(RouteTaskKind::Action),
                None,
            )
            .kind,
            ActionKind::Defer
        );
    }

    #[test]
    fn irreversible_tool_risk_requires_a_question() {
        assert_eq!(
            select_action(
                &purpose(&[]),
                None,
                &snapshot("windows", false, &[]),
                &route(RouteTaskKind::Action),
                Some(ToolRisk::R3),
            )
            .kind,
            ActionKind::Ask
        );
    }
}
