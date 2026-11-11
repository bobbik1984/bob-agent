//! Tool-Call Loop Circuit Breaker and Approval Rejection Tracker.
//!
//! Enforces:
//! - Deterministic budget limit on total tool calls per turn (default 25).
//! - Loop detection for identical consecutive calls (default threshold 3).
//! - Alternating ping-pong loop detection (A -> B -> A -> B -> A -> B).
//! - Key-order-insensitive canonical JSON hashing.
//! - Deterministic blocking of actions rejected by the user within the current turn.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const DEFAULT_MAX_TOTAL_CALLS: usize = 25;
pub const DEFAULT_REPEAT_THRESHOLD: usize = 3;

/// Recursively sorts all JSON object keys so identical data structures
/// produce identical serialized strings regardless of insertion order.
pub fn canonicalize_json(val: &Value) -> Value {
    match val {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut sorted = serde_json::Map::new();
            for (k, v) in entries {
                sorted.insert(k.clone(), canonicalize_json(v));
            }
            Value::Object(sorted)
        }
        Value::Array(arr) => {
            let mapped = arr.iter().map(canonicalize_json).collect();
            Value::Array(mapped)
        }
        primitive => primitive.clone(),
    }
}

pub fn canonical_json_hash(val: &Value) -> u64 {
    let canonical = canonicalize_json(val);
    let s = serde_json::to_string(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolCallRecord {
    pub name: String,
    pub hash: u64,
}

#[derive(Debug, Clone)]
pub struct ToolCallTracker {
    history: Vec<ToolCallRecord>,
    total_calls: usize,
    max_total_calls: usize,
    repeat_threshold: usize,
    rejected_in_turn: HashSet<(String, u64)>,
}

impl Default for ToolCallTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCallTracker {
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_MAX_TOTAL_CALLS, DEFAULT_REPEAT_THRESHOLD)
    }

    pub fn with_limits(max_total_calls: usize, repeat_threshold: usize) -> Self {
        Self {
            history: Vec::new(),
            total_calls: 0,
            max_total_calls,
            repeat_threshold: repeat_threshold.max(1),
            rejected_in_turn: HashSet::new(),
        }
    }

    pub fn total_calls(&self) -> usize {
        self.total_calls
    }

    pub fn max_total_calls(&self) -> usize {
        self.max_total_calls
    }

    pub fn repeat_threshold(&self) -> usize {
        self.repeat_threshold
    }

    /// Records an action rejected by human approval within the current turn.
    pub fn record_rejected(&mut self, tool_name: &str, args: &Value) {
        let hash = canonical_json_hash(args);
        self.rejected_in_turn
            .insert((tool_name.trim().to_string(), hash));
    }

    /// Checks if the identical action was previously rejected in this turn.
    pub fn is_rejected_in_turn(&self, tool_name: &str, args: &Value) -> bool {
        let hash = canonical_json_hash(args);
        self.rejected_in_turn
            .contains(&(tool_name.trim().to_string(), hash))
    }

    /// Clears turn-scoped rejected actions (e.g. when starting a new user conversation turn).
    pub fn reset_turn(&mut self) {
        self.rejected_in_turn.clear();
    }

    /// Resets entire tracker state (history, counters, rejections).
    pub fn reset_all(&mut self) {
        self.history.clear();
        self.total_calls = 0;
        self.rejected_in_turn.clear();
    }

    /// Evaluates whether the proposed tool call is allowed to proceed.
    ///
    /// Checks:
    /// 1. Prior human rejection in this turn -> `Err("ACTION_PREVIOUSLY_REJECTED")`
    /// 2. Total turn/worker tool-call budget exhausted -> `Err("TOOL_CALL_BUDGET_EXHAUSTED")`
    /// 3. Identical consecutive calls reach `repeat_threshold` -> `Err("TOOL_CALL_LOOP_DETECTED")`
    /// 4. Ping-pong alternation loop `A->B->A->B->A->B` detected -> `Err("TOOL_CALL_PING_PONG_DETECTED")`
    pub fn check(&self, tool_name: &str, args: &Value) -> Result<(), &'static str> {
        let name = tool_name.trim();

        // 1. Rejected action check
        if self.is_rejected_in_turn(name, args) {
            return Err("ACTION_PREVIOUSLY_REJECTED");
        }

        // 2. Total budget check
        if self.total_calls >= self.max_total_calls {
            return Err("TOOL_CALL_BUDGET_EXHAUSTED");
        }

        let hash = canonical_json_hash(args);
        let candidate = ToolCallRecord {
            name: name.to_string(),
            hash,
        };

        // 3. Consecutive identical calls check
        let recent_same = self
            .history
            .iter()
            .rev()
            .take_while(|rec| rec.name == candidate.name && rec.hash == candidate.hash)
            .count();
        if recent_same >= self.repeat_threshold {
            return Err("TOOL_CALL_LOOP_DETECTED");
        }

        // 4. Ping-pong loop detection (A -> B -> A -> B -> A -> B)
        if self.is_ping_pong_sequence(&self.history) {
            return Err("TOOL_CALL_PING_PONG_DETECTED");
        }

        let mut simulated = self.history.clone();
        simulated.push(candidate);
        if self.is_ping_pong_sequence(&simulated) {
            return Err("TOOL_CALL_PING_PONG_DETECTED");
        }

        Ok(())
    }

    fn is_ping_pong_sequence(&self, slice: &[ToolCallRecord]) -> bool {
        if slice.len() < 6 {
            return false;
        }
        let len = slice.len();
        let a = &slice[len - 2];
        let b = &slice[len - 1];
        if a == b {
            return false;
        }
        slice[len - 6] == *a
            && slice[len - 5] == *b
            && slice[len - 4] == *a
            && slice[len - 3] == *b
            && slice[len - 2] == *a
            && slice[len - 1] == *b
    }

    /// Records an executed tool call into history and increments total call count.
    pub fn record(&mut self, tool_name: &str, args: &Value) {
        let hash = canonical_json_hash(args);
        self.history.push(ToolCallRecord {
            name: tool_name.trim().to_string(),
            hash,
        });
        self.total_calls += 1;
        if self.history.len() > 60 {
            self.history.drain(..self.history.len() - 60);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_canonical_json_hash_key_order_insensitivity() {
        let v1 = json!({
            "beta": 123,
            "alpha": "hello",
            "nested": {
                "z": true,
                "a": [1, 2, {"k2": "v2", "k1": "v1"}]
            }
        });

        let v2 = json!({
            "alpha": "hello",
            "nested": {
                "a": [1, 2, {"k1": "v1", "k2": "v2"}],
                "z": true
            },
            "beta": 123
        });

        assert_eq!(canonical_json_hash(&v1), canonical_json_hash(&v2));
    }

    #[test]
    fn test_budget_exhaustion() {
        let mut tracker = ToolCallTracker::with_limits(3, 10);
        let args = json!({"path": "file1.txt"});

        assert!(tracker.check("read_file", &args).is_ok());
        tracker.record("read_file", &args);

        assert!(tracker.check("read_file", &args).is_ok());
        tracker.record("read_file", &args);

        assert!(tracker.check("read_file", &args).is_ok());
        tracker.record("read_file", &args);

        // 4th call exceeds max_total_calls = 3
        let res = tracker.check("read_file", &args);
        assert_eq!(res.unwrap_err(), "TOOL_CALL_BUDGET_EXHAUSTED");
    }

    #[test]
    fn test_identical_repeat_tripping() {
        let mut tracker = ToolCallTracker::with_limits(25, 3);
        let args = json!({"cmd": "ls"});

        // 3 consecutive identical calls
        tracker.record("run_command", &args);
        assert!(tracker.check("run_command", &args).is_ok());

        tracker.record("run_command", &args);
        assert!(tracker.check("run_command", &args).is_ok());

        tracker.record("run_command", &args);
        // Now 3 identical calls have been recorded consecutively
        let res = tracker.check("run_command", &args);
        assert_eq!(res.unwrap_err(), "TOOL_CALL_LOOP_DETECTED");

        // A different command is still allowed
        let different_args = json!({"cmd": "pwd"});
        assert!(tracker.check("run_command", &different_args).is_ok());
    }

    #[test]
    fn test_ping_pong_loop_tripping() {
        let mut tracker = ToolCallTracker::with_limits(25, 5);
        let args_a = json!({"step": "fetch"});
        let args_b = json!({"step": "parse"});

        // Sequence: A -> B -> A -> B -> A
        tracker.record("tool_a", &args_a);
        tracker.record("tool_b", &args_b);
        tracker.record("tool_a", &args_a);
        tracker.record("tool_b", &args_b);
        tracker.record("tool_a", &args_a);

        // Candidate B would complete A -> B -> A -> B -> A -> B
        let err = tracker.check("tool_b", &args_b).unwrap_err();
        assert_eq!(err, "TOOL_CALL_PING_PONG_DETECTED");

        // Tool C is not part of the alternation, so it is allowed
        let args_c = json!({"step": "clean"});
        assert!(tracker.check("tool_c", &args_c).is_ok());
    }

    #[test]
    fn test_rejected_action_retry_blocking() {
        let mut tracker = ToolCallTracker::new();
        let dangerous_action = json!({"cmd": "rm -rf /data"});

        assert!(tracker.check("run_command", &dangerous_action).is_ok());

        // User rejected the card
        tracker.record_rejected("run_command", &dangerous_action);

        assert!(tracker.is_rejected_in_turn("run_command", &dangerous_action));
        assert_eq!(
            tracker.check("run_command", &dangerous_action).unwrap_err(),
            "ACTION_PREVIOUSLY_REJECTED"
        );

        // Submitting with slightly different key ordering still rejected due to canonicalization
        let reordered = json!({"cmd": "rm -rf /data"});
        assert_eq!(
            tracker.check("run_command", &reordered).unwrap_err(),
            "ACTION_PREVIOUSLY_REJECTED"
        );

        // A different command is allowed
        let safe_action = json!({"cmd": "echo ok"});
        assert!(tracker.check("run_command", &safe_action).is_ok());

        // Reset turn clears rejected action
        tracker.reset_turn();
        assert!(!tracker.is_rejected_in_turn("run_command", &dangerous_action));
        assert!(tracker.check("run_command", &dangerous_action).is_ok());
    }
}
