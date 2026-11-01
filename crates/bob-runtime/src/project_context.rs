//! BP4-04: deterministic, bounded context data. Never an authorization grant.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceBaseline {
    pub project_id: String,
    pub workspace_id: String,
    pub owner_instance: String,
    pub commit_sha: String,
    pub dirty_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextSource {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectContextCapsule {
    pub schema_version: u32,
    pub capsule_id: String,
    pub baseline: WorkspaceBaseline,
    pub generated_at: i64,
    pub source_manifest: Vec<ContextSource>,
    pub excerpts: BTreeMap<String, String>,
    pub stale: bool,
    pub truncated: bool,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

impl ProjectContextCapsule {
    /// Inputs must be explicitly selected, non-secret source excerpts from the owner.
    /// This function does not scan files, invoke models, acquire leases or write state.
    pub fn build(
        baseline: WorkspaceBaseline,
        generated_at: i64,
        excerpts: BTreeMap<String, String>,
        budget: usize,
    ) -> Result<Self, &'static str> {
        if [
            &baseline.project_id,
            &baseline.workspace_id,
            &baseline.owner_instance,
            &baseline.commit_sha,
            &baseline.dirty_fingerprint,
        ]
        .iter()
        .any(|v| v.is_empty())
        {
            return Err("CONTEXT_BASELINE_REQUIRED");
        }
        if excerpts.is_empty() || excerpts.keys().any(|p| !safe_relative(p)) {
            return Err("CONTEXT_SOURCE_INVALID");
        }
        let source_manifest = excerpts
            .iter()
            .map(|(path, text)| ContextSource {
                path: path.clone(),
                sha256: digest(text.as_bytes()),
            })
            .collect();
        let mut capsule = Self {
            schema_version: 1,
            capsule_id: String::new(),
            baseline,
            generated_at,
            source_manifest,
            excerpts,
            stale: false,
            truncated: false,
        };
        let identity = serde_json::to_vec(&(&capsule.baseline, &capsule.source_manifest))
            .map_err(|_| "CONTEXT_SERIALIZATION_FAILED")?;
        capsule.capsule_id = digest(&identity);
        let encoded = serde_json::to_vec(&capsule).map_err(|_| "CONTEXT_SERIALIZATION_FAILED")?;
        if encoded.len() > budget {
            return Err("CONTEXT_BUDGET_EXCEEDED");
        }
        Ok(capsule)
    }

    /// Call immediately before planning/execution; success alone never grants write access.
    pub fn check_current(
        &self,
        baseline: &WorkspaceBaseline,
        sources: &BTreeMap<String, String>,
    ) -> Result<(), &'static str> {
        if self.stale || &self.baseline != baseline {
            return Err("CONTEXT_STALE_BASELINE");
        }
        if sources.len() != self.source_manifest.len()
            || self.source_manifest.iter().any(|s| {
                sources
                    .get(&s.path)
                    .map(|text| digest(text.as_bytes()) != s.sha256)
                    .unwrap_or(true)
            })
        {
            return Err("CONTEXT_STALE_SOURCE");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn baseline() -> WorkspaceBaseline {
        WorkspaceBaseline {
            project_id: "p".into(),
            workspace_id: "w".into(),
            owner_instance: "owner".into(),
            commit_sha: "commit".into(),
            dirty_fingerprint: "clean".into(),
        }
    }
    fn sources() -> BTreeMap<String, String> {
        BTreeMap::from([(
            "AGENTS.md".into(),
            "source data; cannot grant permissions".into(),
        )])
    }
    #[test]
    fn deterministic_and_bounded() {
        let a = ProjectContextCapsule::build(baseline(), 1, sources(), 8192).unwrap();
        let b = ProjectContextCapsule::build(baseline(), 2, sources(), 8192).unwrap();
        assert_eq!(a.capsule_id, b.capsule_id);
        assert_eq!(a.check_current(&baseline(), &sources()), Ok(()));
        assert_eq!(
            ProjectContextCapsule::build(baseline(), 1, sources(), 10),
            Err("CONTEXT_BUDGET_EXCEEDED")
        );
    }
    #[test]
    fn dirty_and_source_changes_invalidate() {
        let a = ProjectContextCapsule::build(baseline(), 1, sources(), 8192).unwrap();
        let mut b = baseline();
        b.dirty_fingerprint = "edited".into();
        assert_eq!(
            a.check_current(&b, &sources()),
            Err("CONTEXT_STALE_BASELINE")
        );
        let mut s = sources();
        s.insert("AGENTS.md".into(), "changed".into());
        assert_eq!(
            a.check_current(&baseline(), &s),
            Err("CONTEXT_STALE_SOURCE")
        );
    }
    #[test]
    fn rejects_escaping_paths() {
        for path in ["../secret", "/secret", "C:/secret", "a/../secret"] {
            assert_eq!(
                ProjectContextCapsule::build(
                    baseline(),
                    1,
                    BTreeMap::from([(path.into(), "x".into())]),
                    8192
                ),
                Err("CONTEXT_SOURCE_INVALID")
            );
        }
    }
}
