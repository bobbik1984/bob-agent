//! `BP4-05`: Bounded Native API Subagents (有界原生 API 子代理)
//!
//! Architectural requirements (CURRENT_STAGE §3.8, wiki/kernel-schema §10.4):
//! 1. **Topology**: Exactly 1 Architect Orchestrator + 1 Bounded Read-Only Subagent.
//!    - Recursion depth limit = 1 (`depth == 1`). Subagents are strictly prohibited from spawning child subagents.
//!    - Concurrency limit = 1 active subagent per architect orchestrator / parent task.
//! 2. **Permission Intersection & Sandboxing**:
//!    - Subagent permissions are strictly the intersection of parent grants and read-only policy.
//!    - Workspace write authorization is ALWAYS `false`. Attempts to mutate disk, state, or git are rejected.
//!    - Path confinement: File reads must stay within the parent's `workspace_root`. Escapes (`..`) are rejected.
//!    - Secrets: Subagents never receive raw vault master keys or provider API keys.
//! 3. **Resource & Cost Quotas**:
//!    - Strict `max_tokens`, `max_budget_usd`, and `max_timeout_ms` bounds.
//!    - If provider does not report cost or model identity, explicitly recorded as `"unknown"`.
//! 4. **Cancellation & Restart Reconciliation**:
//!    - Cascade cancellation: Cancelling parent immediately aborts in-flight subagents and generates termination evidence.
//!    - Crash / restart recovery: Any in-flight run transitions to `Interrupted` and is NEVER auto-replayed (prevents double billing).
//!    - Duplicate results do not trigger duplicate wakeups.
//! 5. **Structured Synthesis vs. Isolated Spool**:
//!    - Raw tool calls, thinking frames, and verbose intermediate dialogues are stored in isolated audit logs (`subagents/audit/<id>.jsonl`).
//!    - Main conversation context receives ONLY a structured synthesized conclusion (`SubagentSynthesizedConclusion`)
//!      with verified citations, discrepancies, and model claims verified against on-disk ground truth.

use crate::claude_harness::sha256_hex;
use crate::now_millis;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SUBAGENTS_STATE_DIR: &str = "subagent-gate";
pub const RUNS_INDEX_FILE: &str = "runs.json";
pub const AUDIT_DIR_NAME: &str = "audit";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentRole {
    ReadOnlyResearcher,
    CodeAuditor,
    VerificationChecker,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentGrant {
    pub parent_task_id: String,
    pub parent_session_id: String,
    pub parent_grant_id: String,
    pub project_id: String,
    pub workspace_root: PathBuf,
    pub allowed_subpaths: Vec<String>,
    pub depth: u32,
    pub max_tokens: u64,
    pub max_budget_usd: f64,
    pub max_timeout_ms: u64,
    pub parent_cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentInvocationRequest {
    pub request_id: String,
    pub parent_task_id: String,
    pub parent_session_id: String,
    pub role: SubagentRole,
    pub goal: String,
    pub prompt: String,
    pub estimated_cost_usd: Option<f64>,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentRunStatus {
    Dispatched,
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
    BudgetExceeded,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentCostUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: Option<f64>,
    pub cost_status: String, // "exact" | "estimated" | "unknown"
    pub identity: String,    // model identity or "unknown"
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancellationEvidence {
    pub cancelled_at: i64,
    pub reason: String,
    pub process_terminated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedCitation {
    pub relative_path: String,
    pub sha256: String,
    pub line_range: Option<String>,
    pub verified_on_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscrepancyReport {
    pub item: String,
    pub expected: String,
    pub observed: String,
    pub severity: String, // "warning" | "error" | "info"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelClaimVerification {
    pub claim: String,
    pub verification_status: String, // "verified" | "unverified" | "disproved"
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentSynthesizedConclusion {
    pub run_id: String,
    pub parent_task_id: String,
    pub role: SubagentRole,
    pub summary: String,
    pub verified_facts: Vec<String>,
    pub discrepancies: Vec<DiscrepancyReport>,
    pub citations: Vec<VerifiedCitation>,
    pub model_claims: Vec<ModelClaimVerification>,
    pub evidence_hashes: Vec<String>,
    pub usage: SubagentCostUsage,
    pub completed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentAuditStep {
    pub step_index: usize,
    pub action: String,
    pub tool: Option<String>,
    pub input_summary: String,
    pub output_digest: String,
    pub tokens: u64,
    pub cost_usd: Option<f64>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentRunRecord {
    pub run_id: String,
    pub parent_task_id: String,
    pub parent_session_id: String,
    pub role: SubagentRole,
    pub request_hash: String,
    pub status: SubagentRunStatus,
    pub depth: u32,
    pub charged_cost_usd: f64,
    pub accumulated_tokens: u64,
    pub error_code: Option<String>,
    pub cancellation_evidence: Option<CancellationEvidence>,
    pub conclusion: Option<SubagentSynthesizedConclusion>,
    pub isolated_audit_log_path: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Simulated / Mock step execution request for the bounded subagent engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentStepAction {
    pub tool: String,
    pub relative_path: Option<String>,
    pub search_query: Option<String>,
    pub claim_check: Option<String>,
    pub write_attempt_content: Option<String>, // if present, must trigger write forbidden
    pub spawn_subagent_attempt: bool,         // if true, must trigger depth limit exceeded
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub reported_cost_usd: Option<f64>,
    pub reported_identity: Option<String>,
    pub delay_ms: u64,
}

fn state_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(SUBAGENTS_STATE_DIR)
}

fn index_file(data_dir: &Path) -> PathBuf {
    state_dir(data_dir).join(RUNS_INDEX_FILE)
}

fn audit_dir(data_dir: &Path) -> PathBuf {
    state_dir(data_dir).join(AUDIT_DIR_NAME)
}

fn load_runs(data_dir: &Path) -> Result<BTreeMap<String, SubagentRunRecord>, &'static str> {
    let p = index_file(data_dir);
    if !p.exists() {
        return Ok(BTreeMap::new());
    }
    let data = fs::read(&p).map_err(|_| "SUBAGENT_STORE_READ_FAILED")?;
    if data.is_empty() {
        return Ok(BTreeMap::new());
    }
    serde_json::from_slice(&data).map_err(|_| "SUBAGENT_STORE_CORRUPT")
}

fn save_runs(data_dir: &Path, runs: &BTreeMap<String, SubagentRunRecord>) -> Result<(), &'static str> {
    let s_dir = state_dir(data_dir);
    fs::create_dir_all(&s_dir).map_err(|_| "SUBAGENT_STORE_WRITE_FAILED")?;
    let tmp = s_dir.join("runs.json.tmp");
    let target = index_file(data_dir);
    let bytes = serde_json::to_vec_pretty(runs).map_err(|_| "SUBAGENT_STORE_SERIALIZE_FAILED")?;
    fs::write(&tmp, bytes).map_err(|_| "SUBAGENT_STORE_WRITE_FAILED")?;
    fs::rename(tmp, target).map_err(|_| "SUBAGENT_STORE_RENAME_FAILED")?;
    Ok(())
}

fn append_audit_step(data_dir: &Path, run_id: &str, step: &SubagentAuditStep) -> Result<String, &'static str> {
    let a_dir = audit_dir(data_dir);
    fs::create_dir_all(&a_dir).map_err(|_| "SUBAGENT_AUDIT_WRITE_FAILED")?;
    let path = a_dir.join(format!("{}.jsonl", run_id));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|_| "SUBAGENT_AUDIT_WRITE_FAILED")?;
    let line = serde_json::to_string(step).map_err(|_| "SUBAGENT_AUDIT_SERIALIZE_FAILED")?;
    writeln!(file, "{}", line).map_err(|_| "SUBAGENT_AUDIT_WRITE_FAILED")?;
    Ok(path.to_string_lossy().to_string())
}

/// Strictly verify and resolve path against workspace root.
/// Returns error if path traverses outside root (via `..` or absolute drive)
/// or if it falls outside allowed_subpaths (if specified).
pub fn resolve_and_verify_path(
    root: &Path,
    rel_path: &str,
    allowed_subpaths: &[String],
) -> Result<PathBuf, &'static str> {
    if rel_path.is_empty() {
        return Err("PATH_ESCAPES_WORKSPACE");
    }
    // Prevent obvious path traversal tokens
    let normalized = rel_path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();
    if parts.iter().any(|&p| p == "..") {
        return Err("PATH_ESCAPES_WORKSPACE");
    }
    let candidate = root.join(rel_path);
    // Canonicalize if root exists, otherwise verify path components
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_candidate = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.clone());

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err("PATH_ESCAPES_WORKSPACE");
    }

    if !allowed_subpaths.is_empty() {
        let authorized = allowed_subpaths.iter().any(|allowed| {
            let allowed_norm = allowed.replace('\\', "/").trim_matches('/').to_string();
            normalized == allowed_norm || normalized.starts_with(&format!("{}/", allowed_norm))
        });
        if !authorized {
            return Err("SUBAGENT_SUBPATH_UNAUTHORIZED");
        }
    }

    Ok(candidate)
}

pub fn compute_subagent_request_hash(req: &SubagentInvocationRequest) -> String {
    let wire = format!(
        "{}:{}:{}:{:?}:{}:{}",
        req.parent_task_id, req.parent_session_id, req.request_id, req.role, req.goal, req.prompt
    );
    sha256_hex(wire.as_bytes())
}

pub struct SubagentPoolCoordinator;

impl SubagentPoolCoordinator {
    /// Dispatches a bounded read-only subagent.
    ///
    /// Bounds enforced:
    /// 1. Depth == 1 (nested spawning fails with SUBAGENT_DEPTH_LIMIT_EXCEEDED).
    /// 2. Concurrency == 1 (concurrent active subagents for the same parent task fail with SUBAGENT_CONCURRENCY_LIMIT_EXCEEDED).
    /// 3. Parent not cancelled (if parent_cancelled, fails with PARENT_CANCELLED).
    /// 4. Budget cap (if estimated_cost_usd > max_budget_usd, fails with SUBAGENT_BUDGET_EXCEEDED).
    /// 5. Idempotent deduplication (same request hash returns existing record; duplicate wakeups ignored).
    pub fn dispatch(
        data_dir: &Path,
        grant: &SubagentGrant,
        req: &SubagentInvocationRequest,
    ) -> Result<SubagentRunRecord, &'static str> {
        if grant.parent_cancelled {
            return Err("PARENT_CANCELLED");
        }
        if grant.depth != 1 {
            return Err("SUBAGENT_DEPTH_LIMIT_EXCEEDED");
        }
        if grant.parent_task_id != req.parent_task_id {
            return Err("SUBAGENT_GRANT_MISMATCH");
        }
        let est_cost = req.estimated_cost_usd.unwrap_or(0.0);
        if est_cost > grant.max_budget_usd {
            return Err("SUBAGENT_BUDGET_EXCEEDED");
        }

        let mut runs = load_runs(data_dir)?;
        let req_hash = compute_subagent_request_hash(req);

        // Check if there is already an active run for this parent task (concurrency limit = 1)
        for r in runs.values() {
            if r.parent_task_id == grant.parent_task_id
                && (r.status == SubagentRunStatus::Dispatched || r.status == SubagentRunStatus::Running)
            {
                // If it is the exact same request, return it idempotently
                if r.request_hash == req_hash {
                    return Ok(r.clone());
                }
                return Err("SUBAGENT_CONCURRENCY_LIMIT_EXCEEDED");
            }
        }

        // Check if exact request was already completed
        if let Some(existing) = runs.values().find(|r| r.request_hash == req_hash) {
            if existing.status == SubagentRunStatus::Completed {
                return Ok(existing.clone());
            }
        }

        let now = now_millis();
        let run_id = format!("subagent-{}", req.request_id);
        let audit_log_rel = format!("{}/{}/{}.jsonl", SUBAGENTS_STATE_DIR, AUDIT_DIR_NAME, run_id);

        let record = SubagentRunRecord {
            run_id: run_id.clone(),
            parent_task_id: grant.parent_task_id.clone(),
            parent_session_id: grant.parent_session_id.clone(),
            role: req.role,
            request_hash: req_hash,
            status: SubagentRunStatus::Dispatched,
            depth: grant.depth,
            charged_cost_usd: est_cost,
            accumulated_tokens: 0,
            error_code: None,
            cancellation_evidence: None,
            conclusion: None,
            isolated_audit_log_path: audit_log_rel,
            created_at: now,
            updated_at: now,
        };

        runs.insert(run_id.clone(), record.clone());
        save_runs(data_dir, &runs)?;

        Ok(record)
    }

    /// Executes a bounded turn for a dispatched or running subagent.
    ///
    /// Validates tool safety:
    /// - Write attempts reject immediately with `SUBAGENT_WRITE_FORBIDDEN`.
    /// - Nested subagent spawn attempts reject immediately with `SUBAGENT_DEPTH_LIMIT_EXCEEDED`.
    /// - File reads outside root or allowed_subpaths reject with `PATH_ESCAPES_WORKSPACE` or `SUBAGENT_SUBPATH_UNAUTHORIZED`.
    /// - Quotas (tokens, budget, timeout) strictly checked.
    /// - Model claims are checked against actual workspace files.
    pub fn execute_bounded_turn(
        data_dir: &Path,
        grant: &SubagentGrant,
        run_id: &str,
        actions: &[SubagentStepAction],
    ) -> Result<SubagentRunRecord, &'static str> {
        let mut runs = load_runs(data_dir)?;
        let mut record = runs.get(run_id).cloned().ok_or("SUBAGENT_NOT_FOUND")?;

        if record.status == SubagentRunStatus::Completed
            || record.status == SubagentRunStatus::Cancelled
            || record.status == SubagentRunStatus::Interrupted
        {
            // Already finalized; duplicate calls do not re-run or double-charge
            return Ok(record);
        }

        if grant.parent_cancelled {
            record.status = SubagentRunStatus::Cancelled;
            record.cancellation_evidence = Some(CancellationEvidence {
                cancelled_at: now_millis(),
                reason: "PARENT_CANCELLED".to_string(),
                process_terminated: true,
            });
            record.updated_at = now_millis();
            runs.insert(run_id.to_string(), record.clone());
            save_runs(data_dir, &runs)?;
            return Ok(record);
        }

        record.status = SubagentRunStatus::Running;
        record.updated_at = now_millis();

        let mut verified_citations = Vec::new();
        let mut discrepancies = Vec::new();
        let mut model_claim_verifications = Vec::new();
        let mut evidence_hashes = Vec::new();
        let mut total_duration_ms: u64 = 0;
        let mut last_identity = "unknown".to_string();
        let mut cost_status = "unknown".to_string();

        for (idx, action) in actions.iter().enumerate() {
            // Check timeout
            total_duration_ms += action.delay_ms;
            if total_duration_ms > grant.max_timeout_ms {
                record.status = SubagentRunStatus::TimedOut;
                record.error_code = Some("SUBAGENT_TIMEOUT".to_string());
                record.updated_at = now_millis();
                runs.insert(run_id.to_string(), record.clone());
                save_runs(data_dir, &runs)?;
                return Err("SUBAGENT_TIMEOUT");
            }

            // Check depth limit on subagent attempt
            if action.spawn_subagent_attempt {
                record.status = SubagentRunStatus::Failed;
                record.error_code = Some("SUBAGENT_DEPTH_LIMIT_EXCEEDED".to_string());
                record.updated_at = now_millis();
                runs.insert(run_id.to_string(), record.clone());
                save_runs(data_dir, &runs)?;
                return Err("SUBAGENT_DEPTH_LIMIT_EXCEEDED");
            }

            // Check write permission ban
            if action.write_attempt_content.is_some() || action.tool == "write_file" || action.tool == "edit_file" {
                record.status = SubagentRunStatus::Failed;
                record.error_code = Some("SUBAGENT_WRITE_FORBIDDEN".to_string());
                record.updated_at = now_millis();
                runs.insert(run_id.to_string(), record.clone());
                save_runs(data_dir, &runs)?;
                return Err("SUBAGENT_WRITE_FORBIDDEN");
            }

            // Check tokens cap
            let step_tokens = action.prompt_tokens + action.completion_tokens;
            if record.accumulated_tokens + step_tokens > grant.max_tokens {
                record.status = SubagentRunStatus::BudgetExceeded;
                record.error_code = Some("SUBAGENT_TOKEN_LIMIT_EXCEEDED".to_string());
                record.updated_at = now_millis();
                runs.insert(run_id.to_string(), record.clone());
                save_runs(data_dir, &runs)?;
                return Err("SUBAGENT_TOKEN_LIMIT_EXCEEDED");
            }
            record.accumulated_tokens += step_tokens;

            // Check budget cost cap
            if let Some(cost) = action.reported_cost_usd {
                if record.charged_cost_usd + cost > grant.max_budget_usd {
                    record.status = SubagentRunStatus::BudgetExceeded;
                    record.error_code = Some("SUBAGENT_BUDGET_EXCEEDED".to_string());
                    record.updated_at = now_millis();
                    runs.insert(run_id.to_string(), record.clone());
                    save_runs(data_dir, &runs)?;
                    return Err("SUBAGENT_BUDGET_EXCEEDED");
                }
                record.charged_cost_usd += cost;
                cost_status = "exact".to_string();
            }

            if let Some(ref id) = action.reported_identity {
                if !id.is_empty() {
                    last_identity = id.clone();
                }
            }

            // Execute read / audit tool
            let mut step_digest = String::new();
            if let Some(ref rel) = action.relative_path {
                let resolved = resolve_and_verify_path(&grant.workspace_root, rel, &grant.allowed_subpaths)?;
                if resolved.exists() {
                    let bytes = fs::read(&resolved).unwrap_or_default();
                    let hash = sha256_hex(&bytes);
                    evidence_hashes.push(hash.clone());
                    step_digest = format!("read_file:{}:sha256={}", rel, &hash[..12]);
                    verified_citations.push(VerifiedCitation {
                        relative_path: rel.clone(),
                        sha256: hash,
                        line_range: None,
                        verified_on_disk: true,
                    });

                    // If checking a claim against disk
                    if let Some(ref check) = action.claim_check {
                        let content_str = String::from_utf8_lossy(&bytes);
                        if content_str.contains(check) {
                            model_claim_verifications.push(ModelClaimVerification {
                                claim: format!("File {} contains '{}'", rel, check),
                                verification_status: "verified".to_string(),
                                evidence: format!("Confirmed on disk in {}", rel),
                            });
                        } else {
                            model_claim_verifications.push(ModelClaimVerification {
                                claim: format!("File {} contains '{}'", rel, check),
                                verification_status: "disproved".to_string(),
                                evidence: format!("Pattern '{}' not found in {}", check, rel),
                            });
                            discrepancies.push(DiscrepancyReport {
                                item: format!("Claim verification for {}", rel),
                                expected: format!("Contains '{}'", check),
                                observed: "Pattern absent in current workspace file".to_string(),
                                severity: "warning".to_string(),
                            });
                        }
                    }
                } else {
                    step_digest = format!("read_file:{}:missing", rel);
                    discrepancies.push(DiscrepancyReport {
                        item: format!("File access {}", rel),
                        expected: "File should exist in workspace".to_string(),
                        observed: "File not found on disk".to_string(),
                        severity: "error".to_string(),
                    });
                }
            } else if let Some(ref q) = action.search_query {
                step_digest = format!("search_code:query={}", q);
                let q_hash = sha256_hex(q.as_bytes());
                evidence_hashes.push(q_hash);
            }

            // Append isolated audit step (does NOT pollute main conversation)
            let audit_step = SubagentAuditStep {
                step_index: idx,
                action: action.tool.clone(),
                tool: Some(action.tool.clone()),
                input_summary: format!(
                    "rel={:?}, search={:?}, claim={:?}",
                    action.relative_path, action.search_query, action.claim_check
                ),
                output_digest: step_digest,
                tokens: step_tokens,
                cost_usd: action.reported_cost_usd,
                timestamp: now_millis(),
            };
            let _ = append_audit_step(data_dir, run_id, &audit_step);
        }

        // Build structured conclusion for parent context
        let usage = SubagentCostUsage {
            prompt_tokens: record.accumulated_tokens / 2,
            completion_tokens: record.accumulated_tokens - (record.accumulated_tokens / 2),
            total_tokens: record.accumulated_tokens,
            cost_usd: if cost_status == "unknown" { None } else { Some(record.charged_cost_usd) },
            cost_status,
            identity: last_identity,
            duration_ms: total_duration_ms,
        };

        let summary = format!(
            "Bounded subagent ({:?}) completed with {} verified citations and {} discrepancies.",
            record.role,
            verified_citations.len(),
            discrepancies.len()
        );

        let conclusion = SubagentSynthesizedConclusion {
            run_id: run_id.to_string(),
            parent_task_id: grant.parent_task_id.clone(),
            role: record.role,
            summary,
            verified_facts: verified_citations
                .iter()
                .map(|c| format!("Verified file: {} (sha256: {})", c.relative_path, &c.sha256[..12]))
                .collect(),
            discrepancies,
            citations: verified_citations,
            model_claims: model_claim_verifications,
            evidence_hashes,
            usage,
            completed_at: now_millis(),
        };

        record.status = SubagentRunStatus::Completed;
        record.conclusion = Some(conclusion);
        record.updated_at = now_millis();

        runs.insert(run_id.to_string(), record.clone());
        save_runs(data_dir, &runs)?;

        Ok(record)
    }

    /// Cancels an in-flight subagent run.
    ///
    /// Cascaded from parent cancellation or user explicit stop.
    /// Records cancellation evidence and confirms termination without re-invocation.
    pub fn cancel(
        data_dir: &Path,
        parent_task_id: &str,
        run_id: Option<&str>,
        reason: &str,
    ) -> Result<SubagentRunRecord, &'static str> {
        let mut runs = load_runs(data_dir)?;
        let mut target_key = None;

        if let Some(rid) = run_id {
            if runs.contains_key(rid) {
                target_key = Some(rid.to_string());
            }
        } else {
            // Find active run for parent_task_id
            for (k, v) in &runs {
                if v.parent_task_id == parent_task_id
                    && (v.status == SubagentRunStatus::Dispatched || v.status == SubagentRunStatus::Running)
                {
                    target_key = Some(k.clone());
                    break;
                }
            }
        }

        let key = target_key.ok_or("SUBAGENT_NOT_FOUND")?;
        let mut record = runs.get(&key).cloned().ok_or("SUBAGENT_NOT_FOUND")?;

        if record.status != SubagentRunStatus::Completed && record.status != SubagentRunStatus::Cancelled {
            record.status = SubagentRunStatus::Cancelled;
            record.cancellation_evidence = Some(CancellationEvidence {
                cancelled_at: now_millis(),
                reason: reason.to_string(),
                process_terminated: true,
            });
            record.updated_at = now_millis();
            runs.insert(key, record.clone());
            save_runs(data_dir, &runs)?;
        }

        Ok(record)
    }

    /// Reconciles runs on node restart / crash recovery:
    /// Any run left in `Running` or `Dispatched` transitions to `Interrupted`,
    /// never re-dispatched automatically (no double billing).
    pub fn reconcile_on_restart(data_dir: &Path) -> Result<Vec<SubagentRunRecord>, &'static str> {
        let mut runs = load_runs(data_dir)?;
        let mut reconciled = Vec::new();
        let now = now_millis();

        for record in runs.values_mut() {
            if record.status == SubagentRunStatus::Dispatched || record.status == SubagentRunStatus::Running {
                record.status = SubagentRunStatus::Interrupted;
                record.error_code = Some("RECONCILED_AFTER_RESTART".to_string());
                record.updated_at = now;
                reconciled.push(record.clone());
            }
        }

        if !reconciled.is_empty() {
            save_runs(data_dir, &runs)?;
        }

        Ok(reconciled)
    }

    pub fn get_run(data_dir: &Path, run_id: &str) -> Result<SubagentRunRecord, &'static str> {
        let runs = load_runs(data_dir)?;
        runs.get(run_id).cloned().ok_or("SUBAGENT_NOT_FOUND")
    }

    pub fn list_runs_for_task(data_dir: &Path, parent_task_id: &str) -> Result<Vec<SubagentRunRecord>, &'static str> {
        let runs = load_runs(data_dir)?;
        Ok(runs
            .into_values()
            .filter(|r| r.parent_task_id == parent_task_id)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_workspace(dir: &Path) -> PathBuf {
        let ws = dir.join("workspace");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("src.rs"), b"pub fn calculate_total() -> u64 { 42 }").unwrap();
        fs::write(ws.join("README.md"), b"# Project Docs").unwrap();
        let sub = ws.join("submodule");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("mod.rs"), b"pub const V: usize = 1;").unwrap();
        ws
    }

    fn sample_grant(ws: &Path) -> SubagentGrant {
        SubagentGrant {
            parent_task_id: "task-parent-101".to_string(),
            parent_session_id: "session-p-101".to_string(),
            parent_grant_id: "grant-p-101".to_string(),
            project_id: "proj-alpha".to_string(),
            workspace_root: ws.to_path_buf(),
            allowed_subpaths: vec!["src.rs".to_string(), "submodule".to_string()],
            depth: 1,
            max_tokens: 4096,
            max_budget_usd: 0.10,
            max_timeout_ms: 10000,
            parent_cancelled: false,
        }
    }

    fn sample_request() -> SubagentInvocationRequest {
        SubagentInvocationRequest {
            request_id: "req-sub-001".to_string(),
            parent_task_id: "task-parent-101".to_string(),
            parent_session_id: "session-p-101".to_string(),
            role: SubagentRole::ReadOnlyResearcher,
            goal: "Audit codebase for calculate_total signature".to_string(),
            prompt: "Please inspect src.rs and verify return type".to_string(),
            estimated_cost_usd: Some(0.01),
            model_id: Some("claude-3-5-haiku".to_string()),
        }
    }

    #[test]
    fn test_depth_limit_enforced_at_dispatch_and_turn() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path();
        let ws = setup_workspace(data_dir);
        let mut grant = sample_grant(&ws);
        let req = sample_request();

        // Depth != 1 rejected at dispatch
        grant.depth = 2;
        let err = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req).unwrap_err();
        assert_eq!(err, "SUBAGENT_DEPTH_LIMIT_EXCEEDED");

        // Normal dispatch with depth == 1 succeeds
        grant.depth = 1;
        let record = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req).unwrap();
        assert_eq!(record.status, SubagentRunStatus::Dispatched);

        // Attempting to spawn subagent during execution turn is rejected
        let action = SubagentStepAction {
            tool: "invoke_subagent".to_string(),
            relative_path: None,
            search_query: None,
            claim_check: None,
            write_attempt_content: None,
            spawn_subagent_attempt: true,
            prompt_tokens: 10,
            completion_tokens: 10,
            reported_cost_usd: Some(0.001),
            reported_identity: Some("mock".to_string()),
            delay_ms: 5,
        };
        let err2 = SubagentPoolCoordinator::execute_bounded_turn(data_dir, &grant, &record.run_id, &[action]).unwrap_err();
        assert_eq!(err2, "SUBAGENT_DEPTH_LIMIT_EXCEEDED");
    }

    #[test]
    fn test_concurrency_limit_and_idempotent_dedup() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path();
        let ws = setup_workspace(data_dir);
        let grant = sample_grant(&ws);
        let req1 = sample_request();

        // Dispatch first run
        let r1 = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req1).unwrap();
        assert_eq!(r1.status, SubagentRunStatus::Dispatched);

        // Same request dispatched again returns same record (idempotent, no duplicate run)
        let r1_dup = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req1).unwrap();
        assert_eq!(r1_dup.run_id, r1.run_id);

        // A second different request for same parent while first is active fails concurrency limit
        let mut req2 = sample_request();
        req2.request_id = "req-sub-002".to_string();
        req2.prompt = "Different subtask".to_string();
        let err = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req2).unwrap_err();
        assert_eq!(err, "SUBAGENT_CONCURRENCY_LIMIT_EXCEEDED");
    }

    #[test]
    fn test_write_attempts_and_path_escapes_strictly_forbidden() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path();
        let ws = setup_workspace(data_dir);
        let grant = sample_grant(&ws);
        let req = sample_request();

        let record = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req).unwrap();

        // Path escape with ..
        let escape_action = SubagentStepAction {
            tool: "read_file".to_string(),
            relative_path: Some("../secret.txt".to_string()),
            search_query: None,
            claim_check: None,
            write_attempt_content: None,
            spawn_subagent_attempt: false,
            prompt_tokens: 10,
            completion_tokens: 10,
            reported_cost_usd: Some(0.001),
            reported_identity: Some("mock".to_string()),
            delay_ms: 5,
        };
        let err_escape = SubagentPoolCoordinator::execute_bounded_turn(data_dir, &grant, &record.run_id, &[escape_action]).unwrap_err();
        assert_eq!(err_escape, "PATH_ESCAPES_WORKSPACE");

        // Subpath unauthorized (README.md is not in allowed_subpaths)
        let unauth_action = SubagentStepAction {
            tool: "read_file".to_string(),
            relative_path: Some("README.md".to_string()),
            search_query: None,
            claim_check: None,
            write_attempt_content: None,
            spawn_subagent_attempt: false,
            prompt_tokens: 10,
            completion_tokens: 10,
            reported_cost_usd: Some(0.001),
            reported_identity: Some("mock".to_string()),
            delay_ms: 5,
        };
        let err_unauth = SubagentPoolCoordinator::execute_bounded_turn(data_dir, &grant, &record.run_id, &[unauth_action]).unwrap_err();
        assert_eq!(err_unauth, "SUBAGENT_SUBPATH_UNAUTHORIZED");

        // Write attempt forbidden
        let write_action = SubagentStepAction {
            tool: "write_file".to_string(),
            relative_path: Some("src.rs".to_string()),
            search_query: None,
            claim_check: None,
            write_attempt_content: Some("malicious mutation".to_string()),
            spawn_subagent_attempt: false,
            prompt_tokens: 10,
            completion_tokens: 10,
            reported_cost_usd: Some(0.001),
            reported_identity: Some("mock".to_string()),
            delay_ms: 5,
        };
        let err_write = SubagentPoolCoordinator::execute_bounded_turn(data_dir, &grant, &record.run_id, &[write_action]).unwrap_err();
        assert_eq!(err_write, "SUBAGENT_WRITE_FORBIDDEN");
    }

    #[test]
    fn test_cascade_cancellation_and_restart_reconciliation_never_replays() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path();
        let ws = setup_workspace(data_dir);
        let grant = sample_grant(&ws);
        let req = sample_request();

        let record = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req).unwrap();

        // Cancel cascaded from parent
        let cancelled = SubagentPoolCoordinator::cancel(data_dir, &grant.parent_task_id, Some(&record.run_id), "PARENT_CANCELLED").unwrap();
        assert_eq!(cancelled.status, SubagentRunStatus::Cancelled);
        let evid = cancelled.cancellation_evidence.unwrap();
        assert_eq!(evid.reason, "PARENT_CANCELLED");
        assert!(evid.process_terminated);

        // Dispatch a new run after first is finalized
        let mut req2 = sample_request();
        req2.request_id = "req-sub-002".to_string();
        let r2 = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req2).unwrap();
        assert_eq!(r2.status, SubagentRunStatus::Dispatched);

        // Node restart reconciliation marks in-flight run as Interrupted and does NOT re-dispatch
        let reconciled = SubagentPoolCoordinator::reconcile_on_restart(data_dir).unwrap();
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].run_id, r2.run_id);
        assert_eq!(reconciled[0].status, SubagentRunStatus::Interrupted);
        assert_eq!(reconciled[0].error_code, Some("RECONCILED_AFTER_RESTART".to_string()));
    }

    #[test]
    fn test_structured_synthesis_and_model_claim_verification_without_raw_bloat() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path();
        let ws = setup_workspace(data_dir);
        let grant = sample_grant(&ws);
        let req = sample_request();

        let record = SubagentPoolCoordinator::dispatch(data_dir, &grant, &req).unwrap();

        let valid_action = SubagentStepAction {
            tool: "read_file".to_string(),
            relative_path: Some("src.rs".to_string()),
            search_query: None,
            claim_check: Some("calculate_total".to_string()),
            write_attempt_content: None,
            spawn_subagent_attempt: false,
            prompt_tokens: 120,
            completion_tokens: 45,
            reported_cost_usd: Some(0.0025),
            reported_identity: Some("gemini-1.5-flash".to_string()),
            delay_ms: 25,
        };

        let discrepancy_action = SubagentStepAction {
            tool: "read_file".to_string(),
            relative_path: Some("submodule/mod.rs".to_string()),
            search_query: None,
            claim_check: Some("non_existent_symbol".to_string()),
            write_attempt_content: None,
            spawn_subagent_attempt: false,
            prompt_tokens: 80,
            completion_tokens: 30,
            reported_cost_usd: None, // provider omitted cost -> must label unknown
            reported_identity: None, // provider omitted identity -> must label unknown
            delay_ms: 20,
        };

        let completed = SubagentPoolCoordinator::execute_bounded_turn(
            data_dir,
            &grant,
            &record.run_id,
            &[valid_action, discrepancy_action],
        )
        .unwrap();

        assert_eq!(completed.status, SubagentRunStatus::Completed);
        let conclusion = completed.conclusion.expect("conclusion");
        assert_eq!(conclusion.citations.len(), 2);
        assert!(conclusion.citations[0].verified_on_disk);
        assert_eq!(conclusion.citations[0].relative_path, "src.rs");

        // Model claim verification separated into verified vs disproved
        assert_eq!(conclusion.model_claims.len(), 2);
        assert_eq!(conclusion.model_claims[0].verification_status, "verified");
        assert_eq!(conclusion.model_claims[1].verification_status, "disproved");

        // Discrepancy reported
        assert_eq!(conclusion.discrepancies.len(), 1);
        assert_eq!(conclusion.discrepancies[0].severity, "warning");

        // Usage and cost attribution
        assert_eq!(conclusion.usage.total_tokens, 275);
        assert_eq!(conclusion.usage.cost_usd, Some(0.0125)); // initial 0.01 estimated + 0.0025

        // Check that isolated audit log was created and contains raw steps
        let audit_file = data_dir.join(&completed.isolated_audit_log_path);
        assert!(audit_file.exists());
        let audit_content = fs::read_to_string(&audit_file).unwrap();
        let lines: Vec<&str> = audit_content.lines().collect();
        assert_eq!(lines.len(), 2);
    }
}
