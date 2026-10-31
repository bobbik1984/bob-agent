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
            messages.retain(|m| m.sequence != sequence);
            messages.push(Message { sequence, role: role.into(), text });
        }
    }
    messages.sort_by_key(|m| m.sequence);
    let directory = snapshot["projections"]["values"].get("workingDirectory")
        .or_else(|| snapshot["header"].get("cwd"));
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
