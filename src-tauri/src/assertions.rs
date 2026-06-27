use serde_json::Value;

/// 断言检查结果
pub struct AssertionResult {
    pub passed: bool,
    pub failures: Vec<AssertionFailure>,
    pub warnings: Vec<String>,
}

pub struct AssertionFailure {
    pub rule: String,
    pub description: String,
}

impl AssertionResult {
    pub fn has_fatal(&self) -> bool {
        !self.failures.is_empty()
    }

    /// Format failures + warnings into a single feedback string for injection into the message stream
    pub fn format_feedback(&self) -> String {
        let mut parts = Vec::new();
        for f in &self.failures {
            parts.push(format!("- ❌ [{}] {}", f.rule, f.description));
        }
        for w in &self.warnings {
            parts.push(format!("- ⚠️ {}", w));
        }
        format!("确定性检查结果:\n{}", parts.join("\n"))
    }
}

/// Read-only tools that don't produce side effects
const READ_ONLY_TOOLS: &[&str] = &[
    "read_file", "list_dir", "list_skills", "read_skill", "system_time",
    "brain_search", "search_messages",
];

/// Write tools that produce observable side effects
const WRITE_TOOLS: &[&str] = &[
    "create_file", "create_directory", "move_file", "copy_file",
    "delete_file", "rename_file", "run_command",
];

/// Run deterministic assertions on the tool execution summary.
/// Returns pass/fail result with zero LLM token cost.
pub fn run_assertions(tool_summary: &Value) -> AssertionResult {
    let mut failures = Vec::new();
    let mut warnings = Vec::new();

    let total_calls = tool_summary.get("total_calls")
        .and_then(|v| v.as_i64()).unwrap_or(0);
    let total_failures = tool_summary.get("total_failures")
        .and_then(|v| v.as_i64()).unwrap_or(0);
    let calls = tool_summary.get("calls")
        .and_then(|v| v.as_array());

    // ── Rule 1: NO_EMPTY_EXECUTION ──
    // Maker didn't call any tools at all
    if total_calls == 0 {
        failures.push(AssertionFailure {
            rule: "NO_EMPTY_EXECUTION".to_string(),
            description: "Maker 未调用任何工具。请使用可用工具来完成目标，而不是仅输出文本。".to_string(),
        });
        // Early return - no point checking other rules
        return AssertionResult { passed: false, failures, warnings };
    }

    // ── Rule 2: ERROR_RATIO ──
    // More than 50% of tool calls failed
    if total_calls > 2 && total_failures * 2 > total_calls {
        failures.push(AssertionFailure {
            rule: "ERROR_RATIO".to_string(),
            description: format!(
                "工具调用错误率过高：{}/{} 次调用失败 ({:.0}%)。请检查参数和前置条件后重试。",
                total_failures, total_calls,
                (total_failures as f64 / total_calls as f64) * 100.0
            ),
        });
    }

    // ── Rule 3: NO_ANALYSIS_PARALYSIS ──
    // Last 10+ calls are all read-only with no writes
    if let Some(call_list) = calls {
        let tail_size = 10.min(call_list.len());
        if tail_size >= 10 {
            let tail = &call_list[call_list.len() - tail_size..];
            let all_readonly = tail.iter().all(|c| {
                let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                READ_ONLY_TOOLS.contains(&name)
            });
            // Also check if there were ANY write calls in the entire session
            let has_any_write = call_list.iter().any(|c| {
                let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                WRITE_TOOLS.contains(&name)
            });
            if all_readonly && !has_any_write {
                failures.push(AssertionFailure {
                    rule: "NO_ANALYSIS_PARALYSIS".to_string(),
                    description: "检测到分析瘫痪：最近 10 次工具调用全部为只读操作且无任何写入。请基于已收集的信息采取行动。".to_string(),
                });
            }
        }

        // ── Rule 4: FILE_WRITE_SUCCESS ──
        // All file write operations failed
        let write_calls: Vec<&Value> = call_list.iter().filter(|c| {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            name == "create_file" || name == "create_directory"
        }).collect();
        if !write_calls.is_empty() {
            let all_failed = write_calls.iter().all(|c| {
                c.get("success").and_then(|v| v.as_bool()).unwrap_or(true) == false
            });
            if all_failed {
                failures.push(AssertionFailure {
                    rule: "FILE_WRITE_SUCCESS".to_string(),
                    description: format!(
                        "所有 {} 次文件写入操作均失败。请检查目标路径是否存在、权限是否充足。",
                        write_calls.len()
                    ),
                });
            }
        }

        // ── Rule 5: COMMAND_EXIT_CODE ──
        // Last run_command call failed (warning only)
        let last_cmd = call_list.iter().rev().find(|c| {
            c.get("name").and_then(|v| v.as_str()) == Some("run_command")
        });
        if let Some(cmd) = last_cmd {
            if cmd.get("success").and_then(|v| v.as_bool()) == Some(false) {
                warnings.push("最近一次命令执行失败（exit code ≠ 0），请检查命令输出并修正。".to_string());
            }
        }
    }

    // ── Rule 6: BUDGET_EFFICIENCY ──
    // Used >80% budget but no write operations
    if let Some(call_list) = calls {
        let budget = 50i64; // Goal mode budget
        if total_calls > (budget * 4 / 5) {
            let has_write = call_list.iter().any(|c| {
                let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                WRITE_TOOLS.contains(&name)
            });
            if !has_write {
                warnings.push(format!(
                    "预算效率低：已消耗 {}/{} 次工具调用但未产生任何写入操作。",
                    total_calls, budget
                ));
            }
        }
    }

    let passed = failures.is_empty();
    AssertionResult { passed, failures, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_empty_execution() {
        let summary = json!({ "total_calls": 0, "total_failures": 0, "calls": [] });
        let result = run_assertions(&summary);
        assert!(!result.passed);
        assert!(result.has_fatal());
        assert_eq!(result.failures[0].rule, "NO_EMPTY_EXECUTION");
    }

    #[test]
    fn test_high_error_ratio() {
        let summary = json!({
            "total_calls": 6, "total_failures": 4,
            "calls": [
                {"name": "read_file", "success": true},
                {"name": "create_file", "success": false},
                {"name": "create_file", "success": false},
                {"name": "run_command", "success": false},
                {"name": "run_command", "success": false},
                {"name": "read_file", "success": true},
            ]
        });
        let result = run_assertions(&summary);
        assert!(!result.passed);
        assert!(result.failures.iter().any(|f| f.rule == "ERROR_RATIO"));
    }

    #[test]
    fn test_analysis_paralysis() {
        let calls: Vec<Value> = (0..12).map(|_| json!({"name": "read_file", "success": true})).collect();
        let summary = json!({
            "total_calls": 12, "total_failures": 0,
            "calls": calls
        });
        let result = run_assertions(&summary);
        assert!(!result.passed);
        assert!(result.failures.iter().any(|f| f.rule == "NO_ANALYSIS_PARALYSIS"));
    }

    #[test]
    fn test_all_pass() {
        let summary = json!({
            "total_calls": 5, "total_failures": 0,
            "calls": [
                {"name": "read_file", "success": true},
                {"name": "list_dir", "success": true},
                {"name": "create_file", "success": true},
                {"name": "run_command", "success": true},
                {"name": "read_file", "success": true},
            ]
        });
        let result = run_assertions(&summary);
        assert!(result.passed);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_all_writes_failed() {
        let summary = json!({
            "total_calls": 4, "total_failures": 2,
            "calls": [
                {"name": "read_file", "success": true},
                {"name": "create_file", "success": false},
                {"name": "create_file", "success": false},
                {"name": "read_file", "success": true},
            ]
        });
        let result = run_assertions(&summary);
        assert!(!result.passed);
        assert!(result.failures.iter().any(|f| f.rule == "FILE_WRITE_SUCCESS"));
    }

    #[test]
    fn test_command_warning() {
        let summary = json!({
            "total_calls": 3, "total_failures": 1,
            "calls": [
                {"name": "read_file", "success": true},
                {"name": "create_file", "success": true},
                {"name": "run_command", "success": false},
            ]
        });
        let result = run_assertions(&summary);
        // Should pass (no fatal), but have warning
        assert!(result.passed);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_null_summary_graceful() {
        let result = run_assertions(&Value::Null);
        assert!(!result.passed); // NO_EMPTY_EXECUTION fires
    }
}
