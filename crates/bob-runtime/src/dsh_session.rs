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
    Ok(History { address, messages, workspace, partial, omitted })
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
