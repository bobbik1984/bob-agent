//! Controlled host operations for PC Companion nodes, authorized workspace roots,
//! and offline PC queue reconciliation.

use crate::tool_bus::{BobToolBus, ToolBusOutcome, ToolExecutionMode};
use bob_kernel::app::LocalApp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceGrant {
    pub id: String,
    pub path: String, // canonicalized directory path
    pub label: String,
    pub read_only: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceGrantStoreData {
    pub workspaces: Vec<WorkspaceGrant>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceGrantStore {
    file_path: PathBuf,
    pub data: WorkspaceGrantStoreData,
}

impl WorkspaceGrantStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = fs::create_dir_all(data_dir);
        let file_path = data_dir.join("host_workspaces.json");
        let data = if file_path.is_file() {
            let content = fs::read_to_string(&file_path).map_err(|_| "HOST_WORKSPACES_READ_FAILED")?;
            serde_json::from_str::<WorkspaceGrantStoreData>(&content).unwrap_or_default()
        } else {
            WorkspaceGrantStoreData::default()
        };
        Ok(Self { file_path, data })
    }

    pub fn save(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "HOST_WORKSPACES_SERIALIZE_FAILED")?;
        let tmp = self.file_path.with_extension("json.tmp");
        fs::write(&tmp, serialized.as_bytes()).map_err(|_| "HOST_WORKSPACES_WRITE_FAILED")?;
        fs::rename(&tmp, &self.file_path).map_err(|_| "HOST_WORKSPACES_WRITE_FAILED")?;
        Ok(())
    }

    pub fn authorize_workspace(
        &mut self,
        path: &str,
        label: &str,
        read_only: bool,
        now: i64,
    ) -> Result<WorkspaceGrant, &'static str> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err("HOST_PATH_EMPTY");
        }
        let canonical = fs::canonicalize(trimmed).map_err(|_| "HOST_DIR_NOT_FOUND")?;
        if !canonical.is_dir() {
            return Err("HOST_NOT_A_DIRECTORY");
        }

        // Strictly reject sensitive system directories or Bob's own credential / profile vaults
        let path_str = canonical.to_string_lossy().to_lowercase();
        let sensitive_fragments = [
            ".ssh",
            ".gnupg",
            ".aws",
            ".kube",
            "browser_profile",
            "vault",
            "secrets",
            "windows\\system32",
            "/etc",
            "/var/root",
        ];
        for s in sensitive_fragments {
            if path_str.contains(s) {
                return Err("HOST_WORKSPACE_SENSITIVE_REJECTED");
            }
        }

        let canonical_str = canonical.to_string_lossy().to_string();

        // Avoid duplicate paths
        if let Some(existing) = self.data.workspaces.iter_mut().find(|w| w.path == canonical_str) {
            existing.label = if label.trim().is_empty() {
                existing.label.clone()
            } else {
                label.trim().to_string()
            };
            existing.read_only = read_only;
            let res = existing.clone();
            self.save()?;
            return Ok(res);
        }

        let id = format!("ws-{}", now);
        let grant = WorkspaceGrant {
            id,
            path: canonical_str,
            label: if label.trim().is_empty() {
                trimmed.to_string()
            } else {
                label.trim().to_string()
            },
            read_only,
            created_at: now,
        };

        self.data.workspaces.push(grant.clone());
        self.save()?;
        Ok(grant)
    }

    pub fn revoke_workspace(&mut self, id: &str) -> Result<WorkspaceGrant, &'static str> {
        let idx = self
            .data
            .workspaces
            .iter()
            .position(|w| w.id == id)
            .ok_or("HOST_WORKSPACE_NOT_FOUND")?;
        let removed = self.data.workspaces.remove(idx);
        self.save()?;
        Ok(removed)
    }

    pub fn list_workspaces(&self) -> Vec<WorkspaceGrant> {
        self.data.workspaces.clone()
    }

    pub fn resolve_authorized_path(
        &self,
        target_path: &str,
        require_writable: bool,
    ) -> Result<PathBuf, &'static str> {
        let trimmed = target_path.trim();
        if trimmed.is_empty() {
            return Err("HOST_PATH_EMPTY");
        }

        let p = Path::new(trimmed);

        // Disallow any ".." parent dir components
        for comp in p.components() {
            if matches!(comp, Component::ParentDir) {
                return Err("HOST_PATH_OUTSIDE_WORKSPACE");
            }
        }

        let canonical = if p.exists() {
            fs::canonicalize(p).map_err(|_| "HOST_PATH_RESOLUTION_FAILED")?
        } else {
            // New file that does not yet exist: walk up to nearest existing ancestor directory
            let mut curr = p.to_path_buf();
            let mut relative_parts = Vec::new();
            while !curr.exists() {
                if let Some(file_name) = curr.file_name() {
                    relative_parts.push(file_name.to_os_string());
                    match curr.parent() {
                        Some(parent) if parent != curr => curr = parent.to_path_buf(),
                        _ => return Err("HOST_DIR_NOT_FOUND"),
                    }
                } else {
                    return Err("HOST_DIR_NOT_FOUND");
                }
            }
            let canonical_ancestor = fs::canonicalize(&curr).map_err(|_| "HOST_DIR_NOT_FOUND")?;
            relative_parts.reverse();
            let mut res = canonical_ancestor;
            for part in relative_parts {
                res.push(part);
            }
            res
        };

        // Re-check zero traversal components in canonical
        for comp in canonical.components() {
            if matches!(comp, Component::ParentDir) {
                return Err("HOST_PATH_OUTSIDE_WORKSPACE");
            }
        }

        // Must start with at least one authorized workspace root
        let matching_grant = self.data.workspaces.iter().find(|g| {
            let g_path = Path::new(&g.path);
            canonical.starts_with(g_path)
        });

        let grant = matching_grant.ok_or("HOST_PATH_OUTSIDE_WORKSPACE")?;
        if require_writable && grant.read_only {
            return Err("HOST_WORKSPACE_READ_ONLY");
        }

        // Hard credential denylist
        let fname = canonical
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let can_str_lower = canonical.to_string_lossy().to_ascii_lowercase();

        if fname == ".env"
            || fname.starts_with(".env.")
            || fname.ends_with(".pem")
            || fname.ends_with(".key")
            || fname == "id_rsa"
            || fname == "id_ed25519"
            || fname == "cookies"
            || fname == "login data"
            || fname.starts_with("vault")
            || fname == "node_access.sqlite"
            || can_str_lower.contains("node_access.sqlite")
            || can_str_lower.contains("browser_profile")
        {
            return Err("HOST_SENSITIVE_PATH_FORBIDDEN");
        }

        Ok(canonical)
    }
}

// ---------------------------------------------------------------------------
// Controlled Host Operations
// ---------------------------------------------------------------------------

pub fn host_list_files(
    data_dir: &Path,
    dir_path: &str,
    max_entries: Option<usize>,
) -> Result<Value, String> {
    let store = WorkspaceGrantStore::open(data_dir).map_err(|e| e.to_string())?;
    let canonical = store
        .resolve_authorized_path(dir_path, false)
        .map_err(|e| e.to_string())?;

    if !canonical.is_dir() {
        return Err("HOST_NOT_A_DIRECTORY".to_string());
    }

    let limit = max_entries.unwrap_or(200).min(1000);
    let mut entries = Vec::new();

    let rd = fs::read_dir(&canonical).map_err(|e| format!("READ_DIR_FAILED: {e}"))?;
    for item in rd {
        if entries.len() >= limit {
            break;
        }
        let entry = match item {
            Ok(e) => e,
            Err(_) => continue,
        };
        let file_name = entry.file_name().to_string_lossy().to_string();
        let fname_lower = file_name.to_ascii_lowercase();

        // Skip hidden secret files
        if fname_lower.starts_with('.')
            || fname_lower.ends_with(".pem")
            || fname_lower.ends_with(".key")
            || fname_lower == "id_rsa"
            || fname_lower == "id_ed25519"
        {
            continue;
        }

        let md = entry.metadata().ok();
        let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size_bytes = md.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified_at = md
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        entries.push(json!({
            "name": file_name,
            "kind": if is_dir { "dir" } else { "file" },
            "size_bytes": size_bytes,
            "modified_at": modified_at,
        }));
    }

    Ok(json!({
        "path": canonical.to_string_lossy(),
        "entries": entries,
        "count": entries.len()
    }))
}

pub fn host_read_file(
    data_dir: &Path,
    file_path: &str,
    max_bytes: Option<usize>,
) -> Result<Value, String> {
    let store = WorkspaceGrantStore::open(data_dir).map_err(|e| e.to_string())?;
    let canonical = store
        .resolve_authorized_path(file_path, false)
        .map_err(|e| e.to_string())?;

    if !canonical.is_file() {
        return Err("HOST_NOT_A_FILE".to_string());
    }

    let cap = max_bytes.unwrap_or(131_072).min(131_072);
    let mut file = fs::File::open(&canonical).map_err(|e| format!("OPEN_FAILED: {e}"))?;
    let mut buf = Vec::new();
    file.by_ref()
        .take((cap + 1) as u64)
        .read_to_end(&mut buf)
        .map_err(|e| format!("READ_FAILED: {e}"))?;

    // Check for null bytes (binary file detector)
    if buf.contains(&0) {
        return Err("HOST_BINARY_FILE_REJECTED".to_string());
    }

    let is_truncated = buf.len() > cap;
    if is_truncated {
        buf.truncate(cap);
    }

    let text = String::from_utf8(buf).map_err(|_| "HOST_BINARY_FILE_REJECTED".to_string())?;

    Ok(json!({
        "path": canonical.to_string_lossy(),
        "content": text,
        "bytes": text.len(),
        "truncated": is_truncated
    }))
}

pub fn host_write_file(data_dir: &Path, file_path: &str, content: &str) -> Result<Value, String> {
    let store = WorkspaceGrantStore::open(data_dir).map_err(|e| e.to_string())?;
    let canonical = store
        .resolve_authorized_path(file_path, true)
        .map_err(|e| e.to_string())?;

    if content.len() > 524_288 {
        return Err("CONTENT_TOO_LARGE".to_string());
    }

    if let Some(parent) = canonical.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("CREATE_DIR_FAILED: {e}"))?;
    }

    let tmp = canonical.with_extension(format!("tmp.{}", crate::now_millis()));
    fs::write(&tmp, content.as_bytes()).map_err(|e| format!("WRITE_FAILED: {e}"))?;
    fs::rename(&tmp, &canonical).map_err(|e| format!("RENAME_FAILED: {e}"))?;

    Ok(json!({
        "written": true,
        "path": canonical.to_string_lossy(),
        "bytes": content.len()
    }))
}

pub fn host_run_command(
    data_dir: &Path,
    command: &str,
    cwd: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<Value, String> {
    let store = WorkspaceGrantStore::open(data_dir).map_err(|e| e.to_string())?;

    let canonical_cwd = if let Some(dir) = cwd {
        store
            .resolve_authorized_path(dir, true)
            .map_err(|e| e.to_string())?
    } else {
        let first_writable = store
            .data
            .workspaces
            .iter()
            .find(|w| !w.read_only)
            .map(|w| PathBuf::from(&w.path));
        first_writable.ok_or_else(|| "NO_WRITABLE_WORKSPACE_CONFIGURED".to_string())?
    };

    let trimmed_cmd = command.trim();
    if trimmed_cmd.is_empty() {
        return Err("EMPTY_COMMAND".to_string());
    }
    if trimmed_cmd.len() > 4096 {
        return Err("COMMAND_TOO_LONG".to_string());
    }

    let timeout_duration = Duration::from_millis(timeout_ms.unwrap_or(15_000).clamp(100, 60_000));

    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("cmd");
    #[cfg(target_os = "windows")]
    cmd.args(["/C", trimmed_cmd]);

    #[cfg(not(target_os = "windows"))]
    let mut cmd = std::process::Command::new("sh");
    #[cfg(not(target_os = "windows"))]
    cmd.args(["-c", trimmed_cmd]);

    cmd.current_dir(&canonical_cwd);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("PROCESS_SPAWN_FAILED: {e}"))?;

    // Wait with timeout
    let (tx, rx) = std::sync::mpsc::channel();
    let mut stdout_handle = child.stdout.take();
    let mut stderr_handle = child.stderr.take();

    let waiter = std::thread::spawn(move || {
        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();

        if let Some(mut so) = stdout_handle.take() {
            let _ = so.by_ref().take(65_536).read_to_end(&mut stdout_bytes);
        }
        if let Some(mut se) = stderr_handle.take() {
            let _ = se.by_ref().take(65_536).read_to_end(&mut stderr_bytes);
        }

        let status = child.wait();
        let _ = tx.send((status, stdout_bytes, stderr_bytes));
    });

    match rx.recv_timeout(timeout_duration) {
        Ok((status, stdout_bytes, stderr_bytes)) => {
            let _ = waiter.join();
            let exit_code = status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
            let stdout_str = String::from_utf8_lossy(&stdout_bytes).to_string();
            let stderr_str = String::from_utf8_lossy(&stderr_bytes).to_string();

            Ok(json!({
                "exit_code": exit_code,
                "stdout": stdout_str,
                "stderr": stderr_str,
                "timed_out": false
            }))
        }
        Err(_) => {
            // Timeout reached
            Ok(json!({
                "exit_code": -1,
                "stdout": "",
                "stderr": "COMMAND_TIMED_OUT",
                "timed_out": true
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Offline PC Queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostQueueItem {
    pub id: String,
    pub node_id: String,
    pub tool_name: String,
    pub args: Value,
    pub status: String, // "waiting_for_pc" | "pending_approval" | "completed" | "failed"
    pub result: Option<Value>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostOfflineQueueData {
    pub items: Vec<HostQueueItem>,
}

#[derive(Debug, Clone)]
pub struct HostOfflineQueueStore {
    file_path: PathBuf,
    pub data: HostOfflineQueueData,
}

impl HostOfflineQueueStore {
    pub fn open(data_dir: &Path) -> Result<Self, &'static str> {
        let _ = fs::create_dir_all(data_dir);
        let file_path = data_dir.join("host_offline_queue.json");
        let data = if file_path.is_file() {
            let content = fs::read_to_string(&file_path).map_err(|_| "HOST_QUEUE_READ_FAILED")?;
            serde_json::from_str::<HostOfflineQueueData>(&content).unwrap_or_default()
        } else {
            HostOfflineQueueData::default()
        };
        Ok(Self { file_path, data })
    }

    pub fn save(&self) -> Result<(), &'static str> {
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|_| "HOST_QUEUE_SERIALIZE_FAILED")?;
        let tmp = self.file_path.with_extension("json.tmp");
        fs::write(&tmp, serialized.as_bytes()).map_err(|_| "HOST_QUEUE_WRITE_FAILED")?;
        fs::rename(&tmp, &self.file_path).map_err(|_| "HOST_QUEUE_WRITE_FAILED")?;
        Ok(())
    }

    pub fn enqueue_for_pc(
        &mut self,
        node_id: &str,
        tool_name: &str,
        args: Value,
        now_ms: i64,
    ) -> Result<HostQueueItem, &'static str> {
        let id = format!("hq-{}-{}", now_ms, self.data.items.len() + 1);
        let item = HostQueueItem {
            id,
            node_id: node_id.to_string(),
            tool_name: tool_name.to_string(),
            args,
            status: "waiting_for_pc".to_string(),
            result: None,
            error: None,
            created_at: now_ms,
            updated_at: now_ms,
        };
        self.data.items.push(item.clone());
        self.save()?;
        Ok(item)
    }

    pub fn list_queue(&self, node_id_opt: Option<&str>) -> Vec<HostQueueItem> {
        match node_id_opt {
            Some(nid) => self
                .data
                .items
                .iter()
                .filter(|it| it.node_id == nid)
                .cloned()
                .collect(),
            None => self.data.items.clone(),
        }
    }

    pub fn reconcile_when_pc_online(
        data_dir: &Path,
        node_id: &str,
        bus: &mut BobToolBus,
        app: &mut LocalApp,
        now_ms: i64,
    ) -> Result<Vec<HostQueueItem>, String> {
        let mut store = Self::open(data_dir).map_err(|e| e.to_string())?;
        let mut reconciled = Vec::new();

        for item in store.data.items.iter_mut() {
            if item.node_id == node_id && item.status == "waiting_for_pc" {
                match bus.execute(&item.tool_name, &item.args, ToolExecutionMode::Interactive, app) {
                    Ok(ToolBusOutcome::Executed { output, .. }) => {
                        item.status = "completed".to_string();
                        item.result = Some(output);
                        item.updated_at = now_ms;
                    }
                    Ok(ToolBusOutcome::RequiresApproval { .. }) => {
                        item.status = "pending_approval".to_string();
                        item.updated_at = now_ms;
                    }
                    Ok(ToolBusOutcome::SavedAsDraft { .. }) => {
                        item.status = "pending_approval".to_string();
                        item.updated_at = now_ms;
                    }
                    Err(e) => {
                        item.status = "failed".to_string();
                        item.error = Some(e);
                        item.updated_at = now_ms;
                    }
                }
                reconciled.push(item.clone());
            }
        }

        store.save().map_err(|e| e.to_string())?;
        Ok(reconciled)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_bus::ActionRiskLevel;
    use tempfile::tempdir;

    #[test]
    fn test_authorized_workspace_list_and_read() {
        let td = tempdir().unwrap();
        let ws_dir = td.path().join("workspace");
        fs::create_dir_all(&ws_dir).unwrap();
        let test_file = ws_dir.join("hello.txt");
        fs::write(&test_file, b"Hello Bob").unwrap();

        let mut store = WorkspaceGrantStore::open(td.path()).unwrap();
        let grant = store
            .authorize_workspace(ws_dir.to_str().unwrap(), "Test Workspace", false, 1000)
            .unwrap();

        assert_eq!(grant.label, "Test Workspace");
        assert!(!grant.read_only);

        // List files
        let list_res = host_list_files(td.path(), ws_dir.to_str().unwrap(), None).unwrap();
        let entries = list_res["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["name"], "hello.txt");
        assert_eq!(entries[0]["kind"], "file");

        // Read file
        let read_res = host_read_file(td.path(), test_file.to_str().unwrap(), None).unwrap();
        assert_eq!(read_res["content"], "Hello Bob");
        assert_eq!(read_res["bytes"], 9);
        assert_eq!(read_res["truncated"], false);
    }

    #[test]
    fn test_path_traversal_and_sensitive_files_blocked() {
        let td = tempdir().unwrap();
        let ws_dir = td.path().join("workspace");
        fs::create_dir_all(&ws_dir).unwrap();
        let outside_file = td.path().join("outside.txt");
        fs::write(&outside_file, b"Secret outside").unwrap();

        let mut store = WorkspaceGrantStore::open(td.path()).unwrap();
        store
            .authorize_workspace(ws_dir.to_str().unwrap(), "Test Workspace", false, 1000)
            .unwrap();

        // 1. Path traversal via ..
        let traversal_path = format!("{}/../outside.txt", ws_dir.to_str().unwrap());
        let err = store.resolve_authorized_path(&traversal_path, false).unwrap_err();
        assert_eq!(err, "HOST_PATH_OUTSIDE_WORKSPACE");

        // 2. Sensitive files inside workspace: .env
        let env_file = ws_dir.join(".env");
        fs::write(&env_file, b"SECRET_KEY=123").unwrap();
        let err_env = store
            .resolve_authorized_path(env_file.to_str().unwrap(), false)
            .unwrap_err();
        assert_eq!(err_env, "HOST_SENSITIVE_PATH_FORBIDDEN");

        // 3. Sensitive files inside workspace: id_rsa
        let key_file = ws_dir.join("id_rsa");
        fs::write(&key_file, b"PEM_PRIVATE_KEY").unwrap();
        let err_key = store
            .resolve_authorized_path(key_file.to_str().unwrap(), false)
            .unwrap_err();
        assert_eq!(err_key, "HOST_SENSITIVE_PATH_FORBIDDEN");

        // 4. Sensitive directories rejected during authorization
        let ssh_dir = td.path().join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();
        let err_auth = store
            .authorize_workspace(ssh_dir.to_str().unwrap(), "SSH", false, 1000)
            .unwrap_err();
        assert_eq!(err_auth, "HOST_WORKSPACE_SENSITIVE_REJECTED");
    }

    #[test]
    fn test_host_write_file_approval_gate_and_atomic_write() {
        let td = tempdir().unwrap();
        let ws_dir = td.path().join("workspace");
        fs::create_dir_all(&ws_dir).unwrap();

        let mut store = WorkspaceGrantStore::open(td.path()).unwrap();
        store
            .authorize_workspace(ws_dir.to_str().unwrap(), "Writable WS", false, 1000)
            .unwrap();

        let target_file = ws_dir.join("output.txt");
        let mut bus = BobToolBus::with_approval_store(td.path()).unwrap();
        let mut app = LocalApp::open(&td.path().join("kernel.sqlite")).unwrap();

        let args = json!({
            "file_path": target_file.to_str().unwrap(),
            "content": "Updated content from Bob"
        });

        // Interactive execution requires approval
        let outcome = bus
            .execute(
                "host_write_file",
                &args,
                ToolExecutionMode::Interactive,
                &mut app,
            )
            .unwrap();

        match outcome {
            ToolBusOutcome::RequiresApproval { approval } => {
                assert_eq!(approval.tool_name, "host_write_file");
                assert_eq!(approval.card.kind, "approval");
                assert!(!target_file.exists()); // Not written yet!

                // Approve once
                let exec_outcome = bus.approve_once(&approval.id, &mut app).unwrap();
                match exec_outcome {
                    ToolBusOutcome::Executed { output, .. } => {
                        assert_eq!(output["written"], true);
                        assert_eq!(output["bytes"], 24);
                    }
                    _ => panic!("Expected Executed outcome after approval"),
                }

                // Verify file exists on disk
                assert_eq!(fs::read_to_string(&target_file).unwrap(), "Updated content from Bob");
            }
            _ => panic!("Expected RequiresApproval outcome"),
        }
    }

    #[test]
    fn test_host_run_command_gates_interactive_and_routine() {
        let td = tempdir().unwrap();
        let ws_dir = td.path().join("workspace");
        fs::create_dir_all(&ws_dir).unwrap();

        let mut store = WorkspaceGrantStore::open(td.path()).unwrap();
        store
            .authorize_workspace(ws_dir.to_str().unwrap(), "Command WS", false, 1000)
            .unwrap();

        let mut bus = BobToolBus::with_approval_store(td.path()).unwrap();
        let mut app = LocalApp::open(&td.path().join("kernel.sqlite")).unwrap();

        let args = json!({
            "command": "echo hello",
            "cwd": ws_dir.to_str().unwrap()
        });

        // 1. Unattended routine blocks Irreversible action
        let routine_res = bus.execute(
            "host_run_command",
            &args,
            ToolExecutionMode::UnattendedRoutine,
            &mut app,
        );
        assert_eq!(routine_res.unwrap_err(), "IRREVERSIBLE_ACTION_BLOCKED_IN_ROUTINE");

        // 2. Interactive execution requires human approval card
        let outcome = bus
            .execute(
                "host_run_command",
                &args,
                ToolExecutionMode::Interactive,
                &mut app,
            )
            .unwrap();

        match outcome {
            ToolBusOutcome::RequiresApproval { approval } => {
                assert_eq!(approval.tool_name, "host_run_command");
                assert_eq!(approval.card.risk_level, ActionRiskLevel::Irreversible);

                let exec_outcome = bus.approve_once(&approval.id, &mut app).unwrap();
                match exec_outcome {
                    ToolBusOutcome::Executed { output, .. } => {
                        assert_eq!(output["exit_code"], 0);
                        assert!(output["stdout"].as_str().unwrap().contains("hello"));
                    }
                    _ => panic!("Expected Executed after command approval"),
                }
            }
            _ => panic!("Expected RequiresApproval outcome"),
        }
    }

    #[test]
    fn test_host_offline_queue_reconcile() {
        let td = tempdir().unwrap();
        let ws_dir = td.path().join("workspace");
        fs::create_dir_all(&ws_dir).unwrap();
        let test_file = ws_dir.join("test.txt");
        fs::write(&test_file, b"content").unwrap();

        let mut store = WorkspaceGrantStore::open(td.path()).unwrap();
        store
            .authorize_workspace(ws_dir.to_str().unwrap(), "Offline WS", false, 1000)
            .unwrap();

        let mut queue_store = HostOfflineQueueStore::open(td.path()).unwrap();

        // Enqueue read-only task
        let q1 = queue_store
            .enqueue_for_pc(
                "node-pc-1",
                "host_read_file",
                json!({ "file_path": test_file.to_str().unwrap() }),
                1000,
            )
            .unwrap();
        assert_eq!(q1.status, "waiting_for_pc");

        // Enqueue side-effect task
        let q2 = queue_store
            .enqueue_for_pc(
                "node-pc-1",
                "host_write_file",
                json!({ "file_path": ws_dir.join("new.txt").to_str().unwrap(), "content": "123" }),
                1001,
            )
            .unwrap();
        assert_eq!(q2.status, "waiting_for_pc");

        // Reconcile when PC online
        let mut bus = BobToolBus::with_approval_store(td.path()).unwrap();
        let mut app = LocalApp::open(&td.path().join("kernel.sqlite")).unwrap();

        let reconciled = HostOfflineQueueStore::reconcile_when_pc_online(
            td.path(),
            "node-pc-1",
            &mut bus,
            &mut app,
            2000,
        )
        .unwrap();

        assert_eq!(reconciled.len(), 2);
        assert_eq!(reconciled[0].status, "completed"); // read-only executed
        assert_eq!(reconciled[1].status, "pending_approval"); // write gated behind approval!
    }
}
