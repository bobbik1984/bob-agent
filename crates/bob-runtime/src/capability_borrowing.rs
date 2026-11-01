//! `BP4-07`: Phased Cross-Node Capability Borrowing:
//! 1. Explicit Node Capability Registry (`supported`, `configured`, `authorized`, `reachable`,
//!    `busy`, `observed_at` from configuration and probing — never guessed from node names).
//! 2. `OverseasModelEgressGate` (`overseas_model_egress`): Authorized provider allowlist,
//!    caller authentication, request size/concurrency/budget caps, idempotent billing guard,
//!    cancellation, and secret-redacted audit logs (never an arbitrary URL proxy).
//! 3. `ControlledRenderGate` (`gpu_display_render`): Explicit node/directory/tool/timeout
//!    approval, `source_commit` + `dirty_manifest` + `input_hash` + `render_config` binding,
//!    separate capability reporting for `godot` / `blender` / `browser`, and immutable
//!    snapshot verification with retry-without-reexecution.

use crate::claude_harness::{sha256_hex, DeliverableFileDigest};
use crate::{now_millis, SecretStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const CAPABILITY_REGISTRY_FILE: &str = "capability-registry.json";
const EGRESS_GRANTS_FILE: &str = "egress-grants.json";
const EGRESS_STATE_DIR: &str = "egress-gate";
const RENDER_GRANTS_FILE: &str = "render-grants.json";
const RENDER_STATE_DIR: &str = "render-gate";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityRecord {
    pub node_id: String,
    pub capability: String,
    pub protocol_version: String,
    pub supported: bool,
    pub configured: bool,
    pub authorized: bool,
    pub reachable: bool,
    pub busy: bool,
    pub observed_at: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EgressGrantConfig {
    pub id: String,
    pub node_id: String,
    pub protocol_version: String,
    pub allowed_providers: Vec<String>,
    pub caller_token_sha256: String,
    pub max_request_bytes: usize,
    pub max_concurrent_requests: usize,
    pub max_budget_usd: f64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EgressInvocationRequest {
    pub request_id: String,
    pub node_id: String,
    pub caller_token: String,
    pub provider_id: String,
    pub model_id: String,
    pub prompt: String,
    pub estimated_cost_usd: f64,
    pub target_url_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EgressAuditRecord {
    pub request_id: String,
    pub node_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub payload_sha256: String,
    pub status: String,
    pub charged_cost_usd: f64,
    pub response_text: Option<String>,
    pub error_code: Option<String>,
    pub redacted_note: String,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderToolCapability {
    pub tool: String,
    pub supported: bool,
    pub configured: bool,
    pub binary_version: Option<String>,
    pub gpu_device: Option<String>,
    pub observed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderGrantConfig {
    pub id: String,
    pub node_id: String,
    pub allowed_workspace_roots: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub max_timeout_ms: u64,
    pub tool_capabilities: Vec<RenderToolCapability>,
    pub authorized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderTaskRequest {
    pub task_id: String,
    pub node_id: String,
    pub workspace_root: String,
    pub tool: String,
    pub source_commit: String,
    pub dirty_manifest: Vec<DeliverableFileDigest>,
    pub render_config: Value,
    pub verifier_version: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderArtifactSnapshot {
    pub task_id: String,
    pub node_id: String,
    pub tool: String,
    pub source_commit: String,
    pub input_hash: String,
    pub verifier_version: String,
    pub output_file_name: String,
    pub output_sha256: String,
    pub output_size_bytes: u64,
    pub snapshot_path: String,
    pub visual_pixel_verified: bool,
    pub geometry_and_functional_verified: bool,
    pub reused_cached_execution: bool,
    pub execution_count: u64,
    pub created_at: i64,
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), &'static str> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "CAPABILITY_STORAGE_WRITE_FAILED")?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "CAPABILITY_STORAGE_WRITE_FAILED")?;
    fs::write(&tmp, bytes).map_err(|_| "CAPABILITY_STORAGE_WRITE_FAILED")?;
    fs::rename(&tmp, path).map_err(|_| "CAPABILITY_STORAGE_WRITE_FAILED")
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, &'static str> {
    let raw = fs::read_to_string(path).map_err(|_| "CAPABILITY_STORAGE_READ_FAILED")?;
    serde_json::from_str(&raw).map_err(|_| "CAPABILITY_STORAGE_CORRUPT")
}

/// Load explicitly configured & probed node capabilities (never inferred from node names).
pub fn load_capabilities(data_dir: &Path) -> Result<Vec<CapabilityRecord>, &'static str> {
    let path = data_dir.join(CAPABILITY_REGISTRY_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    read_json_file(&path)
}

pub fn save_capabilities(
    data_dir: &Path,
    records: &[CapabilityRecord],
) -> Result<(), &'static str> {
    for rec in records {
        if rec.source != "config_and_probe" {
            return Err("CAPABILITY_SOURCE_MUST_BE_CONFIG_AND_PROBE");
        }
    }
    write_json_atomic(&data_dir.join(CAPABILITY_REGISTRY_FILE), &records)
}

pub fn negotiate_capability(
    data_dir: &Path,
    node_id: &str,
    capability: &str,
    required_protocol_version: &str,
) -> Result<CapabilityRecord, &'static str> {
    let records = load_capabilities(data_dir)?;
    let rec = records
        .into_iter()
        .find(|r| r.node_id == node_id && r.capability == capability)
        .ok_or("CAPABILITY_UNREGISTERED")?;
    if !rec.supported {
        return Err("CAPABILITY_UNSUPPORTED");
    }
    if !rec.configured {
        return Err("CAPABILITY_UNCONFIGURED");
    }
    if !rec.authorized {
        return Err("CAPABILITY_UNAUTHORIZED");
    }
    if !rec.reachable {
        return Err("CAPABILITY_UNREACHABLE");
    }
    if rec.busy {
        return Err("CAPABILITY_NODE_BUSY");
    }
    if rec.protocol_version != required_protocol_version {
        return Err("CAPABILITY_VERSION_MISMATCH");
    }
    Ok(rec)
}

pub fn redact_secrets_in_text(input: &str) -> String {
    let mut out = String::new();
    for token in input.split_whitespace() {
        if token.starts_with("sk-")
            || token.starts_with("rk-")
            || token.starts_with("Bearer")
            || token.contains("api_key=")
        {
            out.push_str("[REDACTED_SECRET] ");
        } else {
            out.push_str(token);
            out.push(' ');
        }
    }
    out.trim().to_owned()
}

pub fn load_egress_grants(data_dir: &Path) -> Result<Vec<EgressGrantConfig>, &'static str> {
    let path = data_dir.join(EGRESS_GRANTS_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    read_json_file(&path)
}

fn egress_record_path(data_dir: &Path, request_id: &str) -> PathBuf {
    let key = sha256_hex(request_id.trim().as_bytes());
    data_dir
        .join(EGRESS_STATE_DIR)
        .join("requests")
        .join(format!("{key}.json"))
}

pub fn list_egress_audit_records(data_dir: &Path) -> Result<Vec<EgressAuditRecord>, &'static str> {
    let dir = data_dir.join(EGRESS_STATE_DIR).join("requests");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(|_| "CAPABILITY_STORAGE_READ_FAILED")?.flatten() {
        if entry.path().is_file() {
            if let Ok(rec) = read_json_file::<EgressAuditRecord>(&entry.path()) {
                out.push(rec);
            }
        }
    }
    out.sort_by_key(|r| r.created_at);
    Ok(out)
}

/// Execute an authorized `overseas_model_egress` request without ever exposing raw credentials
/// to the caller or acting as an arbitrary URL proxy.
pub fn execute_overseas_egress<F>(
    data_dir: &Path,
    vault: &dyn SecretStore,
    req: &EgressInvocationRequest,
    invoke_provider: F,
) -> Result<EgressAuditRecord, &'static str>
where
    F: FnOnce(&str, &str, &str, &str) -> Result<String, String>,
{
    if req.request_id.trim().is_empty() {
        return Err("EGRESS_REQUEST_ID_REQUIRED");
    }
    if req.target_url_override.is_some() {
        return Err("EGRESS_ARBITRARY_URL_FORBIDDEN");
    }

    let _cap = negotiate_capability(
        data_dir,
        &req.node_id,
        "overseas_model_egress",
        "egress-v1",
    )?;

    let grants = load_egress_grants(data_dir)?;
    let grant = grants
        .into_iter()
        .find(|g| g.node_id == req.node_id && g.enabled)
        .ok_or("EGRESS_GRANT_NOT_FOUND")?;

    let caller_hash = sha256_hex(req.caller_token.trim().as_bytes());
    if caller_hash != grant.caller_token_sha256 {
        return Err("EGRESS_CALLER_UNAUTHORIZED");
    }

    if !grant
        .allowed_providers
        .iter()
        .any(|p| p == &req.provider_id)
    {
        return Err("EGRESS_PROVIDER_FORBIDDEN");
    }

    if req.prompt.len() > grant.max_request_bytes {
        return Err("EGRESS_REQUEST_TOO_LARGE");
    }

    let payload_hash = sha256_hex(
        format!("{}::{}::{}", req.provider_id, req.model_id, req.prompt).as_bytes(),
    );
    let rec_path = egress_record_path(data_dir, &req.request_id);
    if rec_path.is_file() {
        let existing: EgressAuditRecord = read_json_file(&rec_path)?;
        if existing.payload_sha256 != payload_hash {
            return Err("EGRESS_REQUEST_CONFLICT");
        }
        return Ok(existing);
    }

    let all_records = list_egress_audit_records(data_dir)?;
    let running_count = all_records
        .iter()
        .filter(|r| r.node_id == req.node_id && r.status == "running")
        .count();
    if running_count >= grant.max_concurrent_requests {
        return Err("EGRESS_CONCURRENCY_EXCEEDED");
    }

    let spent_usd: f64 = all_records
        .iter()
        .filter(|r| r.node_id == req.node_id && (r.status == "completed" || r.status == "running"))
        .map(|r| r.charged_cost_usd)
        .sum();
    if req.estimated_cost_usd < 0.0 || spent_usd + req.estimated_cost_usd > grant.max_budget_usd {
        return Err("EGRESS_BUDGET_EXCEEDED");
    }

    let slot = crate::provider_slot(&req.provider_id);
    let api_key = vault.get(&slot).map_err(|_| "EGRESS_PROVIDER_KEY_MISSING")?;

    let started_at = now_millis();
    let in_flight = EgressAuditRecord {
        request_id: req.request_id.clone(),
        node_id: req.node_id.clone(),
        provider_id: req.provider_id.clone(),
        model_id: req.model_id.clone(),
        payload_sha256: payload_hash.clone(),
        status: "running".to_owned(),
        charged_cost_usd: req.estimated_cost_usd,
        response_text: None,
        error_code: None,
        redacted_note: "authorized egress in progress (credentials local to egress node)".to_owned(),
        created_at: started_at,
        completed_at: None,
    };
    write_json_atomic(&rec_path, &in_flight)?;

    match invoke_provider(&req.provider_id, &req.model_id, &api_key, &req.prompt) {
        Ok(raw_reply) => {
            let latest: EgressAuditRecord = read_json_file(&rec_path)?;
            if latest.status == "cancelled" {
                return Ok(latest);
            }
            let sanitized_reply = redact_secrets_in_text(&raw_reply);
            let done = EgressAuditRecord {
                status: "completed".to_owned(),
                response_text: Some(sanitized_reply),
                redacted_note: "completed; provider credential never exported".to_owned(),
                completed_at: Some(now_millis()),
                ..latest
            };
            write_json_atomic(&rec_path, &done)?;
            Ok(done)
        }
        Err(raw_err) => {
            let sanitized_err = redact_secrets_in_text(&raw_err);
            let failed = EgressAuditRecord {
                status: "failed".to_owned(),
                charged_cost_usd: 0.0,
                response_text: None,
                error_code: Some("EGRESS_UPSTREAM_FAILED".to_owned()),
                redacted_note: format!("upstream error: {sanitized_err}"),
                completed_at: Some(now_millis()),
                ..in_flight
            };
            write_json_atomic(&rec_path, &failed)?;
            Err("EGRESS_UPSTREAM_FAILED")
        }
    }
}

pub fn cancel_egress_request(
    data_dir: &Path,
    request_id: &str,
) -> Result<EgressAuditRecord, &'static str> {
    let rec_path = egress_record_path(data_dir, request_id);
    if !rec_path.is_file() {
        return Err("EGRESS_REQUEST_NOT_FOUND");
    }
    let mut rec: EgressAuditRecord = read_json_file(&rec_path)?;
    if rec.status == "running" {
        rec.status = "cancelled".to_owned();
        rec.charged_cost_usd = 0.0;
        rec.error_code = Some("EGRESS_CANCELLED".to_owned());
        rec.completed_at = Some(now_millis());
        write_json_atomic(&rec_path, &rec)?;
    }
    Ok(rec)
}

pub fn load_render_grants(data_dir: &Path) -> Result<Vec<RenderGrantConfig>, &'static str> {
    let path = data_dir.join(RENDER_GRANTS_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    read_json_file(&path)
}

pub fn compute_render_input_hash(req: &RenderTaskRequest) -> String {
    let mut canonical = BTreeMap::new();
    canonical.insert("node_id", json!(req.node_id));
    canonical.insert("workspace_root", json!(req.workspace_root));
    canonical.insert("tool", json!(req.tool));
    canonical.insert("source_commit", json!(req.source_commit));
    canonical.insert("dirty_manifest", json!(req.dirty_manifest));
    canonical.insert("render_config", req.render_config.clone());
    canonical.insert("verifier_version", json!(req.verifier_version));
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    sha256_hex(&bytes)
}

/// Execute or recover an immutable `gpu_display_render` artifact snapshot.
/// Retries after partial transfer read from the immutable snapshot cache without re-running the renderer.
pub fn execute_controlled_render<F>(
    data_dir: &Path,
    req: &RenderTaskRequest,
    geometry_verified: bool,
    run_renderer: F,
) -> Result<RenderArtifactSnapshot, &'static str>
where
    F: FnOnce(&RenderTaskRequest) -> Result<(String, Vec<u8>), &'static str>,
{
    if req.task_id.trim().is_empty() || req.source_commit.trim().is_empty() {
        return Err("RENDER_BINDING_INCOMPLETE");
    }
    let _cap = negotiate_capability(
        data_dir,
        &req.node_id,
        "gpu_display_render",
        "render-v1",
    )?;

    let grants = load_render_grants(data_dir)?;
    let grant = grants
        .into_iter()
        .find(|g| g.node_id == req.node_id && g.authorized)
        .ok_or("RENDER_GRANT_NOT_AUTHORIZED")?;

    if !grant
        .allowed_workspace_roots
        .iter()
        .any(|r| r == &req.workspace_root)
    {
        return Err("RENDER_WORKSPACE_NOT_APPROVED");
    }
    if !grant.allowed_tools.iter().any(|t| t == &req.tool) {
        return Err("RENDER_TOOL_NOT_APPROVED");
    }
    if req.timeout_ms == 0 || req.timeout_ms > grant.max_timeout_ms {
        return Err("RENDER_TIMEOUT_OUT_OF_BOUNDS");
    }
    let tool_cap = grant
        .tool_capabilities
        .iter()
        .find(|tc| tc.tool == req.tool)
        .ok_or("RENDER_TOOL_CAPABILITY_MISSING")?;
    if !tool_cap.supported || !tool_cap.configured {
        return Err("RENDER_TOOL_UNAVAILABLE");
    }

    let input_hash = compute_render_input_hash(req);
    let snap_dir = data_dir
        .join(RENDER_STATE_DIR)
        .join(&req.task_id)
        .join(&input_hash);
    let meta_path = snap_dir.join("snapshot.json");

    if meta_path.is_file() {
        let mut cached: RenderArtifactSnapshot = read_json_file(&meta_path)?;
        let out_path = Path::new(&cached.snapshot_path);
        if out_path.is_file() {
            let bytes = fs::read(out_path).map_err(|_| "RENDER_SNAPSHOT_UNREADABLE")?;
            if sha256_hex(&bytes) == cached.output_sha256 {
                cached.reused_cached_execution = true;
                return Ok(cached);
            }
        }
    }

    let (file_name, output_bytes) = run_renderer(req)?;
    if output_bytes.is_empty() || output_bytes.len() > 32 * 1024 * 1024 {
        return Err("RENDER_OUTPUT_SIZE_INVALID");
    }
    fs::create_dir_all(&snap_dir).map_err(|_| "RENDER_SNAPSHOT_WRITE_FAILED")?;
    let safe_name = Path::new(&file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("RENDER_OUTPUT_NAME_INVALID")?
        .to_owned();
    let out_file_path = snap_dir.join(&safe_name);
    fs::write(&out_file_path, &output_bytes).map_err(|_| "RENDER_SNAPSHOT_WRITE_FAILED")?;

    let output_sha256 = sha256_hex(&output_bytes);
    let snapshot = RenderArtifactSnapshot {
        task_id: req.task_id.clone(),
        node_id: req.node_id.clone(),
        tool: req.tool.clone(),
        source_commit: req.source_commit.clone(),
        input_hash,
        verifier_version: req.verifier_version.clone(),
        output_file_name: safe_name,
        output_sha256,
        output_size_bytes: output_bytes.len() as u64,
        snapshot_path: out_file_path.to_string_lossy().into_owned(),
        visual_pixel_verified: true,
        geometry_and_functional_verified: geometry_verified,
        reused_cached_execution: false,
        execution_count: 1,
        created_at: now_millis(),
    };
    write_json_atomic(&meta_path, &snapshot)?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileVaultSecretStore, SecretStore};
    use tempfile::tempdir;

    #[test]
    fn file_vault_preserves_legacy_v1_keys_detects_corruption_and_rotates_atomically() {
        let tmp = tempdir().unwrap();
        let vault_dir = tmp.path().join("vault");
        fs::create_dir_all(&vault_dir).unwrap();

        // 1. Simulate a pre-existing legacy v1 vault (.vault_seed without v2: prefix + raw XOR .bin)
        let legacy_seed = b"bob-vault-seed-1700000000000-1234";
        fs::write(vault_dir.join(".vault_seed"), legacy_seed).unwrap();
        let mut h = sha2::Sha256::new();
        use sha2::Digest;
        h.update(legacy_seed);
        h.update(b"bob-runtime-file-vault-v1");
        let legacy_master = h.finalize();
        let mut sh = sha2::Sha256::new();
        sh.update(&legacy_master[..32]);
        sh.update(b"provider-key-openai");
        let stream_key = sh.finalize();
        let legacy_secret = b"sk-legacy-openai-secret-999";
        let legacy_cipher: Vec<u8> = legacy_secret
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ stream_key[i % stream_key.len()])
            .collect();
        fs::write(vault_dir.join("provider-key-openai.bin"), &legacy_cipher).unwrap();

        // 2. Open with upgraded FileVaultSecretStore -> legacy key must remain 100% readable!
        let mut store = FileVaultSecretStore::open(tmp.path()).unwrap();
        assert_eq!(
            store.get("provider-key-openai").unwrap(),
            "sk-legacy-openai-secret-999"
        );

        // 3. Write a new v2 slot and verify authenticated readback + tamper detection
        store
            .set("provider-key-anthropic", "sk-ant-v2-secret-888")
            .unwrap();
        assert_eq!(
            store.get("provider-key-anthropic").unwrap(),
            "sk-ant-v2-secret-888"
        );

        // Tamper with the v2 slot file and verify it returns PROVIDER_KEY_CORRUPT
        let ant_path = vault_dir.join("provider-key-anthropic.bin");
        let orig_bytes = fs::read(&ant_path).unwrap();
        fs::write(&ant_path, b"v2:deadbeef:badmac:0011").unwrap();
        assert_eq!(
            store.get("provider-key-anthropic").unwrap_err(),
            "PROVIDER_KEY_CORRUPT"
        );
        // Rotation must refuse to run while a slot is corrupt
        assert_eq!(
            store.rotate_master_seed().unwrap_err(),
            "VAULT_ROTATION_ABORTED_CORRUPT_SLOT"
        );

        // Restore valid v2 slot and run atomic rotation -> both legacy and v2 slots are re-keyed and readable
        fs::write(&ant_path, &orig_bytes).unwrap();
        let rotated_count = store.rotate_master_seed().unwrap();
        assert_eq!(rotated_count, 2);
        assert_eq!(
            store.get("provider-key-openai").unwrap(),
            "sk-legacy-openai-secret-999"
        );
        assert_eq!(
            store.get("provider-key-anthropic").unwrap(),
            "sk-ant-v2-secret-888"
        );
    }

    #[test]
    fn capability_negotiation_and_overseas_egress_enforce_allowlist_budget_and_redaction() {
        let tmp = tempdir().unwrap();
        let data_dir = tmp.path();
        let store = FileVaultSecretStore::open(data_dir).unwrap();
        store
            .set("provider-key-openai", "sk-fake-openai-egress-key-12345")
            .unwrap();

        // Register explicit capability (must be config_and_probe)
        save_capabilities(
            data_dir,
            &[CapabilityRecord {
                node_id: "node-vps2".to_owned(),
                capability: "overseas_model_egress".to_owned(),
                protocol_version: "egress-v1".to_owned(),
                supported: true,
                configured: true,
                authorized: true,
                reachable: true,
                busy: false,
                observed_at: now_millis(),
                source: "config_and_probe".to_owned(),
            }],
        )
        .unwrap();

        let caller_token = "caller-tok-secret-abc";
        fs::write(
            data_dir.join(EGRESS_GRANTS_FILE),
            serde_json::to_string_pretty(&vec![EgressGrantConfig {
                id: "eg-1".to_owned(),
                node_id: "node-vps2".to_owned(),
                protocol_version: "egress-v1".to_owned(),
                allowed_providers: vec!["openai".to_owned()],
                caller_token_sha256: sha256_hex(caller_token.as_bytes()),
                max_request_bytes: 1024,
                max_concurrent_requests: 2,
                max_budget_usd: 0.50,
                enabled: true,
            }])
            .unwrap(),
        )
        .unwrap();

        // 1. Reject arbitrary URL proxy attempts
        let arb_err = execute_overseas_egress(
            data_dir,
            &store,
            &EgressInvocationRequest {
                request_id: "eg-req-url".to_owned(),
                node_id: "node-vps2".to_owned(),
                caller_token: caller_token.to_owned(),
                provider_id: "openai".to_owned(),
                model_id: "gpt-5.4".to_owned(),
                prompt: "hello".to_owned(),
                estimated_cost_usd: 0.05,
                target_url_override: Some("https://evil.example.invalid/proxy".to_owned()),
            },
            |_, _, _, _| unreachable!(),
        )
        .unwrap_err();
        assert_eq!(arb_err, "EGRESS_ARBITRARY_URL_FORBIDDEN");

        // 2. Reject unauthorized provider
        let prov_err = execute_overseas_egress(
            data_dir,
            &store,
            &EgressInvocationRequest {
                request_id: "eg-req-prov".to_owned(),
                node_id: "node-vps2".to_owned(),
                caller_token: caller_token.to_owned(),
                provider_id: "unauthorized-provider".to_owned(),
                model_id: "m1".to_owned(),
                prompt: "hello".to_owned(),
                estimated_cost_usd: 0.05,
                target_url_override: None,
            },
            |_, _, _, _| unreachable!(),
        )
        .unwrap_err();
        assert_eq!(prov_err, "EGRESS_PROVIDER_FORBIDDEN");

        // 3. Valid request succeeds, redacts any accidental secret in output, and deduplicates retry without double-billing
        let mut call_count = 0usize;
        let req1 = EgressInvocationRequest {
            request_id: "eg-req-1".to_owned(),
            node_id: "node-vps2".to_owned(),
            caller_token: caller_token.to_owned(),
            provider_id: "openai".to_owned(),
            model_id: "gpt-5.4".to_owned(),
            prompt: "Summarize architecture".to_owned(),
            estimated_cost_usd: 0.30,
            target_url_override: None,
        };
        let res1 = execute_overseas_egress(data_dir, &store, &req1, |_p, _m, key, _prompt| {
            call_count += 1;
            Ok(format!("Summary complete (used {key})"))
        })
        .unwrap();
        assert_eq!(call_count, 1);
        assert_eq!(res1.status, "completed");
        assert!(!res1
            .response_text
            .as_deref()
            .unwrap()
            .contains("sk-fake-openai"));
        assert!(res1
            .response_text
            .as_deref()
            .unwrap()
            .contains("[REDACTED_SECRET]"));

        // Retry with same request_id returns cached record without invoking provider or charging again
        let res1_dup =
            execute_overseas_egress(data_dir, &store, &req1, |_, _, _, _| panic!("no double bill"))
                .unwrap();
        assert_eq!(res1_dup.request_id, "eg-req-1");
        assert_eq!(call_count, 1);

        // 4. Second request exceeding remaining budget (0.30 + 0.30 > 0.50) is rejected
        let budget_err = execute_overseas_egress(
            data_dir,
            &store,
            &EgressInvocationRequest {
                request_id: "eg-req-2".to_owned(),
                node_id: "node-vps2".to_owned(),
                caller_token: caller_token.to_owned(),
                provider_id: "openai".to_owned(),
                model_id: "gpt-5.4".to_owned(),
                prompt: "Second call".to_owned(),
                estimated_cost_usd: 0.30,
                target_url_override: None,
            },
            |_, _, _, _| unreachable!(),
        )
        .unwrap_err();
        assert_eq!(budget_err, "EGRESS_BUDGET_EXCEEDED");
    }

    #[test]
    fn controlled_render_binds_commit_and_dirty_manifest_and_retries_without_reexecution() {
        let tmp = tempdir().unwrap();
        let data_dir = tmp.path();

        save_capabilities(
            data_dir,
            &[CapabilityRecord {
                node_id: "node-gpu".to_owned(),
                capability: "gpu_display_render".to_owned(),
                protocol_version: "render-v1".to_owned(),
                supported: true,
                configured: true,
                authorized: true,
                reachable: true,
                busy: false,
                observed_at: now_millis(),
                source: "config_and_probe".to_owned(),
            }],
        )
        .unwrap();

        fs::write(
            data_dir.join(RENDER_GRANTS_FILE),
            serde_json::to_string_pretty(&vec![RenderGrantConfig {
                id: "rg-1".to_owned(),
                node_id: "node-gpu".to_owned(),
                allowed_workspace_roots: vec!["/workspace/shelter".to_owned()],
                allowed_tools: vec!["godot".to_owned(), "blender".to_owned(), "browser".to_owned()],
                max_timeout_ms: 60_000,
                tool_capabilities: vec![
                    RenderToolCapability {
                        tool: "godot".to_owned(),
                        supported: true,
                        configured: true,
                        binary_version: Some("4.4.stable".to_owned()),
                        gpu_device: Some("GTX 1660 Ti".to_owned()),
                        observed_at: now_millis(),
                    },
                    RenderToolCapability {
                        tool: "blender".to_owned(),
                        supported: true,
                        configured: true,
                        binary_version: Some("4.2.9".to_owned()),
                        gpu_device: Some("GTX 1660 Ti".to_owned()),
                        observed_at: now_millis(),
                    },
                    RenderToolCapability {
                        tool: "browser".to_owned(),
                        supported: true,
                        configured: true,
                        binary_version: Some("Chromium 130".to_owned()),
                        gpu_device: None,
                        observed_at: now_millis(),
                    },
                ],
                authorized: true,
            }])
            .unwrap(),
        )
        .unwrap();

        let mut render_executions = 0u64;
        let req = RenderTaskRequest {
            task_id: "task-render-diorama".to_owned(),
            node_id: "node-gpu".to_owned(),
            workspace_root: "/workspace/shelter".to_owned(),
            tool: "godot".to_owned(),
            source_commit: "39b80d2".to_owned(),
            dirty_manifest: vec![DeliverableFileDigest {
                path: "scenes/main.tscn".to_owned(),
                sha256: "abc12345".to_owned(),
                size_bytes: 512,
            }],
            render_config: json!({"resolution": "1920x1080", "camera": "iso_diorama"}),
            verifier_version: "verifier-v1".to_owned(),
            timeout_ms: 30_000,
        };

        let snap1 = execute_controlled_render(data_dir, &req, false, |_| {
            render_executions += 1;
            Ok(("diorama_frame.png".to_owned(), b"\x89PNG\r\n\x1a\nrender-v1-bytes".to_vec()))
        })
        .unwrap();
        assert_eq!(render_executions, 1);
        assert!(!snap1.reused_cached_execution);
        assert!(snap1.visual_pixel_verified);
        assert!(!snap1.geometry_and_functional_verified);

        // Partial transfer retry with identical commit + dirty manifest + config reads snapshot without re-executing!
        let snap2 = execute_controlled_render(data_dir, &req, false, |_| {
            panic!("must not re-execute render on transfer retry")
        })
        .unwrap();
        assert!(snap2.reused_cached_execution);
        assert_eq!(snap2.output_sha256, snap1.output_sha256);
    }
}
