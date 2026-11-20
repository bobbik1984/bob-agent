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

// ---------------------------------------------------------------------------
// BOB-D-014 Step 4: Confirmed Project todo.md Writeback + Daily 21:00 Auto-Triage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTodoWriteback {
    pub capture_id: String,
    pub project_ref_id: String,
    pub project_label: String,
    pub content: String,
    pub created_at: i64,
    pub status: String, // "waiting_for_pc"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WritebackQueueData {
    pub items: Vec<PendingTodoWriteback>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTriageSchedule {
    #[serde(default = "default_true_flag")]
    pub enabled: bool,
    #[serde(default = "default_triage_hour")]
    pub hour: u32,
    #[serde(default)]
    pub minute: u32,
    #[serde(default = "default_utc_offset")]
    pub utc_offset_hours: i32,
    #[serde(default)]
    pub last_run_date: Option<String>,
}

fn default_true_flag() -> bool {
    true
}
fn default_triage_hour() -> u32 {
    21
}
fn default_utc_offset() -> i32 {
    8
}

impl Default for DailyTriageSchedule {
    fn default() -> Self {
        Self {
            enabled: true,
            hour: 21,
            minute: 0,
            utc_offset_hours: 8,
            last_run_date: None,
        }
    }
}

fn resolve_project_todo_path(project_ref_id: &str, project_label: &str) -> Option<std::path::PathBuf> {
    let reg_path = bob_runtime::project_registry::discover_registry_path()?;
    let raw = std::fs::read_to_string(&reg_path).ok()?;
    let root_dir = reg_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let entries = bob_runtime::project_registry::parse_project_registry_yaml(&raw);
    let entry = entries.into_iter().find(|e| {
        (!project_ref_id.is_empty() && e.id.eq_ignore_ascii_case(project_ref_id))
            || (!project_label.is_empty()
                && (e.name.eq_ignore_ascii_case(project_label)
                    || e.id.eq_ignore_ascii_case(project_label)))
    })?;
    let rel = entry.path.as_deref()?;
    let proj_dir = if std::path::Path::new(rel).is_absolute() {
        std::path::PathBuf::from(rel)
    } else {
        root_dir.join(rel)
    };
    if !proj_dir.is_dir() {
        return None;
    }
    Some(proj_dir.join("todo.md"))
}

fn append_todo_line_idempotent(
    todo_path: &std::path::Path,
    capture_id: &str,
    content: &str,
) -> Result<&'static str, &'static str> {
    let marker = format!("<!-- bob-capture:{capture_id} -->");
    let existing = if todo_path.is_file() {
        std::fs::read_to_string(todo_path).map_err(|_| "TODO_WRITEBACK_READ_FAILED")?
    } else {
        "# TODO\n\n".to_string()
    };
    if existing.contains(&marker) {
        return Ok("already_present");
    }
    let clean_line = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if clean_line.is_empty() {
        return Ok("skipped_empty");
    }
    let mut updated = existing;
    if !updated.ends_with('\n') && !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(&format!("- [ ] {clean_line} {marker}\n"));
    let tmp = todo_path.with_extension("md.tmp");
    std::fs::write(&tmp, updated.as_bytes()).map_err(|_| "TODO_WRITEBACK_WRITE_FAILED")?;
    std::fs::rename(&tmp, todo_path).map_err(|_| "TODO_WRITEBACK_WRITE_FAILED")?;
    Ok("written")
}

/// Writes a user-confirmed `project_todo` inbox capture into the target project's `todo.md`,
/// or queues it into `inbox_todo_writeback_queue.json` (`waiting_for_pc`) if the repo is offline/not mounted here.
pub fn writeback_or_queue_project_todo(
    data_dir: &std::path::Path,
    capture_id: &str,
    project_ref_id: &str,
    project_label: &str,
    content: &str,
    now_ms: i64,
) -> Value {
    if let Some(todo_path) = resolve_project_todo_path(project_ref_id, project_label) {
        if let Ok(status) = append_todo_line_idempotent(&todo_path, capture_id, content) {
            return json!({
                "status": status,
                "path": todo_path.to_string_lossy(),
            });
        }
    }

    let q_path = data_dir.join("inbox_todo_writeback_queue.json");
    let mut q: WritebackQueueData = if q_path.is_file() {
        std::fs::read_to_string(&q_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        WritebackQueueData::default()
    };
    if !q.items.iter().any(|it| it.capture_id == capture_id) {
        q.items.push(PendingTodoWriteback {
            capture_id: capture_id.to_string(),
            project_ref_id: project_ref_id.to_string(),
            project_label: project_label.to_string(),
            content: content.to_string(),
            created_at: now_ms,
            status: "waiting_for_pc".to_string(),
        });
        if let Ok(raw) = serde_json::to_string_pretty(&q) {
            let tmp = q_path.with_extension("json.tmp");
            if std::fs::write(&tmp, raw.as_bytes()).is_ok() {
                let _ = std::fs::rename(&tmp, &q_path);
            }
        }
    }
    json!({
        "status": "waiting_for_pc",
        "queued": true,
    })
}

/// Reconciles any queued `waiting_for_pc` inbox-to-todo writebacks whose local workspace is now mounted.
pub fn reconcile_writeback_queue(data_dir: &std::path::Path) -> Value {
    let q_path = data_dir.join("inbox_todo_writeback_queue.json");
    if !q_path.is_file() {
        return json!({ "flushed": 0, "remaining": 0 });
    }
    let mut q: WritebackQueueData = std::fs::read_to_string(&q_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut flushed = 0usize;
    let mut remaining = Vec::new();
    for item in q.items.drain(..) {
        if let Some(todo_path) = resolve_project_todo_path(&item.project_ref_id, &item.project_label)
        {
            if append_todo_line_idempotent(&todo_path, &item.capture_id, &item.content).is_ok() {
                flushed += 1;
                continue;
            }
        }
        remaining.push(item);
    }
    q.items = remaining;
    if let Ok(raw) = serde_json::to_string_pretty(&q) {
        let tmp = q_path.with_extension("json.tmp");
        if std::fs::write(&tmp, raw.as_bytes()).is_ok() {
            let _ = std::fs::rename(&tmp, &q_path);
        }
    }
    json!({ "flushed": flushed, "remaining": q.items.len() })
}

fn local_date_and_hm(now_ms: i64, offset_hours: i32) -> (String, u32, u32) {
    let total_secs = now_ms / 1000 + (offset_hours as i64) * 3600;
    let days = total_secs.div_euclid(86400);
    let rem = total_secs.rem_euclid(86400) as u32;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (format!("{year:04}-{m:02}-{d:02}"), hour, minute)
}

/// Checks if the daily scheduled inbox triage (default 21:00 UTC+8) is due.
/// Never auto-sorts items: only creates a single coalesced draft batch for 100% human tap-to-confirm
/// and fires a Web Push reminder.
pub fn tick_daily_triage(state: &Arc<Mutex<NodeContext>>, data_dir: &std::path::Path, now_ms: i64) {
    let sched_path = data_dir.join("inbox_triage_schedule.json");
    let mut sched: DailyTriageSchedule = if sched_path.is_file() {
        std::fs::read_to_string(&sched_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        DailyTriageSchedule::default()
    };
    if !sched.enabled {
        return;
    }
    let (today, hour, minute) = local_date_and_hm(now_ms, sched.utc_offset_hours);
    if sched.last_run_date.as_deref() == Some(today.as_str()) {
        return;
    }
    if hour < sched.hour || (hour == sched.hour && minute < sched.minute) {
        return;
    }

    // Check quickly under lock if there are unsorted items and no open draft yet.
    let should_run = {
        let Ok(guard) = state.try_lock() else {
            return;
        };
        let has_open = guard
            .app
            .latest_triage_draft()
            .ok()
            .flatten()
            .is_some();
        if has_open {
            false
        } else {
            guard
                .app
                .list_inbox(Some("unsorted"))
                .map(|items| !items.is_empty())
                .unwrap_or(false)
        }
    };

    // Mark today's slot checked regardless of empty/open to avoid polling every tick.
    sched.last_run_date = Some(today.clone());
    if let Ok(raw) = serde_json::to_string_pretty(&sched) {
        let tmp = sched_path.with_extension("json.tmp");
        if std::fs::write(&tmp, raw.as_bytes()).is_ok() {
            let _ = std::fs::rename(&tmp, &sched_path);
        }
    }

    if !should_run {
        return;
    }

    if draft(state, &json!({ "regenerate": false })).is_ok() {
        push_channels::notify_approval_wakeup_async(
            data_dir.to_path_buf(),
            format!("inbox-daily-triage:{today}"),
        );
    }
}

#[cfg(test)]
mod step4_tests {
    use super::*;

    #[test]
    fn todo_writeback_is_idempotent_and_queues_when_project_offline() {
        let tmp = tempfile::tempdir().unwrap();
        let todo_file = tmp.path().join("todo.md");

        let s1 = append_todo_line_idempotent(&todo_file, "cap-1", "Add dark mode polish").unwrap();
        assert_eq!(s1, "written");
        let s2 = append_todo_line_idempotent(&todo_file, "cap-1", "Add dark mode polish").unwrap();
        assert_eq!(s2, "already_present");
        let content = std::fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content.matches("bob-capture:cap-1").count(), 1);

        // Offline project queues cleanly as waiting_for_pc
        let data_dir = tmp.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let q_res = writeback_or_queue_project_todo(
            &data_dir,
            "cap-offline",
            "nonexistent-pc-project",
            "Offline PC Project",
            "Sync local design assets",
            1_795_000_000_000,
        );
        assert_eq!(q_res["status"], "waiting_for_pc");
        let rec = reconcile_writeback_queue(&data_dir);
        assert_eq!(rec["remaining"], 1);
    }

    #[test]
    fn local_date_and_hm_computes_utc8_civil_time() {
        // 2026-11-20 13:30:00 UTC = 2026-11-20 21:30:00 UTC+8
        let ms = 1795181400000_i64;
        let (date, h, m) = local_date_and_hm(ms, 8);
        assert_eq!(date, "2026-11-20");
        assert_eq!((h, m), (21, 30));
    }
}
