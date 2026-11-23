//! Bounded Codex JSONL result reducer. Process ownership and authorization live
//! outside this parser. A parsed reply is not user acceptance or artifact verification.
use serde_json::Value;

#[derive(Debug, PartialEq, Eq)]
pub enum ReplyError { InvalidEvent, TooLarge, Failed, Incomplete, ProcessFailed }
#[derive(Default)]
pub struct ReplyStream {
    thread: Option<String>,
    replies: std::collections::BTreeMap<String,String>,
    order: Vec<String>,
    completed: bool,
    started: bool,
    failed: bool,
    bytes: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub struct HarnessReply { pub native_session_id: String, pub text: String }
impl ReplyStream {
    pub fn push(&mut self, line: &str) -> Result<(),ReplyError> {
        self.bytes=self.bytes.saturating_add(line.len());
        if line.len()>256*1024 || self.bytes>2*1024*1024 {
            self.failed=true; return Err(ReplyError::TooLarge);
        }
        let event:Value=match serde_json::from_str(line) { Ok(v)=>v, Err(_)=>{self.failed=true;return Err(ReplyError::InvalidEvent)} };
        let kind=event.get("type").and_then(Value::as_str).ok_or_else(||{self.failed=true;ReplyError::InvalidEvent})?;
        if self.completed { self.failed=true; return Err(ReplyError::InvalidEvent) }
        match kind {
            "thread.started" => {
                let id=event.get("thread_id").and_then(Value::as_str).filter(|s|!s.is_empty()).ok_or_else(||{self.failed=true;ReplyError::InvalidEvent})?;
                if self.thread.as_deref().is_some_and(|old|old!=id) {self.failed=true;return Err(ReplyError::InvalidEvent)}
                self.thread=Some(id.into());
            }
            "turn.started" => {
                if self.thread.is_none() || self.started {self.failed=true;return Err(ReplyError::InvalidEvent)}
                self.started=true;
            }
            "item.completed" if event["item"]["type"]=="agent_message" => {
                if !self.started {self.failed=true;return Err(ReplyError::InvalidEvent)}
                let id=event["item"]["id"].as_str().filter(|s|!s.is_empty());
                let text=event["item"]["text"].as_str();
                match (id,text) {
                    (Some(id),Some(text)) => {
                        if self.replies.get(id).is_some_and(|old|old!=text) {self.failed=true;return Err(ReplyError::InvalidEvent)}
                        if !self.replies.contains_key(id) { self.order.push(id.into()); }
                        self.replies.insert(id.into(),text.into());
                    },
                    _=>{self.failed=true;return Err(ReplyError::InvalidEvent)}
                }
            }
            "turn.completed" => {
                if !self.started {self.failed=true;return Err(ReplyError::InvalidEvent)}
                self.completed=true;
            },
            "turn.failed" | "error" => self.failed=true,
            _ => {} // Tool/progress records are not assistant replies.
        }
        Ok(())
    }
    pub fn finish(self, exit_code: Option<i32>) -> Result<HarnessReply,ReplyError> {
        if exit_code!=Some(0) {return Err(ReplyError::ProcessFailed)}
        if self.failed {return Err(ReplyError::Failed)}
        if !self.completed || self.replies.is_empty() || self.thread.is_none() {return Err(ReplyError::Incomplete)}
        let text=self.order.iter().map(|id|self.replies[id].as_str()).collect::<Vec<_>>().join("\n\n");
        if text.trim().is_empty() {return Err(ReplyError::Incomplete)}
        Ok(HarnessReply{native_session_id:self.thread.unwrap(),text})
    }
}
