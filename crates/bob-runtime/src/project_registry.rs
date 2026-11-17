//! Formal project SSOT loader (`project_registry.yaml`) for Bob Runtime / Node / App.
//!
//! Ensures active project catalogs stay aligned with the workspace Project Meta SSOT
//! (`project_registry.yaml`) and avoids collecting scratch/test buckets as formal projects.

use bob_kernel::app::LocalApp;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormalProjectEntry {
    pub id: String,
    pub name: String,
    pub stage: String,
    pub path: Option<String>,
    pub bob_project: Option<bool>,
}

impl FormalProjectEntry {
    pub fn is_excluded_from_bob(&self) -> bool {
        self.bob_project == Some(false)
            || matches!(
                self.stage.to_ascii_lowercase().as_str(),
                "archived" | "runtime" | "ops"
            )
    }
}

fn parse_optional_bool(val: &str) -> Option<bool> {
    match val.to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn strip_inline_comment_and_unquote(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" || trimmed == "~" {
        return String::new();
    }
    if let Some(after_open) = trimmed.strip_prefix('"') {
        let mut out = String::new();
        let mut escaped = false;
        for ch in after_open.chars() {
            if escaped {
                match ch {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    other => out.push(other),
                }
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                break;
            }
            out.push(ch);
        }
        return out.trim().to_owned();
    }
    if let Some(after_open) = trimmed.strip_prefix('\'') {
        if let Some(end_idx) = after_open.find('\'') {
            return after_open[..end_idx].trim().to_owned();
        }
        return after_open.trim().to_owned();
    }
    let mut prev_ws = true;
    let mut cut = trimmed.len();
    for (idx, ch) in trimmed.char_indices() {
        if ch == '#' && prev_ws {
            cut = idx;
            break;
        }
        prev_ws = ch.is_whitespace();
    }
    trimmed[..cut].trim().to_owned()
}

/// Parse the top-level `projects:` list from `project_registry.yaml` without external YAML crates.
pub fn parse_project_registry_yaml(content: &str) -> Vec<FormalProjectEntry> {
    let mut entries = Vec::new();
    let mut in_projects = false;
    let mut current: Option<FormalProjectEntry> = None;

    for raw_line in content.lines() {
        let trimmed_start = raw_line.trim_start();
        if trimmed_start.is_empty() || trimmed_start.starts_with('#') {
            continue;
        }
        let indent = raw_line.len() - trimmed_start.len();
        if !in_projects {
            if indent == 0 && trimmed_start.trim_end() == "projects:" {
                in_projects = true;
            }
            continue;
        }
        if indent == 0 && !trimmed_start.starts_with('-') {
            break;
        }
        if indent <= 2 && trimmed_start.starts_with("- ") {
            if let Some(prev) = current.take() {
                if !prev.id.is_empty() && !prev.name.is_empty() {
                    entries.push(prev);
                }
            }
            let mut next = FormalProjectEntry {
                id: String::new(),
                name: String::new(),
                stage: "dev".to_owned(),
                path: None,
                bob_project: None,
            };
            let rest = trimmed_start[2..].trim_start();
            if let Some((k, v)) = rest.split_once(':') {
                let val = strip_inline_comment_and_unquote(v);
                match k.trim() {
                    "id" => next.id = val,
                    "name" => next.name = val,
                    "stage" if !val.is_empty() => next.stage = val,
                    "path" if !val.is_empty() => next.path = Some(val),
                    "bob_project" => next.bob_project = parse_optional_bool(&val),
                    "runtime_only" if parse_optional_bool(&val) == Some(true) => {
                        next.bob_project = Some(false);
                    }
                    _ => {}
                }
            }
            current = Some(next);
            continue;
        }
        if indent <= 4 {
            if let Some(ref mut cur) = current {
                if let Some((k, v)) = trimmed_start.split_once(':') {
                    let val = strip_inline_comment_and_unquote(v);
                    match k.trim() {
                        "id" if !val.is_empty() => cur.id = val,
                        "name" if !val.is_empty() => cur.name = val,
                        "stage" if !val.is_empty() => cur.stage = val,
                        "path" if !val.is_empty() => cur.path = Some(val),
                        "bob_project" => cur.bob_project = parse_optional_bool(&val),
                        "runtime_only" if parse_optional_bool(&val) == Some(true) => {
                            cur.bob_project = Some(false);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if let Some(prev) = current.take() {
        if !prev.id.is_empty() && !prev.name.is_empty() {
            entries.push(prev);
        }
    }
    entries
}

/// Discover `project_registry.yaml` using portable environment variables (`BOB_PROJECT_REGISTRY_PATH`,
/// `GEMINI_WORKSPACE`, `HOME`, `USERPROFILE`) without hardcoding drive letters.
pub fn discover_registry_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("BOB_PROJECT_REGISTRY_PATH") {
        let p = PathBuf::from(explicit.trim());
        if !explicit.trim().is_empty() && p.is_file() {
            return Some(p);
        }
    }
    if let Ok(ws) = std::env::var("GEMINI_WORKSPACE") {
        let trimmed = ws.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed).join("project_registry.yaml");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    for env_key in ["HOME", "USERPROFILE"] {
        if let Ok(home) = std::env::var(env_key) {
            let trimmed = home.trim();
            if !trimmed.is_empty() {
                let p = PathBuf::from(trimmed)
                    .join("work")
                    .join("common")
                    .join("project_registry.yaml");
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Sync parsed formal projects from `project_registry.yaml` into `LocalApp`.
/// Non-archived entries become `active` projects so both catalog & inbox triage accept them;
/// `archived`, `runtime`/`ops`, or `bob_project: false` entries become `archived` tombstones.
pub fn sync_registry_file(app: &mut LocalApp, path: &Path, now: i64) -> Result<usize, &'static str> {
    let raw = std::fs::read_to_string(path).map_err(|_| "PROJECT_REGISTRY_READ_FAILED")?;
    let entries = parse_project_registry_yaml(&raw);
    let mut synced = 0usize;
    for (idx, entry) in entries.iter().enumerate() {
        if entry.is_excluded_from_bob() {
            app.upsert_formal_project(&entry.id, &entry.name, "archived", now, now)
                .map_err(|_| "PROJECT_REGISTRY_SYNC_FAILED")?;
        } else {
            let seed_ts = if entry.id == "bob_agent" {
                now + 1000
            } else if entry.stage.eq_ignore_ascii_case("dev") {
                now - (idx as i64)
            } else {
                now - 10_000 - (idx as i64)
            };
            app.upsert_formal_project(&entry.id, &entry.name, "active", seed_ts, now)
                .map_err(|_| "PROJECT_REGISTRY_SYNC_FAILED")?;
        }
        synced += 1;
    }
    Ok(synced)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_YAML: &str = r#"# Gemini 项目注册表
projects:

  - id: quant_lab

    stage: dev

    name: Quant Lab

    path: quant_lab

    keywords:
      - 量化
      - id: not_a_project

  - id: bob_agent_legacy

    stage: archived

    name: "bob-agent (legacy v0.9)"

    path: D:\Code\bob-agent

  - id: bob_agent

    stage: dev

    name: bob-agent # primary workspace

    path: ~/work/code/bob-agent-next

  - id: o2_analysis

    stage: dev

    name: "O₂ 氧气舱"

    path: o2_analysis

  - id: bedtime_story

    stage: completed

    name: Bedtime Story

    path: ~/work/code/bedtime_story

ideas:
  - id: ignored_idea
    name: Should Not Become Project
"#;

    #[test]
    fn parse_registry_extracts_projects_and_ignores_ideas_or_nested_lists() {
        let parsed = parse_project_registry_yaml(SAMPLE_YAML);
        assert_eq!(parsed.len(), 5);
        assert_eq!(parsed[0].id, "quant_lab");
        assert_eq!(parsed[0].name, "Quant Lab");
        assert_eq!(parsed[0].stage, "dev");

        assert_eq!(parsed[1].id, "bob_agent_legacy");
        assert_eq!(parsed[1].stage, "archived");
        assert_eq!(parsed[1].name, "bob-agent (legacy v0.9)");

        assert_eq!(parsed[2].id, "bob_agent");
        assert_eq!(parsed[2].name, "bob-agent");

        assert_eq!(parsed[3].id, "o2_analysis");
        assert_eq!(parsed[3].name, "O₂ 氧气舱");

        assert_eq!(parsed[4].id, "bedtime_story");
        assert_eq!(parsed[4].stage, "completed");
    }

    #[test]
    fn sync_registry_is_idempotent_and_preserves_user_created_projects() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("kernel.sqlite");
        let reg = dir.path().join("project_registry.yaml");
        std::fs::write(&reg, SAMPLE_YAML).unwrap();

        let mut app = LocalApp::open(&db).unwrap();
        app.create_project("proj-user-custom", "My Custom Idea", 500)
            .unwrap();
        app.create_project("bob_agent_legacy", "bob-agent (legacy v0.9)", 500)
            .unwrap();

        let count1 = sync_registry_file(&mut app, &reg, 1_000_000).unwrap();
        assert_eq!(count1, 5);
        let snap1 = app.catalog_snapshot().unwrap();
        assert_eq!(snap1.projects[0].id, "bob_agent");
        assert_eq!(snap1.projects[0].title, "bob-agent");
        assert!(snap1.projects.iter().any(|p| p.id == "proj-user-custom"));
        assert!(!snap1.projects.iter().any(|p| p.id == "bob_agent_legacy"));
        assert!(snap1.projects.iter().any(|p| p.id == "bedtime_story"));

        // Second sync is idempotent and does not bump versions or timestamps.
        let count2 = sync_registry_file(&mut app, &reg, 2_000_000).unwrap();
        assert_eq!(count2, 5);
        let snap2 = app.catalog_snapshot().unwrap();
        assert_eq!(snap1.projects, snap2.projects);
    }

    #[test]
    fn bob_project_false_and_runtime_stage_archive_previously_active_entries() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("kernel.sqlite");
        let reg = dir.path().join("project_registry.yaml");
        let yaml = r#"projects:
  - id: quant_bot
    stage: dev
    bob_project: false  # /opt runtime daemon
    name: Quant Bot
    path: ~/work/code/quant_bot

  - id: ops_daemon
    stage: runtime
    name: Ops Daemon

  - id: bob_agent
    stage: dev
    name: bob-agent
"#;
        std::fs::write(&reg, yaml).unwrap();

        let mut app = LocalApp::open(&db).unwrap();
        app.upsert_formal_project("quant_bot", "Quant Bot", "active", 500, 500)
            .unwrap();
        app.upsert_formal_project("ops_daemon", "Ops Daemon", "active", 500, 500)
            .unwrap();

        let count = sync_registry_file(&mut app, &reg, 1_000_000).unwrap();
        assert_eq!(count, 3);
        let snap = app.catalog_snapshot().unwrap();
        assert!(snap.projects.iter().any(|p| p.id == "bob_agent"));
        assert!(!snap.projects.iter().any(|p| p.id == "quant_bot"));
        assert!(!snap.projects.iter().any(|p| p.id == "ops_daemon"));
    }
}
