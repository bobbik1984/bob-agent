use bob_kernel::harness_reply::*;
fn reply() -> ReplyStream {
 let mut s=ReplyStream::default();
 s.push(r#"{"type":"thread.started","thread_id":"test-session"}"#).unwrap();
 s.push(r#"{"type":"turn.started"}"#).unwrap();
 let item=r#"{"type":"item.completed","item":{"id":"message-1","type":"agent_message","text":"actual reply"}}"#;
 s.push(item).unwrap();s.push(item).unwrap();s
}
#[test] fn completion_requires_reply_event_and_successful_exit() {
 let mut s=reply();s.push(r#"{"type":"turn.completed"}"#).unwrap();
 assert_eq!(s.finish(Some(0)).unwrap().text,"actual reply");
 assert_eq!(reply().finish(Some(0)),Err(ReplyError::Incomplete));
 assert_eq!(reply().finish(None),Err(ReplyError::ProcessFailed));
}
#[test] fn failures_and_malformed_streams_cannot_become_success() {
 let mut s=reply();s.push(r#"{"type":"turn.failed"}"#).unwrap();s.push(r#"{"type":"turn.completed"}"#).unwrap();
 assert_eq!(s.finish(Some(0)),Err(ReplyError::Failed));
 let mut s=reply();assert!(s.push("broken").is_err());s.push(r#"{"type":"turn.completed"}"#).unwrap();assert_eq!(s.finish(Some(0)),Err(ReplyError::Failed));
}
#[test] fn conflicting_session_or_oversize_fails_closed() {
 let mut s=reply();assert!(s.push(r#"{"type":"thread.started","thread_id":"different"}"#).is_err());
 let mut s=reply();assert_eq!(s.push(&"x".repeat(256*1024+1)),Err(ReplyError::TooLarge));
}

#[test] fn terminal_before_start_and_events_after_terminal_are_rejected() {
 let mut s=ReplyStream::default();assert!(s.push(r#"{"type":"turn.completed"}"#).is_err());
 let mut s=reply();s.push(r#"{"type":"turn.completed"}"#).unwrap();
 assert!(s.push(r#"{"type":"thread.started","thread_id":"test-session"}"#).is_err());
 assert_eq!(s.finish(Some(0)),Err(ReplyError::Failed));
}
#[test] fn message_arrival_order_is_not_lexicographic_id_order() {
 let mut s=reply();
 s.push(r#"{"type":"item.completed","item":{"id":"a","type":"agent_message","text":"later"}}"#).unwrap();
 s.push(r#"{"type":"turn.completed"}"#).unwrap();
 assert_eq!(s.finish(Some(0)).unwrap().text,"actual reply\n\nlater");
}
