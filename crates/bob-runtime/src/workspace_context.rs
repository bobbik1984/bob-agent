//! Explicit-source workspace context extraction. No recursive discovery or model calls.
use crate::project_context::{digest, safe_relative, ProjectContextCapsule, WorkspaceBaseline};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

pub struct WorkspaceContextRequest {
    pub project_id: String,
    pub workspace_id: String,
    pub owner_instance: String,
    pub root: PathBuf,
    pub sources: Vec<String>,
    pub generated_at: i64,
    pub budget: usize,
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, &'static str> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|_| "CONTEXT_GIT_UNAVAILABLE")?;
    if !output.status.success() {
        return Err("CONTEXT_GIT_FAILED");
    }
    if output.stdout.len() > 1024 * 1024 {
        return Err("CONTEXT_STATUS_TOO_LARGE");
    }
    Ok(output.stdout)
}

fn read_source(root: &Path, name: &str) -> Result<String, &'static str> {
    if !safe_relative(name)
        || name.split('/').any(|p| {
            p.starts_with('.')
                || matches!(
                    p.to_ascii_lowercase().as_str(),
                    "credentials" | "secrets" | "node_modules" | "target"
                )
        })
    {
        return Err("CONTEXT_SOURCE_DENIED");
    }
    let mut path = root.to_path_buf();
    for part in name.split('/') {
        path.push(part);
        let meta = fs::symlink_metadata(&path).map_err(|_| "CONTEXT_SOURCE_MISSING")?;
        if meta.file_type().is_symlink() {
            return Err("CONTEXT_SOURCE_SYMLINK");
        }
    }
    let resolved = path.canonicalize().map_err(|_| "CONTEXT_SOURCE_MISSING")?;
    if !resolved.starts_with(root) {
        return Err("CONTEXT_SOURCE_ESCAPE");
    }
    let file = fs::File::open(resolved).map_err(|_| "CONTEXT_SOURCE_UNREADABLE")?;
    if !file
        .metadata()
        .map_err(|_| "CONTEXT_SOURCE_UNREADABLE")?
        .is_file()
    {
        return Err("CONTEXT_SOURCE_INVALID");
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "CONTEXT_SOURCE_UNREADABLE")?;
    if bytes.len() > 65536 {
        return Err("CONTEXT_SOURCE_TOO_LARGE");
    }
    String::from_utf8(bytes).map_err(|_| "CONTEXT_SOURCE_NOT_UTF8")
}

/// Fingerprint covers Git status metadata and the explicitly selected source contents.
/// It does NOT attest unrelated dirty file bytes or acquire workspace write ownership.
pub fn extract(request: &WorkspaceContextRequest) -> Result<ProjectContextCapsule, &'static str> {
    if request.sources.is_empty() || request.sources.len() > 8 {
        return Err("CONTEXT_SOURCE_COUNT");
    }
    let root = request
        .root
        .canonicalize()
        .map_err(|_| "CONTEXT_ROOT_MISSING")?;
    let commit = git(&root, &["rev-parse", "--verify", "HEAD"])?;
    let status = git(
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
    )?;
    let mut full = BTreeMap::new();
    for name in &request.sources {
        if full
            .insert(name.clone(), read_source(&root, name)?)
            .is_some()
        {
            return Err("CONTEXT_SOURCE_DUPLICATE");
        }
    }
    let mut fingerprint = status.clone();
    for (path, text) in &full {
        fingerprint.extend_from_slice(path.as_bytes());
        fingerprint.push(0);
        fingerprint.extend_from_slice(digest(text.as_bytes()).as_bytes());
    }
    let baseline = WorkspaceBaseline {
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        owner_instance: request.owner_instance.clone(),
        commit_sha: String::from_utf8(commit.clone())
            .map_err(|_| "CONTEXT_GIT_FAILED")?
            .trim()
            .into(),
        dirty_fingerprint: digest(&fingerprint),
    };
    let excerpts = full
        .iter()
        .map(|(path, text)| (path.clone(), text.chars().take(384).collect::<String>()))
        .collect();
    let mut capsule =
        ProjectContextCapsule::build(baseline, request.generated_at, excerpts, request.budget)?;
    capsule.truncated = full
        .iter()
        .any(|(path, text)| capsule.excerpts[path].len() < text.len());
    for source in &mut capsule.source_manifest {
        source.sha256 = digest(full[&source.path].as_bytes());
    }
    capsule.capsule_id = digest(
        &serde_json::to_vec(&(&capsule.baseline, &capsule.source_manifest))
            .map_err(|_| "CONTEXT_SERIALIZATION_FAILED")?,
    );
    if serde_json::to_vec(&capsule)
        .map_err(|_| "CONTEXT_SERIALIZATION_FAILED")?
        .len()
        > request.budget
    {
        return Err("CONTEXT_BUDGET_EXCEEDED");
    }
    // Reject a moving snapshot; never replace an earlier persisted version on failure.
    if git(&root, &["rev-parse", "--verify", "HEAD"])? != commit
        || git(
            &root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
        )? != status
    {
        return Err("CONTEXT_WORKSPACE_CHANGED");
    }
    for (path, text) in &full {
        if read_source(&root, path)? != *text {
            return Err("CONTEXT_WORKSPACE_CHANGED");
        }
    }
    Ok(capsule)
}

/// Pre-dispatch grounding hook. Caller still checks Owner lease, authorization and budget.
/// Never calls a model, resumes a session or automatically rebuilds a stale task card.
pub fn validate_for_dispatch(
    request: &WorkspaceContextRequest,
    capsule: &ProjectContextCapsule,
) -> Result<(), &'static str> {
    if capsule.stale {
        return Err("CONTEXT_STALE_BASELINE");
    }
    let current = extract(request)?;
    if current.baseline != capsule.baseline {
        return Err("CONTEXT_STALE_BASELINE");
    }
    if current.source_manifest != capsule.source_manifest
        || current.capsule_id != capsule.capsule_id
    {
        return Err("CONTEXT_STALE_SOURCE");
    }
    Ok(())
}

pub fn save(store: &Path, capsule: &ProjectContextCapsule) -> Result<(), &'static str> {
    if capsule.capsule_id.len() != 64 || !capsule.capsule_id.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("CONTEXT_ID_INVALID");
    }
    fs::create_dir_all(store).map_err(|_| "CONTEXT_STORE_FAILED")?;
    let data = serde_json::to_vec(capsule).map_err(|_| "CONTEXT_SERIALIZATION_FAILED")?;
    let target = store.join(format!("{}.json", capsule.capsule_id));
    if target.exists() {
        return if load(store, &capsule.capsule_id)?.baseline == capsule.baseline {
            Ok(())
        } else {
            Err("CONTEXT_STORE_CONFLICT")
        };
    }
    let temporary = store.join(format!(
        "{}.{}.pending",
        capsule.capsule_id,
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "CONTEXT_STORE_BUSY")?;
    let result = (|| {
        file.write_all(&data).map_err(|_| "CONTEXT_STORE_FAILED")?;
        file.sync_all().map_err(|_| "CONTEXT_STORE_FAILED")?;
        fs::rename(&temporary, &target).map_err(|_| "CONTEXT_STORE_FAILED")
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn load(store: &Path, id: &str) -> Result<ProjectContextCapsule, &'static str> {
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("CONTEXT_ID_INVALID");
    }
    let file =
        fs::File::open(store.join(format!("{id}.json"))).map_err(|_| "CONTEXT_STORE_MISSING")?;
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "CONTEXT_STORE_FAILED")?;
    if bytes.len() > 65536 {
        return Err("CONTEXT_STORE_INVALID");
    }
    let capsule: ProjectContextCapsule =
        serde_json::from_slice(&bytes).map_err(|_| "CONTEXT_STORE_INVALID")?;
    let identity = digest(
        &serde_json::to_vec(&(&capsule.baseline, &capsule.source_manifest))
            .map_err(|_| "CONTEXT_STORE_INVALID")?,
    );
    if capsule.schema_version != 1 || capsule.capsule_id != id || identity != id {
        return Err("CONTEXT_STORE_INVALID");
    }
    Ok(capsule)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_git_extract_save_read_and_dirty_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        fs::create_dir(&root).unwrap();
        assert!(Command::new("git")
            .arg("init")
            .arg(&root)
            .output()
            .unwrap()
            .status
            .success());
        fs::write(root.join("AGENTS.md"), "Explicit source data").unwrap();
        assert!(Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "AGENTS.md"])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .arg("-C")
            .arg(&root)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "fixture"
            ])
            .output()
            .unwrap()
            .status
            .success());
        let request = WorkspaceContextRequest {
            project_id: "p".into(),
            workspace_id: "w".into(),
            owner_instance: "owner".into(),
            root: root.clone(),
            sources: vec!["AGENTS.md".into()],
            generated_at: 1,
            budget: 8192,
        };
        let first = extract(&request).unwrap();
        assert_eq!(validate_for_dispatch(&request, &first), Ok(()));
        let store = dir.path().join("context");
        save(&store, &first).unwrap();
        assert_eq!(load(&store, &first.capsule_id).unwrap(), first);
        fs::write(root.join("AGENTS.md"), "modified source").unwrap();
        let second = extract(&request).unwrap();
        assert_eq!(
            validate_for_dispatch(&request, &first),
            Err("CONTEXT_STALE_BASELINE")
        );
        assert_ne!(
            first.baseline.dirty_fingerprint,
            second.baseline.dirty_fingerprint
        );
        assert_eq!(load(&store, &first.capsule_id).unwrap(), first);
        fs::remove_file(root.join("AGENTS.md")).unwrap();
        assert_eq!(extract(&request), Err("CONTEXT_SOURCE_MISSING"));
        assert!(load(&store, &first.capsule_id).is_ok());
    }
    #[test]
    fn rejects_invalid_store_identity() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path(), "../escape"), Err("CONTEXT_ID_INVALID"));
    }
}
