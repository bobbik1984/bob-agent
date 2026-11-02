//! `BP4-06`: Native `Claude Code` (`claude` CLI) execution adapter, `TmuxHarnessRunner`
//! process keep-alive & lifecycle manager, native session resumption (`--resume <session_id>`),
//! serialized workspace write lock, and governed `V1 -> V2` deliverable verification chain.

use crate::now_millis;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const GRANTS_FILE: &str = "claude-harness-grants.json";
const HARNESS_DIR: &str = "claude-harness";
const MAX_PROMPT_BYTES: usize = 32_000;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeHarnessGrant {
    pub id: String,
    pub connection_id: String,
    pub session_id: String,
    pub title: String,
    pub cwd: String,
    #[serde(default = "default_claude_bin")]
    pub claude_bin: String,
    #[serde(default)]
    pub claude_args_prefix: Vec<String>,
    #[serde(default = "default_output_format")]
    pub output_format: String,
    #[serde(default = "default_true")]
    pub prefer_tmux: bool,
    #[serde(default)]
    pub tmux_bin: Option<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_deliverable_kind")]
    pub deliverable_kind: String,
    #[serde(default)]
    pub deliverable_paths: Vec<String>,
    #[serde(default)]
    pub auth_configured: Option<bool>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub max_budget_usd: Option<f64>,
    #[serde(default)]
    pub max_revisions: Option<u64>,
    #[serde(default)]
    pub remote_ssh_target: Option<String>,
    #[serde(default)]
    pub remote_cwd: Option<String>,
}

fn default_claude_bin() -> String {
    "claude".to_owned()
}

fn default_output_format() -> String {
    "json".to_owned()
}

fn default_true() -> bool {
    true
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_deliverable_kind() -> String {
    "auto".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeCapabilityReport {
    pub grant_id: String,
    pub connection_id: String,
    pub session_id: String,
    pub harness: String,
    pub tool_label: String,
    pub binary_path: String,
    pub available: bool,
    pub version: Option<String>,
    pub auth_status: String,
    pub supports_output_format_json: bool,
    pub supports_output_format_stream_json: bool,
    pub supports_resume: bool,
    pub tmux_available: bool,
    pub tmux_version: Option<String>,
    pub execution_mode: String,
    pub cwd_available: bool,
    pub observed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeSessionState {
    pub grant_id: String,
    pub connection_id: String,
    pub session_id: String,
    pub title: String,
    pub cwd: String,
    pub native_session_id: Option<String>,
    pub last_harness: String,
    pub turn_count: u64,
    pub active_request_id: Option<String>,
    pub latest_version_id: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceLease {
    pub workspace_hash: String,
    pub cwd: String,
    pub connection_id: String,
    pub session_id: String,
    pub harness: String,
    pub request_id: String,
    pub request_hash: String,
    pub pid: Option<u32>,
    pub tmux_session_name: Option<String>,
    pub started_at: i64,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartedExecutionRecord {
    pub grant_id: String,
    pub connection_id: String,
    pub session_id: String,
    pub request_id: String,
    pub request_hash: String,
    pub text_hash: String,
    pub prompt_text: String,
    pub wire_prompt: String,
    pub route_kind: String,
    pub expected_native_session_id: Option<String>,
    pub parent_version_id: Option<String>,
    pub execution_mode: String,
    pub tmux_session_name: Option<String>,
    pub pid: Option<u32>,
    pub started_at: i64,
    pub timeout_ms: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverableFileDigest {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationCheckItem {
    pub id: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationReport {
    pub id: String,
    pub version_id: String,
    pub deliverable_kind: String,
    pub status: String,
    pub summary: String,
    pub checks: Vec<VerificationCheckItem>,
    pub verified_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactVersionRecord {
    pub version_id: String,
    pub version_number: u64,
    pub parent_version_id: Option<String>,
    pub grant_id: String,
    pub session_id: String,
    pub request_id: String,
    pub instruction: String,
    pub deliverable_kind: String,
    pub snapshot_dir: String,
    pub bundle_sha256: String,
    pub files: Vec<DeliverableFileDigest>,
    pub lifecycle_state: String,
    pub verification: VerificationReport,
    pub review_status: String,
    pub review_note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeExecutionReceipt {
    pub grant_id: String,
    pub connection_id: String,
    pub session_id: String,
    pub request_id: String,
    pub request_hash: String,
    pub text_hash: String,
    pub status: String,
    pub route_kind: String,
    pub execution_mode: String,
    pub tmux_session_name: Option<String>,
    pub native_session_id: Option<String>,
    pub reply_text: Option<String>,
    pub error_code: Option<String>,
    pub recovered: bool,
    pub artifact_version: Option<ArtifactVersionRecord>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedClaudeOutput {
    pub session_id: String,
    pub result_text: String,
    pub is_error: bool,
    pub subtype: String,
    #[serde(default)]
    pub reported_model: Option<String>,
    #[serde(default)]
    pub total_cost_usd: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn workspace_hash(cwd: &str) -> String {
    let normalized = cwd.trim().replace('\\', "/").to_lowercase();
    sha256_hex(normalized.as_bytes())[..24].to_owned()
}

fn request_key_hash(connection_id: &str, session_id: &str, request_id: &str) -> String {
    let mut h = Sha256::new();
    for part in [connection_id, session_id, request_id] {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), &'static str> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "CLAUDE_STORAGE_WRITE_FAILED")?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "CLAUDE_STORAGE_WRITE_FAILED")?;
    fs::write(&tmp, bytes).map_err(|_| "CLAUDE_STORAGE_WRITE_FAILED")?;
    fs::rename(&tmp, path).map_err(|_| "CLAUDE_STORAGE_WRITE_FAILED")
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, &'static str> {
    let raw = fs::read_to_string(path).map_err(|_| "CLAUDE_STORAGE_READ_FAILED")?;
    serde_json::from_str(&raw).map_err(|_| "CLAUDE_STORAGE_CORRUPT")
}

pub fn load_grants(data_dir: &Path) -> Result<Vec<ClaudeHarnessGrant>, &'static str> {
    let path = data_dir.join(GRANTS_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|_| "CLAUDE_GRANTS_READ_FAILED")?;
    serde_json::from_str(&raw).map_err(|_| "CLAUDE_GRANTS_INVALID")
}

pub fn find_grant(
    data_dir: &Path,
    connection_id: &str,
    session_id: &str,
) -> Result<Option<ClaudeHarnessGrant>, &'static str> {
    let grants = load_grants(data_dir)?;
    Ok(grants
        .into_iter()
        .find(|g| g.connection_id == connection_id && (g.session_id == session_id || g.id == session_id)))
}

pub fn find_grant_by_id(
    data_dir: &Path,
    grant_id: &str,
) -> Result<ClaudeHarnessGrant, &'static str> {
    load_grants(data_dir)?
        .into_iter()
        .find(|g| g.id == grant_id || g.session_id == grant_id)
        .ok_or("CLAUDE_GRANT_NOT_FOUND")
}

fn session_state_path(data_dir: &Path, grant_id: &str) -> PathBuf {
    data_dir
        .join(HARNESS_DIR)
        .join("sessions")
        .join(format!("{grant_id}.json"))
}

fn history_path(data_dir: &Path, grant_id: &str) -> PathBuf {
    data_dir
        .join(HARNESS_DIR)
        .join("history")
        .join(format!("{grant_id}.json"))
}

fn artifacts_index_path(data_dir: &Path, grant_id: &str) -> PathBuf {
    data_dir
        .join(HARNESS_DIR)
        .join("artifacts")
        .join(grant_id)
        .join("versions.json")
}

fn workspace_lock_path(data_dir: &Path, cwd: &str) -> PathBuf {
    data_dir
        .join(HARNESS_DIR)
        .join("locks")
        .join(format!("{}.json", workspace_hash(cwd)))
}

fn workspace_activity_path(data_dir: &Path, cwd: &str) -> PathBuf {
    data_dir
        .join(HARNESS_DIR)
        .join("locks")
        .join(format!("{}-activity.json", workspace_hash(cwd)))
}

fn spool_dir(data_dir: &Path, grant_id: &str, req_hash: &str) -> PathBuf {
    data_dir
        .join(HARNESS_DIR)
        .join("spool")
        .join(grant_id)
        .join(req_hash)
}

pub fn load_session_state(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
) -> Result<NativeSessionState, &'static str> {
    let path = session_state_path(data_dir, &grant.id);
    if path.is_file() {
        let mut state: NativeSessionState = read_json_file(&path)?;
        if let Ok(activity) = read_json_file::<Value>(&workspace_activity_path(data_dir, &grant.cwd)) {
            if let Some(h) = activity.get("last_harness").and_then(Value::as_str) {
                state.last_harness = h.to_owned();
            }
        }
        return Ok(state);
    }
    let last_harness = read_json_file::<Value>(&workspace_activity_path(data_dir, &grant.cwd))
        .ok()
        .and_then(|v| v.get("last_harness").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| "claude".to_owned());
    Ok(NativeSessionState {
        grant_id: grant.id.clone(),
        connection_id: grant.connection_id.clone(),
        session_id: grant.session_id.clone(),
        title: grant.title.clone(),
        cwd: grant.cwd.clone(),
        native_session_id: None,
        last_harness,
        turn_count: 0,
        active_request_id: None,
        latest_version_id: None,
        updated_at: now_millis(),
    })
}

fn save_session_state(data_dir: &Path, state: &NativeSessionState) -> Result<(), &'static str> {
    write_json_atomic(&session_state_path(data_dir, &state.grant_id), state)
}

pub fn record_workspace_harness_activity(
    data_dir: &Path,
    cwd: &str,
    harness: &str,
    session_id: &str,
) -> Result<(), &'static str> {
    let path = workspace_activity_path(data_dir, cwd);
    write_json_atomic(
        &path,
        &json!({
            "cwd": cwd,
            "last_harness": harness,
            "session_id": session_id,
            "updated_at": now_millis(),
        }),
    )
}

pub fn record_external_harness_turn_for_session(
    data_dir: &Path,
    connection_id: &str,
    session_id: &str,
    harness: &str,
) -> Result<(), &'static str> {
    if let Ok( raw) = fs::read_to_string(data_dir.join("workspace-context-grants.json")) {
        if let Ok(entries) = serde_json::from_str::<Vec<Value>>(&raw) {
            for entry in entries {
                if entry.get("connection_id").and_then(Value::as_str) == Some(connection_id)
                    && entry.get("session_id").and_then(Value::as_str) == Some(session_id)
                {
                    if let Some(root) = entry.get("root").or_else(|| entry.get("workspace_root")).and_then(Value::as_str) {
                        let _ = record_workspace_harness_activity(data_dir, root, harness, session_id);
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn check_workspace_not_locked_by_other(
    data_dir: &Path,
    connection_id: &str,
    session_id: &str,
) -> Result<(), &'static str> {
    if let Ok(raw) = fs::read_to_string(data_dir.join("workspace-context-grants.json")) {
        if let Ok(entries) = serde_json::from_str::<Vec<Value>>(&raw) {
            for entry in entries {
                if entry.get("connection_id").and_then(Value::as_str) == Some(connection_id)
                    && entry.get("session_id").and_then(Value::as_str) == Some(session_id)
                {
                    if let Some(root) = entry.get("root").or_else(|| entry.get("workspace_root")).and_then(Value::as_str) {
                        let lock_path = workspace_lock_path(data_dir, root);
                        if lock_path.is_file() {
                            if let Ok(lease) = read_json_file::<WorkspaceLease>(&lock_path) {
                                let now = now_millis();
                                let expired = now.saturating_sub(lease.started_at) as u64 > lease.timeout_ms;
                                if !expired && lease.session_id != session_id {
                                    return Err("HARNESS_WORKSPACE_BUSY");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn acquire_workspace_lock(
    data_dir: &Path,
    cwd: &str,
    connection_id: &str,
    session_id: &str,
    harness: &str,
    request_id: &str,
    request_hash: &str,
    timeout_ms: u64,
) -> Result<WorkspaceLease, &'static str> {
    let lock_path = workspace_lock_path(data_dir, cwd);
    let now = now_millis();
    if lock_path.is_file() {
        if let Ok(existing) = read_json_file::<WorkspaceLease>(&lock_path) {
            let elapsed = now.saturating_sub(existing.started_at) as u64;
            let still_alive = elapsed <= existing.timeout_ms && is_execution_alive(&existing);
            if still_alive && existing.request_hash != request_hash {
                return Err("HARNESS_WORKSPACE_BUSY");
            }
        }
    }
    let lease = WorkspaceLease {
        workspace_hash: workspace_hash(cwd),
        cwd: cwd.to_owned(),
        connection_id: connection_id.to_owned(),
        session_id: session_id.to_owned(),
        harness: harness.to_owned(),
        request_id: request_id.to_owned(),
        request_hash: request_hash.to_owned(),
        pid: None,
        tmux_session_name: None,
        started_at: now,
        timeout_ms,
    };
    write_json_atomic(&lock_path, &lease)?;
    Ok(lease)
}

fn update_workspace_lock_handles(
    data_dir: &Path,
    cwd: &str,
    request_hash: &str,
    pid: Option<u32>,
    tmux_session_name: Option<String>,
) -> Result<(), &'static str> {
    let lock_path = workspace_lock_path(data_dir, cwd);
    if !lock_path.is_file() {
        return Ok(());
    }
    let mut lease: WorkspaceLease = read_json_file(&lock_path)?;
    if lease.request_hash == request_hash {
        lease.pid = pid;
        lease.tmux_session_name = tmux_session_name;
        write_json_atomic(&lock_path, &lease)?;
    }
    Ok(())
}

pub fn release_workspace_lock(data_dir: &Path, cwd: &str, request_hash: &str) {
    let lock_path = workspace_lock_path(data_dir, cwd);
    if let Ok(lease) = read_json_file::<WorkspaceLease>(&lock_path) {
        if lease.request_hash == request_hash {
            let _ = fs::remove_file(&lock_path);
        }
    }
}

fn is_execution_alive(lease: &WorkspaceLease) -> bool {
    if let Some(ref tmux_name) = lease.tmux_session_name {
        if let Ok(status) = Command::new("tmux")
            .args(["has-session", "-t", tmux_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            if status.success() {
                return true;
            }
        }
    }
    if let Some(pid) = lease.pid {
        return is_pid_alive(pid);
    }
    true
}

fn is_pid_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.contains(&pid.to_string()))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn kill_execution_process(
    tmux_bin: &str,
    tmux_session_name: Option<&str>,
    pid: Option<u32>,
) -> bool {
    let mut cleaned = true;
    if let Some(name) = tmux_session_name {
        let _ = Command::new(tmux_bin)
            .args(["kill-session", "-t", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let still_has = Command::new(tmux_bin)
            .args(["has-session", "-t", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if still_has {
            cleaned = false;
        }
    }
    if let Some(p) = pid {
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &p.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(not(windows))]
        {
            let _ = Command::new("kill")
                .args(["-9", &p.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        for _ in 0..10 {
            if !is_pid_alive(p) {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        if is_pid_alive(p) {
            cleaned = false;
        }
    }
    cleaned
}

/// Parse Claude Code CLI `--output-format json` or `--output-format stream-json` output.
pub fn parse_claude_output(
    raw: &str,
    expected_resume_session_id: Option<&str>,
) -> Result<ParsedClaudeOutput, &'static str> {
    if raw.len() > MAX_OUTPUT_BYTES {
        return Err("CLAUDE_OUTPUT_TOO_LARGE");
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("CLAUDE_OUTPUT_EMPTY");
    }

    let mut result_obj: Option<Value> = None;
    let mut stream_text_parts: Vec<String> = Vec::new();
    let mut fallback_session_id: Option<String> = None;

    if let Ok(single) = serde_json::from_str::<Value>(trimmed) {
        if single.is_object() {
            result_obj = Some(single);
        }
    }

    if result_obj.is_none() {
        for line in trimmed.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let val: Value = serde_json::from_str(line).map_err(|_| "CLAUDE_OUTPUT_INVALID")?;
            if let Some(sid) = val.get("session_id").and_then(Value::as_str) {
                if !sid.trim().is_empty() {
                    fallback_session_id = Some(sid.trim().to_owned());
                }
            }
            match val.get("type").and_then(Value::as_str) {
                Some("assistant") => {
                    if let Some(text) = val
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(Value::as_array)
                    {
                        for block in text {
                            if let Some(t) = block.get("text").and_then(Value::as_str) {
                                stream_text_parts.push(t.to_owned());
                            }
                        }
                    } else if let Some(t) = val.get("text").and_then(Value::as_str) {
                        stream_text_parts.push(t.to_owned());
                    }
                }
                Some("result") => {
                    result_obj = Some(val);
                }
                _ => {}
            }
        }
    }

    let obj = result_obj.ok_or("CLAUDE_OUTPUT_INVALID")?;
    let is_error = obj.get("is_error").and_then(Value::as_bool).unwrap_or(false);
    let subtype = obj
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or(if is_error { "error" } else { "success" })
        .to_owned();

    if is_error || subtype.starts_with("error") {
        return Err("CLAUDE_REPORTED_ERROR");
    }

    let session_id = obj
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or(fallback_session_id)
        .ok_or("CLAUDE_SESSION_ID_MISSING")?;

    if let Some(expected) = expected_resume_session_id {
        if expected != session_id {
            return Err("CLAUDE_SESSION_MISMATCH");
        }
    }

    let result_text = obj
        .get("result")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| stream_text_parts.join(""))
        .trim()
        .to_owned();

    if result_text.is_empty() {
        return Err("CLAUDE_RESULT_EMPTY");
    }

    let reported_model = obj
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|m| m.keys().next().cloned());
    let total_cost_usd = obj.get("total_cost_usd").and_then(Value::as_f64);
    let duration_ms = obj.get("duration_ms").and_then(Value::as_u64);

    Ok(ParsedClaudeOutput {
        session_id,
        result_text,
        is_error: false,
        subtype,
        reported_model,
        total_cost_usd,
        duration_ms,
    })
}

// Commands are built against the execution host, never a mixture of local and remote probes.
fn host_command(grant: &ClaudeHarnessGrant, executable: &str, args: &[&str]) -> Command {
    if let Some(target) = &grant.remote_ssh_target {
        let mut command = Command::new("ssh");
        let line = std::iter::once(executable).chain(args.iter().copied())
            .map(shell_quote).collect::<Vec<_>>().join(" ");
        command.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "--", target]);
        command.arg(format!("{line} < /dev/null"));
        command
    } else {
        let mut command = Command::new(executable);
        if executable == grant.claude_bin {
            command.args(&grant.claude_args_prefix);
        }
        command.args(args);
        command
    }
}

fn observed_auth(raw: Option<&[u8]>) -> &'static str {
    match raw.and_then(|b| serde_json::from_slice::<Value>(b).ok())
        .and_then(|v| v.get("loggedIn").and_then(Value::as_bool)) {
        Some(true) => "authenticated",
        Some(false) => "missing",
        None => "unverified",
    }
}

/// Read-only observations on the execution host. Configuration is not login evidence.
pub fn probe_capabilities(_data_dir: &Path, grant: &ClaudeHarnessGrant) -> ClaudeCapabilityReport {
    let probe = |bin: &str, args: &[&str]| {
        host_command(grant, bin, args).stdin(Stdio::null())
            .stdout(Stdio::piped()).stderr(Stdio::null()).output().ok()
    };
    let version_out = probe(&grant.claude_bin, &["--version"]);
    let available = version_out.as_ref().is_some_and(|o| o.status.success());
    let version = version_out.filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let tmux = probe(grant.tmux_bin.as_deref().unwrap_or("tmux"), &["-V"]);
    let tmux_available = tmux.as_ref().is_some_and(|o| o.status.success());
    let tmux_version = tmux.filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let cwd_available = if grant.remote_ssh_target.is_some() {
        grant.remote_cwd.as_deref().is_some_and(|cwd|
            probe("test", &["-d", cwd]).is_some_and(|o| o.status.success()))
    } else { Path::new(&grant.cwd).is_dir() };
    let auth = probe(&grant.claude_bin, &["auth", "status", "--json"]);
    let auth_status = observed_auth(auth.as_ref().filter(|o| o.status.success())
        .map(|o| o.stdout.as_slice())).to_owned();
    let help = probe(&grant.claude_bin, &["--help"]);
    let help_text = help.filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    // Remote SSH execution needs a resident receipt/process supervisor. The old
    // branch would start local tmux and could report remote cancellation falsely.
    let execution_mode = if grant.remote_ssh_target.is_some() {
        "remote_supervisor_required"
    } else if grant.prefer_tmux && tmux_available { "tmux" } else { "process" };
    ClaudeCapabilityReport {
        grant_id: grant.id.clone(), connection_id: grant.connection_id.clone(),
        session_id: grant.session_id.clone(), harness: "claude".to_owned(),
        tool_label: "Claude".to_owned(), binary_path: grant.claude_bin.clone(),
        available, version, auth_status,
        supports_output_format_json: help_text.contains("--output-format") && help_text.contains("json"),
        supports_output_format_stream_json: help_text.contains("--output-format") && help_text.contains("stream-json"),
        supports_resume: help_text.contains("--resume"),
        tmux_available, tmux_version, execution_mode: execution_mode.to_owned(),
        cwd_available, observed_at: now_millis(),
    }
}

pub fn list_capabilities(
    data_dir: &Path,
    connection_id: Option<&str>,
) -> Result<Vec<ClaudeCapabilityReport>, &'static str> {
    let grants = load_grants(data_dir)?;
    Ok(grants
        .iter()
        .filter(|g| connection_id.map(|cid| g.connection_id == cid).unwrap_or(true))
        .map(|g| probe_capabilities(data_dir, g))
        .collect())
}

pub fn load_history(data_dir: &Path, grant_id: &str) -> Result<Vec<Value>, &'static str> {
    let path = history_path(data_dir, grant_id);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    read_json_file(&path)
}

fn append_history_turn(
    data_dir: &Path,
    grant_id: &str,
    request_id: &str,
    user_text: &str,
    assistant_text: &str,
    native_session_id: &str,
    route_kind: &str,
    artifact_version: Option<&ArtifactVersionRecord>,
    started_at: i64,
    completed_at: i64,
) -> Result<Vec<Value>, &'static str> {
    let mut items = load_history(data_dir, grant_id)?;
    let already_recorded = items.iter().any(|item| {
        item.get("requestId").and_then(Value::as_str) == Some(request_id)
            && item.get("role").and_then(Value::as_str) == Some("assistant")
    });
    if !already_recorded {
        let user_seq = (items.len() as i64) + 1;
        let asst_seq = user_seq + 1;
        items.push(json!({
            "sequence": user_seq,
            "id": format!("claude-user-{request_id}"),
            "requestId": request_id,
            "role": "user",
            "text": user_text,
            "createdAt": started_at,
            "harness": "claude",
            "toolLabel": "Claude",
        }));
        items.push(json!({
            "sequence": asst_seq,
            "id": format!("claude-asst-{request_id}"),
            "requestId": request_id,
            "role": "assistant",
            "text": assistant_text,
            "createdAt": completed_at,
            "harness": "claude",
            "toolLabel": "Claude",
            "nativeSessionId": native_session_id,
            "routeKind": route_kind,
            "artifactVersion": artifact_version,
        }));
        write_json_atomic(&history_path(data_dir, grant_id), &items)?;
    }
    Ok(items)
}

pub fn load_artifact_versions(
    data_dir: &Path,
    grant_id: &str,
) -> Result<Vec<ArtifactVersionRecord>, &'static str> {
    let path = artifacts_index_path(data_dir, grant_id);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    read_json_file(&path)
}

fn save_artifact_versions(
    data_dir: &Path,
    grant_id: &str,
    versions: &[ArtifactVersionRecord],
) -> Result<(), &'static str> {
    write_json_atomic(&artifacts_index_path(data_dir, grant_id), &versions)
}

fn collect_deliverable_files(
    cwd: &Path,
    configured_paths: &[String],
) -> Result<Vec<String>, &'static str> {
    if !configured_paths.is_empty() {
        let mut out = Vec::new();
        for rel in configured_paths {
            let candidate = cwd.join(rel);
            if candidate.is_file() {
                out.push(rel.replace('\\', "/"));
            }
        }
        out.sort();
        out.dedup();
        return Ok(out);
    }

    let mut discovered = Vec::new();
    let entries = fs::read_dir(cwd).map_err(|_| "CLAUDE_WORKSPACE_UNREADABLE")?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if !name.starts_with('.') {
                    discovered.push(name.to_owned());
                }
            }
        }
    }
    discovered.sort();
    Ok(discovered)
}

fn verify_deliverable_snapshot(
    snapshot_files_dir: &Path,
    deliverable_kind: &str,
    files: &[DeliverableFileDigest],
    version_id: &str,
) -> VerificationReport {
    let resolved_kind = if deliverable_kind == "auto" {
        if files.iter().any(|f| f.path.ends_with(".html")) {
            "web"
        } else {
            "markdown"
        }
    } else {
        deliverable_kind
    };

    let mut checks = Vec::new();
    if resolved_kind == "web" {
        let html_file = files
            .iter()
            .find(|f| f.path == "index.html" || f.path.ends_with(".html"));
        let mut html_ok = false;
        let mut html_detail = "missing HTML entry file".to_owned();
        let mut referenced_assets: Vec<String> = Vec::new();
        if let Some(hf) = html_file {
            if let Ok(content) = fs::read_to_string(snapshot_files_dir.join(&hf.path)) {
                let lower = content.to_lowercase();
                if (lower.contains("<!doctype html") || lower.contains("<html"))
                    && lower.contains("</body>")
                {
                    html_ok = true;
                    html_detail = format!("{} contains valid HTML document structure", hf.path);
                } else {
                    html_detail = format!("{} is missing HTML root/body tags", hf.path);
                }
                for token in content.split('"') {
                    let t = token.trim();
                    if (t.ends_with(".js") || t.ends_with(".css") || t.ends_with(".json"))
                        && !t.starts_with("http://")
                        && !t.starts_with("https://")
                        && !t.starts_with("//")
                    {
                        let clean = t.trim_start_matches("./").trim_start_matches('/');
                        referenced_assets.push(clean.to_owned());
                    }
                }
            }
        }
        checks.push(VerificationCheckItem {
            id: "entry_html_exists".to_owned(),
            passed: html_ok,
            detail: html_detail,
        });

        let mut assets_ok = true;
        let mut missing_assets = Vec::new();
        for asset in &referenced_assets {
            let p = snapshot_files_dir.join(asset);
            if !p.is_file() || fs::metadata(&p).map(|m| m.len() == 0).unwrap_or(true) {
                assets_ok = false;
                missing_assets.push(asset.clone());
            }
        }
        checks.push(VerificationCheckItem {
            id: "asset_integrity".to_owned(),
            passed: assets_ok,
            detail: if assets_ok {
                format!("all {} referenced assets present and non-empty", referenced_assets.len())
            } else {
                format!("missing or empty referenced assets: {}", missing_assets.join(", "))
            },
        });

        let mut json_ok = true;
        let mut json_checked = 0usize;
        for f in files.iter().filter(|f| f.path.ends_with(".json")) {
            json_checked += 1;
            if let Ok(raw) = fs::read_to_string(snapshot_files_dir.join(&f.path)) {
                if serde_json::from_str::<Value>(&raw).is_err() {
                    json_ok = false;
                }
            } else {
                json_ok = false;
            }
        }
        checks.push(VerificationCheckItem {
            id: "package_data_consistency".to_owned(),
            passed: json_ok,
            detail: format!("verified {json_checked} JSON package/data files"),
        });
    } else {
        let md_files: Vec<&DeliverableFileDigest> = files
            .iter()
            .filter(|f| f.path.ends_with(".md") || f.path.ends_with(".txt"))
            .collect();
        let non_empty = !md_files.is_empty() && md_files.iter().all(|f| f.size_bytes > 0);
        checks.push(VerificationCheckItem {
            id: "utf8_non_empty".to_owned(),
            passed: non_empty,
            detail: if non_empty {
                format!("{} markdown/document file(s) non-empty", md_files.len())
            } else {
                "no non-empty markdown/document files found".to_owned()
            },
        });

        let mut has_heading = false;
        for mf in &md_files {
            if let Ok(text) = fs::read_to_string(snapshot_files_dir.join(&mf.path)) {
                if text.lines().any(|l| l.trim_start().starts_with("# ")) {
                    has_heading = true;
                }
            }
        }
        checks.push(VerificationCheckItem {
            id: "heading_structure".to_owned(),
            passed: has_heading,
            detail: if has_heading {
                "document contains top-level Markdown heading".to_owned()
            } else {
                "document is missing a top-level '# ' Markdown heading".to_owned()
            },
        });
    }

    let all_passed = !checks.is_empty() && checks.iter().all(|c| c.passed);
    let now = now_millis();
    VerificationReport {
        id: format!("ver-{version_id}-{now}"),
        version_id: version_id.to_owned(),
        deliverable_kind: resolved_kind.to_owned(),
        status: if all_passed {
            "passed".to_owned()
        } else {
            "failed".to_owned()
        },
        summary: if all_passed {
            format!("{resolved_kind} deliverable {version_id} passed all {} checks", checks.len())
        } else {
            format!("{resolved_kind} deliverable {version_id} failed verification checks")
        },
        checks,
        verified_at: now,
    }
}

fn snapshot_and_verify_deliverable(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
    request_id: &str,
    instruction: &str,
    requested_parent_version_id: Option<&str>,
) -> Result<ArtifactVersionRecord, &'static str> {
    let mut existing = load_artifact_versions(data_dir, &grant.id)?;
    let parent_version_id = if let Some(req_parent) = requested_parent_version_id {
        if !existing.iter().any(|v| v.version_id == req_parent) {
            return Err("CLAUDE_PARENT_VERSION_NOT_FOUND");
        }
        Some(req_parent.to_owned())
    } else {
        existing.last().map(|v| v.version_id.clone())
    };

    let version_number = (existing.len() as u64) + 1;
    let version_id = format!("v{version_number}");
    let version_root = data_dir
        .join(HARNESS_DIR)
        .join("artifacts")
        .join(&grant.id)
        .join(&version_id);
    let snapshot_files_dir = version_root.join("files");
    fs::create_dir_all(&snapshot_files_dir).map_err(|_| "CLAUDE_SNAPSHOT_WRITE_FAILED")?;

    let cwd_path = Path::new(&grant.cwd);
    let rel_paths = collect_deliverable_files(cwd_path, &grant.deliverable_paths)?;
    let mut digests = Vec::new();
    let mut bundle_hasher = Sha256::new();

    for rel in rel_paths {
        let src = cwd_path.join(&rel);
        if !src.is_file() {
            continue;
        }
        let bytes = fs::read(&src).map_err(|_| "CLAUDE_SNAPSHOT_READ_FAILED")?;
        let file_sha = sha256_hex(&bytes);
        let dest = snapshot_files_dir.join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|_| "CLAUDE_SNAPSHOT_WRITE_FAILED")?;
        }
        fs::write(&dest, &bytes).map_err(|_| "CLAUDE_SNAPSHOT_WRITE_FAILED")?;
        bundle_hasher.update(rel.as_bytes());
        bundle_hasher.update(b":");
        bundle_hasher.update(file_sha.as_bytes());
        bundle_hasher.update(b"\n");
        digests.push(DeliverableFileDigest {
            path: rel,
            sha256: file_sha,
            size_bytes: bytes.len() as u64,
        });
    }

    let bundle_sha256: String = bundle_hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let verification = verify_deliverable_snapshot(
        &snapshot_files_dir,
        &grant.deliverable_kind,
        &digests,
        &version_id,
    );
    let lifecycle_state = if verification.status == "passed" {
        "awaiting_review".to_owned()
    } else {
        "generated".to_owned()
    };
    let now = now_millis();
    let record = ArtifactVersionRecord {
        version_id: version_id.clone(),
        version_number,
        parent_version_id,
        grant_id: grant.id.clone(),
        session_id: grant.session_id.clone(),
        request_id: request_id.to_owned(),
        instruction: instruction.to_owned(),
        deliverable_kind: verification.deliverable_kind.clone(),
        snapshot_dir: snapshot_files_dir.to_string_lossy().into_owned(),
        bundle_sha256,
        files: digests,
        lifecycle_state,
        verification,
        review_status: "pending_review".to_owned(),
        review_note: None,
        created_at: now,
        updated_at: now,
    };

    existing.push(record.clone());
    save_artifact_versions(data_dir, &grant.id, &existing)?;
    Ok(record)
}

pub fn review_artifact_version(
    data_dir: &Path,
    grant_id: &str,
    version_id: &str,
    verdict: &str,
    note: Option<&str>,
) -> Result<ArtifactVersionRecord, &'static str> {
    let normalized = match verdict {
        "accepted" | "approved" => "accepted",
        "changes_requested" | "rejected" => "changes_requested",
        _ => return Err("CLAUDE_REVIEW_VERDICT_INVALID"),
    };
    let mut versions = load_artifact_versions(data_dir, grant_id)?;
    let target = versions
        .iter_mut()
        .find(|v| v.version_id == version_id)
        .ok_or("CLAUDE_VERSION_NOT_FOUND")?;
    if normalized == "accepted" && target.verification.status != "passed" {
        return Err("CLAUDE_CANNOT_ACCEPT_UNVERIFIED");
    }
    target.review_status = normalized.to_owned();
    target.lifecycle_state = normalized.to_owned();
    target.review_note = note.map(str::to_owned);
    target.updated_at = now_millis();
    let updated = target.clone();
    save_artifact_versions(data_dir, grant_id, &versions)?;
    Ok(updated)
}

/// Read a file from either the live workspace or a pinned `ArtifactVersion` snapshot.
pub fn preview_deliverable_file(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
    rel_path: &str,
    version_id: Option<&str>,
) -> Result<Value, &'static str> {
    let clean = rel_path.trim().trim_start_matches("./").trim_start_matches('/');
    if clean.is_empty() || clean.contains("..") {
        return Err("CLAUDE_PATH_INVALID");
    }
    let base_dir = if let Some(vid) = version_id {
        let versions = load_artifact_versions(data_dir, &grant.id)?;
        let ver = versions
            .into_iter()
            .find(|v| v.version_id == vid)
            .ok_or("CLAUDE_VERSION_NOT_FOUND")?;
        PathBuf::from(ver.snapshot_dir)
    } else {
        PathBuf::from(&grant.cwd)
    };
    let full = base_dir.join(clean);
    let bytes = fs::read(&full).map_err(|_| "CLAUDE_FILE_NOT_FOUND")?;
    let sha = sha256_hex(&bytes);
    let content = String::from_utf8_lossy(&bytes).into_owned();
    let ext = Path::new(clean)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt")
        .to_lowercase();
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(json!({
        "path": clean,
        "extension": ext,
        "data": b64,
        "bytes": bytes.len(),
        "version": version_id.unwrap_or("workspace"),
        "versionId": version_id,
        "sha256": sha,
        "sizeBytes": bytes.len(),
        "source": if version_id.is_some() { "artifact_snapshot" } else { "current_workspace" },
        "connection_id": grant.connection_id,
        "session_id": grant.session_id,
        "content": content,
    }))
}

#[derive(Debug, Clone)]
pub struct ClaudeDispatchOptions {
    pub parent_version_id: Option<String>,
    pub detached_hold_ms: Option<u64>,
    pub capsule_wire_override: Option<String>,
}

/// Dispatch a prompt to the native Claude Code CLI via `TmuxHarnessRunner` (or supervised process),
/// enforcing serialized workspace locks, native `--resume <session_id>`, and V1->V2 deliverable snapshots.
pub fn dispatch_prompt(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
    request_id: &str,
    user_text: &str,
    opts: ClaudeDispatchOptions,
) -> Result<ClaudeExecutionReceipt, &'static str> {
    let trimmed_text = user_text.trim();
    if trimmed_text.is_empty() || trimmed_text.len() > MAX_PROMPT_BYTES {
        return Err("CLAUDE_TEXT_REQUIRED");
    }
    let trimmed_req = request_id.trim();
    if trimmed_req.is_empty() || trimmed_req.len() > 200 {
        return Err("CLAUDE_REQUEST_REQUIRED");
    }
    if !Path::new(&grant.cwd).is_dir() {
        return Err("CLAUDE_WORKSPACE_UNAVAILABLE");
    }

    let req_hash = request_key_hash(&grant.connection_id, &grant.session_id, trimmed_req);
    let text_hash = sha256_hex(trimmed_text.as_bytes());
    let dir = spool_dir(data_dir, &grant.id, &req_hash);
    let started_path = dir.join("started.json");
    let receipt_path = dir.join("receipt.json");

    if receipt_path.is_file() {
        let existing: ClaudeExecutionReceipt = read_json_file(&receipt_path)?;
        if existing.text_hash != text_hash {
            return Err("CLAUDE_REQUEST_CONFLICT");
        }
        return Ok(existing);
    }
    if started_path.is_file() {
        let started: StartedExecutionRecord = read_json_file(&started_path)?;
        if started.text_hash != text_hash {
            return Err("CLAUDE_REQUEST_CONFLICT");
        }
        return inspect_or_finalize_execution(data_dir, grant, &req_hash);
    }

    if grant.remote_ssh_target.is_some() {
        return Err("CLAUDE_REMOTE_SUPERVISOR_REQUIRED");
    }
    let mut session_state = load_session_state(data_dir, grant)?;
    if let Some(max_rev) = grant.max_revisions {
        let existing_versions = load_artifact_versions(data_dir, &grant.id)?;
        if existing_versions.len() as u64 >= max_rev {
            return Err("CLAUDE_MAX_REVISIONS_EXCEEDED");
        }
    }
    if let Some(ref req_parent) = opts.parent_version_id {
        let versions = load_artifact_versions(data_dir, &grant.id)?;
        if !versions.iter().any(|v| &v.version_id == req_parent) {
            return Err("CLAUDE_PARENT_VERSION_NOT_FOUND");
        }
    }

    let _lease = acquire_workspace_lock(
        data_dir,
        &grant.cwd,
        &grant.connection_id,
        &grant.session_id,
        "claude",
        trimmed_req,
        &req_hash,
        grant.timeout_ms,
    )?;

    let (route_kind, wire_prompt, expected_resume) = match (
        session_state.native_session_id.as_deref(),
        session_state.last_harness.as_str(),
        opts.capsule_wire_override.as_deref(),
    ) {
        (Some(sid), "claude", _) => (
            "native_resume".to_owned(),
            trimmed_text.to_owned(),
            Some(sid.to_owned()),
        ),
        (Some(sid), _, Some(capsule_wire)) => (
            "task_card_handoff".to_owned(),
            capsule_wire.to_owned(),
            Some(sid.to_owned()),
        ),
        (None, _, Some(capsule_wire)) => (
            "task_card_initial".to_owned(),
            capsule_wire.to_owned(),
            None,
        ),
        (Some(sid), _, None) => (
            "task_card_handoff".to_owned(),
            trimmed_text.to_owned(),
            Some(sid.to_owned()),
        ),
        (None, _, None) => ("initial_prompt".to_owned(), trimmed_text.to_owned(), None),
    };

    let cap = probe_capabilities(data_dir, grant);
    let execution_mode = cap.execution_mode;
    let tmux_session_name = if execution_mode == "tmux" {
        Some(format!("bob-claude-{}", &req_hash[..12]))
    } else {
        None
    };

    fs::create_dir_all(&dir).map_err(|_| {
        release_workspace_lock(data_dir, &grant.cwd, &req_hash);
        "CLAUDE_SPOOL_CREATE_FAILED"
    })?;

    let started_at = now_millis();
    let mut started = StartedExecutionRecord {
        grant_id: grant.id.clone(),
        connection_id: grant.connection_id.clone(),
        session_id: grant.session_id.clone(),
        request_id: trimmed_req.to_owned(),
        request_hash: req_hash.clone(),
        text_hash,
        prompt_text: trimmed_text.to_owned(),
        wire_prompt: wire_prompt.clone(),
        route_kind: route_kind.clone(),
        expected_native_session_id: expected_resume.clone(),
        parent_version_id: opts.parent_version_id.clone(),
        execution_mode: execution_mode.clone(),
        tmux_session_name: tmux_session_name.clone(),
        pid: None,
        started_at,
        timeout_ms: grant.timeout_ms,
        status: "running".to_owned(),
    };
    write_json_atomic(&started_path, &started).inspect_err(|_| {
        release_workspace_lock(data_dir, &grant.cwd, &req_hash);
    })?;

    session_state.active_request_id = Some(trimmed_req.to_owned());
    session_state.updated_at = started_at;
    let _ = save_session_state(data_dir, &session_state);

    let stdout_path = dir.join("stdout.json");
    let stderr_path = dir.join("stderr.log");
    let exit_code_path = dir.join("exit_code.txt");

    let mut args_vec: Vec<String> = grant.claude_args_prefix.clone();
    args_vec.push("-p".to_owned());
    args_vec.push(wire_prompt.clone());
    args_vec.push("--output-format".to_owned());
    args_vec.push(grant.output_format.clone());
    if let Some(ref pmode) = grant.permission_mode {
        args_vec.push("--permission-mode".to_owned());
        args_vec.push(pmode.clone());
    }
    if let Some(budget) = grant.max_budget_usd {
        args_vec.push("--max-budget-usd".to_owned());
        args_vec.push(format!("{budget:.4}"));
    }
    if let Some(ref sid) = expected_resume {
        args_vec.push("--resume".to_owned());
        args_vec.push(sid.clone());
    }

    if execution_mode == "tmux" {
        let tmux_bin = grant.tmux_bin.as_deref().unwrap_or("tmux");
        let tname = tmux_session_name.as_deref().unwrap_or("bob-claude");
        let wrapper_sh = dir.join("run_in_tmux.sh");
        let mut quoted_cmd = shell_quote(&grant.claude_bin);
        for a in &args_vec {
            quoted_cmd.push(' ');
            quoted_cmd.push_str(&shell_quote(a));
        }
        let script = format!(
            "#!/bin/sh\ncd {} || exit 1\n{} > {} 2> {}\nprintf '%s' \"$?\" > {}\n",
            shell_quote(&grant.cwd),
            quoted_cmd,
            shell_quote(&stdout_path.to_string_lossy()),
            shell_quote(&stderr_path.to_string_lossy()),
            shell_quote(&exit_code_path.to_string_lossy())
        );
        fs::write(&wrapper_sh, script).map_err(|_| {
            release_workspace_lock(data_dir, &grant.cwd, &req_hash);
            "CLAUDE_TMUX_SCRIPT_FAILED"
        })?;
        let status = Command::new(tmux_bin)
            .args([
                "new-session",
                "-d",
                "-s",
                tname,
                "sh",
                &wrapper_sh.to_string_lossy(),
            ])
            .current_dir(&grant.cwd)
            .status()
            .map_err(|_| {
                release_workspace_lock(data_dir, &grant.cwd, &req_hash);
                "CLAUDE_TMUX_SPAWN_FAILED"
            })?;
        if !status.success() {
            release_workspace_lock(data_dir, &grant.cwd, &req_hash);
            return Err("CLAUDE_TMUX_SPAWN_FAILED");
        }
        let _ = update_workspace_lock_handles(
            data_dir,
            &grant.cwd,
            &req_hash,
            None,
            tmux_session_name.clone(),
        );
    } else {
        let stdout_file = fs::File::create(&stdout_path).map_err(|_| {
            release_workspace_lock(data_dir, &grant.cwd, &req_hash);
            "CLAUDE_SPOOL_CREATE_FAILED"
        })?;
        let stderr_file = fs::File::create(&stderr_path).map_err(|_| {
            release_workspace_lock(data_dir, &grant.cwd, &req_hash);
            "CLAUDE_SPOOL_CREATE_FAILED"
        })?;

        let mut cmd = if let Some(ref ssh_host) = grant.remote_ssh_target {
            let remote_dir = grant.remote_cwd.as_deref().unwrap_or("/tmp/bob-claude-ws");
            let mut remote_cmd = format!(
                "mkdir -p {} && cd {} && {} -p --output-format {}",
                shell_quote(remote_dir),
                shell_quote(remote_dir),
                shell_quote(&grant.claude_bin),
                shell_quote(&grant.output_format),
            );
            if let Some(ref pmode) = grant.permission_mode {
                remote_cmd.push_str(&format!(" --permission-mode {}", shell_quote(pmode)));
            }
            if let Some(budget) = grant.max_budget_usd {
                remote_cmd.push_str(&format!(" --max-budget-usd {budget:.4}"));
            }
            if let Some(ref sid) = expected_resume {
                remote_cmd.push_str(&format!(" --resume {}", shell_quote(sid)));
            }
            use base64::Engine;
            let prompt_b64 = base64::engine::general_purpose::STANDARD.encode(wire_prompt.as_bytes());
            remote_cmd.push_str(&format!(
                " \"$(printf '%s' '{}' | base64 -d)\" < /dev/null",
                prompt_b64
            ));
            let mut c = Command::new("ssh");
            c.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", ssh_host, &remote_cmd]);
            c
        } else {
            let mut c = Command::new(&grant.claude_bin);
            c.args(&args_vec);
            c
        };
        cmd.current_dir(&grant.cwd)
            .env("BOB_CLAUDE_REQUEST_ID", trimmed_req)
            .env("BOB_CLAUDE_SESSION_ID", &grant.session_id)
            .env("BOB_CLAUDE_ROUTE_KIND", &route_kind)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));
        if let Some(ref pvid) = opts.parent_version_id {
            cmd.env("BOB_CLAUDE_PARENT_VERSION_ID", pvid);
        }
        if let Some(hold_ms) = opts.detached_hold_ms {
            cmd.env("BOB_CLAUDE_HOLD_MS", hold_ms.to_string());
        }

        let mut child = cmd.spawn().map_err(|_| {
            release_workspace_lock(data_dir, &grant.cwd, &req_hash);
            "CLAUDE_SPAWN_FAILED"
        })?;
        let pid = child.id();
        started.pid = Some(pid);
        write_json_atomic(&started_path, &started)?;
        let _ = update_workspace_lock_handles(data_dir, &grant.cwd, &req_hash, Some(pid), None);

        if let Some(hold_ms) = opts.detached_hold_ms {
            let exit_path_clone = exit_code_path.clone();
            thread::spawn(move || {
                if hold_ms > 0 {
                    thread::sleep(Duration::from_millis(hold_ms));
                }
                let code = child
                    .wait()
                    .ok()
                    .and_then(|s| s.code())
                    .unwrap_or(1);
                let _ = fs::write(&exit_path_clone, code.to_string());
            });
            return Ok(ClaudeExecutionReceipt {
                grant_id: grant.id.clone(),
                connection_id: grant.connection_id.clone(),
                session_id: grant.session_id.clone(),
                request_id: trimmed_req.to_owned(),
                request_hash: req_hash,
                text_hash: started.text_hash,
                status: "running".to_owned(),
                route_kind,
                execution_mode,
                tmux_session_name,
                native_session_id: session_state.native_session_id,
                reply_text: None,
                error_code: None,
                recovered: false,
                artifact_version: None,
                started_at,
                completed_at: None,
            });
        }

        let start_inst = Instant::now();
        let timeout_dur = Duration::from_millis(grant.timeout_ms);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code().unwrap_or(1);
                    let _ = fs::write(&exit_code_path, code.to_string());
                    break;
                }
                Ok(None) => {
                    if start_inst.elapsed() >= timeout_dur {
                        let _ = kill_execution_process("tmux", None, Some(pid));
                        return finalize_non_success_receipt(
                            data_dir,
                            grant,
                            &started,
                            "timed_out",
                            "CLAUDE_EXECUTION_TIMED_OUT",
                            false,
                        );
                    }
                    thread::sleep(Duration::from_millis(15));
                }
                Err(_) => {
                    let _ = fs::write(&exit_code_path, "1");
                    break;
                }
            }
        }
    }

    if execution_mode == "tmux" {
        let start_inst = Instant::now();
        let timeout_dur = Duration::from_millis(grant.timeout_ms);
        while !exit_code_path.is_file() {
            if opts.detached_hold_ms.is_some() {
                return Ok(ClaudeExecutionReceipt {
                    grant_id: grant.id.clone(),
                    connection_id: grant.connection_id.clone(),
                    session_id: grant.session_id.clone(),
                    request_id: trimmed_req.to_owned(),
                    request_hash: req_hash,
                    text_hash: started.text_hash,
                    status: "running".to_owned(),
                    route_kind,
                    execution_mode,
                    tmux_session_name,
                    native_session_id: session_state.native_session_id,
                    reply_text: None,
                    error_code: None,
                    recovered: false,
                    artifact_version: None,
                    started_at,
                    completed_at: None,
                });
            }
            if start_inst.elapsed() >= timeout_dur {
                let tmux_bin = grant.tmux_bin.as_deref().unwrap_or("tmux");
                let _ = kill_execution_process(
                    tmux_bin,
                    started.tmux_session_name.as_deref(),
                    started.pid,
                );
                return finalize_non_success_receipt(
                    data_dir,
                    grant,
                    &started,
                    "timed_out",
                    "CLAUDE_EXECUTION_TIMED_OUT",
                    false,
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    inspect_or_finalize_execution(data_dir, grant, &req_hash)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn finalize_non_success_receipt(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
    started: &StartedExecutionRecord,
    status: &str,
    error_code: &str,
    recovered: bool,
) -> Result<ClaudeExecutionReceipt, &'static str> {
    let dir = spool_dir(data_dir, &grant.id, &started.request_hash);
    let receipt_path = dir.join("receipt.json");
    let completed_at = now_millis();
    let receipt = ClaudeExecutionReceipt {
        grant_id: grant.id.clone(),
        connection_id: grant.connection_id.clone(),
        session_id: grant.session_id.clone(),
        request_id: started.request_id.clone(),
        request_hash: started.request_hash.clone(),
        text_hash: started.text_hash.clone(),
        status: status.to_owned(),
        route_kind: started.route_kind.clone(),
        execution_mode: started.execution_mode.clone(),
        tmux_session_name: started.tmux_session_name.clone(),
        native_session_id: started.expected_native_session_id.clone(),
        reply_text: None,
        error_code: Some(error_code.to_owned()),
        recovered,
        artifact_version: None,
        started_at: started.started_at,
        completed_at: Some(completed_at),
    };
    write_json_atomic(&receipt_path, &receipt)?;
    let mut state = load_session_state(data_dir, grant)?;
    if state.active_request_id.as_deref() == Some(&started.request_id) {
        state.active_request_id = None;
        state.updated_at = completed_at;
        let _ = save_session_state(data_dir, &state);
    }
    release_workspace_lock(data_dir, &grant.cwd, &started.request_hash);
    Ok(receipt)
}

pub fn inspect_or_finalize_execution(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
    req_hash: &str,
) -> Result<ClaudeExecutionReceipt, &'static str> {
    let dir = spool_dir(data_dir, &grant.id, req_hash);
    let receipt_path = dir.join("receipt.json");
    if receipt_path.is_file() {
        return read_json_file(&receipt_path);
    }
    let started_path = dir.join("started.json");
    if !started_path.is_file() {
        return Err("CLAUDE_EXECUTION_NOT_FOUND");
    }
    let started: StartedExecutionRecord = read_json_file(&started_path)?;
    let exit_code_path = dir.join("exit_code.txt");
    let stdout_path = dir.join("stdout.json");

    if grant.remote_ssh_target.is_some() && !receipt_path.is_file() {
        return Err("CLAUDE_REMOTE_RECONCILIATION_REQUIRED");
    }
    if !exit_code_path.is_file() {
        let now = now_millis();
        if now.saturating_sub(started.started_at) as u64 > started.timeout_ms {
            let tmux_bin = grant.tmux_bin.as_deref().unwrap_or("tmux");
            let _ = kill_execution_process(
                tmux_bin,
                started.tmux_session_name.as_deref(),
                started.pid,
            );
            return finalize_non_success_receipt(
                data_dir,
                grant,
                &started,
                "timed_out",
                "CLAUDE_EXECUTION_TIMED_OUT",
                false,
            );
        }
        return Ok(ClaudeExecutionReceipt {
            grant_id: grant.id.clone(),
            connection_id: grant.connection_id.clone(),
            session_id: grant.session_id.clone(),
            request_id: started.request_id,
            request_hash: started.request_hash,
            text_hash: started.text_hash,
            status: "running".to_owned(),
            route_kind: started.route_kind,
            execution_mode: started.execution_mode,
            tmux_session_name: started.tmux_session_name,
            native_session_id: started.expected_native_session_id,
            reply_text: None,
            error_code: None,
            recovered: false,
            artifact_version: None,
            started_at: started.started_at,
            completed_at: None,
        });
    }

    let exit_code = fs::read_to_string(&exit_code_path)
        .ok()
        .and_then(|s| s.trim().parse::<i32>().ok())
        .unwrap_or(1);
    if exit_code != 0 {
        let _ = finalize_non_success_receipt(
            data_dir,
            grant,
            &started,
            "failed",
            "CLAUDE_PROCESS_FAILED",
            false,
        );
        return Err("CLAUDE_PROCESS_FAILED");
    }

    let raw_stdout = fs::read_to_string(&stdout_path).map_err(|_| {
        release_workspace_lock(data_dir, &grant.cwd, &started.request_hash);
        "CLAUDE_OUTPUT_INVALID"
    })?;
    let parsed = match parse_claude_output(
        &raw_stdout,
        started.expected_native_session_id.as_deref(),
    ) {
        Ok(p) => p,
        Err(code) => {
            let _ = finalize_non_success_receipt(data_dir, grant, &started, "failed", code, false);
            return Err(code);
        }
    };

    if let Some(ref ssh_host) = grant.remote_ssh_target {
        let remote_dir = grant.remote_cwd.as_deref().unwrap_or("/tmp/bob-claude-ws");
        for rel in &grant.deliverable_paths {
            let remote_file = format!("{}/{}", remote_dir.trim_end_matches('/'), rel);
            if let Ok(out) = Command::new("ssh")
                .args([
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "ConnectTimeout=8",
                    ssh_host,
                    &format!("cat {} 2>/dev/null", shell_quote(&remote_file)),
                ])
                .stdin(Stdio::null())
                .output()
            {
                if out.status.success() && !out.stdout.is_empty() {
                    let local_dest = Path::new(&grant.cwd).join(rel);
                    if let Some(p) = local_dest.parent() {
                        let _ = fs::create_dir_all(p);
                    }
                    let _ = fs::write(&local_dest, &out.stdout);
                }
            }
        }
    }

    let artifact_version = snapshot_and_verify_deliverable(
        data_dir,
        grant,
        &started.request_id,
        &started.prompt_text,
        started.parent_version_id.as_deref(),
    )?;

    let completed_at = now_millis();
    let _ = append_history_turn(
        data_dir,
        &grant.id,
        &started.request_id,
        &started.prompt_text,
        &parsed.result_text,
        &parsed.session_id,
        &started.route_kind,
        Some(&artifact_version),
        started.started_at,
        completed_at,
    )?;

    let mut session_state = load_session_state(data_dir, grant)?;
    session_state.native_session_id = Some(parsed.session_id.clone());
    session_state.last_harness = "claude".to_owned();
    session_state.turn_count += 1;
    session_state.active_request_id = None;
    session_state.latest_version_id = Some(artifact_version.version_id.clone());
    session_state.updated_at = completed_at;
    save_session_state(data_dir, &session_state)?;
    record_workspace_harness_activity(data_dir, &grant.cwd, "claude", &grant.session_id)?;

    let receipt = ClaudeExecutionReceipt {
        grant_id: grant.id.clone(),
        connection_id: grant.connection_id.clone(),
        session_id: grant.session_id.clone(),
        request_id: started.request_id,
        request_hash: started.request_hash.clone(),
        text_hash: started.text_hash,
        status: "completed".to_owned(),
        route_kind: started.route_kind,
        execution_mode: started.execution_mode,
        tmux_session_name: started.tmux_session_name,
        native_session_id: Some(parsed.session_id),
        reply_text: Some(parsed.result_text),
        error_code: None,
        recovered: false,
        artifact_version: Some(artifact_version),
        started_at: started.started_at,
        completed_at: Some(completed_at),
    };
    write_json_atomic(&receipt_path, &receipt)?;
    release_workspace_lock(data_dir, &grant.cwd, &started.request_hash);
    Ok(receipt)
}

/// Cancel an in-flight execution, confirming process / tmux termination before releasing the workspace lock.
pub fn cancel_execution(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
    request_id: &str,
) -> Result<ClaudeExecutionReceipt, &'static str> {
    let req_hash = request_key_hash(&grant.connection_id, &grant.session_id, request_id.trim());
    let dir = spool_dir(data_dir, &grant.id, &req_hash);
    let receipt_path = dir.join("receipt.json");
    if receipt_path.is_file() {
        return read_json_file(&receipt_path);
    }
    let started_path = dir.join("started.json");
    if !started_path.is_file() {
        return Err("CLAUDE_EXECUTION_NOT_FOUND");
    }
    let started: StartedExecutionRecord = read_json_file(&started_path)?;
    if grant.remote_ssh_target.is_some() {
        // Retain receipt and lease: killing a local SSH client is not remote cleanup.
        return Err("CLAUDE_CANCEL_CLEANUP_UNCONFIRMED");
    }
    let tmux_bin = grant.tmux_bin.as_deref().unwrap_or("tmux");
    let cleaned = kill_execution_process(
        tmux_bin,
        started.tmux_session_name.as_deref(),
        started.pid,
    );
    if !cleaned {
        return Err("CLAUDE_CANCEL_CLEANUP_UNCONFIRMED");
    }
    finalize_non_success_receipt(
        data_dir,
        grant,
        &started,
        "cancelled",
        "CLAUDE_CANCELLED_BY_USER",
        false,
    )
}

/// Reconcile in-flight executions after a disconnect or node restart without replaying the model.
pub fn reconcile_grant(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
) -> Result<Vec<ClaudeExecutionReceipt>, &'static str> {
    let grant_spool = data_dir.join(HARNESS_DIR).join("spool").join(&grant.id);
    if !grant_spool.is_dir() {
        return Ok(Vec::new());
    }
    let mut results = Vec::new();
    let entries = fs::read_dir(&grant_spool).map_err(|_| "CLAUDE_STORAGE_READ_FAILED")?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let receipt_path = path.join("receipt.json");
        if receipt_path.is_file() {
            if let Ok(r) = read_json_file::<ClaudeExecutionReceipt>(&receipt_path) {
                results.push(r);
            }
            continue;
        }
        let started_path = path.join("started.json");
        if !started_path.is_file() {
            continue;
        }
        let started: StartedExecutionRecord = read_json_file(&started_path)?;
        if grant.remote_ssh_target.is_some() {
            return Err("CLAUDE_REMOTE_RECONCILIATION_REQUIRED");
        }

        let exit_code_path = path.join("exit_code.txt");
        if exit_code_path.is_file() {
            if let Ok(mut finalized) =
                inspect_or_finalize_execution(data_dir, grant, &started.request_hash)
            {
                finalized.recovered = true;
                write_json_atomic(&receipt_path, &finalized)?;
                results.push(finalized);
                continue;
            }
        }

        let lease_check = WorkspaceLease {
            workspace_hash: workspace_hash(&grant.cwd),
            cwd: grant.cwd.clone(),
            connection_id: grant.connection_id.clone(),
            session_id: grant.session_id.clone(),
            harness: "claude".to_owned(),
            request_id: started.request_id.clone(),
            request_hash: started.request_hash.clone(),
            pid: started.pid,
            tmux_session_name: started.tmux_session_name.clone(),
            started_at: started.started_at,
            timeout_ms: started.timeout_ms,
        };
        let elapsed = now_millis().saturating_sub(started.started_at) as u64;
        if elapsed > started.timeout_ms {
            let tmux_bin = grant.tmux_bin.as_deref().unwrap_or("tmux");
            let _ = kill_execution_process(
                tmux_bin,
                started.tmux_session_name.as_deref(),
                started.pid,
            );
            let r = finalize_non_success_receipt(
                data_dir,
                grant,
                &started,
                "timed_out",
                "CLAUDE_EXECUTION_TIMED_OUT",
                true,
            )?;
            results.push(r);
        } else if (started.pid.is_some() || started.tmux_session_name.is_some())
            && is_execution_alive(&lease_check)
        {
            results.push(ClaudeExecutionReceipt {
                grant_id: grant.id.clone(),
                connection_id: grant.connection_id.clone(),
                session_id: grant.session_id.clone(),
                request_id: started.request_id,
                request_hash: started.request_hash,
                text_hash: started.text_hash,
                status: "running".to_owned(),
                route_kind: started.route_kind,
                execution_mode: started.execution_mode,
                tmux_session_name: started.tmux_session_name,
                native_session_id: started.expected_native_session_id,
                reply_text: None,
                error_code: None,
                recovered: true,
                artifact_version: None,
                started_at: started.started_at,
                completed_at: None,
            });
        } else {
            let r = finalize_non_success_receipt(
                data_dir,
                grant,
                &started,
                "unknown",
                "CLAUDE_RESTART_INTERRUPTED",
                true,
            )?;
            results.push(r);
        }
    }
    Ok(results)
}

/// Build unified session list entries for `ConversationDirectory.vue` / `DshSessions.vue`
/// so Claude sessions appear in the same sidebar with device (`X1T`), project (`cwd`), and tool badge (`Claude`).
pub fn list_unified_sessions(
    data_dir: &Path,
    connection_id: Option<&str>,
) -> Result<Vec<Value>, &'static str> {
    let grants = load_grants(data_dir)?;
    let mut out = Vec::new();
    for g in grants {
        if let Some(cid) = connection_id {
            if g.connection_id != cid {
                continue;
            }
        }
        let state = load_session_state(data_dir, &g)?;
        let versions = load_artifact_versions(data_dir, &g.id).unwrap_or_default();
        let latest_ver = versions.last();
        let running = state.active_request_id.is_some();
        out.push(json!({
            "id": g.session_id,
            "grantId": g.id,
            "connectionId": g.connection_id,
            "title": g.title,
            "workspace": g.cwd,
            "cwd": g.cwd,
            "source_kind": "claude_native",
            "harness": "claude",
            "tool_label": "Claude",
            "toolLabel": "Claude",
            "nativeSessionId": state.native_session_id,
            "lastHarness": state.last_harness,
            "turnCount": state.turn_count,
            "running": running,
            "status": if running { "running" } else { "ready" },
            "activeRequestId": state.active_request_id,
            "latestVersion": latest_ver,
            "updated_at": state.updated_at,
            "updatedAt": state.updated_at,
        }));
    }
    Ok(out)
}

/// Format history page in the exact shape validated by `DshSessions.vue` (`address`, `cursor`,
/// `before`, `messages`, `workspace`, `request_results`) plus Claude governance metadata.
pub fn history_page_for_ui(
    data_dir: &Path,
    grant: &ClaudeHarnessGrant,
) -> Result<Value, &'static str> {
    let items = load_history(data_dir, &grant.id)?;
    let state = load_session_state(data_dir, grant)?;
    let versions = load_artifact_versions(data_dir, &grant.id)?;
    let mut files_map = BTreeMap::new();
    if let Some(latest) = versions.last() {
        for f in &latest.files {
            files_map.insert(
                f.path.clone(),
                json!({
                    "path": f.path,
                    "sha256": f.sha256,
                    "sizeBytes": f.size_bytes,
                    "versionId": latest.version_id,
                }),
            );
        }
    }
    let files: Vec<Value> = files_map.into_values().collect();
    let cursor = items
        .last()
        .and_then(|m| m.get("sequence"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let before = items
        .first()
        .and_then(|m| m.get("sequence"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let mut request_results = Vec::new();
    let mut turn_idx = 0i64;
    for item in &items {
        if item.get("role").and_then(Value::as_str) == Some("assistant") {
            turn_idx += 1;
            if let Some(req_id) = item.get("requestId").and_then(Value::as_str) {
                let seq = item.get("sequence").and_then(Value::as_i64).unwrap_or(turn_idx * 2);
                let text = item.get("text").and_then(Value::as_str).unwrap_or("");
                request_results.push(json!({
                    "request_id": req_id,
                    "turn": turn_idx,
                    "status": "completed",
                    "text": text,
                    "sequences": [seq],
                }));
            }
        }
    }
    Ok(json!({
        "address": {
            "connection_id": grant.connection_id,
            "session_id": grant.session_id,
        },
        "sessionId": grant.session_id,
        "grantId": grant.id,
        "title": grant.title,
        "workspace": grant.cwd,
        "cwd": grant.cwd,
        "harness": "claude",
        "toolLabel": "Claude",
        "nativeSessionId": state.native_session_id,
        "lastHarness": state.last_harness,
        "status": if state.active_request_id.is_some() { "running" } else { "ready" },
        "cursor": cursor,
        "before": before,
        "partial": false,
        "omitted": 0,
        "has_more": false,
        "hasOlder": false,
        "total": items.len(),
        "start": 0,
        "through": cursor,
        "messages": items,
        "items": items,
        "request_results": request_results,
        "replacements": [],
        "files": files,
        "artifactVersions": versions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_fake_claude_script(dir: &Path) -> PathBuf {
        let script = dir.join("fake_claude.py");
        let code = r##"import json, os, sys, time
from pathlib import Path

hold_ms = int(os.environ.get("BOB_CLAUDE_HOLD_MS", "0") or "0")
if hold_ms > 0:
    time.sleep(hold_ms / 1000.0)

args = sys.argv[1:]
if "--help" in args:
    print("--output-format json stream-json --resume")
    sys.exit(0)
if args[:2] == ["auth", "status"]:
    print(json.dumps({"loggedIn": True}))
    sys.exit(0)
if "--version" in args:
    print("2.1.0 (Claude Code fake fixture)")
    sys.exit(0)

prompt = ""
out_fmt = "json"
resume_id = None
i = 0
while i < len(args):
    if args[i] == "-p" and i + 1 < len(args):
        prompt = args[i + 1]
        i += 2
    elif args[i] == "--output-format" and i + 1 < len(args):
        out_fmt = args[i + 1]
        i += 2
    elif args[i] == "--resume" and i + 1 < len(args):
        resume_id = args[i + 1]
        i += 2
    else:
        i += 1

cwd = Path.cwd()
log_file = cwd / ".claude_invocations.jsonl"
existing = []
if log_file.exists():
    existing = [line for line in log_file.read_text(encoding="utf-8").splitlines() if line.strip()]
turn_num = len(existing) + 1
entry = {
    "turn": turn_num,
    "prompt": prompt,
    "output_format": out_fmt,
    "resume": resume_id,
    "route_kind": os.environ.get("BOB_CLAUDE_ROUTE_KIND", ""),
    "parent_version_id": os.environ.get("BOB_CLAUDE_PARENT_VERSION_ID", ""),
}
with open(log_file, "a", encoding="utf-8") as f:
    f.write(json.dumps(entry, ensure_ascii=False) + "\n")

if "SIMULATE_MISMATCH_SESSION" in prompt:
    sid = "wrong-session-id-999"
else:
    sid = resume_id or "claude-sess-fixture-001"

if "MARKDOWN_TASK" in prompt:
    md_path = cwd / "ARCHITECTURE_NOTES.md"
    if turn_num == 1:
        md_path.write_text("# Architecture Notes V1\n\nInitial design baseline.\n", encoding="utf-8")
        reply = "Generated ARCHITECTURE_NOTES.md V1."
    else:
        md_path.write_text("# Architecture Notes V2\n\nRevised with multi-node tmux supervision.\n", encoding="utf-8")
        reply = f"Updated ARCHITECTURE_NOTES.md to V2 (resumed={resume_id})."
else:
    html_path = cwd / "index.html"
    js_path = cwd / "app.js"
    pkg_path = cwd / "package.json"
    if turn_num == 1:
        html_path.write_text('<!DOCTYPE html><html><head><title>App V1</title></head><body><h1>V1</h1><script src="app.js"></script></body></html>', encoding="utf-8")
        js_path.write_text('console.log("app v1 ready");', encoding="utf-8")
        pkg_path.write_text(json.dumps({"name": "web-deliverable", "version": "1.0.0"}), encoding="utf-8")
        reply = "Built interactive web deliverable V1."
    else:
        html_path.write_text('<!DOCTYPE html><html><head><title>App V2</title></head><body><h1>V2 Revised</h1><script src="app.js"></script></body></html>', encoding="utf-8")
        js_path.write_text('console.log("app v2 interactive controls ready");', encoding="utf-8")
        pkg_path.write_text(json.dumps({"name": "web-deliverable", "version": "2.0.0"}), encoding="utf-8")
        reply = f"Revised web deliverable to V2 (resumed={resume_id})."

if out_fmt == "stream-json":
    print(json.dumps({"type": "system", "subtype": "init", "session_id": sid}))
    print(json.dumps({"type": "assistant", "session_id": sid, "message": {"content": [{"type": "text", "text": reply}]}}))
    print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": reply, "session_id": sid}))
else:
    print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": reply, "session_id": sid}))
"##;
        fs::write(&script, code).unwrap();
        script
    }

    #[test]
    fn parse_json_and_stream_json_and_enforce_session_identity() {
        let single = r#"{"type":"result","subtype":"success","is_error":false,"result":"Done V1","session_id":"sess-1"}"#;
        let p1 = parse_claude_output(single, None).unwrap();
        assert_eq!(p1.session_id, "sess-1");
        assert_eq!(p1.result_text, "Done V1");

        let stream = concat!(
            "{\"type\":\"system\",\"session_id\":\"sess-2\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Streamed reply\"}]}}\n",
            "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"Streamed reply\",\"session_id\":\"sess-2\"}\n"
        );
        let p2 = parse_claude_output(stream, Some("sess-2")).unwrap();
        assert_eq!(p2.session_id, "sess-2");
        assert_eq!(p2.result_text, "Streamed reply");

        assert_eq!(
            parse_claude_output(single, Some("expected-other")).unwrap_err(),
            "CLAUDE_SESSION_MISMATCH"
        );
        let err_json = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"boom","session_id":"sess-1"}"#;
        assert_eq!(
            parse_claude_output(err_json, None).unwrap_err(),
            "CLAUDE_REPORTED_ERROR"
        );
    }

    #[test]
    fn web_deliverable_v1_to_parent_pinned_v2_and_native_resume_preserve_v1_snapshot() {
        let tmp = tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let ws_dir = tmp.path().join("ws-web");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&ws_dir).unwrap();
        let script = write_fake_claude_script(tmp.path());

        let grant = ClaudeHarnessGrant {
            id: "grant-web-1".to_owned(),
            connection_id: "node-x1t".to_owned(),
            session_id: "claude:web-demo".to_owned(),
            title: "Web Deliverable Session".to_owned(),
            cwd: ws_dir.to_string_lossy().into_owned(),
            claude_bin: "python".to_owned(),
            claude_args_prefix: vec![script.to_string_lossy().into_owned()],
            output_format: "json".to_owned(),
            prefer_tmux: false,
            tmux_bin: None,
            timeout_ms: 30_000,
            deliverable_kind: "web".to_owned(),
            deliverable_paths: vec![
                "index.html".to_owned(),
                "app.js".to_owned(),
                "package.json".to_owned(),
            ],
            auth_configured: Some(true),
            permission_mode: None,
            max_budget_usd: None,
            max_revisions: None,
            remote_ssh_target: None,
            remote_cwd: None,
        };
        fs::write(
            data_dir.join(GRANTS_FILE),
            serde_json::to_string_pretty(&vec![&grant]).unwrap(),
        )
        .unwrap();

        let cap = probe_capabilities(&data_dir, &grant);
        assert!(cap.available);
        assert_eq!(cap.auth_status, "authenticated");
        assert!(cap.supports_resume);

        // Round 1 -> V1
        let r1 = dispatch_prompt(
            &data_dir,
            &grant,
            "req-web-v1",
            "Build interactive web app V1",
            ClaudeDispatchOptions {
                parent_version_id: None,
                detached_hold_ms: None,
                capsule_wire_override: None,
            },
        )
        .unwrap();
        assert_eq!(r1.status, "completed");
        assert_eq!(r1.route_kind, "initial_prompt");
        assert_eq!(
            r1.native_session_id.as_deref(),
            Some("claude-sess-fixture-001")
        );
        let v1 = r1.artifact_version.unwrap();
        assert_eq!(v1.version_id, "v1");
        assert_eq!(v1.parent_version_id, None);
        assert_eq!(v1.verification.status, "passed");
        assert_eq!(v1.lifecycle_state, "awaiting_review");
        assert_eq!(v1.review_status, "pending_review");
        let v1_html_preview =
            preview_deliverable_file(&data_dir, &grant, "index.html", Some("v1")).unwrap();
        let v1_html_sha = v1_html_preview["sha256"].as_str().unwrap().to_owned();

        // Round 2 -> parent-pinned V2 via native --resume
        let r2 = dispatch_prompt(
            &data_dir,
            &grant,
            "req-web-v2",
            "Revise controls for V2",
            ClaudeDispatchOptions {
                parent_version_id: Some("v1".to_owned()),
                detached_hold_ms: None,
                capsule_wire_override: None,
            },
        )
        .unwrap();
        assert_eq!(r2.status, "completed");
        assert_eq!(r2.route_kind, "native_resume");
        assert!(r2
            .reply_text
            .as_deref()
            .unwrap_or("")
            .contains("resumed=claude-sess-fixture-001"));
        let v2 = r2.artifact_version.unwrap();
        assert_eq!(v2.version_id, "v2");
        assert_eq!(v2.parent_version_id.as_deref(), Some("v1"));
        assert_eq!(v2.verification.status, "passed");
        assert_ne!(v2.bundle_sha256, v1.bundle_sha256);

        // Verify V1 snapshot remained 100% immutable after V2 modified the workspace
        let v1_after_v2 =
            preview_deliverable_file(&data_dir, &grant, "index.html", Some("v1")).unwrap();
        assert_eq!(v1_after_v2["sha256"].as_str().unwrap(), v1_html_sha);
        assert!(v1_after_v2["content"].as_str().unwrap().contains("<h1>V1</h1>"));

        // Review V2 -> accepted
        let reviewed =
            review_artifact_version(&data_dir, &grant.id, "v2", "accepted", Some("LGTM")).unwrap();
        assert_eq!(reviewed.lifecycle_state, "accepted");
        assert_eq!(reviewed.review_status, "accepted");
    }

    #[test]
    fn workspace_lock_cancel_and_restart_reconciliation_never_replay_model() {
        let tmp = tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let ws_dir = tmp.path().join("ws-lock");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&ws_dir).unwrap();
        let script = write_fake_claude_script(tmp.path());

        let grant = ClaudeHarnessGrant {
            id: "grant-lock-1".to_owned(),
            connection_id: "node-x1t".to_owned(),
            session_id: "claude:lock-demo".to_owned(),
            title: "Lock & Reconcile Session".to_owned(),
            cwd: ws_dir.to_string_lossy().into_owned(),
            claude_bin: "python".to_owned(),
            claude_args_prefix: vec![script.to_string_lossy().into_owned()],
            output_format: "stream-json".to_owned(),
            prefer_tmux: false,
            tmux_bin: None,
            timeout_ms: 30_000,
            deliverable_kind: "markdown".to_owned(),
            deliverable_paths: vec!["ARCHITECTURE_NOTES.md".to_owned()],
            auth_configured: Some(true),
            permission_mode: None,
            max_budget_usd: None,
            max_revisions: None,
            remote_ssh_target: None,
            remote_cwd: None,
        };

        // Start a detached background execution holding the workspace lock
        let running = dispatch_prompt(
            &data_dir,
            &grant,
            "req-hold-1",
            "MARKDOWN_TASK initial",
            ClaudeDispatchOptions {
                parent_version_id: None,
                detached_hold_ms: Some(1500),
                capsule_wire_override: None,
            },
        )
        .unwrap();
        assert_eq!(running.status, "running");

        // Concurrent request with a different request_id on the same workspace is blocked
        let busy_err = dispatch_prompt(
            &data_dir,
            &grant,
            "req-concurrent-2",
            "MARKDOWN_TASK concurrent",
            ClaudeDispatchOptions {
                parent_version_id: None,
                detached_hold_ms: None,
                capsule_wire_override: None,
            },
        )
        .unwrap_err();
        assert_eq!(busy_err, "HARNESS_WORKSPACE_BUSY");

        // Conflicting payload with same request_id is rejected
        let conflict_err = dispatch_prompt(
            &data_dir,
            &grant,
            "req-hold-1",
            "MARKDOWN_TASK tampered payload",
            ClaudeDispatchOptions {
                parent_version_id: None,
                detached_hold_ms: None,
                capsule_wire_override: None,
            },
        )
        .unwrap_err();
        assert_eq!(conflict_err, "CLAUDE_REQUEST_CONFLICT");

        // Cancel the running execution and confirm lock is released
        let cancelled = cancel_execution(&data_dir, &grant, "req-hold-1").unwrap();
        assert_eq!(cancelled.status, "cancelled");

        // Simulate a node restart with an interrupted in-flight record (dead PID, no exit_code.txt)
        let interrupted_hash = request_key_hash(&grant.connection_id, &grant.session_id, "req-crash-1");
        let crash_dir = spool_dir(&data_dir, &grant.id, &interrupted_hash);
        fs::create_dir_all(&crash_dir).unwrap();
        let crash_started = StartedExecutionRecord {
            grant_id: grant.id.clone(),
            connection_id: grant.connection_id.clone(),
            session_id: grant.session_id.clone(),
            request_id: "req-crash-1".to_owned(),
            request_hash: interrupted_hash,
            text_hash: sha256_hex(b"MARKDOWN_TASK crash"),
            prompt_text: "MARKDOWN_TASK crash".to_owned(),
            wire_prompt: "MARKDOWN_TASK crash".to_owned(),
            route_kind: "initial_prompt".to_owned(),
            expected_native_session_id: None,
            parent_version_id: None,
            execution_mode: "process".to_owned(),
            tmux_session_name: None,
            pid: Some(999_999_999),
            started_at: now_millis() - 5000,
            timeout_ms: 30_000,
            status: "running".to_owned(),
        };
        write_json_atomic(&crash_dir.join("started.json"), &crash_started).unwrap();

        let reconciled = reconcile_grant(&data_dir, &grant).unwrap();
        let crash_rec = reconciled
            .iter()
            .find(|r| r.request_id == "req-crash-1")
            .unwrap();
        assert_eq!(crash_rec.status, "unknown");
        assert_eq!(
            crash_rec.error_code.as_deref(),
            Some("CLAUDE_RESTART_INTERRUPTED")
        );
        assert!(crash_rec.recovered);
    }
}

#[cfg(test)]
mod host_probe_regressions {
    use super::*;
    #[test]
    fn remote_host_commands_and_dispatch_never_use_local_tmux() {
        let dir = tempfile::tempdir().unwrap();
        let grant: ClaudeHarnessGrant = serde_json::from_value(json!({
            "id":"probe","connection_id":"node","session_id":"session","title":"Probe",
            "cwd":dir.path().to_string_lossy(),"remote_ssh_target":"configured-node",
            "remote_cwd":"/workspace/with space","claude_bin":"/bin/claude"
        })).unwrap();
        let command=host_command(&grant,"test", &["-d","/workspace/with space"]);
        assert_eq!(command.get_program(), "ssh");
        let args=command.get_args().map(|s|s.to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert!(args.last().unwrap().contains("'/workspace/with space'"));
        assert_eq!(dispatch_prompt(dir.path(),&grant,"r","hello",ClaudeDispatchOptions {
            parent_version_id:None,detached_hold_ms:None,capsule_wire_override:None
        }).unwrap_err(), "CLAUDE_REMOTE_SUPERVISOR_REQUIRED");
        assert!(!dir.path().join(HARNESS_DIR).exists());
    }
    #[test]
    fn authentication_requires_observed_boolean_not_cli_presence() {
        assert_eq!(observed_auth(None), "unverified");
        assert_eq!(observed_auth(Some(b"Claude 2.1")), "unverified");
        assert_eq!(observed_auth(Some(br#"{"loggedIn":"true"}"#)), "unverified");
        assert_eq!(observed_auth(Some(br#"{"loggedIn":false}"#)), "missing");
        assert_eq!(observed_auth(Some(br#"{"loggedIn":true}"#)), "authenticated");
    }
}
