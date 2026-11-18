//! BOB-D-014 Step 2: Daily Review draft generation. Network work runs outside the kernel lock.
use super::*;
use bob_runtime::inbox_triage::{parse_triage_batch, sparks_from_inbox, triage_prompt, ProjectInput};

fn batch_json(b: &bob_kernel::spark_inbox::TriageBatch) -> Result<Value, &'static str> {
    let batch: Value = serde_json::from_str(&b.payload_json).map_err(|_| "TRIAGE_READ_FAILED")?;
    Ok(json!({
        "id": b.id,
        "status": b.status,
        "model_ref": b.model_ref,
        "created_at": b.created_at,
        "batch": batch,
    }))
}

fn invoke_triage_model(
    runtime: &ModelHubRuntime,
    profile: &bob_kernel::direct_provider::DirectProviderProfile,
    prompt: &str,
) -> Result<(String, String), &'static str> {
    let main_profile = runtime.resolve_role_profile("main").unwrap_or_else(|_| profile.clone());
    let flash_profile = runtime.resolve_role_profile("flash").unwrap_or_else(|_| profile.clone());
    let mut reply: Result<(String, String), &'static str> = Err("PROVIDER_UNCONFIGURED");
    let mut tried = Vec::new();
    for prof in [&main_profile, &flash_profile, profile] {
        let key = (prof.base_url.clone(), prof.model_id.clone());
        if tried.contains(&key) {
            continue;
        }
        tried.push(key);
        match runtime.invoke_direct_model(prof, &[], prompt) {
            Ok(raw) => {
                reply = Ok((raw, prof.model_id.clone()));
                break;
            }
            Err(e) => reply = Err(e),
        }
    }
    reply
}

fn collect_project_evidence(project_ref_id: &str, project_label: &str) -> String {
    let Some(reg_path) = bob_runtime::project_registry::discover_registry_path() else {
        return String::new();
    };
    let Ok(raw) = std::fs::read_to_string(&reg_path) else {
        return String::new();
    };
    let root_dir = reg_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let entries = bob_runtime::project_registry::parse_project_registry_yaml(&raw);
    let target = entries.into_iter().find(|e| {
        e.id.eq_ignore_ascii_case(project_ref_id)
            || e.name.eq_ignore_ascii_case(project_label)
            || e.id.eq_ignore_ascii_case(project_label)
    });
    let Some(entry) = target else {
        return String::new();
    };
    let Some(rel) = entry.path.as_deref() else {
        return String::new();
    };
    let proj_dir = if std::path::Path::new(rel).is_absolute() {
        std::path::PathBuf::from(rel)
    } else {
        root_dir.join(rel)
    };
    if !proj_dir.exists() {
        return String::new();
    }
    let mut sections = Vec::new();
    for fname in ["todo.md", "progress.yaml"] {
        let fpath = proj_dir.join(fname);
        if let Ok(content) = std::fs::read_to_string(&fpath) {
            let clipped: String = content.chars().take(4000).collect();
            sections.push(format!("### {fname}\n{clipped}"));
        }
    }
    if let Ok(out) = std::process::Command::new("git")
        .args(["-C", &proj_dir.to_string_lossy(), "log", "-n", "25", "--oneline"])
        .output()
    {
        if out.status.success() {
            let log = String::from_utf8_lossy(&out.stdout);
            if !log.trim().is_empty() {
                sections.push(format!("### recent git log\n{}", log.trim()));
            }
        }
    }
    sections.join("\n\n")
}

/// `app_inbox_triage_draft { regenerate?: bool }`
/// Returns the open draft immediately when one exists (no model call). Otherwise, or when
/// `regenerate` is true, asks the configured `main` model for a new draft and persists it.
/// Never writes to any destination: confirmation is a separate, user-driven step.
pub fn draft(state: &Arc<Mutex<NodeContext>>, args: &Value) -> Result<Value, &'static str> {
    let regenerate = args["regenerate"].as_bool().unwrap_or(false);
    let (sparks, projects, schedules, dir, profile) = {
        let ctx = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        if !regenerate {
            if let Some(open) = ctx.app.latest_triage_draft().map_err(|_| "TRIAGE_READ_FAILED")? {
                return batch_json(&open);
            }
        }
        let items = ctx.app.list_inbox(Some("unsorted")).map_err(|_| "INBOX_LIST_FAILED")?;
        let sparks = sparks_from_inbox(&items);
        if sparks.is_empty() {
            return Ok(json!({ "id": null, "status": "empty", "batch": null }));
        }
        let projects: Vec<ProjectInput> = ctx
            .app
            .catalog_snapshot()
            .map(|snap| {
                snap.projects
                    .into_iter()
                    .filter(|p| p.status == "active" && p.id != "proj-default")
                    .take(80)
                    .map(|p| ProjectInput { id: p.id, title: p.title })
                    .collect()
            })
            .unwrap_or_default();
        let schedules = bob_runtime::assistant::schedule_catalog_for_prompt(&ctx.app);
        let profile = ctx
            .app
            .direct_provider_profile()
            .map_err(|_| "PROVIDER_READ_FAILED")?
            .ok_or("PROVIDER_UNCONFIGURED")?;
        (sparks, projects, schedules, ctx.runtime.data_dir.clone(), profile)
    };

    let runtime = ModelHubRuntime::with_file_vault(dir)?;
    let now = now_millis();
    let prompt = triage_prompt(&sparks, &projects, &schedules, now);
    let (raw, model_ref) = invoke_triage_model(&runtime, &profile, &prompt)?;
    let batch = parse_triage_batch(&raw, &sparks, &projects)?;
    let payload = serde_json::to_string(&batch).map_err(|_| "TRIAGE_SAVE_FAILED")?;
    let saved = state
        .lock()
        .map_err(|_| "NODE_LOCK_POISONED")?
        .app
        .save_triage_draft(&format!("tb-{now}"), &payload, Some(&model_ref), now)
        .map_err(|_| "TRIAGE_SAVE_FAILED")?;
    batch_json(&saved)
}

/// `app_inbox_check_implemented { projectId?: string, captureIds?: string[] }`
/// Checks whether sorted inbox items appear implemented based on local project workspace
/// (`todo.md`, `progress.yaml`, recent `git log`) and updates `resolution_status` + `resolution_note`.
pub fn check_implemented(state: &Arc<Mutex<NodeContext>>, args: &Value) -> Result<Value, &'static str> {
    let project_filter = args.get("projectId").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let capture_ids: Vec<String> = args
        .get("captureIds")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();

    let (items, dir, profile) = {
        let ctx = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        let all_sorted = ctx.app.list_inbox(Some("sorted")).map_err(|_| "INBOX_LIST_FAILED")?;
        let filtered: Vec<_> = all_sorted
            .into_iter()
            .filter(|item| {
                if !capture_ids.is_empty() {
                    return capture_ids.iter().any(|id| id == &item.capture.id);
                }
                if let Some(pf) = project_filter {
                    return item.destination_kind.as_deref() == Some("project_todo")
                        && (item.destination_ref_id.as_deref() == Some(pf)
                            || item.destination_label.as_deref() == Some(pf));
                }
                item.destination_kind.as_deref() == Some("project_todo")
            })
            .take(25)
            .collect();
        if filtered.is_empty() {
            return Ok(json!({ "checked": 0, "results": [] }));
        }
        let profile = ctx
            .app
            .direct_provider_profile()
            .map_err(|_| "PROVIDER_READ_FAILED")?
            .ok_or("PROVIDER_UNCONFIGURED")?;
        (filtered, ctx.runtime.data_dir.clone(), profile)
    };

    let mut evidence_by_project: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for item in &items {
        let ref_id = item.destination_ref_id.as_deref().unwrap_or("");
        let label = item.destination_label.as_deref().unwrap_or("");
        let key = format!("{ref_id}|{label}");
        if !evidence_by_project.contains_key(&key) {
            evidence_by_project.insert(key, collect_project_evidence(ref_id, label));
        }
    }

    let mut prompt_items = Vec::new();
    for item in &items {
        let ref_id = item.destination_ref_id.as_deref().unwrap_or("");
        let label = item.destination_label.as_deref().unwrap_or("");
        let key = format!("{ref_id}|{label}");
        let has_ev = evidence_by_project.get(&key).is_some_and(|s| !s.is_empty());
        prompt_items.push(json!({
            "capture_id": item.capture.id,
            "project": if !label.is_empty() { label } else { ref_id },
            "content": item.capture.content_ref,
            "has_local_workspace_evidence": has_ev,
        }));
    }

    let mut evidence_blocks = Vec::new();
    for (k, v) in &evidence_by_project {
        if !v.is_empty() {
            evidence_blocks.push(format!("## Project {k}\n{v}"));
        }
    }
    let evidence_text = if evidence_blocks.is_empty() {
        "No local project repository or todo.md was found on this host.".to_owned()
    } else {
        evidence_blocks.join("\n\n")
    };

    let prompt = format!(
        "You are verifying whether previously triaged project ideas/todos have already been implemented.\n\
         Rules:\n\
         1. Only return status \"implemented\" if there is concrete evidence in the project's todo.md (completed/checked), progress.yaml, or recent git log below.\n\
         2. If there is no matching evidence or no local workspace evidence on this host, return status \"pending\" and state honestly in Chinese why (e.g. \"当前节点未找到该项目本地仓库/完成记录，暂标为待实现\" or \"未在近期提交/todo中命中对应实现\").\n\
         3. Respond ONLY with a valid JSON array of objects: [{{\"capture_id\": \"...\", \"status\": \"implemented\" | \"pending\", \"note\": \"简短中文依据(<=60字)\"}}].\n\n\
         Items to check:\n{}\n\n\
         Workspace evidence:\n{}",
        serde_json::to_string_pretty(&prompt_items).unwrap_or_default(),
        evidence_text
    );

    let runtime = ModelHubRuntime::with_file_vault(dir)?;
    let (raw, _model_ref) = invoke_triage_model(&runtime, &profile, &prompt)?;
    let trimmed = raw.trim();
    let json_slice = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim().trim_end_matches("```").trim())
        .unwrap_or(trimmed);
    let parsed: Vec<Value> = serde_json::from_str(json_slice).map_err(|_| "CHECK_PARSE_FAILED")?;

    let now = now_millis();
    let mut results = Vec::new();
    let mut guard = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
    for entry in parsed {
        let Some(cid) = entry.get("capture_id").and_then(Value::as_str) else {
            continue;
        };
        let status = match entry.get("status").and_then(Value::as_str) {
            Some("implemented") => "implemented",
            _ => "pending",
        };
        let note = entry.get("note").and_then(Value::as_str);
        if let Ok(updated) = guard.app.set_inbox_resolution(cid, status, note, now) {
            results.push(json!({
                "capture_id": updated.capture_id,
                "resolution_status": updated.resolution_status,
                "resolution_note": updated.resolution_note,
            }));
        }
    }
    Ok(json!({
        "checked": results.len(),
        "results": results,
    }))
}
