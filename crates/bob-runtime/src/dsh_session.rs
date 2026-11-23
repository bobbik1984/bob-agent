//! DSH native identity and history projection. Transport and dispatch are host-owned.
//! Never equate imported Bob projects with native DSH sessions.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAddress {
    pub connection_id: String,
    pub session_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub sequence: i64,
    pub role: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}
fn extract_timestamp_ms(record: &Value, event: &Value) -> Option<i64> {
    for candidate in [
        &event["time"],
        &event["timestamp"],
        &event["createdAt"],
        &event["created_at"],
        &event["ts"],
        &record["time"],
        &record["timestamp"],
        &record["createdAt"],
        &record["created_at"],
        &event["data"]["timestamp"],
        &event["data"]["createdAt"],
    ] {
        if let Some(n) = candidate.as_i64().filter(|&n| n > 0) {
            return Some(if n < 1_000_000_000_000 && n > 1_000_000_000 { n * 1000 } else { n });
        }
    }
    None
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub address: SessionAddress,
    pub messages: Vec<Message>,
    pub workspace: Option<String>,
    pub partial: bool,
    pub omitted: usize,
    #[serde(default)]
    pub request_results: Vec<NativeRequestResult>,
}
/// Project pending DSH questions (`open` and `continued`) from projections and legacy records,
/// suppressing `continued` items whose reply is already queued in the agent inbox.
pub(crate) fn pending_questions(snapshot: &Value) -> Vec<Value> {
    let projected = &snapshot["projections"]["values"]["userQuestions"];
    let inbox = &snapshot["projections"]["values"]["inbox"];
    let mut queued_call_ids = std::collections::BTreeSet::<&str>::new();
    for key in ["next-step", "next-turn"] {
        for msg in inbox[key].as_array().into_iter().flatten() {
            if msg["source"]["kind"].as_str() == Some("user-question-reply") {
                if let Some(cid) = msg["source"]["callId"].as_str().filter(|s| !s.is_empty()) {
                    queued_call_ids.insert(cid);
                }
            }
        }
    }
    let mut pending = std::collections::BTreeMap::<String, Value>::new();
    for row in projected["active"].as_array().into_iter().flatten().take(16) {
        let Some(id) = row["callId"].as_str().filter(|id| !id.is_empty() && id.len() <= 256) else { continue; };
        let Some(state @ ("open" | "continued")) = row["state"].as_str() else { continue; };
        if state == "continued" && queued_call_ids.contains(id) {
            continue;
        }
        if let Some(row) = question_row(id, state, &row["questions"]) {
            pending.insert(id.to_owned(), row);
        }
    }

    // Legacy blocking calls are absent from userQuestions. Reconstruct only
    // the open calls visible at this history cut, using their exact call IDs.
    let mut legacy = std::collections::BTreeMap::new();
    let mut records: Vec<_> = snapshot["records"].as_array().into_iter().flatten()
        .filter_map(|record| record.get("event")).collect();
    records.sort_by_key(|event| event["seq"].as_i64().unwrap_or(-1));
    for event in records {
        apply_question_event(&mut legacy, event);
    }
    for (id, row) in legacy {
        if !projected["settled"].as_array().is_some_and(|rows| rows.iter().any(|row| row["callId"].as_str() == Some(id.as_str()))) {
            pending.entry(id).or_insert(row);
        }
    }
    let mut budget = 64_usize * 1024;
    pending.into_values().filter(|row| {
        let Ok(bytes) = serde_json::to_vec(row) else { return false; };
        let Some(remaining) = budget.checked_sub(bytes.len()) else { return false; };
        budget = remaining;
        true
    }).take(8).collect()
}

fn question_row(id: &str, state: &str, input: &Value) -> Option<Value> {
    let items: Vec<_> = input.as_array()?.iter().take(8).filter_map(|item| {
        let question = item["question"].as_str()?.trim();
        if question.is_empty() { return None; }
        let options: Vec<_> = item["options"].as_array().into_iter().flatten().take(12)
            .filter_map(|option| {
                let label = option["label"].as_str()?.trim();
                if label.is_empty() { return None; }
                Some(serde_json::json!({
                    "label": label.chars().take(256).collect::<String>(),
                    "description": option["description"].as_str().map(|s| s.chars().take(512).collect::<String>())
                }))
            }).collect();
        let multi_select = item["multiSelect"]
            .as_bool()
            .or_else(|| item["multi_select"].as_bool())
            .unwrap_or(false);
        let mut obj = serde_json::Map::new();
        if let Some(qid) = item["id"].as_str().map(str::trim).filter(|s| !s.is_empty() && !s.chars().any(char::is_control)) {
            obj.insert("id".into(), Value::String(qid.chars().take(128).collect()));
        }
        obj.insert("question".into(), Value::String(question.chars().take(2048).collect()));
        if let Some(header) = item["header"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            obj.insert("header".into(), Value::String(header.chars().take(128).collect()));
        }
        if let Some(detail) = item["detail"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            obj.insert("detail".into(), Value::String(detail.chars().take(16384).collect()));
        }
        obj.insert("multi_select".into(), Value::Bool(multi_select));
        obj.insert("options".into(), Value::Array(options));
        Some(Value::Object(obj))
    }).collect();
    (!items.is_empty()).then(|| serde_json::json!({"call_id": id, "state": state, "questions": items}))
}

fn apply_question_event(pending: &mut std::collections::BTreeMap<String, Value>, event: &Value) {
    let data = &event["data"];
    if event["surfaceOp"].is_object() {
        // A rewrite can invalidate the causal context of a reconstructed call.
        // The host's authoritative timed projection is kept separately.
        pending.clear();
    }
    match event["type"].as_str() {
        Some("tool/call") if data["name"] == "ask_user_question" && pending.len() < 16 => {
            let Some(id) = data["callId"].as_str().filter(|id| !id.is_empty() && id.len() <= 256) else { return; };
            let Some(raw) = data["arguments"].as_str().filter(|raw| raw.len() <= 64 * 1024) else { return; };
            let Ok(args) = serde_json::from_str::<Value>(raw) else { return; };
            if let Some(row) = question_row(id, "open", &args["questions"]) { pending.insert(id.to_owned(), row); }
        }
        Some("tool/result") => {
            if let Some(id) = data["message"]["toolCallId"].as_str() { pending.remove(id); }
        }
        Some("user/message") if data["source"]["kind"] == "user-question-reply" => {
            if let Some(id) = data["source"]["callId"].as_str() { pending.remove(id); }
        }
        Some("turn/start" | "turn/end") => pending.retain(|_, row| row["state"] == "continued"),
        _ => {}
    }
}

pub fn project_history(address: SessionAddress, snapshot: &Value) -> Result<History, &'static str> {
    if address.connection_id.is_empty() || address.session_id.is_empty()
        || snapshot["type"] != "snapshot"
        || snapshot["header"]["id"].as_str() != Some(address.session_id.as_str()) {
        return Err("DSH_IDENTITY_MISMATCH");
    }
    let records = snapshot["records"].as_array().ok_or("DSH_PROTOCOL")?;
    let mut messages: Vec<Message> = Vec::new();
    let mut omitted = 0;
    for record in records {
        let event = record.get("event").ok_or("DSH_PROTOCOL")?;
        let op = &event["surfaceOp"];
        if !op.is_null() && op != "append" {
            let start = op["startSeq"].as_i64().ok_or("DSH_PROTOCOL")?;
            let end = op["endSeq"].as_i64().ok_or("DSH_PROTOCOL")?;
            if op["op"] != "replace" || start < 0 || start > end { return Err("DSH_PROTOCOL"); }
            messages.retain(|m| m.sequence < start || m.sequence > end);
        }
        let (role, content) = if event["type"] == "assistant/message" {
            ("assistant", &event["data"]["message"]["content"])
        } else if event["type"] == "user/message" && event["data"]["source"]["kind"] == "user" {
            ("user", &event["data"]["content"])
        } else { omitted += 1; continue; };
        let sequence = event["seq"].as_i64().filter(|s| *s >= 0).ok_or("DSH_PROTOCOL")?;
        let parts = content.as_array().ok_or("DSH_PROTOCOL")?;
        let mut text = Vec::new();
        for part in parts {
            if part["type"] == "text" {
                text.push(part["text"].as_str().ok_or("DSH_PROTOCOL")?);
            } else { omitted += 1; }
        }
        let text = text.join("\n");
        if !text.is_empty() {
            let created_at = extract_timestamp_ms(record, event);
            messages.retain(|m| m.sequence != sequence);
            messages.push(Message { sequence, role: role.into(), text, created_at });
        }
    }
    messages.sort_by_key(|m| m.sequence);
    let directory = snapshot["projections"]["values"]
        .get("workingDirectory")
        .filter(|v| !v.is_null())
        .or_else(|| snapshot["header"].get("cwd").filter(|v| !v.is_null()));
    let workspace = match directory {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.len() <= 4096 && !s.chars().any(char::is_control) => Some(s.clone()),
        _ => return Err("DSH_PROTOCOL"),
    };
    let partial = match snapshot.get("hasMore") {
        None => true,
        Some(Value::Bool(b)) => *b,
        _ => return Err("DSH_PROTOCOL"),
    };
    let request_results = request_results(records);
    Ok(History { address, messages, workspace, partial, omitted, request_results })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn address() -> SessionAddress { SessionAddress { connection_id: "node-a".into(), session_id: "existing".into() } }
    #[test]
    fn legacy_question_stays_visible_until_its_own_result_or_turn_end() {
        let args = json!({"questions":[{"id":"q","question":"Choose a target","options":[{"label":"A"}]}]});
        let call = json!({"event":{"type":"tool/call","seq":4,"surfaceOp":"append","data":{"callId":"q1","name":"ask_user_question","arguments":args.to_string()}}});
        let other = json!({"event":{"type":"tool/result","seq":5,"data":{"message":{"toolCallId":"q2"}}}});
        let own = json!({"event":{"type":"tool/result","seq":6,"data":{"message":{"toolCallId":"q1"}}}});
        let mut snapshot = json!({"records":[other,call]});
        assert_eq!(pending_questions(&snapshot)[0]["questions"][0]["options"][0]["label"],"A");
        snapshot["records"].as_array_mut().unwrap().push(own);
        assert!(pending_questions(&snapshot).is_empty());
        snapshot["records"].as_array_mut().unwrap().pop();
        snapshot["records"].as_array_mut().unwrap().push(json!({"event":{"type":"turn/end","seq":7}}));
        assert!(pending_questions(&snapshot).is_empty());
    }
    #[test]
    fn authoritative_continued_question_survives_timeout_and_does_not_block() {
        let snapshot = json!({"records":[{"event":{"type":"tool/result","seq":8,"data":{"message":{"toolCallId":"q1"}}}}],
            "projections":{"values":{"userQuestions":{"active":[{"callId":"q1","state":"continued","questions":[{"id":"q","question":"Still answerable"}]}]}}}});
        assert_eq!(pending_questions(&snapshot)[0]["state"],"continued");
    }
    #[test]
    fn preserves_identity_and_append_deduplicates_sequence() {
        let event = json!({"event":{"type":"assistant/message","seq":2,"surfaceOp":"append","data":{"message":{"content":[{"type":"text","text":"reply"}]}}}});
        let snapshot = json!({"type":"snapshot","header":{"id":"existing","cwd":"/workspace"},"hasMore":false,"records":[event.clone(),event]});
        let result = project_history(address(), &snapshot).unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.address, address());
        assert!(!result.partial);
        assert_eq!(result.workspace.as_deref(), Some("/workspace"));
    }
    #[test]
    fn wrong_session_and_unknown_operation_are_rejected() {
        assert!(project_history(address(), &json!({"type":"snapshot","header":{"id":"other"},"records":[]})).is_err());
        assert!(project_history(address(), &json!({"type":"snapshot","header":{"id":"existing"},"records":[{"event":{"surfaceOp":"delete"}}]})).is_err());
    }
    #[test]
    fn question_row_preserves_metadata_and_multi_select_variants() {
        let snapshot = json!({
            "projections": {
                "values": {
                    "userQuestions": {
                        "active": [{
                            "callId": "call-meta-1",
                            "state": "continued",
                            "questions": [
                                {
                                    "id": "q-camel",
                                    "header": "Deployment",
                                    "detail": "Detailed plan notes",
                                    "question": "Which targets?",
                                    "multiSelect": true,
                                    "options": [{"label": "Staging", "description": "Safe test"}]
                                },
                                {
                                    "id": "q-snake",
                                    "question": "Pick modes?",
                                    "multi_select": true,
                                    "options": [{"label": "Fast"}]
                                }
                            ]
                        }]
                    }
                }
            }
        });
        let rows = pending_questions(&snapshot);
        assert_eq!(rows.len(), 1);
        let q0 = &rows[0]["questions"][0];
        assert_eq!(q0["id"], "q-camel");
        assert_eq!(q0["header"], "Deployment");
        assert_eq!(q0["detail"], "Detailed plan notes");
        assert_eq!(q0["multi_select"], true);
        assert_eq!(q0["options"][0]["label"], "Staging");
        let q1 = &rows[0]["questions"][1];
        assert_eq!(q1["id"], "q-snake");
        assert_eq!(q1["multi_select"], true);
        assert!(q1.get("header").is_none());
        assert!(q1.get("detail").is_none());
    }
    #[test]
    fn continued_question_is_suppressed_while_reply_queued_in_inbox() {
        let snapshot = json!({
            "projections": {
                "values": {
                    "userQuestions": {
                        "active": [
                            {"callId": "call-queued", "state": "continued", "questions": [{"id": "q1", "question": "First?"}]},
                            {"callId": "call-other", "state": "continued", "questions": [{"id": "q2", "question": "Second?"}]},
                            {"callId": "call-open", "state": "open", "questions": [{"id": "q3", "question": "Open?"}]}
                        ]
                    },
                    "inbox": {
                        "next-step": [{"source": {"kind": "user-question-reply", "callId": "call-queued"}}],
                        "next-turn": [{"source": {"kind": "user-question-reply", "callId": "call-open"}}]
                    }
                }
            }
        });
        let rows = pending_questions(&snapshot);
        let ids: Vec<_> = rows.iter().filter_map(|r| r["call_id"].as_str()).collect();
        assert_eq!(ids, vec!["call-open", "call-other"]);
    }
}

/// Request attribution comes from native source.rpcId and turn boundaries, never
/// from matching prompt text or choosing the latest assistant message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeRequestResult {
    pub request_id: String,
    pub turn: i64,
    pub status: String,
    pub text: String,
    pub sequences: Vec<i64>,
}

fn request_results(records: &[Value]) -> Vec<NativeRequestResult> {
    #[derive(Default)]
    struct Turn { users: Vec<Option<String>>, replies: Vec<(i64,String)>, end: Option<String>, unsafe_surface: bool }
    let mut turns = std::collections::BTreeMap::<i64,Turn>::new();
    let mut current = None;
    let mut events: Vec<_> = records.iter().filter_map(|r| r.get("event")).collect();
    // A rewritten surface or conflicting sequence is insufficient attribution
    // evidence. Do not silently attach an old reply to a new request.
    if events.iter().any(|e| e["seq"].as_i64().is_none_or(|n|n<0)
        || (!e["surfaceOp"].is_null() && e["surfaceOp"]!="append")) { return Vec::new(); }
    events.sort_by_key(|e| e["seq"].as_i64().unwrap_or(-1));
    if events.windows(2).any(|p|p[0]["seq"]==p[1]["seq"] && p[0]!=p[1]) { return Vec::new(); }
    events.dedup_by_key(|e| e["seq"].as_i64().unwrap_or(-1));
    for e in events {
        match e["type"].as_str().unwrap_or("") {
            "turn/start" => { current=e["data"]["turn"].as_i64(); if let Some(t)=current { turns.entry(t).or_default(); } }
            "user/message" if e["data"]["source"]["kind"]=="user" => {
                if let Some(t)=current { turns.entry(t).or_default().users.push(e["data"]["source"]["rpcId"].as_str().map(str::to_owned)); }
            }
            "assistant/message" => {
                if let Some(t)=e["data"]["turn"].as_i64().filter(|t|Some(*t)==current) {
                    let turn=turns.entry(t).or_default();
                    if e["data"]["interrupted"]==true || (!e["surfaceOp"].is_null() && e["surfaceOp"]!="append") { turn.unsafe_surface=true; }
                    let text=e["data"]["message"]["content"].as_array().map(|parts|parts.iter().filter(|p|p["type"]=="text").filter_map(|p|p["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                    if let Some(seq)=e["seq"].as_i64() { if !text.is_empty() { turn.replies.push((seq,text)); } }
                }
            }
            "turn/end" => {
                if let Some(t)=e["data"]["turn"].as_i64().filter(|t|Some(*t)==current) {
                    turns.entry(t).or_default().end=e["data"]["reason"]["kind"].as_str().map(str::to_owned);
                    current=None;
                }
            }
            _ => {}
        }
    }
    turns.into_iter().flat_map(|(turn,t)| {
        let unambiguous=t.users.len()==1 && !t.unsafe_surface;
        t.users.iter().filter_map(|id|id.as_ref()).map(|id| NativeRequestResult {
            request_id:id.clone(),turn,
            status:if !unambiguous {"ambiguous".into()} else { t.end.clone().unwrap_or_else(||"running".into()) },
            text:if unambiguous {t.replies.iter().map(|(_,s)|s.as_str()).collect::<Vec<_>>().join("\n\n")} else {String::new()},
            sequences:if unambiguous {t.replies.iter().map(|(seq,_)|*seq).collect()} else {Vec::new()},
        }).collect::<Vec<_>>()
    }).collect()
}

#[cfg(test)]
mod result_tests {
    use super::*;
    use serde_json::json;
    fn e(seq:i64,kind:&str,data:Value)->Value {json!({"event":{"seq":seq,"type":kind,"data":data}})}
    #[test]
    fn result_uses_rpc_identity_and_completed_turn_not_latest_text() {
        let records=vec![e(1,"turn/start",json!({"turn":1})),e(2,"user/message",json!({"source":{"kind":"user","rpcId":"a"}})),e(3,"assistant/message",json!({"turn":1,"message":{"content":[{"type":"text","text":"result a"}]}})),e(4,"turn/end",json!({"turn":1,"reason":{"kind":"completed"}})),e(5,"turn/start",json!({"turn":2})),e(6,"user/message",json!({"source":{"kind":"user","rpcId":"b"}}))];
        let result=request_results(&records);
        assert_eq!(result[0].request_id,"a");assert_eq!(result[0].status,"completed");assert_eq!(result[0].text,"result a");assert_eq!(result[1].status,"running");
    }
    #[test]
    fn simultaneous_users_and_missing_start_never_claim_a_result() {
        let result=request_results(&[e(1,"turn/start",json!({"turn":1})),e(2,"user/message",json!({"source":{"kind":"user","rpcId":"a"}})),e(3,"user/message",json!({"source":{"kind":"user","rpcId":"b"}})),e(4,"turn/end",json!({"turn":1,"reason":{"kind":"completed"}}))]);
        assert!(result.iter().all(|r|r.status=="ambiguous" && r.text.is_empty()));
        assert!(request_results(&[e(3,"user/message",json!({"source":{"kind":"user","rpcId":"a"}}))]).is_empty());
    }
}
