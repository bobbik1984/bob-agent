//! Actual web send path. Network work runs outside the global kernel lock.
use super::*;
use bob_kernel::direct_provider::DirectExecutionOutcome;
use bob_runtime::dsh_session::SessionAddress;

pub fn execute(state: &Arc<Mutex<NodeContext>>, args: &Value) -> Result<Value, &'static str> {
    let request = args["requestId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .ok_or("REQUEST_ID_REQUIRED")?;
    let (prepared, profile, dir, schedules, local_candidates) = {
        let mut ctx = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        let assistant = ctx
            .app
            .assistant_conversation(now_millis())
            .map_err(|_| "ASSISTANT_OPEN_FAILED")?;
        let history = ctx
            .app
            .history(&assistant)
            .map_err(|_| "ASSISTANT_OPEN_FAILED")?;
        if !history
            .iter()
            .any(|m| m.role == "user" && m.request_id.as_deref() == Some(request))
        {
            return Err("ASSISTANT_REQUEST_MISMATCH");
        }
        let prep = ctx
            .app
            .prepare_direct_execution(request, now_millis())
            .map_err(|_| "EXECUTION_PREPARE_FAILED")?;
        if !prep.can_start {
            let e = ctx
                .app
                .local_executions(&assistant)
                .map_err(|_| "EXECUTION_READ_FAILED")?
                .into_iter()
                .find(|e| e.id == prep.execution_id)
                .ok_or("EXECUTION_READ_FAILED")?;
            return serde_json::to_value(e).map_err(|_| "EXECUTION_READ_FAILED");
        }
        let profile = ctx
            .app
            .direct_provider_profile()
            .map_err(|_| "PROVIDER_READ_FAILED")?
            .ok_or("PROVIDER_UNCONFIGURED")?;
        let sched_catalog = bob_runtime::assistant::schedule_catalog_for_prompt(&ctx.app);
        let local_convs: Vec<Value> = ctx
            .app
            .catalog_snapshot()
            .map(|snap| {
                snap.conversations
                    .into_iter()
                    .filter(|c| !c.id.starts_with("assistant:"))
                    .take(30)
                    .map(|c| {
                        json!({
                            "connection_id": "local",
                            "harness": "local",
                            "session_id": c.id,
                            "title": c.title.chars().take(120).collect::<String>(),
                            "device": "local",
                            "workspace": c.project_title,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        (
            prep,
            profile,
            ctx.runtime.data_dir.clone(),
            sched_catalog,
            local_convs,
        )
    };
    let work = (|| -> Result<String, &'static str> {
        let runtime = ModelHubRuntime::with_file_vault(dir)?;
        let hub = runtime.load_hub_state();
        let nodes: Vec<_> = hub["remoteNodes"]
            .as_array()
            .map(|n| n.iter().filter(|n| n["enabled"] == true).collect())
            .unwrap_or_default();
        let mut complete = nodes.len() <= 4;
        let mut candidates = local_candidates;
        for node in nodes.iter().take(4) {
            let Some(id) = node["id"].as_str() else {
                complete = false;
                continue;
            };
            if id == "local" {
                let _ = bob_runtime::agy_directory::ensure_local_grant_registered(&runtime.data_dir, "local");
            }
            let merged = claude_rpc::merge_claude_into_dsh_list(
                &runtime.data_dir,
                id,
                runtime.dsh_client(id).and_then(|c| c.list()),
            );
            let mut node_items: Vec<Value> = match merged {
                Ok(mut list) => {
                    if let Ok(extra) = bob_runtime::agy_directory::list(&runtime.data_dir, id) {
                        if let Some(rows) = list.as_array_mut() {
                            rows.extend(extra);
                        }
                    }
                    list.as_array().cloned().unwrap_or_default()
                }
                Err(_) => bob_runtime::agy_directory::list(&runtime.data_dir, id).unwrap_or_default(),
            };
            if node_items.is_empty() && id != "local" {
                if let Ok(remote_rows) = bob_runtime::node_pairing::forward_dsh_rpc_to_remote_node(
                    &runtime,
                    id,
                    "app_dsh_list",
                    &json!({"connectionId": id}),
                ) {
                    if let Some(arr) = remote_rows.as_array() {
                        node_items = arr.clone();
                    }
                }
            }
            for item in node_items {
                if candidates.len() >= 200 {
                    complete = false;
                    break;
                }
                let harness = item["harness"].as_str().unwrap_or_else(|| {
                    if item["source_kind"].as_str() == Some("claude_native") {
                        "claude"
                    } else if item["id"].as_str().is_some_and(|s| s.starts_with("agy:")) {
                        "agy"
                    } else {
                        "dsh"
                    }
                });
                let mut cand = json!({
                    "connection_id": id,
                    "harness": harness,
                    "session_id": item["id"],
                    "title": item["title"].as_str().unwrap_or("").chars().take(120).collect::<String>(),
                    "device": node["name"],
                    "workspace": item["workspace"]
                });
                if let Some(sc) = item.get("stepCount").and_then(Value::as_i64) {
                    cand["turns"] = json!(sc);
                }
                if let Some(fp) = item.get("firstPrompt").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                    cand["first_prompt"] = json!(fp.chars().take(100).collect::<String>());
                }
                if let Some(ls) = item.get("lastSnippet").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                    cand["last_snippet"] = json!(ls.chars().take(100).collect::<String>());
                }
                candidates.push(cand);
            }
        }
        let user = prepared
            .messages
            .last()
            .ok_or("ASSISTANT_REQUEST_MISMATCH")?
            .content
            .as_str();
        let prompt = bob_runtime::assistant::routing_prompt_with_context(
            user,
            &candidates,
            complete,
            &schedules,
            now_millis(),
        );
        let flash_profile = runtime
            .resolve_role_profile("flash")
            .unwrap_or_else(|_| profile.clone());
        let main_profile = runtime
            .resolve_role_profile("main")
            .unwrap_or_else(|_| profile.clone());
        let vision_profile = runtime
            .resolve_role_profile("vision")
            .unwrap_or_else(|_| profile.clone());
        let chosen_profile = if user.contains("[image:") || user.contains("<image>") {
            &main_profile
        } else {
            &flash_profile
        };
        let prior_len = prepared.messages.len().saturating_sub(1);
        let context_turns: Vec<_> = prepared.messages[..prior_len]
            .iter()
            .filter(|m| {
                !m.content.contains("上线连通检查：仅在当前助理对话回答")
                    && m.content.trim() != "助理已就绪。"
                    && !m.content.contains("当前对话被限定为")
            })
            .cloned()
            .collect();
        let mut raw_res: Result<String, &'static str> = Err("PROVIDER_UNCONFIGURED");
        let mut tried = Vec::new();
        for prof in [chosen_profile, &main_profile, &vision_profile, &profile] {
            let key = (prof.base_url.clone(), prof.model_id.clone());
            if tried.contains(&key) {
                continue;
            }
            tried.push(key);
            match runtime.invoke_direct_model(prof, &context_turns, &prompt) {
                Ok(out) => {
                    raw_res = Ok(out);
                    break;
                }
                Err(e) => {
                    raw_res = Err(e);
                }
            }
        }
        let raw = raw_res?;
        let proposal = bob_runtime::assistant::parse_proposal(&raw, &candidates, complete)?;
        let mut reply_text = proposal.text.clone();
        if proposal.kind == "dispatch" {
            if user.len() > 32000 {
                return Err("DSH_TEXT_REQUIRED");
            }
            let connection = proposal
                .connection_id
                .as_deref()
                .ok_or("ASSISTANT_TARGET_UNVERIFIED")?;
            let session = proposal
                .session_id
                .as_deref()
                .ok_or("ASSISTANT_TARGET_UNVERIFIED")?;
            let candidate = candidates
                .iter()
                .find(|c| c["connection_id"] == connection && c["session_id"] == session)
                .ok_or("ASSISTANT_TARGET_UNVERIFIED")?;
            // Recheck enabled connection and native identity immediately before the only send.
            let client = runtime.dsh_client(connection)?;
            if !client
                .list()?
                .as_array()
                .is_some_and(|items| items.iter().any(|s| s["id"] == session))
            {
                return Err("ASSISTANT_TARGET_UNVERIFIED");
            }
            state
                .lock()
                .map_err(|_| "NODE_LOCK_POISONED")?
                .app
                .assistant_bind_dispatch(
                    request,
                    connection,
                    session,
                    candidate["title"].as_str().unwrap_or(""),
                    now_millis(),
                )
                .map_err(|_| "ASSISTANT_LINK_SAVE_FAILED")?;
            let wire = context_capsule::before_dispatch(
                &runtime.data_dir,
                connection,
                session,
                request,
                user,
            )?;
            let sent = bob_runtime::dsh_transport::dispatch_once(
                &runtime.data_dir,
                connection,
                session,
                user,
                request,
                || client.prompt(session, &wire, request),
            );
            if let Ok(receipt) = sent {
                if receipt["status"] == "accepted"
                    && receipt["request_id"] == request
                    && receipt["session_id"] == session
                {
                    state
                        .lock()
                        .map_err(|_| "NODE_LOCK_POISONED")?
                        .app
                        .assistant_record_status(request, "accepted", now_millis())
                        .map_err(|_| "ASSISTANT_LINK_SAVE_FAILED")?;
                }
            }
            // Unknown receipt stays unknown. Poll only observes; it never resends.
        } else {
            let mut ctx = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
            if let Some(receipt_note) = bob_runtime::assistant::apply_personal_proposal(
                &mut ctx.app,
                request,
                &proposal,
                now_millis(),
            )? {
                reply_text = format!("{}\n\n{}", reply_text.trim(), receipt_note);
            }
        }
        Ok(reply_text)
    })();
    let outcome = match work {
        Ok(text) => DirectExecutionOutcome::Completed {
            text,
            provider_reported_model: None,
            input_tokens: None,
            output_tokens: None,
        },
        Err(reason) => DirectExecutionOutcome::Failed {
            reason: reason.to_owned(),
        },
    };
    let result = state
        .lock()
        .map_err(|_| "NODE_LOCK_POISONED")?
        .app
        .finish_direct_execution(&prepared.execution_id, outcome, now_millis())
        .map_err(|_| "EXECUTION_SAVE_FAILED")?;
    serde_json::to_value(result).map_err(|_| "EXECUTION_READ_FAILED")
}

pub fn poll(state: &Arc<Mutex<NodeContext>>, args: &Value) -> Result<Value, &'static str> {
    let conversation = args["sessionId"].as_str().ok_or("SESSION_REQUIRED")?;
    let (links, dir) = {
        let ctx = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
        (
            ctx.app
                .assistant_links(conversation)
                .map_err(|_| "ASSISTANT_LINK_READ_FAILED")?,
            ctx.runtime.data_dir.clone(),
        )
    };
    let runtime = ModelHubRuntime::with_file_vault(dir)?;
    let mut errors = Vec::new();
    for link in links
        .iter()
        .filter(|l| matches!(l.status.as_str(), "accepted" | "unknown"))
        .take(4)
    {
        let result = runtime.dsh_client(&link.connection_id).and_then(|c| {
            c.history(SessionAddress {
                connection_id: link.connection_id.clone(),
                session_id: link.native_session_id.clone(),
            })
        });
        match result {
            Ok(history) => {
                let matched: Vec<_> = history
                    .request_results
                    .iter()
                    .filter(|r| r.request_id == link.request_id)
                    .collect();
                if matched.len() != 1 {
                    continue;
                }
                let r = matched[0];
                let mut ctx = state.lock().map_err(|_| "NODE_LOCK_POISONED")?;
                match r.status.as_str() {
                    "completed" if !r.text.trim().is_empty() => ctx
                        .app
                        .assistant_return_result(
                            &link.request_id,
                            &link.connection_id,
                            &link.native_session_id,
                            &r.text,
                            &r.sequences,
                            now_millis(),
                        )
                        .map_err(|_| "ASSISTANT_RESULT_SAVE_FAILED")?,
                    "error" | "blocked" | "cancelled" => ctx
                        .app
                        .assistant_record_status(&link.request_id, "failed", now_millis())
                        .map_err(|_| "ASSISTANT_LINK_SAVE_FAILED")?,
                    "ambiguous" => ctx
                        .app
                        .assistant_record_status(&link.request_id, "ambiguous", now_millis())
                        .map_err(|_| "ASSISTANT_LINK_SAVE_FAILED")?,
                    _ => {}
                }
            }
            Err(code) => errors.push(json!({"request_id":link.request_id,"code":code})),
        }
    }
    let links = state
        .lock()
        .map_err(|_| "NODE_LOCK_POISONED")?
        .app
        .assistant_links(conversation)
        .map_err(|_| "ASSISTANT_LINK_READ_FAILED")?;
    Ok(json!({"links":links,"errors":errors}))
}
