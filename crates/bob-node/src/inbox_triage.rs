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
                    .filter(|p| p.status == "active")
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
    let main_profile = runtime.resolve_role_profile("main").unwrap_or_else(|_| profile.clone());
    let flash_profile = runtime.resolve_role_profile("flash").unwrap_or_else(|_| profile.clone());
    let mut reply: Result<(String, String), &'static str> = Err("PROVIDER_UNCONFIGURED");
    let mut tried = Vec::new();
    for prof in [&main_profile, &flash_profile, &profile] {
        let key = (prof.base_url.clone(), prof.model_id.clone());
        if tried.contains(&key) {
            continue;
        }
        tried.push(key);
        match runtime.invoke_direct_model(prof, &[], &prompt) {
            Ok(raw) => {
                reply = Ok((raw, prof.model_id.clone()));
                break;
            }
            Err(e) => reply = Err(e),
        }
    }
    let (raw, model_ref) = reply?;
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
