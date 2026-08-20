use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorClass {
    TransientNetwork,
    SqliteBusy,
    InvalidArguments,
    PermissionDenied,
    CapabilityUnavailable,
    VerificationFailed,
    UnknownSideEffect,
    BudgetExhausted,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SideEffectState {
    None,
    Applied,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecoveryAction {
    RetryOnce,
    RepairArgumentsOnce,
    ModelRepairOnce,
    Ask,
    Defer,
    Stop,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorDisposition {
    pub class: ErrorClass,
    pub side_effect_state: SideEffectState,
    pub recovery: RecoveryAction,
}

pub(crate) fn classify_tool_error(
    error: &str,
    timed_out: bool,
    side_effect_possible: bool,
    retry_count: u32,
) -> ErrorDisposition {
    let normalized = error.to_ascii_lowercase();
    let side_effect_state = if side_effect_possible {
        SideEffectState::Unknown
    } else {
        SideEffectState::None
    };
    if side_effect_possible && timed_out {
        return disposition(
            ErrorClass::UnknownSideEffect,
            SideEffectState::Unknown,
            RecoveryAction::Stop,
        );
    }
    if contains_any(
        &normalized,
        &[
            "permission",
            "denied",
            "forbidden",
            "unauthorized",
            "拒绝",
            "权限",
        ],
    ) {
        return disposition(
            ErrorClass::PermissionDenied,
            side_effect_state,
            RecoveryAction::Ask,
        );
    }
    if contains_any(
        &normalized,
        &[
            "capability",
            "not available",
            "not supported",
            "unavailable",
            "未连接",
            "不可用",
        ],
    ) {
        return disposition(
            ErrorClass::CapabilityUnavailable,
            side_effect_state,
            RecoveryAction::Defer,
        );
    }
    if contains_any(
        &normalized,
        &["budget", "circuit", "调用上限", "预算", "循环检测"],
    ) {
        return disposition(
            ErrorClass::BudgetExhausted,
            side_effect_state,
            RecoveryAction::Stop,
        );
    }
    if contains_any(
        &normalized,
        &[
            "database is locked",
            "database is busy",
            "sqlite_busy",
            "sqlite_locked",
        ],
    ) {
        return retryable(ErrorClass::SqliteBusy, side_effect_state, retry_count);
    }
    if timed_out
        || contains_any(
            &normalized,
            &[
                "timeout",
                "timed out",
                "connection reset",
                "temporarily unavailable",
                "network",
                "超时",
                "网络",
            ],
        )
    {
        return retryable(ErrorClass::TransientNetwork, side_effect_state, retry_count);
    }
    if contains_any(
        &normalized,
        &[
            "invalid argument",
            "missing required",
            "required field",
            "不能为空",
            "参数",
            "格式错误",
        ],
    ) {
        return disposition(
            ErrorClass::InvalidArguments,
            side_effect_state,
            if retry_count == 0 && side_effect_state == SideEffectState::None {
                RecoveryAction::RepairArgumentsOnce
            } else {
                RecoveryAction::Stop
            },
        );
    }
    if side_effect_possible {
        disposition(
            ErrorClass::UnknownSideEffect,
            SideEffectState::Unknown,
            RecoveryAction::Stop,
        )
    } else {
        disposition(
            ErrorClass::VerificationFailed,
            SideEffectState::None,
            if retry_count == 0 {
                RecoveryAction::ModelRepairOnce
            } else {
                RecoveryAction::Stop
            },
        )
    }
}

pub(crate) fn recovery_from_tool_summary(
    tool_summary: &Value,
    verification_failed: bool,
    attempt_index: u32,
) -> RecoveryAction {
    let calls = tool_summary
        .get("calls")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let failed = calls
        .iter()
        .filter(|call| call.get("success").and_then(Value::as_bool) == Some(false))
        .collect::<Vec<_>>();
    if failed
        .iter()
        .any(|call| call.get("sideEffectState").and_then(Value::as_str) == Some("unknown"))
    {
        return RecoveryAction::Stop;
    }
    for action in ["ask", "defer", "stop"] {
        if failed
            .iter()
            .any(|call| call.get("recoveryAction").and_then(Value::as_str) == Some(action))
        {
            return match action {
                "ask" => RecoveryAction::Ask,
                "defer" => RecoveryAction::Defer,
                _ => RecoveryAction::Stop,
            };
        }
    }
    if attempt_index == 0 {
        if failed.iter().any(|call| {
            call.get("retryCount").and_then(Value::as_u64).unwrap_or(0) == 0
                && matches!(
                    call.get("recoveryAction").and_then(Value::as_str),
                    Some("retry_once" | "repair_arguments_once" | "model_repair_once")
                )
        }) {
            return RecoveryAction::ModelRepairOnce;
        }
        if failed.is_empty() && verification_failed {
            return RecoveryAction::ModelRepairOnce;
        }
    }
    RecoveryAction::Stop
}

pub(crate) fn recovery_prompt(action: RecoveryAction) -> Option<&'static str> {
    match action {
        RecoveryAction::RetryOnce => Some(
            "A transient failure occurred. Retry once with the same bounded scope. Do not repeat any operation whose side effect is unknown.",
        ),
        RecoveryAction::RepairArgumentsOnce => Some(
            "One tool call had invalid arguments. Correct only the arguments once; do not broaden the task.",
        ),
        RecoveryAction::ModelRepairOnce => Some(
            "The previous bounded attempt lacked verifiable evidence. Change strategy once using safe available tools; never claim completion without evidence.",
        ),
        RecoveryAction::Ask | RecoveryAction::Defer | RecoveryAction::Stop => None,
    }
}

fn retryable(
    class: ErrorClass,
    side_effect_state: SideEffectState,
    retry_count: u32,
) -> ErrorDisposition {
    disposition(
        class,
        side_effect_state,
        if retry_count == 0 && side_effect_state == SideEffectState::None {
            RecoveryAction::RetryOnce
        } else {
            RecoveryAction::Stop
        },
    )
}

fn disposition(
    class: ErrorClass,
    side_effect_state: SideEffectState,
    recovery: RecoveryAction,
) -> ErrorDisposition {
    ErrorDisposition {
        class,
        side_effect_state,
        recovery,
    }
}

fn contains_any(value: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| value.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_only_timeout_retries_once_then_stops() {
        assert_eq!(
            classify_tool_error("timed out", true, false, 0).recovery,
            RecoveryAction::RetryOnce
        );
        assert_eq!(
            classify_tool_error("timed out", true, false, 1).recovery,
            RecoveryAction::Stop
        );
    }

    #[test]
    fn write_timeout_is_unknown_and_never_repeated() {
        let result = classify_tool_error("timed out", true, true, 0);
        assert_eq!(result.class, ErrorClass::UnknownSideEffect);
        assert_eq!(result.side_effect_state, SideEffectState::Unknown);
        assert_eq!(result.recovery, RecoveryAction::Stop);
    }

    #[test]
    fn permission_and_capability_errors_do_not_retry() {
        assert_eq!(
            classify_tool_error("permission denied", false, false, 0).recovery,
            RecoveryAction::Ask
        );
        assert_eq!(
            classify_tool_error("capability unavailable", false, false, 0).recovery,
            RecoveryAction::Defer
        );
    }

    #[test]
    fn unknown_side_effect_in_summary_stops_goal_repair() {
        let summary = json!({"calls":[{
            "success":false,
            "sideEffectState":"unknown",
            "recoveryAction":"retry_once",
            "retryCount":0
        }]});
        assert_eq!(
            recovery_from_tool_summary(&summary, true, 0),
            RecoveryAction::Stop
        );
    }
}
