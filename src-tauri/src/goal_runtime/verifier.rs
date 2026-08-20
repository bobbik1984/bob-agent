use serde_json::Value;

use super::models::{EvidenceRule, EvidenceRuleKind, VerificationState};

#[derive(Clone, Debug, PartialEq)]
pub struct VerificationOutcome {
    pub state: VerificationState,
    pub verifier: String,
    pub detail: String,
}

pub fn verify_tool_summary(rule: &EvidenceRule, tool_summary: &Value) -> VerificationOutcome {
    if rule.kind != EvidenceRuleKind::Deterministic
        || rule.verifier.get("kind").and_then(Value::as_str) != Some("tool_receipt")
    {
        return VerificationOutcome {
            state: VerificationState::Unverified,
            verifier: "deterministic".into(),
            detail: "规则不是可由工具回执验证的确定性规则".into(),
        };
    }
    let total_calls = tool_summary
        .get("total_calls")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_failures = tool_summary
        .get("total_failures")
        .and_then(Value::as_u64)
        .unwrap_or(total_calls);
    if total_calls == 0 {
        return VerificationOutcome {
            state: VerificationState::Unverified,
            verifier: "tool_receipt".into(),
            detail: "没有实际工具调用回执".into(),
        };
    }
    if total_failures > 0 {
        return VerificationOutcome {
            state: VerificationState::Rejected,
            verifier: "tool_receipt".into(),
            detail: format!("{total_failures} 个工具调用失败"),
        };
    }
    VerificationOutcome {
        state: VerificationState::Verified,
        verifier: "tool_receipt".into(),
        detail: format!("{total_calls} 个工具调用具有成功回执"),
    }
}

pub fn verify_file_reference(rule: &EvidenceRule, reference: &str) -> VerificationOutcome {
    if rule.kind != EvidenceRuleKind::Deterministic
        || rule.verifier.get("kind").and_then(Value::as_str) != Some("file_exists")
    {
        return VerificationOutcome {
            state: VerificationState::Unverified,
            verifier: "file_exists".into(),
            detail: "规则未声明 file_exists 验证器".into(),
        };
    }
    let path = std::path::Path::new(reference);
    if path.is_file() {
        VerificationOutcome {
            state: VerificationState::Verified,
            verifier: "file_exists".into(),
            detail: "文件存在".into(),
        }
    } else {
        VerificationOutcome {
            state: VerificationState::Rejected,
            verifier: "file_exists".into(),
            detail: "文件不存在或不是普通文件".into(),
        }
    }
}

pub fn parse_rubric_verdict(raw: &str) -> VerificationOutcome {
    let parsed = raw.find('{').zip(raw.rfind('}')).and_then(|(start, end)| {
        if end < start {
            None
        } else {
            serde_json::from_str::<Value>(&raw[start..=end]).ok()
        }
    });
    let Some(value) = parsed else {
        return VerificationOutcome {
            state: VerificationState::Unverified,
            verifier: "clerk_rubric".into(),
            detail: "评估器未返回有效 JSON".into(),
        };
    };
    match value.get("verdict").and_then(Value::as_str) {
        Some("PASS") => VerificationOutcome {
            state: VerificationState::Verified,
            verifier: "clerk_rubric".into(),
            detail: value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("rubric 通过")
                .into(),
        },
        Some("FAIL") => VerificationOutcome {
            state: VerificationState::Rejected,
            verifier: "clerk_rubric".into(),
            detail: value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("rubric 未通过")
                .into(),
        },
        _ => VerificationOutcome {
            state: VerificationState::Unverified,
            verifier: "clerk_rubric".into(),
            detail: "评估器 verdict 无效".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal_runtime::models::{EvidenceRule, EvidenceRuleKind};
    use serde_json::json;

    fn tool_rule() -> EvidenceRule {
        EvidenceRule {
            rule_id: "tool".into(),
            description: "工具成功".into(),
            kind: EvidenceRuleKind::Deterministic,
            required: true,
            allowed_evidence_types: vec!["tool_receipt".into()],
            verifier: json!({"kind":"tool_receipt"}),
            verification_state: VerificationState::Pending,
        }
    }

    #[test]
    fn model_text_without_tool_receipt_is_not_evidence() {
        let outcome =
            verify_tool_summary(&tool_rule(), &json!({"total_calls":0,"total_failures":0}));
        assert_eq!(outcome.state, VerificationState::Unverified);
    }

    #[test]
    fn failed_tool_receipt_is_rejected() {
        let outcome =
            verify_tool_summary(&tool_rule(), &json!({"total_calls":2,"total_failures":1}));
        assert_eq!(outcome.state, VerificationState::Rejected);
    }

    #[test]
    fn rubric_requires_strict_verdict_json() {
        assert_eq!(
            parse_rubric_verdict("looks good").state,
            VerificationState::Unverified
        );
        assert_eq!(
            parse_rubric_verdict(r#"{"verdict":"PASS","reason":"符合规则"}"#).state,
            VerificationState::Verified
        );
    }
}
