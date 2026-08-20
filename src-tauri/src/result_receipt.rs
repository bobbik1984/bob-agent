use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::hash::{Hash, Hasher};
use tauri::{AppHandle, Manager};

use crate::db::DbState;
use crate::execution_error::SideEffectState;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResultReceipt {
    pub decision_id: String,
    pub status: String,
    pub verified_evidence: Vec<Value>,
    pub state_changes: Vec<Value>,
    pub side_effect_state: SideEffectState,
    pub correction_refs: Vec<String>,
    pub completed_at: i64,
}

pub(crate) fn from_tool_summary(
    tool_summary: &Value,
    completed_at: i64,
    decision_seed: &str,
) -> ResultReceipt {
    let calls = tool_summary
        .get("calls")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let verified_evidence = calls
        .iter()
        .enumerate()
        .filter(|(_, call)| call.get("success").and_then(Value::as_bool) == Some(true))
        .map(|(index, call)| {
            json!({
                "type": "tool_receipt",
                "callIndex": index,
                "tool": call.get("name").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    let state_changes = calls
        .iter()
        .enumerate()
        .filter(|(_, call)| {
            call.get("success").and_then(Value::as_bool) == Some(true)
                && call.get("sideEffectState").and_then(Value::as_str) == Some("applied")
        })
        .map(|(index, call)| {
            json!({
                "callIndex": index,
                "tool": call.get("name").cloned().unwrap_or(Value::Null),
                "state": "applied",
            })
        })
        .collect::<Vec<_>>();
    let has_unknown = calls
        .iter()
        .any(|call| call.get("sideEffectState").and_then(Value::as_str) == Some("unknown"));
    let failures = tool_summary
        .get("total_failures")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total = tool_summary
        .get("total_calls")
        .and_then(Value::as_u64)
        .unwrap_or(calls.len() as u64);
    let status = match (total, failures) {
        (0, _) => "no_action",
        (_, 0) => "succeeded",
        (total, failures) if total == failures => "failed",
        _ => "partial",
    };
    ResultReceipt {
        decision_id: format!("decision_{}", stable_hash(decision_seed)),
        status: status.into(),
        verified_evidence,
        state_changes,
        side_effect_state: if has_unknown {
            SideEffectState::Unknown
        } else if calls
            .iter()
            .any(|call| call.get("sideEffectState").and_then(Value::as_str) == Some("applied"))
        {
            SideEffectState::Applied
        } else {
            SideEffectState::None
        },
        correction_refs: Vec::new(),
        completed_at,
    }
}

pub(crate) fn persist_verified_experience_candidate(
    conn: &rusqlite::Connection,
    goal_id: &str,
    run_id: &str,
    outcome: &str,
    tool_summary: &Value,
) -> Result<(), String> {
    let now = crate::now_ms();
    let candidate_id = format!("experience_{}", stable_hash(goal_id));
    let claim = format!(
        "Verified experience candidate for review: {}",
        outcome.chars().take(500).collect::<String>()
    );
    let evidence = json!({
        "goalId": goal_id,
        "runId": run_id,
        "toolSummary": tool_summary,
        "verificationState": "verified",
    });
    conn.execute(
        "INSERT INTO memory_entries
         (id, claim, memory_type, scope, source, confidence, evidence,
          first_seen, last_confirmed, status, version, created_at, updated_at)
         VALUES (?1, ?2, 'experience', 'project', 'goal_verified', 0.7, ?3,
                 ?4, ?4, 'candidate', 1, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET
           claim=excluded.claim,
           evidence=excluded.evidence,
           last_confirmed=excluded.last_confirmed,
           status='candidate',
           version=memory_entries.version + 1,
           updated_at=excluded.updated_at",
        rusqlite::params![candidate_id, claim, evidence.to_string(), now],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn stable_hash(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub(crate) fn persist_direct_action(
    app: &AppHandle,
    conversation_id: &str,
    receipt: &ResultReceipt,
) -> Result<(), String> {
    if receipt.side_effect_state == SideEffectState::None {
        return Ok(());
    }
    let state = app
        .try_state::<DbState>()
        .ok_or_else(|| "receipt database unavailable".to_string())?;
    let conn = state.0.lock().map_err(|error| error.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS action_receipts (
            decision_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            status TEXT NOT NULL,
            verified_evidence_json TEXT NOT NULL,
            state_changes_json TEXT NOT NULL,
            side_effect_state TEXT NOT NULL,
            correction_refs_json TEXT NOT NULL,
            completed_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_action_receipts_conversation
            ON action_receipts(conversation_id, completed_at DESC);",
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO action_receipts (
            decision_id, conversation_id, status, verified_evidence_json,
            state_changes_json, side_effect_state, correction_refs_json, completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            &receipt.decision_id,
            conversation_id,
            &receipt.status,
            serde_json::to_string(&receipt.verified_evidence).map_err(|error| error.to_string())?,
            serde_json::to_string(&receipt.state_changes).map_err(|error| error.to_string())?,
            match receipt.side_effect_state {
                SideEffectState::None => "none",
                SideEffectState::Applied => "applied",
                SideEffectState::Unknown => "unknown",
            },
            serde_json::to_string(&receipt.correction_refs).map_err(|error| error.to_string())?,
            receipt.completed_at,
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_write_produces_a_state_change_receipt() {
        let receipt = from_tool_summary(
            &json!({
                "total_calls": 1,
                "total_failures": 0,
                "calls": [{"name":"write_file","success":true,"sideEffectState":"applied"}]
            }),
            10,
            "conv-1:turn-1",
        );
        assert_eq!(receipt.status, "succeeded");
        assert_eq!(receipt.side_effect_state, SideEffectState::Applied);
        assert_eq!(receipt.verified_evidence.len(), 1);
        assert_eq!(receipt.state_changes.len(), 1);
    }

    #[test]
    fn failed_unknown_write_is_never_reported_as_applied() {
        let receipt = from_tool_summary(
            &json!({
                "total_calls": 1,
                "total_failures": 1,
                "calls": [{"name":"write_file","success":false,"sideEffectState":"unknown"}]
            }),
            10,
            "conv-1:turn-2",
        );
        assert_eq!(receipt.status, "failed");
        assert_eq!(receipt.side_effect_state, SideEffectState::Unknown);
        assert!(receipt.state_changes.is_empty());
    }

    #[test]
    fn the_same_decision_seed_produces_an_idempotent_receipt_id() {
        let summary = json!({"total_calls":0,"total_failures":0,"calls":[]});
        let first = from_tool_summary(&summary, 10, "conv-1:turn-1");
        let replay = from_tool_summary(&summary, 20, "conv-1:turn-1");
        assert_eq!(first.decision_id, replay.decision_id);
    }
}
