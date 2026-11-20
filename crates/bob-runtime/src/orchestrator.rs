//! Single-Window Main Agent Orchestration Core (`BOB-D-018`).
//!
//! Provides:
//! 1. Deterministic workspace bootstrap (`bootstrap_workspace_local`):
//!    creates or clones a project directory under a safe root and registers it in
//!    `WorkspaceGrantStore` BEFORE launching any coding agent (solving the cwd bootstrap paradox).
//! 2. Durable orchestration run ledger (`OrchestrationStore` -> `orchestration_runs.json`):
//!    tracks cross-node dispatches, stages, Definition of Done (DoD), autonomy modes (L1/L2/L3),
//!    round history, and failure signatures.
//! 3. Progress-gated stopping & convergence engine (`evaluate_round_outcome` +
//!    `extract_failure_signature`): enforces hard-stop on DoD, stage gates (`plan`/`release`),
//!    L1 human-in-the-loop review, L2 bounded auto-retry (default max 3 rounds), and
//!    2-round consecutive identical-error no-progress circuit breaking.

use crate::host_tools::WorkspaceGrantStore;
use crate::tool_bus::{ActionRiskLevel, CardField, CardItem, StructuredCard};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub const DEFAULT_L2_MAX_AUTO_ROUNDS: u32 = 3;
pub const HARD_MAX_AUTO_ROUNDS: u32 = 6;

// ---------------------------------------------------------------------------
// 0. Child Deliverables & Auto GPU / Compute Node Discovery
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrchestrationArtifact {
    pub label: String,
    pub kind: String, // "image" | "apk" | "link" | "file"
    pub path_or_url: String,
    pub connection_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuDeviceInfo {
    pub name: String,
    pub vram_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComputeNodeEntry {
    pub name: String,
    pub role: String,
    pub gpu_summary: String,
    pub access_hint: String,
    pub source: String, // "local_probe" | "bob_node" | "node_inventory_ssot"
}

fn classify_artifact_kind(target: &str) -> Option<&'static str> {
    let lower = target.to_ascii_lowercase();
    let clean = lower.split(['?', '#']).next().unwrap_or(&lower);
    if clean.ends_with(".png")
        || clean.ends_with(".jpg")
        || clean.ends_with(".jpeg")
        || clean.ends_with(".webp")
        || clean.ends_with(".gif")
    {
        return Some("image");
    }
    if clean.ends_with(".apk") {
        return Some("apk");
    }
    if clean.starts_with("https://") || clean.starts_with("http://") {
        return Some("link");
    }
    if clean.ends_with(".pdf") || clean.ends_with(".html") || clean.ends_with(".zip") {
        return Some("file");
    }
    None
}

fn strip_path_punctuation(raw: &str) -> &str {
    raw.trim_matches(|c: char| {
        matches!(
            c,
            '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';' | '。' | '，' | '）' | '（'
        )
    })
}

/// Deterministically extracts deliverable URLs, screenshots/renders, APKs, and reports
/// from child agent output so the single main-agent window can preview them directly.
pub fn extract_child_artifacts(
    output: &str,
    connection_id: &str,
    session_id: &str,
) -> Vec<OrchestrationArtifact> {
    let mut out: Vec<OrchestrationArtifact> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut push_item = |label: String, target: &str| {
        if out.len() >= 8 {
            return;
        }
        let cleaned = strip_path_punctuation(target).trim();
        if cleaned.is_empty() || cleaned.len() > 512 || cleaned.contains("..") {
            return;
        }
        let target_norm = cleaned.strip_prefix("file://").unwrap_or(cleaned);
        let Some(kind) = classify_artifact_kind(target_norm) else {
            return;
        };
        if !seen.insert(target_norm.to_string()) {
            return;
        }
        let final_label = if !label.trim().is_empty() {
            label.trim().to_string()
        } else {
            target_norm
                .rsplit(['/', '\\'])
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(target_norm)
                .to_string()
        };
        out.push(OrchestrationArtifact {
            label: final_label.chars().take(80).collect(),
            kind: kind.to_string(),
            path_or_url: target_norm.to_string(),
            connection_id: connection_id.to_string(),
            session_id: session_id.to_string(),
        });
    };

    // 1. Markdown links/images: [label](target) or ![label](target)
    let bytes = output.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close_br) = output[i + 1..].find(']') {
                let close_br_idx = i + 1 + close_br;
                if output.as_bytes().get(close_br_idx + 1) == Some(&b'(') {
                    if let Some(close_pa) = output[close_br_idx + 2..].find(')') {
                        let label = &output[i + 1..close_br_idx];
                        let target = &output[close_br_idx + 2..close_br_idx + 2 + close_pa];
                        if !label.contains('\n') && !target.contains('\n') {
                            push_item(label.to_string(), target);
                        }
                        i = close_br_idx + 2 + close_pa + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }

    // 2. Standalone URLs and absolute/workspace deliverable paths
    for token in output.split_whitespace() {
        let clean = strip_path_punctuation(token);
        if clean.starts_with("https://")
            || clean.starts_with("http://")
            || clean.starts_with('/')
            || clean.contains("dist-release")
        {
            push_item(String::new(), clean);
        }
    }

    out
}

/// Probes local NVIDIA GPUs via `nvidia-smi` once per process (zero overhead when absent).
pub fn probe_local_gpus() -> &'static [GpuDeviceInfo] {
    static LOCAL_GPUS: OnceLock<Vec<GpuDeviceInfo>> = OnceLock::new();
    LOCAL_GPUS.get_or_init(|| {
        let Ok(out) = Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .output()
        else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.split(',');
                let name = parts.next()?.trim().to_string();
                let vram_mb = parts.next()?.trim().parse::<u64>().ok()?;
                if name.is_empty() {
                    return None;
                }
                Some(GpuDeviceInfo { name, vram_mb })
            })
            .collect()
    })
}

/// Pure read-only parser extracting GPU/render compute nodes from the synced
/// `cluster-ops/references/node_inventory.md` quick-reference Markdown table.
/// Never reads SSH keys, tokens, or secrets.
pub fn parse_gpu_nodes_from_inventory_md(markdown: &str) -> Vec<ComputeNodeEntry> {
    let mut entries = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || trimmed.starts_with("|---") || trimmed.starts_with("| ---") {
            continue;
        }
        let cols: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cols.len() < 5 {
            continue;
        }
        let name = cols[0].replace("**", "").trim().to_string();
        if name.is_empty() || name.eq_ignore_ascii_case("node") {
            continue;
        }
        let ts_ip = cols[2].replace('`', "").trim().to_string();
        let provider = cols[3].replace('`', "").trim().to_string();
        let role = cols[4].replace('`', "").trim().to_string();
        let combined = format!("{name} {provider} {role}").to_ascii_lowercase();
        let is_gpu = combined.contains("gpu")
            || combined.contains("optix")
            || combined.contains("blender")
            || combined.contains("cuda")
            || combined.contains("gtx")
            || combined.contains("rtx")
            || combined.contains("t4")
            || combined.contains("a100")
            || combined.contains("l4")
            || role.contains("渲染")
            || role.contains("算力");
        if !is_gpu {
            continue;
        }
        entries.push(ComputeNodeEntry {
            name,
            role: role.clone(),
            gpu_summary: format!("{provider} · {role}"),
            access_hint: if !ts_ip.is_empty() && ts_ip != "—" && ts_ip != "-" {
                format!("Tailscale {ts_ip}")
            } else {
                provider
            },
            source: "node_inventory_ssot".to_string(),
        });
    }
    entries
}

pub fn discover_node_inventory_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("BOB_NODE_INVENTORY_PATH") {
        let p = PathBuf::from(explicit.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let h = PathBuf::from(home.trim());
    let candidates = [
        h.join("common/knowledge/skills/cluster-ops/references/node_inventory.md"),
        h.join("work/common/knowledge/skills/cluster-ops/references/node_inventory.md"),
    ];
    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

/// Discovers available GPU / render compute nodes from local hardware probes,
/// paired node metadata, and the synced `node_inventory.md` SSOT table.
pub fn discover_compute_nodes(data_dir: &Path) -> Vec<ComputeNodeEntry> {
    let mut nodes: Vec<ComputeNodeEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // 1. Local GPU probe
    let local_gpus = probe_local_gpus();
    if !local_gpus.is_empty() {
        let summary = local_gpus
            .iter()
            .map(|g| format!("{} ({} MiB)", g.name, g.vram_mb))
            .collect::<Vec<_>>()
            .join(", ");
        seen.insert("local".to_string());
        nodes.push(ComputeNodeEntry {
            name: "local".to_string(),
            role: "Local GPU Host".to_string(),
            gpu_summary: summary,
            access_hint: "local".to_string(),
            source: "local_probe".to_string(),
        });
    }

    // 2. Paired remote nodes advertising GPUs in model_hub_state.json
    if let Ok(raw) = std::fs::read(data_dir.join("model_hub_state.json")) {
        if let Ok(hub) = serde_json::from_slice::<Value>(&raw) {
            if let Some(arr) = hub["remoteNodes"].as_array() {
                for n in arr {
                    if let Some(gpus) = n.get("gpus").and_then(Value::as_array) {
                        if !gpus.is_empty() {
                            let label = n["label"]
                                .as_str()
                                .or_else(|| n["name"].as_str())
                                .or_else(|| n["id"].as_str())
                                .unwrap_or("remote-gpu")
                                .to_string();
                            let summary = gpus
                                .iter()
                                .filter_map(|g| {
                                    let gn = g["name"].as_str()?;
                                    let mb = g["vram_mb"].as_u64().unwrap_or(0);
                                    Some(format!("{gn} ({mb} MiB)"))
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            if !summary.is_empty() && seen.insert(label.to_ascii_lowercase()) {
                                nodes.push(ComputeNodeEntry {
                                    name: label,
                                    role: "Paired Bob GPU Node".to_string(),
                                    gpu_summary: summary,
                                    access_hint: n["id"].as_str().unwrap_or("").to_string(),
                                    source: "bob_node".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Synced cluster-ops node_inventory.md SSOT projection (for on-demand / dormant GPUs)
    if let Some(inv_path) = discover_node_inventory_path() {
        if let Ok(md) = std::fs::read_to_string(&inv_path) {
            for entry in parse_gpu_nodes_from_inventory_md(&md) {
                if seen.insert(entry.name.to_ascii_lowercase()) {
                    nodes.push(entry);
                }
            }
        }
    }

    nodes
}

// ---------------------------------------------------------------------------
// 1. Enums & Durable Run Ledger Models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationStage {
    Plan,
    #[default]
    Execute,
    Verify,
    Release,
}

impl OrchestrationStage {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.unwrap_or("execute").trim().to_ascii_lowercase().as_str() {
            "plan" => Self::Plan,
            "verify" | "test" => Self::Verify,
            "release" | "deploy" => Self::Release,
            _ => Self::Execute,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Execute => "execute",
            Self::Verify => "verify",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyMode {
    #[default]
    L1Review,
    L2BoundedLoop,
    L3Goal,
}

impl AutonomyMode {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.unwrap_or("l1").trim().to_ascii_lowercase().as_str() {
            "l2" | "l2_bounded_loop" | "bounded" | "loop" => Self::L2BoundedLoop,
            "l3" | "l3_goal" | "goal" => Self::L3Goal,
            _ => Self::L1Review,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::L1Review => "L1",
            Self::L2BoundedLoop => "L2",
            Self::L3Goal => "L3",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationStatus {
    Dispatched,
    Running,
    AwaitingReview,
    AutoRetrying,
    Completed,
    HaltedNoProgress,
    HaltedMaxRounds,
    Failed,
    Cancelled,
}

impl OrchestrationStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Dispatched | Self::Running | Self::AutoRetrying)
    }

    pub fn is_reviewable(self) -> bool {
        matches!(
            self,
            Self::AwaitingReview | Self::HaltedNoProgress | Self::HaltedMaxRounds | Self::Failed
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrchestrationRoundRecord {
    pub round: u32,
    pub request_id: String,
    pub prompt: String,
    pub started_at: i64,
    #[serde(default)]
    pub finished_at: Option<i64>,
    pub status: String,
    #[serde(default)]
    pub child_summary: Option<String>,
    #[serde(default)]
    pub failure_signature: Option<String>,
    #[serde(default)]
    pub review_summary: Option<String>,
    #[serde(default)]
    pub dod_satisfied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationRun {
    pub run_id: String,
    pub conversation_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    pub title: String,
    pub goal: String,
    #[serde(default)]
    pub dod: Vec<String>,
    #[serde(default)]
    pub stage: OrchestrationStage,
    #[serde(default)]
    pub autonomy_mode: AutonomyMode,
    pub max_auto_rounds: u32,
    pub current_round: u32,
    pub connection_id: String,
    pub harness: String,
    pub workspace: String,
    pub session_id: String,
    #[serde(default)]
    pub active_request_id: Option<String>,
    pub status: OrchestrationStatus,
    pub last_prompt: String,
    #[serde(default)]
    pub last_child_summary: Option<String>,
    #[serde(default)]
    pub last_failure_signatures: Vec<String>,
    #[serde(default)]
    pub review_summary: Option<String>,
    #[serde(default)]
    pub review_issues: Vec<String>,
    #[serde(default)]
    pub suggested_next_prompt: Option<String>,
    #[serde(default)]
    pub dod_satisfied: bool,
    #[serde(default)]
    pub review_card_id: Option<String>,
    #[serde(default)]
    pub artifacts: Vec<OrchestrationArtifact>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub rounds: Vec<OrchestrationRoundRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OrchestrationStoreData {
    pub runs: Vec<OrchestrationRun>,
}

#[derive(Debug, Clone)]
pub struct OrchestrationStore {
    file_path: PathBuf,
    data: OrchestrationStoreData,
}

impl OrchestrationStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = std::fs::create_dir_all(data_dir);
        let file_path = data_dir.join("orchestration_runs.json");
        let data = if file_path.is_file() {
            let content =
                std::fs::read_to_string(&file_path).map_err(|_| "ORCHESTRATION_READ_FAILED")?;
            serde_json::from_str::<OrchestrationStoreData>(&content).unwrap_or_default()
        } else {
            OrchestrationStoreData::default()
        };
        Ok(Self { file_path, data })
    }

    pub fn save_atomic(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "ORCHESTRATION_SERIALIZE_FAILED")?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized.as_bytes())
            .map_err(|_| "ORCHESTRATION_WRITE_FAILED")?;
        std::fs::rename(&tmp_path, &self.file_path).map_err(|_| "ORCHESTRATION_WRITE_FAILED")?;
        Ok(())
    }

    pub fn upsert_run(&mut self, run: OrchestrationRun) -> Result<OrchestrationRun, &'static str> {
        if let Some(slot) = self.data.runs.iter_mut().find(|r| r.run_id == run.run_id) {
            *slot = run.clone();
        } else {
            self.data.runs.push(run.clone());
            // Keep bounded history (most recent 100 runs)
            if self.data.runs.len() > 100 {
                let excess = self.data.runs.len() - 100;
                self.data.runs.drain(0..excess);
            }
        }
        self.save_atomic()?;
        Ok(run)
    }

    pub fn get_run(&self, run_id: &str) -> Option<OrchestrationRun> {
        self.data.runs.iter().find(|r| r.run_id == run_id).cloned()
    }

    pub fn list_runs(&self, conversation_id: Option<&str>) -> Vec<OrchestrationRun> {
        let mut out: Vec<OrchestrationRun> = self
            .data
            .runs
            .iter()
            .filter(|r| conversation_id.map(|c| r.conversation_id == c).unwrap_or(true))
            .cloned()
            .collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
    }

    pub fn list_active_runs(&self) -> Vec<OrchestrationRun> {
        self.data
            .runs
            .iter()
            .filter(|r| r.status.is_active())
            .cloned()
            .collect()
    }
}

pub fn clamp_max_auto_rounds(mode: AutonomyMode, requested: Option<u64>) -> u32 {
    match mode {
        AutonomyMode::L1Review => 1,
        AutonomyMode::L2BoundedLoop | AutonomyMode::L3Goal => {
            let n = requested.unwrap_or(DEFAULT_L2_MAX_AUTO_ROUNDS as u64) as u32;
            n.clamp(1, HARD_MAX_AUTO_ROUNDS)
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Deterministic Workspace Bootstrap (CWD Bootstrap Paradox Fix)
// ---------------------------------------------------------------------------

pub fn validate_project_slug(slug: &str) -> Result<&str, &'static str> {
    let trimmed = slug.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err("BOOTSTRAP_SLUG_INVALID");
    }
    if trimmed.starts_with('-') || trimmed.starts_with('.') || trimmed.contains("..") {
        return Err("BOOTSTRAP_SLUG_INVALID");
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err("BOOTSTRAP_SLUG_INVALID");
    }
    Ok(trimmed)
}

pub fn validate_git_url(url: &str, allow_local_path: bool) -> Result<&str, &'static str> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.len() > 512 || trimmed.starts_with('-') {
        return Err("BOOTSTRAP_GIT_URL_INVALID");
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("BOOTSTRAP_GIT_URL_INVALID");
    }
    if trimmed.starts_with("https://")
        || trimmed.starts_with("ssh://")
        || trimmed.starts_with("git@")
    {
        return Ok(trimmed);
    }
    if allow_local_path && Path::new(trimmed).exists() {
        return Ok(trimmed);
    }
    Err("BOOTSTRAP_GIT_URL_UNSUPPORTED")
}

fn validate_branch_name(branch: &str) -> Result<&str, &'static str> {
    let trimmed = branch.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || trimmed.starts_with('-')
        || trimmed.contains("..")
        || trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err("BOOTSTRAP_BRANCH_INVALID");
    }
    Ok(trimmed)
}

fn default_workspace_root() -> Result<PathBuf, &'static str> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "BOOTSTRAP_HOME_UNRESOLVED")?;
    let home_path = PathBuf::from(home.trim());
    if !home_path.is_dir() {
        return Err("BOOTSTRAP_HOME_UNRESOLVED");
    }
    let code_dir = home_path.join("code");
    if code_dir.is_dir() {
        return Ok(code_dir);
    }
    let work_dir = home_path.join("work");
    if work_dir.is_dir() {
        return Ok(work_dir);
    }
    Ok(code_dir)
}

fn resolve_safe_parent_dir(
    data_dir: &Path,
    explicit_parent: Option<&str>,
    allow_any_existing_parent_for_tests: bool,
) -> Result<PathBuf, &'static str> {
    let Some(raw_parent) = explicit_parent.map(str::trim).filter(|s| !s.is_empty()) else {
        let root = default_workspace_root()?;
        std::fs::create_dir_all(&root).map_err(|_| "BOOTSTRAP_MKDIR_FAILED")?;
        return std::fs::canonicalize(&root).map_err(|_| "BOOTSTRAP_MKDIR_FAILED");
    };

    let candidate = PathBuf::from(raw_parent);
    if !candidate.is_absolute() {
        return Err("BOOTSTRAP_PARENT_NOT_ABSOLUTE");
    }
    // Reject obvious system directories even if they exist.
    let lower = raw_parent.replace('\\', "/").to_ascii_lowercase();
    for forbidden in ["/", "/etc", "/usr", "/var", "/bin", "/sbin", "/proc", "/sys", "/boot"] {
        if lower == forbidden || lower.starts_with(&format!("{forbidden}/")) && forbidden != "/" {
            return Err("BOOTSTRAP_PARENT_FORBIDDEN");
        }
    }

    if !allow_any_existing_parent_for_tests {
        let home_ok = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .and_then(|h| std::fs::canonicalize(h.trim()).ok())
            .map(|h| {
                let probe = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
                probe.starts_with(&h)
            })
            .unwrap_or(false);
        let grant_ok = WorkspaceGrantStore::open(data_dir)
            .ok()
            .map(|store| store.resolve_authorized_path(raw_parent, true).is_ok())
            .unwrap_or(false);
        if !home_ok && !grant_ok {
            return Err("BOOTSTRAP_PARENT_UNAUTHORIZED");
        }
    }

    std::fs::create_dir_all(&candidate).map_err(|_| "BOOTSTRAP_MKDIR_FAILED")?;
    std::fs::canonicalize(&candidate).map_err(|_| "BOOTSTRAP_MKDIR_FAILED")
}

/// Deterministically prepares a project workspace on the local node:
/// - creates `<parent>/<slug>` or clones `git_url` into it,
/// - optionally initializes git + minimal README if empty,
/// - registers `<parent>/<slug>` as a writable workspace in `WorkspaceGrantStore`.
pub fn bootstrap_workspace_local(data_dir: &Path, args: &Value) -> Result<Value, &'static str> {
    bootstrap_workspace_local_inner(data_dir, args, false)
}

fn bootstrap_workspace_local_inner(
    data_dir: &Path,
    args: &Value,
    test_mode: bool,
) -> Result<Value, &'static str> {
    let slug = validate_project_slug(args["slug"].as_str().ok_or("BOOTSTRAP_SLUG_REQUIRED")?)?;
    let parent_dir = resolve_safe_parent_dir(
        data_dir,
        args.get("parentDir")
            .or_else(|| args.get("parent_dir"))
            .and_then(Value::as_str),
        test_mode,
    )?;
    let target = parent_dir.join(slug);
    let git_url = args
        .get("gitUrl")
        .or_else(|| args.get("git_url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let branch = args
        .get("branch")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let init_readme = args
        .get("initReadme")
        .or_else(|| args.get("init_readme"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let mut created = false;
    let mut cloned = false;
    let mut git_initialized = false;

    if let Some(raw_url) = git_url {
        let safe_url = validate_git_url(raw_url, test_mode)?;
        if target.exists() {
            if !target.is_dir() {
                return Err("BOOTSTRAP_TARGET_NOT_DIR");
            }
            // Existing directory: do not clobber; if empty, clone into it, otherwise keep intact.
            let is_empty = std::fs::read_dir(&target)
                .map(|mut it| it.next().is_none())
                .unwrap_or(false);
            if is_empty {
                let mut cmd = Command::new("git");
                cmd.arg("clone");
                if let Some(br) = branch {
                    let safe_br = validate_branch_name(br)?;
                    cmd.args(["--branch", safe_br]);
                }
                cmd.arg("--").arg(safe_url).arg(&target);
                let out = cmd.output().map_err(|_| "BOOTSTRAP_GIT_EXEC_FAILED")?;
                if !out.status.success() {
                    return Err("BOOTSTRAP_GIT_CLONE_FAILED");
                }
                cloned = true;
            }
        } else {
            let mut cmd = Command::new("git");
            cmd.arg("clone");
            if let Some(br) = branch {
                let safe_br = validate_branch_name(br)?;
                cmd.args(["--branch", safe_br]);
            }
            cmd.arg("--").arg(safe_url).arg(&target);
            let out = cmd.output().map_err(|_| "BOOTSTRAP_GIT_EXEC_FAILED")?;
            if !out.status.success() {
                return Err("BOOTSTRAP_GIT_CLONE_FAILED");
            }
            created = true;
            cloned = true;
        }
    } else {
        if !target.exists() {
            std::fs::create_dir_all(&target).map_err(|_| "BOOTSTRAP_MKDIR_FAILED")?;
            created = true;
        } else if !target.is_dir() {
            return Err("BOOTSTRAP_TARGET_NOT_DIR");
        }
        if !target.join(".git").exists() {
            let out = Command::new("git")
                .arg("init")
                .current_dir(&target)
                .output();
            if let Ok(o) = out {
                if o.status.success() {
                    git_initialized = true;
                }
            }
        }
        if init_readme && !target.join("README.md").exists() {
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(slug);
            let _ = std::fs::write(
                target.join("README.md"),
                format!("# {title}\n\nInitialized deterministically by Bob Orchestrator.\n"),
            );
        }
    }

    let canonical = std::fs::canonicalize(&target).map_err(|_| "BOOTSTRAP_PATH_UNRESOLVED")?;
    let canonical_str = canonical.to_string_lossy().into_owned();

    let mut store = WorkspaceGrantStore::open(data_dir)?;
    let label = args
        .get("label")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(slug);
    let grant = store.authorize_workspace(&canonical_str, label, false, crate::now_millis())?;

    Ok(json!({
        "ok": true,
        "slug": slug,
        "path": grant.path,
        "workspace_id": grant.id,
        "label": grant.label,
        "created": created,
        "cloned": cloned,
        "git_initialized": git_initialized,
        "authorized": true,
    }))
}

// ---------------------------------------------------------------------------
// 3. Failure Signature Extraction & Convergence / Stopping Engine
// ---------------------------------------------------------------------------

/// Extracts a deterministic failure signature from child agent output so consecutive
/// identical failures trip the No-Progress circuit breaker.
pub fn extract_failure_signature(output: &str) -> Option<String> {
    let mut salient: Vec<String> = Vec::new();
    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let is_err = lower.contains("error[e")
            || lower.starts_with("error:")
            || lower.contains("failed")
            || lower.contains("failure:")
            || lower.contains("assertionerror")
            || lower.contains("panicked at")
            || lower.contains("exit code")
            || lower.starts_with("✖ ")
            || lower.starts_with("not ok ");
        if is_err {
            // Strip volatile digits/timestamps while keeping error codes & test identifiers.
            let compact: String = line
                .split_whitespace()
                .take(12)
                .collect::<Vec<_>>()
                .join(" ");
            if !salient.contains(&compact) {
                salient.push(compact);
            }
            if salient.len() >= 6 {
                break;
            }
        }
    }
    if salient.is_empty() {
        return None;
    }
    salient.sort();
    let joined = salient.join("\n");
    let mut hasher = Sha256::new();
    hasher.update(joined.as_bytes());
    let digest = hasher.finalize();
    let hex_prefix: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    Some(format!("sig-{hex_prefix}"))
}

#[derive(Debug, Clone)]
pub struct ChildTurnOutcome {
    pub completed: bool,
    pub failed: bool,
    pub cancelled: bool,
    pub child_summary: String,
    pub dod_satisfied: bool,
    pub needs_human_decision: bool,
    pub review_summary: String,
    pub review_issues: Vec<String>,
    pub failure_signature: Option<String>,
    pub suggested_next_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundTransition {
    Cancelled,
    /// Stop and present Review Card to the user (DoD satisfied, L1 mode, or stage gate).
    PauseForReview {
        status: OrchestrationStatus,
        reason: &'static str,
    },
    /// L2/L3 objective failure under round cap with a fresh error signature -> auto-dispatch next round.
    AutoRetry {
        next_round: u32,
        next_prompt: String,
    },
}

/// Pure deterministic state transition enforcing:
/// - Hard stop on cancellation
/// - Hard stop on DoD satisfied (no unnecessary polish rounds)
/// - Stage gates on `plan` / `release` or subjective human decisions
/// - L1 always pauses for human review card
/// - L2/L3 trips `HaltedNoProgress` on 2 consecutive identical failure signatures
/// - L2/L3 trips `HaltedMaxRounds` when `current_round >= max_auto_rounds`
pub fn evaluate_round_outcome(
    run: &OrchestrationRun,
    outcome: &ChildTurnOutcome,
) -> RoundTransition {
    if outcome.cancelled {
        return RoundTransition::Cancelled;
    }

    if outcome.dod_satisfied {
        return RoundTransition::PauseForReview {
            status: OrchestrationStatus::AwaitingReview,
            reason: "dod_satisfied",
        };
    }

    if matches!(
        run.stage,
        OrchestrationStage::Plan | OrchestrationStage::Release
    ) || outcome.needs_human_decision
    {
        return RoundTransition::PauseForReview {
            status: OrchestrationStatus::AwaitingReview,
            reason: "stage_or_human_gate",
        };
    }

    if run.autonomy_mode == AutonomyMode::L1Review {
        return RoundTransition::PauseForReview {
            status: if outcome.failed {
                OrchestrationStatus::Failed
            } else {
                OrchestrationStatus::AwaitingReview
            },
            reason: "l1_review_mode",
        };
    }

    // L2 / L3 bounded autonomous loop checks
    if let (Some(curr_sig), Some(prev_sig)) = (
        outcome.failure_signature.as_deref(),
        run.last_failure_signatures.last().map(String::as_str),
    ) {
        if !curr_sig.is_empty() && curr_sig == prev_sig {
            return RoundTransition::PauseForReview {
                status: OrchestrationStatus::HaltedNoProgress,
                reason: "consecutive_identical_failure",
            };
        }
    }

    if run.current_round >= run.max_auto_rounds {
        return RoundTransition::PauseForReview {
            status: OrchestrationStatus::HaltedMaxRounds,
            reason: "max_auto_rounds_reached",
        };
    }

    let Some(next_prompt) = outcome
        .suggested_next_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return RoundTransition::PauseForReview {
            status: if outcome.failed {
                OrchestrationStatus::Failed
            } else {
                OrchestrationStatus::AwaitingReview
            },
            reason: "no_next_prompt",
        };
    };

    RoundTransition::AutoRetry {
        next_round: run.current_round + 1,
        next_prompt: next_prompt.to_string(),
    }
}

/// Builds a structured Review Card (`kind: "orchestration_review"`) rendered in the
/// single main-agent chat window with `[Accept]` / `[Dispatch Next Round]` / `[Inspect Session]`.
pub fn build_review_card(run: &OrchestrationRun, now_ms: i64) -> StructuredCard {
    let status_text = match run.status {
        OrchestrationStatus::Completed => "approved",
        OrchestrationStatus::Cancelled => "rejected",
        _ => "pending",
    };
    let dod_badge = if run.dod_satisfied {
        "DoD Satisfied"
    } else {
        match run.status {
            OrchestrationStatus::HaltedNoProgress => "Halted: No Progress",
            OrchestrationStatus::HaltedMaxRounds => "Halted: Round Limit",
            OrchestrationStatus::Failed => "Execution Failed",
            _ => "Awaiting Review",
        }
    };

    let summary = run
        .review_summary
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(run.last_child_summary.as_deref())
        .unwrap_or("Remote node task finished and is ready for review.")
        .to_string();

    let mut items: Vec<CardItem> = run
        .review_issues
        .iter()
        .take(5)
        .map(|issue| CardItem {
            title: issue.clone(),
            subtitle: None,
            badge: Some("check".to_string()),
        })
        .collect();
    if items.is_empty() && !run.artifacts.is_empty() {
        for art in run.artifacts.iter().take(4) {
            items.push(CardItem {
                title: art.label.clone(),
                subtitle: Some(art.path_or_url.clone()),
                badge: Some(art.kind.clone()),
            });
        }
    }

    let meta = json!({
        "run_id": run.run_id,
        "connection_id": run.connection_id,
        "session_id": run.session_id,
        "harness": run.harness,
        "workspace": run.workspace,
        "stage": run.stage.label(),
        "autonomy_mode": run.autonomy_mode.label(),
        "current_round": run.current_round,
        "max_auto_rounds": run.max_auto_rounds,
        "dod_satisfied": run.dod_satisfied,
        "suggested_next_prompt": run.suggested_next_prompt,
        "artifacts": run.artifacts,
        "can_accept": run.status.is_reviewable(),
        "can_continue": run.status.is_reviewable(),
    });

    StructuredCard {
        id: format!("orch-review-{}-r{}-{}", run.run_id, run.current_round, now_ms),
        kind: "orchestration_review".to_string(),
        icon: Some("terminal".to_string()),
        title: format!("{} · {}", run.title, dod_badge),
        summary,
        risk_level: ActionRiskLevel::ReadOnly,
        fields: vec![
            CardField {
                label: "Node".to_string(),
                value: format!("{} ({})", run.connection_id, run.harness),
            },
            CardField {
                label: "Stage / Round".to_string(),
                value: format!(
                    "{} · {}/{} ({})",
                    run.stage.label(),
                    run.current_round,
                    run.max_auto_rounds,
                    run.autonomy_mode.label()
                ),
            },
            CardField {
                label: "Workspace".to_string(),
                value: run.workspace.clone(),
            },
        ],
        items,
        status: Some(status_text.to_string()),
        technical_detail: Some(meta.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_run(mode: AutonomyMode, stage: OrchestrationStage, round: u32) -> OrchestrationRun {
        OrchestrationRun {
            run_id: "orch-test-1".to_string(),
            conversation_id: "conv-main".to_string(),
            project_id: Some("proj-bob".to_string()),
            title: "Fix unit test".to_string(),
            goal: "Make tests green".to_string(),
            dod: vec!["cargo test passes".to_string()],
            stage,
            autonomy_mode: mode,
            max_auto_rounds: clamp_max_auto_rounds(mode, None),
            current_round: round,
            connection_id: "vps3".to_string(),
            harness: "codex".to_string(),
            workspace: "/home/ubuntu/code/bob-agent-next".to_string(),
            session_id: "codex:sess-1".to_string(),
            active_request_id: Some("req-1".to_string()),
            status: OrchestrationStatus::Running,
            last_prompt: "Fix failing test".to_string(),
            last_child_summary: None,
            last_failure_signatures: vec![],
            review_summary: None,
            review_issues: vec![],
            suggested_next_prompt: None,
            dod_satisfied: false,
            review_card_id: None,
            artifacts: vec![],
            created_at: 1000,
            updated_at: 1000,
            rounds: vec![],
        }
    }

    #[test]
    fn bootstrap_workspace_creates_git_repo_and_authorizes_path() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let parent = tmp.path().join("code");
        std::fs::create_dir_all(&parent).unwrap();

        let res = bootstrap_workspace_local_inner(
            &data_dir,
            &json!({
                "slug": "demo-project",
                "parent_dir": parent.to_str().unwrap(),
                "title": "Demo Project",
                "init_readme": true
            }),
            true,
        )
        .unwrap();

        assert_eq!(res["ok"], true);
        assert_eq!(res["created"], true);
        assert_eq!(res["authorized"], true);
        let ws_path = PathBuf::from(res["path"].as_str().unwrap());
        assert!(ws_path.join("README.md").is_file());
        assert!(ws_path.join(".git").exists());

        // Verify WorkspaceGrantStore authorized it as writable
        let store = WorkspaceGrantStore::open(&data_dir).unwrap();
        assert!(store
            .resolve_authorized_path(ws_path.to_str().unwrap(), true)
            .is_ok());

        // Idempotent second call must succeed without overwriting
        let res2 = bootstrap_workspace_local_inner(
            &data_dir,
            &json!({
                "slug": "demo-project",
                "parent_dir": parent.to_str().unwrap()
            }),
            true,
        )
        .unwrap();
        assert_eq!(res2["created"], false);
        assert_eq!(res2["authorized"], true);
    }

    #[test]
    fn bootstrap_rejects_traversal_and_unsafe_git_urls() {
        assert!(validate_project_slug("../escape").is_err());
        assert!(validate_project_slug("-flag").is_err());
        assert!(validate_project_slug("foo/bar").is_err());
        assert!(validate_project_slug("bob-agent_next.1").is_ok());

        assert!(validate_git_url("--upload-pack=evil", false).is_err());
        assert!(validate_git_url("file:///etc/passwd", false).is_err());
        assert!(validate_git_url("https://github.com/org/repo.git", false).is_ok());
        assert!(validate_git_url("git@github.com:org/repo.git", false).is_ok());
    }

    #[test]
    fn l1_always_pauses_and_dod_hard_stops_l2() {
        let l1 = sample_run(AutonomyMode::L1Review, OrchestrationStage::Execute, 1);
        let outcome_fail = ChildTurnOutcome {
            completed: true,
            failed: false,
            cancelled: false,
            child_summary: "Ran tests, 1 failed".into(),
            dod_satisfied: false,
            needs_human_decision: false,
            review_summary: "1 test still failing".into(),
            review_issues: vec!["test_foo failed".into()],
            failure_signature: Some("sig-aaa".into()),
            suggested_next_prompt: Some("Fix test_foo".into()),
        };
        assert_eq!(
            evaluate_round_outcome(&l1, &outcome_fail),
            RoundTransition::PauseForReview {
                status: OrchestrationStatus::AwaitingReview,
                reason: "l1_review_mode"
            }
        );

        let l2 = sample_run(AutonomyMode::L2BoundedLoop, OrchestrationStage::Verify, 1);
        let outcome_dod = ChildTurnOutcome {
            dod_satisfied: true,
            ..outcome_fail.clone()
        };
        assert_eq!(
            evaluate_round_outcome(&l2, &outcome_dod),
            RoundTransition::PauseForReview {
                status: OrchestrationStatus::AwaitingReview,
                reason: "dod_satisfied"
            }
        );
    }

    #[test]
    fn l2_retries_then_trips_no_progress_and_round_cap_and_stage_gates() {
        let mut l2 = sample_run(AutonomyMode::L2BoundedLoop, OrchestrationStage::Execute, 1);
        let outcome = ChildTurnOutcome {
            completed: true,
            failed: false,
            cancelled: false,
            child_summary: "error[E0599]: missing method".into(),
            dod_satisfied: false,
            needs_human_decision: false,
            review_summary: "Compile failed".into(),
            review_issues: vec!["E0599".into()],
            failure_signature: extract_failure_signature("error[E0599]: no method named foo"),
            suggested_next_prompt: Some("Implement method foo".into()),
        };

        // Round 1 -> fresh error signature -> auto-retry round 2
        assert_eq!(
            evaluate_round_outcome(&l2, &outcome),
            RoundTransition::AutoRetry {
                next_round: 2,
                next_prompt: "Implement method foo".into()
            }
        );

        // Record same failure signature -> Round 2 identical error -> HaltedNoProgress!
        l2.current_round = 2;
        l2.last_failure_signatures
            .push(outcome.failure_signature.clone().unwrap());
        assert_eq!(
            evaluate_round_outcome(&l2, &outcome),
            RoundTransition::PauseForReview {
                status: OrchestrationStatus::HaltedNoProgress,
                reason: "consecutive_identical_failure"
            }
        );

        // Fresh signature at round 3 (max_auto_rounds = 3) -> HaltedMaxRounds!
        l2.current_round = 3;
        let fresh = ChildTurnOutcome {
            failure_signature: Some("sig-different".into()),
            ..outcome.clone()
        };
        assert_eq!(
            evaluate_round_outcome(&l2, &fresh),
            RoundTransition::PauseForReview {
                status: OrchestrationStatus::HaltedMaxRounds,
                reason: "max_auto_rounds_reached"
            }
        );

        // Release stage always gates even at round 1
        let release_run = sample_run(AutonomyMode::L2BoundedLoop, OrchestrationStage::Release, 1);
        assert_eq!(
            evaluate_round_outcome(&release_run, &fresh),
            RoundTransition::PauseForReview {
                status: OrchestrationStatus::AwaitingReview,
                reason: "stage_or_human_gate"
            }
        );
    }

    #[test]
    fn orchestration_store_roundtrip_and_review_card() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = OrchestrationStore::open(tmp.path()).unwrap();
        let mut run = sample_run(AutonomyMode::L1Review, OrchestrationStage::Verify, 1);
        run.status = OrchestrationStatus::AwaitingReview;
        run.review_summary = Some("All 270 tests passed.".into());
        run.dod_satisfied = true;
        store.upsert_run(run.clone()).unwrap();

        let reloaded = OrchestrationStore::open(tmp.path()).unwrap();
        let fetched = reloaded.get_run("orch-test-1").unwrap();
        assert_eq!(fetched.status, OrchestrationStatus::AwaitingReview);

        let card = build_review_card(&fetched, 2000);
        assert_eq!(card.kind, "orchestration_review");
        assert!(card.title.contains("DoD Satisfied"));
        let meta: Value = serde_json::from_str(card.technical_detail.as_deref().unwrap()).unwrap();
        assert_eq!(meta["run_id"], "orch-test-1");
        assert_eq!(meta["can_accept"], true);
        assert_eq!(meta["can_continue"], true);
    }

    #[test]
    fn extract_child_artifacts_captures_images_apks_and_links_without_duplicates() {
        let output = "\
已上线 0.5：\n\
- [查看 Godot 实装截图](/home/bobbik/dsh_projects/winter/render_01.png)\n\
- [下载 Android APK](https://github.com/bobbik1984/demo/releases/download/v0.5/app.apk)\n\
- 本地产物 `/home/bobbik/dist-release/Bob-1.0.2.apk` 以及 https://preview.example.com/demo\n\
- 重复链接 /home/bobbik/dsh_projects/winter/render_01.png\n";
        let arts = extract_child_artifacts(output, "vps3", "codex:s1");
        assert_eq!(arts.len(), 4);
        assert_eq!(arts[0].kind, "image");
        assert_eq!(arts[0].label, "查看 Godot 实装截图");
        assert_eq!(arts[1].kind, "apk");
        assert_eq!(arts[2].kind, "apk");
        assert_eq!(arts[3].kind, "link");
    }

    #[test]
    fn parse_gpu_nodes_from_inventory_md_extracts_gpu_rows_safely() {
        let md = "\
| Node | 公网 IP | Tailscale IP | 服务商 | 角色 | 参与 OpenClaw |
|------|---------|-------------|--------|------|:---:|
| **VPS2 (Hostinger)** | `1.2.3.4` | `100.64.0.6` | Hostinger | Exec Center | ✅ |
| **bobbik-pc** | — | `100.64.0.9` | 本地 | GPU 算力 / Blender 渲染 (GTX 1660 Ti) | ❌ |
| **bob-vm-02-gpu** | 动态 | `100.64.0.10` | GCP (`n1-standard-4` Spot + T4) | 云端 Blender 4.2.3 / OptiX 渲染主力 | ❌ |
| **render-03** | — | `100.64.0.15` | 本地 | RTX 4090 渲染节点 | ❌ |
";
        let nodes = parse_gpu_nodes_from_inventory_md(md);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].name, "bobbik-pc");
        assert_eq!(nodes[1].name, "bob-vm-02-gpu");
        assert_eq!(nodes[2].name, "render-03");
        assert_eq!(nodes[2].access_hint, "Tailscale 100.64.0.15");
    }
}
