//! Adapter-normalized live projection. No remote HTML, credentials or executable links.
use super::HistoryMessage;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

const PROTOCOL: &str = "REMOTE_PROTOCOL";
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Interaction {
    pub event_id: String,
    pub agent_id: String,
    pub kind: String,
    pub request: Value,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveView {
    pub watch_id: String,
    pub session_id: String,
    pub ready: bool,
    pub phase: String,
    pub messages: Vec<HistoryMessage>,
    pub draft: String,
    pub reasoning: String,
    pub interactions: Vec<Interaction>,
    pub has_more: bool,
    pub omitted_records: usize,
    pub error: Option<String>,
}

pub struct LiveState {
    pub view: LiveView,
    pub client_id: Option<String>,
    pub interactions: HashMap<String, Interaction>,
    pub request_id: Option<String>,
    pub request_seen: bool,
    pub sent: HashMap<String, (String, String)>,
    cursor: Option<u64>,
    attempt: Option<String>,
    index: u64,
    pub cancel_turn: Option<u64>,
    turn: Option<u64>,
    cancel_deadline: Option<std::time::Instant>,
    stream_revision: Option<u64>,
    outstanding: HashMap<String, String>,
    pub uncertain: std::collections::HashSet<String>,
}

impl Default for LiveState {
    fn default() -> Self {
        Self {
            view: LiveView {
                watch_id: String::new(),
                session_id: String::new(),
                ready: false,
                phase: "idle".into(),
                messages: vec![],
                draft: String::new(),
                reasoning: String::new(),
                interactions: vec![],
                has_more: false,
                omitted_records: 0,
                error: None,
            },
            client_id: None,
            interactions: HashMap::new(),
            request_id: None,
            request_seen: false,
            sent: HashMap::new(),
            cursor: None,
            attempt: None,
            index: 0,
            cancel_turn: None,
            turn: None,
            cancel_deadline: None,
            stream_revision: None,
            outstanding: HashMap::new(),
            uncertain: std::collections::HashSet::new(),
        }
    }
}

impl LiveState {
    pub fn select(&mut self, id: String, session: String) {
        self.view = LiveView {
            watch_id: id,
            session_id: session,
            ..Self::default().view
        };
        self.cursor = None;
        self.attempt = None;
        self.index = 0;
        self.request_id = self.outstanding.get(&self.view.session_id).cloned();
        self.request_seen = false;
        self.cancel_turn = None;
        self.turn = None;
        self.cancel_deadline = None;
        self.stream_revision = None;
    }
    pub fn view(&self) -> LiveView {
        let mut view = self.view.clone();
        if view.phase == "cancel_requested"
            && self
                .cancel_deadline
                .is_some_and(|d| std::time::Instant::now() >= d)
        {
            view.phase = "cancel_unconfirmed".into();
        }
        // DSH Agent identity is the session's runtime identity; only this scope is actionable.
        view.interactions = self
            .interactions
            .values()
            .filter(|i| i.agent_id == view.session_id)
            .cloned()
            .collect();
        view.interactions
            .sort_by(|a, b| a.event_id.cmp(&b.event_id));
        view
    }
    pub fn fail(&mut self, code: &str) {
        self.view.ready = false;
        self.view.phase = "unknown".into();
        self.view.error = Some(code.into());
        for item in self.interactions.values_mut() {
            item.status = "expired".into();
        }
    }
    pub fn check(&self, watch: &str) -> Result<(), String> {
        if self.view.watch_id != watch || !self.view.ready || self.client_id.is_none() {
            return Err("REMOTE_STALE".into());
        }
        Ok(())
    }
    pub fn begin_send(&mut self, id: &str, text: &str) -> Result<bool, String> {
        if id.is_empty() || id.len() > 128 || text.trim().is_empty() || text.len() > 64 * 1024 {
            return Err(PROTOCOL.into());
        }
        if let Some((session, previous)) = self.sent.get(id) {
            return if session == &self.view.session_id && previous == text {
                Ok(false)
            } else {
                Err(PROTOCOL.into())
            };
        }
        if !["idle", "completed", "stopped", "failed", "blocked"]
            .contains(&self.view.phase.as_str())
        {
            return Err("REMOTE_BUSY".into());
        }
        if self.sent.len() >= 1000 {
            return Err("REMOTE_LIMIT".into());
        }
        self.sent
            .insert(id.into(), (self.view.session_id.clone(), text.into()));
        self.outstanding
            .insert(self.view.session_id.clone(), id.into());
        self.request_id = Some(id.into());
        self.request_seen = false;
        self.view.phase = "sending".into();
        self.view.error = None;
        Ok(true)
    }
    pub fn begin_cancel(&mut self) -> Result<(), String> {
        if self.view.phase != "running" || self.turn.is_none() {
            return Err("REMOTE_NOT_RUNNING".into());
        }
        self.cancel_turn = self.turn;
        self.cancel_deadline = Some(std::time::Instant::now() + std::time::Duration::from_secs(15));
        self.view.phase = "cancel_requested".into();
        Ok(())
    }
    pub fn event_channel(&mut self, value: &Value) -> Result<(), String> {
        match value["type"].as_str() {
            Some("ready") => {
                if self.client_id.is_some() {
                    return Err(PROTOCOL.into());
                }
                self.client_id = Some(nonempty(&value["clientId"])?.into());
            }
            Some("waterfall") => {
                if self.client_id.is_none() || self.interactions.len() >= 100 {
                    return Err(PROTOCOL.into());
                }
                let event_id = nonempty(&value["eventId"])?;
                let agent_id = nonempty(&value["agentId"])?;
                let kind = nonempty(&value["event"])?;
                let request = &value["request"];
                // Store a small, explicit presentation contract, never an arbitrary remote object.
                let projected = match kind {
                    "approval/request" => {
                        json!({"toolName":nonempty(&request["toolName"])?,"reason":request["reason"].as_str().unwrap_or(""),"callId":request["callId"].as_str()})
                    }
                    "user-questions/request" => {
                        // Timed/continued questions need attachWait/answer; refuse capability rather than invent a timeout.
                        if request.pointer("/wait/timed") == Some(&Value::Bool(true)) {
                            return Err("REMOTE_UNSUPPORTED_INTERACTION".into());
                        }
                        let questions = request["questions"].as_array().ok_or(PROTOCOL)?;
                        if questions.is_empty() || questions.len() > 12 {
                            return Err(PROTOCOL.into());
                        }
                        let mut ids = std::collections::HashSet::new();
                        for question in questions {
                            if !ids.insert(nonempty(&question["id"])?) {
                                return Err(PROTOCOL.into());
                            }
                            nonempty(&question["question"])?;
                        }
                        json!({"questions":questions})
                    }
                    _ => return Err("REMOTE_UNSUPPORTED_INTERACTION".into()),
                };
                if self.interactions.contains_key(event_id) {
                    return Err(PROTOCOL.into());
                }
                self.interactions.insert(
                    event_id.into(),
                    Interaction {
                        event_id: event_id.into(),
                        agent_id: agent_id.into(),
                        kind: if kind == "approval/request" {
                            "approval"
                        } else {
                            "questions"
                        }
                        .into(),
                        request: projected,
                        status: "pending".into(),
                    },
                );
            }
            Some("cancel") => {
                if let Some(item) = self.interactions.get_mut(nonempty(&value["eventId"])?) {
                    item.status = "expired".into();
                }
            }
            Some("emit") => {} // Activity notifications are not durable completion evidence.
            _ => return Err(PROTOCOL.into()),
        }
        Ok(())
    }
    pub fn begin_answer(&mut self, event_id: &str, answer: &Value) -> Result<Value, String> {
        let interaction = self.interactions.get_mut(event_id).ok_or("REMOTE_STALE")?;
        if interaction.status != "pending" || interaction.agent_id != self.view.session_id {
            return Err("REMOTE_STALE".into());
        }
        match interaction.kind.as_str() {
            "approval" if answer == "allowed-once" || answer == "rejected" => {}
            "questions" => {
                let questions = interaction.request["questions"]
                    .as_array()
                    .ok_or(PROTOCOL)?;
                let answers = answer["answers"].as_array().ok_or(PROTOCOL)?;
                if answers.len() != questions.len() {
                    return Err(PROTOCOL.into());
                }
                for question in questions {
                    let matches: Vec<_> = answers
                        .iter()
                        .filter(|a| a["id"] == question["id"])
                        .collect();
                    if matches.len() != 1 {
                        return Err(PROTOCOL.into());
                    }
                    let a = matches[0];
                    let selected = a["selected"].as_array().ok_or(PROTOCOL)?;
                    if question["multiSelect"] != true && selected.len() > 1 {
                        return Err(PROTOCOL.into());
                    }
                    let options = question["options"].as_array();
                    if selected.iter().any(|v| {
                        !v.is_string()
                            || !options.is_some_and(|opts| opts.iter().any(|o| o["label"] == *v))
                    }) {
                        return Err(PROTOCOL.into());
                    }
                    if selected.is_empty() && a["custom"].as_str().unwrap_or("").trim().is_empty() {
                        return Err(PROTOCOL.into());
                    }
                }
            }
            _ => return Err(PROTOCOL.into()),
        }
        interaction.status = "submitting".into();
        Ok(
            json!({"clientId": self.client_id, "eventId":event_id,"outcome":{"kind":"result","value":answer}}),
        )
    }
    pub fn follow(&mut self, value: &Value) -> Result<(), String> {
        match value["type"].as_str() {
            Some("snapshot") => {
                if self.view.ready {
                    return Err(PROTOCOL.into());
                }
                let history = super::dsh::project_history(&self.view.session_id, value)?;
                self.view.messages = history.messages;
                self.view.has_more = history.has_more;
                self.view.omitted_records = history.omitted_records;
                self.view.phase = "idle".into();
                for record in value["records"].as_array().ok_or(PROTOCOL)? {
                    self.lifecycle(&record["event"]);
                }
                self.cursor = value["cursor"].as_u64();
                self.view.ready = true;
                if self.request_id.is_some() && !self.request_seen {
                    self.view.phase = if self.uncertain.contains(&self.view.session_id) {
                        "unknown"
                    } else {
                        "accepted"
                    }
                    .into();
                }
                self.stream_revision = value
                    .pointer("/assistantStream/revision")
                    .and_then(Value::as_u64);
                if let Some(active) = value.pointer("/assistantStream/activeAttempt") {
                    self.attempt = Some(nonempty(&active["attemptId"])?.into());
                    // The bounded history window may omit turn/start. An active snapshot is
                    // still evidence of a running turn, never permission to enqueue a duplicate.
                    self.turn = Some(active["turn"].as_u64().ok_or(PROTOCOL)?);
                    if self.request_id.is_none() || self.request_seen {
                        self.view.phase = "running".into();
                    }
                    self.view.draft.clear();
                    self.view.reasoning.clear();
                    for entry in active["stream"].as_array().ok_or(PROTOCOL)? {
                        match entry["type"].as_str() {
                            Some("text-chunks") | Some("reasoning-chunks") => {
                                let target = if entry["type"] == "text-chunks" {
                                    &mut self.view.draft
                                } else {
                                    &mut self.view.reasoning
                                };
                                for text in entry["texts"].as_array().ok_or(PROTOCOL)? {
                                    target.push_str(text.as_str().ok_or(PROTOCOL)?);
                                }
                            }
                            Some("chunk") => self.chunk(&entry["chunk"]),
                            _ => {}
                        }
                    }
                    self.index = active["nextIndex"].as_u64().ok_or(PROTOCOL)?;
                }
            }
            Some("event") => {
                if !self.view.ready {
                    return Err(PROTOCOL.into());
                }
                let event = &value["event"];
                let seq = event["seq"].as_u64().ok_or(PROTOCOL)?;
                if self.cursor.is_some_and(|cursor| seq <= cursor) {
                    return Ok(());
                }
                self.cursor = Some(seq);
                if let Some(op) = event.get("surfaceOp").filter(|v| v.is_object()) {
                    let start = op["startSeq"].as_u64().ok_or(PROTOCOL)?;
                    let end = op["endSeq"].as_u64().ok_or(PROTOCOL)?;
                    if op["op"] != "replace" || start > end {
                        return Err(PROTOCOL.into());
                    }
                    self.view.messages.retain(|m| {
                        m.id.parse::<u64>()
                            .map(|id| id < start || id > end)
                            .unwrap_or(false)
                    });
                }
                let projection = super::dsh::project_history(
                    &self.view.session_id,
                    &json!({"type":"snapshot","header":{"id":self.view.session_id},"hasMore":false,"records":[{"event":event}]}),
                )?;
                self.view.messages.extend(projection.messages);
                self.view.omitted_records += projection.omitted_records;
                self.lifecycle(event);
            }
            Some("assistant-stream") => {
                let frame = &value["frame"];
                let revision = frame["revision"].as_u64().ok_or(PROTOCOL)?;
                if let Some(previous) = self.stream_revision {
                    if revision <= previous && !(frame["type"] == "start" && revision == 1) {
                        return Ok(());
                    }
                    if revision != previous + 1 && !(frame["type"] == "start" && revision == 1) {
                        return Err("REMOTE_STREAM_GAP".into());
                    }
                }
                self.stream_revision = Some(revision);
                match frame["type"].as_str() {
                    Some("start") => {
                        self.attempt = Some(nonempty(&frame["attemptId"])?.into());
                        self.index = 0;
                        self.view.draft.clear();
                        self.view.reasoning.clear();
                    }
                    Some("chunk") | Some("end") => {
                        if self.attempt.as_deref() != frame["attemptId"].as_str() {
                            return Err(PROTOCOL.into());
                        }
                        let index = frame["index"].as_u64().ok_or(PROTOCOL)?;
                        if index < self.index {
                            return Ok(());
                        }
                        if index != self.index {
                            return Err("REMOTE_STREAM_GAP".into());
                        }
                        self.index += 1;
                        if frame["type"] == "chunk" {
                            self.chunk(&frame["chunk"]);
                        } else {
                            self.attempt = None;
                            if frame.pointer("/outcome/kind") == Some(&json!("abandoned"))
                                || frame
                                    .pointer("/outcome/seq")
                                    .and_then(Value::as_u64)
                                    .is_some_and(|s| self.cursor.is_some_and(|c| c >= s))
                            {
                                self.view.draft.clear();
                                self.view.reasoning.clear();
                            }
                        }
                    }
                    _ => return Err(PROTOCOL.into()),
                }
            }
            _ => return Err(PROTOCOL.into()),
        }
        if self.view.messages.len() > 300 {
            self.view.messages.drain(..self.view.messages.len() - 300);
            self.view.has_more = true;
        }
        if self.view.draft.len()
            + self.view.reasoning.len()
            + self
                .view
                .messages
                .iter()
                .map(|m| m.text.len())
                .sum::<usize>()
            > 2 * 1024 * 1024
        {
            return Err("REMOTE_LIMIT".into());
        }
        Ok(())
    }
    fn chunk(&mut self, chunk: &Value) {
        if let Some(text) = chunk["text"].as_str() {
            match chunk["type"].as_str() {
                Some("text-delta") => self.view.draft.push_str(text),
                Some("reasoning-delta") => self.view.reasoning.push_str(text),
                _ => {}
            }
        }
    }
    fn lifecycle(&mut self, event: &Value) {
        match event["type"].as_str() {
            Some("user/message") => {
                if self.request_id.as_deref().is_some_and(|id| {
                    event.pointer("/data/source/rpcId").and_then(Value::as_str) == Some(id)
                }) {
                    self.request_seen = true;
                    self.view.phase = "running".into();
                }
            }
            Some("assistant/message") => {
                self.view.draft.clear();
                self.view.reasoning.clear();
            }
            Some("turn/start") => {
                self.turn = event["data"]["turn"].as_u64();
                if self.request_id.is_none() || self.request_seen {
                    self.view.phase = "running".into();
                }
            }
            Some("turn/end") => {
                if self.turn.is_some() && self.turn != event["data"]["turn"].as_u64() {
                    return;
                }
                if self.request_id.is_some() && !self.request_seen {
                    return;
                }
                let reason = event.pointer("/data/reason/kind").and_then(Value::as_str);
                self.view.phase = match reason {
                    Some("completed") => "completed",
                    Some("aborted") => "stopped",
                    Some("blocked") => "blocked",
                    _ => "failed",
                }
                .into();
                self.turn = None;
                self.cancel_turn = None;
                self.cancel_deadline = None;
                self.request_id = None;
                self.outstanding.remove(&self.view.session_id);
                self.uncertain.remove(&self.view.session_id);
                for item in self
                    .interactions
                    .values_mut()
                    .filter(|i| i.agent_id == self.view.session_id)
                {
                    if item.status == "pending" || item.status == "submitting" {
                        item.status = "expired".into();
                    }
                }
            }
            _ => {}
        }
    }
}
fn nonempty(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 8192)
        .ok_or_else(|| PROTOCOL.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn baseline() -> Value {
        json!({"type":"snapshot","header":{"id":"s"},"hasMore":false,"cursor":0,"records":[],"assistantStream":{"revision":0}})
    }
    #[test]
    fn cancellation_timeout_is_unconfirmed_and_only_matching_turn_can_stop() {
        let mut s = LiveState::default();
        s.select("w".into(), "s".into());
        s.follow(&baseline()).unwrap();
        s.lifecycle(&json!({"type":"turn/start","data":{"turn":7}}));
        s.begin_cancel().unwrap();
        s.cancel_deadline = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        assert_eq!(s.view().phase, "cancel_unconfirmed");
        s.lifecycle(&json!({"type":"turn/end","data":{"turn":6,"reason":{"kind":"aborted"}}}));
        assert_eq!(s.view().phase, "cancel_unconfirmed");
        s.lifecycle(&json!({"type":"turn/end","data":{"turn":7,"reason":{"kind":"aborted"}}}));
        assert_eq!(s.view().phase, "stopped");
    }
    #[test]
    fn revisit_cannot_forget_an_accepted_or_unknown_input() {
        let mut s = LiveState::default();
        s.select("w".into(), "s".into());
        s.follow(&baseline()).unwrap();
        s.begin_send("r1", "text").unwrap();
        s.select("w2".into(), "s".into());
        s.follow(&baseline()).unwrap();
        assert_eq!(s.view.phase, "accepted");
        assert!(s.begin_send("r2", "text").is_err());
        s.uncertain.insert("s".into());
        s.select("w3".into(), "s".into());
        s.follow(&baseline()).unwrap();
        assert_eq!(s.view.phase, "unknown");
        assert!(s.begin_send("r2", "text").is_err());
    }
    #[test]
    fn snapshot_rehydrates_active_stream_and_surface_replacement() {
        let mut s = LiveState::default();
        s.select("w".into(), "s".into());
        let mut b = baseline();
        b["assistantStream"] = json!({"revision":2,"activeAttempt":{"attemptId":"a","turn":3,"nextIndex":1,"stream":[{"type":"text-chunks","texts":["hel","lo"]}]}});
        s.follow(&b).unwrap();
        assert_eq!(s.view.draft, "hello");
        assert_eq!(s.view.phase, "running");
        assert!(s.begin_send("duplicate", "do it again").is_err());
        s.begin_cancel().unwrap();
        assert_eq!(s.cancel_turn, Some(3));
        s.follow(&json!({"type":"assistant-stream","frame":{"type":"chunk","attemptId":"a","revision":3,"index":1,"chunk":{"type":"text-delta","text":"!"}}})).unwrap();
        assert_eq!(s.view.draft, "hello!");
        s.follow(&json!({"type":"event","event":{"type":"assistant/message","seq":1,"data":{"message":{"content":[{"type":"text","text":"old"}]}}}})).unwrap();
        s.follow(&json!({"type":"event","event":{"type":"assistant/message","seq":2,"surfaceOp":{"op":"replace","startSeq":1,"endSeq":1},"data":{"message":{"content":[{"type":"text","text":"new"}]}}}})).unwrap();
        assert_eq!(s.view.messages.len(), 1);
        assert_eq!(s.view.messages[0].text, "new");
    }
    #[test]
    fn unsupported_timed_question_is_not_silently_answered() {
        let mut s = LiveState::default();
        s.client_id = Some("c".into());
        let result=s.event_channel(&json!({"type":"waterfall","eventId":"q","agentId":"s","event":"user-questions/request","request":{"wait":{"timed":true},"questions":[{"id":"q","question":"choose"}]}}));
        assert_eq!(result.unwrap_err(), "REMOTE_UNSUPPORTED_INTERACTION");
        assert!(s.interactions.is_empty());
    }
}
