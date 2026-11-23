//! `BP4-06`: RPC bridge for native `Claude Code` execution adapter (`claude_harness`)
//! and unified session directory routing inside `bob-node`.

use crate::context_capsule;
use crate::claude_harness::{
    self, ClaudeDispatchOptions, ClaudeHarnessGrant,
};
use serde_json::{json, Value};
use std::path::Path;

fn resolve_grant(data_dir: &Path, args: &Value) -> Result<ClaudeHarnessGrant, &'static str> {
    if let Some(gid) = args
        .get("grantId")
        .or_else(|| args.get("grant_id"))
        .and_then(Value::as_str)
    {
        return claude_harness::find_grant_by_id(data_dir, gid);
    }
    let connection = args
        .get("connectionId")
        .or_else(|| args.get("connection_id"))
        .and_then(Value::as_str)
        .ok_or("CLAUDE_CONNECTION_REQUIRED")?;
    let session = args
        .get("sessionId")
        .or_else(|| args.get("session_id"))
        .and_then(Value::as_str)
        .ok_or("CLAUDE_SESSION_REQUIRED")?;
    claude_harness::find_grant(data_dir, connection, session)?
        .ok_or("CLAUDE_GRANT_NOT_FOUND")
}

pub fn rpc(data_dir: &Path, command: &str, args: &Value) -> Result<Value, &'static str> {
    match command {
        "app_claude_capabilities" => {
            let cid = args
                .get("connectionId")
                .or_else(|| args.get("connection_id"))
                .and_then(Value::as_str);
            let caps = claude_harness::list_capabilities(data_dir, cid)?;
            serde_json::to_value(caps).map_err(|_| "CLAUDE_CAPABILITIES_FAILED")
        }
        "app_claude_sessions" => {
            let cid = args
                .get("connectionId")
                .or_else(|| args.get("connection_id"))
                .and_then(Value::as_str);
            let sessions = claude_harness::list_unified_sessions(data_dir, cid)?;
            Ok(json!({ "sessions": sessions }))
        }
        "app_claude_history" => {
            let grant = resolve_grant(data_dir, args)?;
            let _ = claude_harness::reconcile_grant(data_dir, &grant);
            claude_harness::history_page_for_ui(data_dir, &grant)
        }
        "app_claude_prompt" => {
            let grant = resolve_grant(data_dir, args)?;
            let text = args
                .get("text")
                .or_else(|| args.get("prompt"))
                .and_then(Value::as_str)
                .ok_or("CLAUDE_TEXT_REQUIRED")?;
            let request_id = args
                .get("requestId")
                .or_else(|| args.get("request_id"))
                .and_then(Value::as_str)
                .ok_or("CLAUDE_REQUEST_REQUIRED")?;
            let parent_version_id = args
                .get("parentVersionId")
                .or_else(|| args.get("parent_version_id"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let detached_hold_ms = args
                .get("detachedHoldMs")
                .or_else(|| args.get("detached_hold_ms"))
                .and_then(Value::as_u64);

            let session_state = claude_harness::load_session_state(data_dir, &grant)?;
            let needs_capsule = session_state.native_session_id.is_none()
                || session_state.last_harness != "claude";
            let capsule_wire_override = if needs_capsule {
                let _ = context_capsule::refresh_changed(data_dir);
                let wire_after_capsule = context_capsule::before_dispatch(
                    data_dir,
                    &grant.connection_id,
                    &grant.session_id,
                    request_id,
                    text,
                )?;
                if wire_after_capsule != text {
                    Some(wire_after_capsule)
                } else {
                    None
                }
            } else {
                None
            };

            let base_wire = capsule_wire_override.as_deref().unwrap_or(text);
            let staged_wire = crate::dsh_transport::stage_attachments_for_prompt(
                data_dir,
                &grant.connection_id,
                &grant.session_id,
                grant.remote_cwd.as_deref().or(Some(grant.cwd.as_str())),
                base_wire,
                args.get("attachments"),
            );
            let final_override = if staged_wire != text { Some(staged_wire) } else { capsule_wire_override };
            let receipt = claude_harness::dispatch_prompt(
                data_dir,
                &grant,
                request_id,
                text,
                ClaudeDispatchOptions {
                    parent_version_id,
                    detached_hold_ms,
                    capsule_wire_override: final_override,
                },
            )?;
            let page = claude_harness::history_page_for_ui(data_dir, &grant)?;
            Ok(json!({
                "receipt": receipt,
                "page": page,
            }))
        }
        "app_claude_cancel" => {
            let grant = resolve_grant(data_dir, args)?;
            let request_id = args
                .get("requestId")
                .or_else(|| args.get("request_id"))
                .and_then(Value::as_str)
                .ok_or("CLAUDE_REQUEST_REQUIRED")?;
            let receipt = claude_harness::cancel_execution(data_dir, &grant, request_id)?;
            serde_json::to_value(receipt).map_err(|_| "CLAUDE_CANCEL_FAILED")
        }
        "app_claude_reconcile" => {
            let grant = resolve_grant(data_dir, args)?;
            let receipts = claude_harness::reconcile_grant(data_dir, &grant)?;
            let page = claude_harness::history_page_for_ui(data_dir, &grant)?;
            Ok(json!({
                "receipts": receipts,
                "page": page,
            }))
        }
        "app_claude_review" => {
            let grant = resolve_grant(data_dir, args)?;
            let version_id = args
                .get("versionId")
                .or_else(|| args.get("version_id"))
                .and_then(Value::as_str)
                .ok_or("CLAUDE_VERSION_REQUIRED")?;
            let verdict = args
                .get("verdict")
                .or_else(|| args.get("status"))
                .and_then(Value::as_str)
                .ok_or("CLAUDE_REVIEW_VERDICT_INVALID")?;
            let note = args.get("note").and_then(Value::as_str);
            let record = claude_harness::review_artifact_version(
                data_dir,
                &grant.id,
                version_id,
                verdict,
                note,
            )?;
            serde_json::to_value(record).map_err(|_| "CLAUDE_REVIEW_FAILED")
        }
        "app_claude_artifacts" => {
            let grant = resolve_grant(data_dir, args)?;
            let versions = claude_harness::load_artifact_versions(data_dir, &grant.id)?;
            Ok(json!({ "versions": versions }))
        }
        "app_claude_preview" => {
            let grant = resolve_grant(data_dir, args)?;
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or("CLAUDE_PATH_INVALID")?;
            let version_id = args
                .get("versionId")
                .or_else(|| args.get("version_id"))
                .and_then(Value::as_str);
            claude_harness::preview_deliverable_file(data_dir, &grant, path, version_id)
        }
        _ => Err("UNKNOWN_CLAUDE_COMMAND"),
    }
}

pub fn try_handle_dsh_command(
    data_dir: &Path,
    command: &str,
    args: &Value,
) -> Result<Option<Value>, &'static str> {
    let Some(connection) = args.get("connectionId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(session) = args.get("sessionId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(grant) = claude_harness::find_grant(data_dir, connection, session)? else {
        return Ok(None);
    };

    match command {
        "app_dsh_page" | "app_dsh_history" => {
            let _ = claude_harness::reconcile_grant(data_dir, &grant);
            claude_harness::history_page_for_ui(data_dir, &grant).map(Some)
        }
        "app_dsh_preview" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or("CLAUDE_PATH_INVALID")?;
            let version_id = args
                .get("versionId")
                .or_else(|| args.get("version_id"))
                .and_then(Value::as_str);
            claude_harness::preview_deliverable_file(data_dir, &grant, path, version_id).map(Some)
        }
        "app_dsh_receipt" => {
            let request_id = args
                .get("requestId")
                .and_then(Value::as_str)
                .ok_or("CLAUDE_REQUEST_REQUIRED")?;
            let receipts = claude_harness::reconcile_grant(data_dir, &grant)?;
            if let Some(found) = receipts.into_iter().find(|r| r.request_id == request_id) {
                let dsh_status = if found.status == "completed" || found.status == "running" {
                    "accepted"
                } else {
                    found.status.as_str()
                };
                return Ok(Some(json!({
                    "status": dsh_status,
                    "execution_status": found.status,
                    "session_id": grant.session_id,
                    "request_id": request_id,
                    "receipt": found,
                })));
            }
            Ok(Some(json!({
                "status": "missing",
                "session_id": grant.session_id,
                "request_id": request_id,
            })))
        }
        "app_dsh_prompt" => {
            let mut out = rpc(data_dir, "app_claude_prompt", args)?;
            if let Some(obj) = out.as_object_mut() {
                let exec_status = obj
                    .get("receipt")
                    .and_then(|r| r.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or("completed")
                    .to_owned();
                obj.insert(
                    "status".to_owned(),
                    json!(if exec_status == "completed" || exec_status == "running" {
                        "accepted"
                    } else {
                        exec_status.as_str()
                    }),
                );
                obj.insert("session_id".to_owned(), json!(grant.session_id));
                if let Some(req_id) = args.get("requestId").and_then(Value::as_str) {
                    obj.insert("request_id".to_owned(), json!(req_id));
                }
            }
            Ok(Some(out))
        }
        _ => Ok(None),
    }
}

pub fn merge_claude_into_dsh_list(
    data_dir: &Path,
    connection_id: &str,
    dsh_res: Result<Value, &'static str>,
) -> Result<Value, &'static str> {
    let claude_sessions = claude_harness::list_unified_sessions(data_dir, Some(connection_id))?;
    match dsh_res {
        Ok(mut val) => {
            if let Some(arr) = val.as_array_mut() {
                for item in arr.iter_mut() {
                    if let Some(obj) = item.as_object_mut() {
                        obj.entry("harness".to_owned())
                            .or_insert_with(|| json!("dsh"));
                        obj.entry("toolLabel".to_owned())
                            .or_insert_with(|| json!("DSH"));
                    }
                }
                arr.extend(claude_sessions);
            } else if let Some(arr) = val.get_mut("sessions").and_then(Value::as_array_mut) {
                for item in arr.iter_mut() {
                    if let Some(obj) = item.as_object_mut() {
                        obj.entry("harness".to_owned())
                            .or_insert_with(|| json!("dsh"));
                        obj.entry("toolLabel".to_owned())
                            .or_insert_with(|| json!("DSH"));
                    }
                }
                arr.extend(claude_sessions);
            }
            Ok(val)
        }
        Err(e) => {
            if !claude_sessions.is_empty() {
                Ok(json!(claude_sessions))
            } else {
                Err(e)
            }
        }
    }
}


