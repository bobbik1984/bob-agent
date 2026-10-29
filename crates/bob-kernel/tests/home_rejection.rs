use bob_kernel::outbox::{AppendMessage,Store};
use rusqlite::Connection;
const PIN:&str="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn request()->AppendMessage {AppendMessage{request_id:"r".into(),message_id:"m".into(),session_id:"s".into(),origin_device_id:"replica".into(),home_device_id:"home".into(),home_epoch:1,content:serde_json::json!({"text":"hello"})}}
fn fixture(p:&std::path::Path)->Store {
 let mut s=Store::open(p,0).unwrap();s.initialize_local_identity("replica","home",1,0).unwrap();
 Connection::open(p).unwrap().execute("UPDATE devices SET trust_state='paired',public_key_ref='fixture-key' WHERE id='replica'",[]).unwrap();
 s.grant_home_conversation("home",1,"replica",PIN,"s",0,100,true,1).unwrap();s
}
#[test]
fn rejection_survives_reopen_and_cannot_become_an_execution_on_retry() {
 let d=tempfile::tempdir().unwrap();let p=d.path().join("home.db");let mut s=fixture(&p);let ctx=s.authenticate_home_session("home",1,PIN,"s",2).unwrap();
 let r=s.reject_message(&ctx,&request(),"HOME_RUNNER_BUSY",3).unwrap();assert!(!r.accepted);drop(s);
 let mut s=Store::open(&p,4).unwrap();let ctx=s.authenticate_home_session("home",1,PIN,"s",4).unwrap();
 assert!(!s.lookup_message_receipt(&ctx,&request(),5).unwrap().unwrap().accepted);
 assert!(!s.receive_message(&ctx,&request(),6).unwrap().accepted);
 assert!(!s.reject_message(&ctx,&request(),"HOME_EXECUTION_NOT_APPROVED",7).unwrap().accepted);
 let c=Connection::open(p).unwrap();
 assert_eq!(c.query_row("SELECT count(*) FROM messages",[],|r|r.get::<_,i64>(0)).unwrap(),0);
 assert_eq!(c.query_row("SELECT count(*) FROM executions",[],|r|r.get::<_,i64>(0)).unwrap(),0);
 assert_eq!(c.query_row("SELECT json_extract(result_json,'$.code') FROM inbox_receipts",[],|r|r.get::<_,String>(0)).unwrap(),"HOME_RUNNER_BUSY");
}
#[test]
fn rejection_preserves_accepted_request_and_rechecks_exact_identity() {
 let d=tempfile::tempdir().unwrap();let p=d.path().join("home.db");let mut s=fixture(&p);let ctx=s.authenticate_home_session("home",1,PIN,"s",2).unwrap();
 s.receive_message(&ctx,&request(),3).unwrap();assert!(s.reject_message(&ctx,&request(),"HOME_RUNNER_BUSY",4).unwrap().accepted);
 let mut altered=request();altered.content=serde_json::json!({"text":"changed"});assert!(s.reject_message(&ctx,&altered,"HOME_RUNNER_BUSY",4).is_err());
 altered=request();altered.origin_device_id="other".into();assert!(s.reject_message(&ctx,&altered,"HOME_RUNNER_BUSY",4).is_err());
 assert!(s.reject_message(&ctx,&request(),"untrusted arbitrary text",4).is_err());
 s.revoke_home_device("home",1,"replica",1,5).unwrap();assert!(s.reject_message(&ctx,&request(),"HOME_RUNNER_BUSY",6).is_err());
}
