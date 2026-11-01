//! `BP4-06`: RPC bridge for native `Claude Code` execution adapter (`claude_harness`)
//! and unified session directory routing inside `bob-node`.

use crate::context_capsule;
use bob_runtime::claude_harness::{
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

            let receipt = claude_harness::dispatch_prompt(
                data_dir,
                &grant,
                request_id,
                text,
                ClaudeDispatchOptions {
                    parent_version_id,
                    detached_hold_ms,
                    capsule_wire_override,
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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{handle_rpc_command, BobNodeConfig, NodeContext};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn write_fake_claude(dir: &Path) -> std::path::PathBuf {
        let script = dir.join("fake_node_claude.py");
        let code = r##"import json, os, sys
from pathlib import Path

args = sys.argv[1:]
if "--version" in args:
    print("2.1.0 (Claude Code fake fixture)")
    sys.exit(0)

prompt = ""
resume_id = None
i = 0
while i < len(args):
    if args[i] == "-p" and i + 1 < len(args):
        prompt = args[i + 1]
        i += 2
    elif args[i] == "--resume" and i + 1 < len(args):
        resume_id = args[i + 1]
        i += 2
    else:
        i += 1

cwd = Path.cwd()
md = cwd / "DESIGN.md"
if not md.exists() or "V1" not in md.read_text(encoding="utf-8"):
    md.write_text("# Design Spec V1\n\nInitial unified sidebar spec.\n", encoding="utf-8")
    ver = "V1"
else:
    md.write_text("# Design Spec V2\n\nUpdated after cross-tool DSH handoff.\n", encoding="utf-8")
    ver = "V2"

sid = resume_id or "claude-node-sess-777"
has_capsule = "bob-task-card-v1" in prompt
reply = f"Completed {ver} (sid={sid}, capsule={has_capsule})"
print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": reply, "session_id": sid}))
"##;
        fs::write(&script, code).unwrap();
        script
    }

    #[test]
    fn unified_dsh_and_claude_rpc_handle_capsule_handoff_and_v1_v2_deliverables() {
        let tmp = tempdir().unwrap();
        let data_dir = tmp.path().join("node-data");
        let ws_dir = tmp.path().join("shared-ws");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&ws_dir).unwrap();
        let git = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&ws_dir)
                .args(args)
                .output()
                .unwrap()
                .status
                .success());
        };
        git(&["init"]);
        fs::write(ws_dir.join("README.md"), "# Shared Project\n\nContext baseline.\n").unwrap();
        git(&["add", "README.md"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "init shared workspace",
        ]);
        let script = write_fake_claude(tmp.path());

        // Configure both BP4-04 workspace-context-grants.json and BP4-06 claude-harness-grants.json
        fs::write(
            data_dir.join("workspace-context-grants.json"),
            serde_json::to_string_pretty(&json!([{
                "id": "ctx-shared-1",
                "project_id": "proj-shared",
                "workspace_id": "ws-shared",
                "owner_instance": "node-x1t",
                "root": ws_dir.to_string_lossy(),
                "sources": ["README.md"],
                "connection_id": "node-x1t",
                "session_id": "claude:shared-design"
            }]))
            .unwrap(),
        )
        .unwrap();

        fs::write(
            data_dir.join("claude-harness-grants.json"),
            serde_json::to_string_pretty(&json!([{
                "id": "grant-shared-1",
                "connection_id": "node-x1t",
                "session_id": "claude:shared-design",
                "title": "Shared Design Session",
                "cwd": ws_dir.to_string_lossy(),
                "claude_bin": "python",
                "claude_args_prefix": [script.to_string_lossy()],
                "output_format": "json",
                "prefer_tmux": false,
                "timeout_ms": 30000,
                "deliverable_kind": "markdown",
                "deliverable_paths": ["DESIGN.md"],
                "auth_configured": true
            }]))
            .unwrap(),
        )
        .unwrap();

        let mut ctx = NodeContext::open(&BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: data_dir.clone(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();

        // 1. Unified sidebar `app_dsh_list` returns the Claude session with toolLabel == "Claude"
        let list_val = handle_rpc_command(
            &mut ctx,
            "app_dsh_list",
            &json!({ "connectionId": "node-x1t" }),
        )
        .unwrap();
        let arr = list_val.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "claude:shared-design");
        assert_eq!(arr[0]["harness"], "claude");
        assert_eq!(arr[0]["toolLabel"], "Claude");

        // 2. Round 1 via unified `app_dsh_prompt` -> attaches initial bob-task-card-v1 capsule & produces V1
        let prompt_v1 = handle_rpc_command(
            &mut ctx,
            "app_dsh_prompt",
            &json!({
                "connectionId": "node-x1t",
                "sessionId": "claude:shared-design",
                "requestId": "req-node-1",
                "text": "Draft DESIGN.md V1"
            }),
        )
        .unwrap();
        assert_eq!(prompt_v1["status"], "accepted");
        assert_eq!(prompt_v1["receipt"]["route_kind"], "task_card_initial");
        assert_eq!(
            prompt_v1["receipt"]["artifact_version"]["version_id"],
            "v1"
        );
        assert!(prompt_v1["receipt"]["reply_text"]
            .as_str()
            .unwrap()
            .contains("capsule=True"));

        // 3. Round 2 direct follow-up on Claude -> uses native_resume (--resume claude-node-sess-777)
        let prompt_v2 = handle_rpc_command(
            &mut ctx,
            "app_claude_prompt",
            &json!({
                "connectionId": "node-x1t",
                "sessionId": "claude:shared-design",
                "requestId": "req-node-2",
                "parentVersionId": "v1",
                "text": "Revise DESIGN.md to V2"
            }),
        )
        .unwrap();
        assert_eq!(prompt_v2["receipt"]["route_kind"], "native_resume");
        assert_eq!(
            prompt_v2["receipt"]["artifact_version"]["version_id"],
            "v2"
        );
        assert_eq!(
            prompt_v2["receipt"]["artifact_version"]["parent_version_id"],
            "v1"
        );

        // 4. Simulate DSH modifying the same workspace, then follow up on Claude -> uses task_card_handoff
        bob_runtime::claude_harness::record_workspace_harness_activity(
            &data_dir,
            &ws_dir.to_string_lossy(),
            "dsh",
            "dsh-peer-session",
        )
        .unwrap();
        let prompt_v3 = handle_rpc_command(
            &mut ctx,
            "app_claude_prompt",
            &json!({
                "connectionId": "node-x1t",
                "sessionId": "claude:shared-design",
                "requestId": "req-node-3",
                "parentVersionId": "v2",
                "text": "Continue after DSH update"
            }),
        )
        .unwrap();
        assert_eq!(prompt_v3["receipt"]["route_kind"], "task_card_handoff");
        assert_eq!(
            prompt_v3["receipt"]["artifact_version"]["version_id"],
            "v3"
        );

        // 5. Unified `app_dsh_page` returns messages in exact DshSessions.vue schema
        let page = handle_rpc_command(
            &mut ctx,
            "app_dsh_page",
            &json!({
                "connectionId": "node-x1t",
                "sessionId": "claude:shared-design"
            }),
        )
        .unwrap();
        assert_eq!(page["address"]["session_id"], "claude:shared-design");
        assert_eq!(page["harness"], "claude");
        assert_eq!(page["messages"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn live_x1t_claude_code_v1_to_v2_two_round_closed_loop() {
        if std::env::var("BOB_LIVE_X1T_CLAUDE_TEST").as_deref() != Ok("1") {
            return;
        }
        let tmp = tempdir().unwrap();
        let data_dir = tmp.path().join("node-data");
        let local_mirror_dir = tmp.path().join("x1t-mirror-ws");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&local_mirror_dir).unwrap();

        let remote_ws = "/tmp/bob-bp406-x1t-live";
        let _ = std::process::Command::new("ssh")
            .args([
                "-o",
                "BatchMode=yes",
                "x1t",
                &format!("rm -rf {remote_ws} && mkdir -p {remote_ws}"),
            ])
            .status();

        fs::write(
            data_dir.join("claude-harness-grants.json"),
            serde_json::to_string_pretty(&json!([{
                "id": "grant-x1t-live",
                "connection_id": "node-x1t",
                "session_id": "claude:x1t-live-verify",
                "title": "X1T Live Claude Code Session",
                "cwd": local_mirror_dir.to_string_lossy(),
                "claude_bin": "/home/bobbik/.local/bin/claude",
                "output_format": "json",
                "prefer_tmux": false,
                "timeout_ms": 90000,
                "deliverable_kind": "markdown",
                "deliverable_paths": ["LIVE_NOTE.md"],
                "permission_mode": "acceptEdits",
                "max_budget_usd": 0.40,
                "max_revisions": 3,
                "remote_ssh_target": "x1t",
                "remote_cwd": remote_ws
            }]))
            .unwrap(),
        )
        .unwrap();

        let mut ctx = NodeContext::open(&BobNodeConfig {
            bind_addr: "127.0.0.1:0".to_owned(),
            data_dir: data_dir.clone(),
            static_dir: None,
            legacy_db_path: None,
        })
        .unwrap();

        // 1. Probe real capabilities on X1T
        let caps = handle_rpc_command(
            &mut ctx,
            "app_claude_capabilities",
            &json!({ "connectionId": "node-x1t" }),
        )
        .unwrap();
        assert_eq!(caps[0]["available"], true);
        assert!(caps[0]["version"]
            .as_str()
            .unwrap_or("")
            .contains("Claude Code"));

        // 2. Round 1 (V1): Create LIVE_NOTE.md on X1T
        let r1 = handle_rpc_command(
            &mut ctx,
            "app_claude_prompt",
            &json!({
                "connectionId": "node-x1t",
                "sessionId": "claude:x1t-live-verify",
                "requestId": "req-x1t-live-v1",
                "text": "Create a file named LIVE_NOTE.md in the current directory with the exact heading '# X1T Live V1' and one short sentence confirming V1 baseline. Then reply briefly."
            }),
        )
        .unwrap();
        let sid_v1 = r1["receipt"]["native_session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(!sid_v1.is_empty());
        assert_eq!(r1["receipt"]["artifact_version"]["version_id"], "v1");
        assert_eq!(
            r1["receipt"]["artifact_version"]["verification"]["status"],
            "passed"
        );
        let v1_sha = r1["receipt"]["artifact_version"]["bundle_sha256"]
            .as_str()
            .unwrap()
            .to_owned();

        // 3. Round 2 (V2): Native --resume follow-up on the exact same session_id
        let r2 = handle_rpc_command(
            &mut ctx,
            "app_claude_prompt",
            &json!({
                "connectionId": "node-x1t",
                "sessionId": "claude:x1t-live-verify",
                "requestId": "req-x1t-live-v2",
                "parentVersionId": "v1",
                "text": "Update LIVE_NOTE.md so the top heading is '# X1T Live V2' and add a second line 'Resumed native session for V2.' Then reply briefly."
            }),
        )
        .unwrap();
        assert_eq!(r2["receipt"]["route_kind"], "native_resume");
        assert_eq!(r2["receipt"]["native_session_id"].as_str(), Some(sid_v1.as_str()));
        assert_eq!(r2["receipt"]["artifact_version"]["version_id"], "v2");
        assert_eq!(r2["receipt"]["artifact_version"]["parent_version_id"], "v1");
        assert_eq!(
            r2["receipt"]["artifact_version"]["verification"]["status"],
            "passed"
        );
        let v2_sha = r2["receipt"]["artifact_version"]["bundle_sha256"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_ne!(v1_sha, v2_sha);

        // Verify V1 snapshot remained immutable
        let v1_preview = handle_rpc_command(
            &mut ctx,
            "app_claude_preview",
            &json!({
                "connectionId": "node-x1t",
                "sessionId": "claude:x1t-live-verify",
                "path": "LIVE_NOTE.md",
                "versionId": "v1"
            }),
        )
        .unwrap();
        assert!(v1_preview["content"].as_str().unwrap().contains("# X1T Live V1"));

        // Write live evidence summary
        if let Ok(ev_path) = std::env::var("BOB_LIVE_X1T_EVIDENCE_OUT") {
            let _ = fs::write(
                ev_path,
                serde_json::to_string_pretty(&json!({
                    "capabilities": caps[0],
                    "round1_receipt": r1["receipt"],
                    "round2_receipt": r2["receipt"],
                    "v1_preview_sha256": v1_preview["sha256"],
                }))
                .unwrap(),
            );
        }
    }
}
