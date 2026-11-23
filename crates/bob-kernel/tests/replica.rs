use bob_kernel::{app::LocalApp,dispatch::{DispatchPolicy,TransportResult,JobKind,WireIntent},outbox::HomeReceipt};
fn policy()->DispatchPolicy {DispatchPolicy{retry_ms:10,lease_ms:100,max_attempts:3}}
#[test]
fn binding_preserves_old_messages_and_new_delivery_survives_restart() {
 let d=tempfile::tempdir().unwrap();let p=d.path().join("app.sqlite");
 let mut app=LocalApp::open(&p).unwrap();let session=app.default_session().unwrap();
 app.accept("old","old-m",&session,"local only".into(),1).unwrap();
 assert!(app.claim_home_delivery(2,policy()).unwrap().is_none());
 app.bind_confirmed_home("home",1,&"a".repeat(64),3).unwrap();
 assert!(app.claim_home_delivery(4,policy()).unwrap().is_none());
 let saved=app.accept("new","new-m",&session,"deliver".into(),5).unwrap();
 let job=app.claim_home_delivery(6,policy()).unwrap().unwrap();assert_eq!(job.intent.request_id(),"new");
 app.complete_home_delivery(&job,TransportResult::Uncertain,7,policy()).unwrap();drop(app);
 let mut app=LocalApp::open(&p).unwrap();
 let job=app.claim_home_delivery(30,policy()).unwrap().unwrap();assert!(job.kind==JobKind::Lookup);
 app.complete_home_delivery(&job,TransportResult::Receipt(HomeReceipt{request_id:"new".into(),home_device_id:"home".into(),home_epoch:1,payload_sha256:saved.payload_sha256,accepted:true}),31,policy()).unwrap();
 let h=app.history(&session).unwrap();assert_eq!(h[0].send_state,"local_saved");assert_eq!(h[1].send_state,"peer_accepted");
 assert!(app.claim_home_delivery(40,policy()).unwrap().is_none());
}
#[test]
fn offline_is_retryable_but_unknown_never_blindly_resends() {
 let d=tempfile::tempdir().unwrap();let mut app=LocalApp::open(&d.path().join("app.sqlite")).unwrap();let s=app.default_session().unwrap();
 app.bind_confirmed_home("home",1,&"b".repeat(64),1).unwrap();app.accept("r","m",&s,"hello".into(),2).unwrap();
 let j=app.claim_home_delivery(3,policy()).unwrap().unwrap();app.complete_home_delivery(&j,TransportResult::ConfirmedZero,4,policy()).unwrap();
 assert!(app.claim_home_delivery(5,policy()).unwrap().is_none());let j=app.claim_home_delivery(15,policy()).unwrap().unwrap();assert!(j.kind==JobKind::Send);assert!(matches!(j.intent,WireIntent::Message(_)));
 app.complete_home_delivery(&j,TransportResult::Uncertain,16,policy()).unwrap();let j=app.claim_home_delivery(30,policy()).unwrap().unwrap();assert!(j.kind==JobKind::Lookup);
 app.complete_home_delivery(&j,TransportResult::Absent,31,policy()).unwrap();let j=app.claim_home_delivery(50,policy()).unwrap().unwrap();assert!(j.kind==JobKind::Lookup);
 assert!(app.bind_confirmed_home("other",1,&"b".repeat(64),51).is_err());assert!(app.bind_confirmed_home("home",1,&"c".repeat(64),51).is_err());
}

#[test]
fn authenticated_reply_is_idempotent_and_cannot_regress_after_restart() {
 use bob_kernel::home_result::{HomeResult,HomeExecutionResult,HomeReply};
 let d=tempfile::tempdir().unwrap();let p=d.path().join("app.sqlite");let mut app=LocalApp::open(&p).unwrap();let s=app.default_session().unwrap();
 app.bind_confirmed_home("home",1,&"a".repeat(64),1).unwrap();let accepted=app.accept("r","m",&s,"hello".into(),2).unwrap();
 let job=app.claim_home_delivery(3,policy()).unwrap().unwrap();let WireIntent::Message(request)=job.intent.clone() else {panic!()};
 let receipt=HomeReceipt{request_id:"r".into(),home_device_id:"home".into(),home_epoch:1,payload_sha256:accepted.payload_sha256,accepted:true};
 let mut result=HomeResult{receipt:Some(receipt.clone()),execution:Some(HomeExecutionResult{execution_id:"e".into(),version:2,status:"completed".into(),stop_requested:false,stop_confirmed:false,reply:Some(HomeReply{message_id:"reply".into(),text:"actual persisted text fixture".into(),created_at:5})}),sync_bundle:None};
 assert!(app.apply_authenticated_home_result(&request,&result,5).is_err());
 app.complete_home_delivery(&job,TransportResult::Receipt(receipt),4,policy()).unwrap();
 app.apply_authenticated_home_result(&request,&result,5).unwrap();drop(app);let mut app=LocalApp::open(&p).unwrap();
 app.apply_authenticated_home_result(&request,&result,6).unwrap();assert_eq!(app.history(&s).unwrap().len(),2);
 let e=result.execution.as_mut().unwrap();e.version=3;e.status="running".into();e.reply=None;
 assert!(app.apply_authenticated_home_result(&request,&result,7).is_err());
 result.execution.as_mut().unwrap().version=1;app.apply_authenticated_home_result(&request,&result,8).unwrap();assert_eq!(app.history(&s).unwrap()[1].content,"actual persisted text fixture");
}

#[test]
fn remote_stop_is_durable_control_intent_not_stop_confirmation() {
 use bob_kernel::home_result::{HomeResult,HomeExecutionResult};
 let d=tempfile::tempdir().unwrap();let p=d.path().join("app.sqlite");let mut app=LocalApp::open(&p).unwrap();let s=app.default_session().unwrap();
 app.bind_confirmed_home("home",1,&"a".repeat(64),1).unwrap();let accepted=app.accept("r","m",&s,"hello".into(),2).unwrap();
 let job=app.claim_home_delivery(3,policy()).unwrap().unwrap();let WireIntent::Message(request)=job.intent.clone() else {panic!()};
 let receipt=HomeReceipt{request_id:"r".into(),home_device_id:"home".into(),home_epoch:1,payload_sha256:accepted.payload_sha256,accepted:true};
 app.complete_home_delivery(&job,TransportResult::Receipt(receipt.clone()),4,policy()).unwrap();
 let mut result=HomeResult{receipt:Some(receipt),execution:Some(HomeExecutionResult{execution_id:"e".into(),version:2,status:"running".into(),stop_requested:false,stop_confirmed:false,reply:None}),sync_bundle:None};
 app.apply_authenticated_home_result(&request,&result,5).unwrap();
 assert!(app.remote_executions("wrong-session").is_err());
 assert!(app.request_remote_stop("other-execution",6).is_err());
 let view=app.request_remote_stop("e",6).unwrap();assert!(view.stop_pending);assert!(!view.stop_requested);assert!(!view.stop_confirmed);
 let again=app.request_remote_stop("e",7).unwrap();assert!(again.stop_pending);drop(app);
 let mut app=LocalApp::open(&p).unwrap();assert!(app.remote_executions(&s).unwrap()[0].stop_pending);
 let job=app.claim_home_delivery(8,policy()).unwrap().unwrap();let WireIntent::Cancel(cancel)=job.intent.clone() else {panic!()};assert_eq!(cancel.execution_id,"e");assert_eq!(cancel.expected_version,2);
 app.complete_home_delivery(&job,TransportResult::Receipt(HomeReceipt{request_id:cancel.request_id,home_device_id:"home".into(),home_epoch:1,payload_sha256:job.payload_sha256.clone(),accepted:true}),9,policy()).unwrap();
 // Even a cancel ACK is not process termination evidence.
 assert!(!app.remote_executions(&s).unwrap()[0].stop_confirmed);
 let e=result.execution.as_mut().unwrap();e.version=3;e.stop_requested=true;app.apply_authenticated_home_result(&request,&result,10).unwrap();
 let view=app.remote_executions(&s).unwrap().remove(0);assert!(view.stop_requested);assert!(!view.stop_pending);assert!(!view.stop_confirmed);
 let e=result.execution.as_mut().unwrap();e.version=4;e.status="cancelled".into();e.stop_confirmed=true;app.apply_authenticated_home_result(&request,&result,11).unwrap();
 assert!(app.remote_executions(&s).unwrap()[0].stop_confirmed);assert!(app.claim_home_delivery(12,policy()).unwrap().is_none());
}

#[test]
fn result_polling_reaches_beyond_first_sixteen_and_skips_terminal() {
 use bob_kernel::home_result::{HomeResult,HomeExecutionResult};
 let d=tempfile::tempdir().unwrap();let mut app=LocalApp::open(&d.path().join("app.sqlite")).unwrap();let s=app.default_session().unwrap();
 app.accept("old-local","old-local-m",&s,"local only".into(),0).unwrap();
 app.bind_confirmed_home("home",1,&"a".repeat(64),1).unwrap();
 assert!(app.pending_home_result_at(0).unwrap().is_none());
 let mut receipts=vec![];
 for i in 0..20 {
  let request_id=format!("request-{i:02}");let message_id=format!("message-{i:02}");
  let saved=app.accept(&request_id,&message_id,&s,"message".into(),2+i*3).unwrap();
  let job=app.claim_home_delivery(3+i*3,policy()).unwrap().unwrap();
  let receipt=HomeReceipt{request_id,home_device_id:"home".into(),home_epoch:1,payload_sha256:saved.payload_sha256,accepted:true};
  app.complete_home_delivery(&job,TransportResult::Receipt(receipt.clone()),4+i*3,policy()).unwrap();
  receipts.push(receipt);
 }
 // The earliest sixteen have no execution; later requests must still be queried.
 for i in 0..20 { assert_eq!(app.pending_home_result_at(i).unwrap().unwrap().request_id,format!("request-{i:02}")); }
 assert_eq!(app.pending_home_result_at(20).unwrap().unwrap().request_id,"request-00");
 let request=app.pending_home_result_at(19).unwrap().unwrap();
 app.apply_authenticated_home_result(&request,&HomeResult{receipt:Some(receipts[19].clone()),execution:Some(HomeExecutionResult{execution_id:"last-execution".into(),version:1,status:"failed".into(),stop_requested:false,stop_confirmed:false,reply:None}),sync_bundle:None},70).unwrap();
 assert_eq!(app.pending_home_result_at(19).unwrap().unwrap().request_id,"request-00");
 assert_eq!(app.pending_home_result_at(18).unwrap().unwrap().request_id,"request-18");
}

#[test]
fn latest_cancel_and_actual_terminal_replace_old_cancel_rejection() {
 use bob_kernel::home_result::{HomeResult,HomeExecutionResult};
 let d=tempfile::tempdir().unwrap();let mut app=LocalApp::open(&d.path().join("app.sqlite")).unwrap();let s=app.default_session().unwrap();
 app.bind_confirmed_home("home",1,&"a".repeat(64),1).unwrap();let saved=app.accept("r","m",&s,"hello".into(),2).unwrap();
 let job=app.claim_home_delivery(3,policy()).unwrap().unwrap();let WireIntent::Message(request)=job.intent.clone() else {panic!()};
 let receipt=HomeReceipt{request_id:"r".into(),home_device_id:"home".into(),home_epoch:1,payload_sha256:saved.payload_sha256,accepted:true};
 app.complete_home_delivery(&job,TransportResult::Receipt(receipt.clone()),4,policy()).unwrap();
 let mut result=HomeResult{receipt:Some(receipt),execution:Some(HomeExecutionResult{execution_id:"e".into(),version:2,status:"running".into(),stop_requested:false,stop_confirmed:false,reply:None}),sync_bundle:None};
 app.apply_authenticated_home_result(&request,&result,5).unwrap();app.request_remote_stop("e",6).unwrap();
 let job=app.claim_home_delivery(7,policy()).unwrap().unwrap();app.complete_home_delivery(&job,TransportResult::Receipt(HomeReceipt{request_id:job.intent.request_id().into(),home_device_id:"home".into(),home_epoch:1,payload_sha256:job.payload_sha256.clone(),accepted:false}),8,policy()).unwrap();
 assert!(app.remote_executions(&s).unwrap()[0].cancel_rejected);
 result.execution.as_mut().unwrap().version=3;app.apply_authenticated_home_result(&request,&result,9).unwrap();
 let view=app.request_remote_stop("e",10).unwrap();assert!(view.stop_pending);assert!(!view.cancel_rejected);
 let job=app.claim_home_delivery(11,policy()).unwrap().unwrap();app.complete_home_delivery(&job,TransportResult::Receipt(HomeReceipt{request_id:job.intent.request_id().into(),home_device_id:"home".into(),home_epoch:1,payload_sha256:job.payload_sha256.clone(),accepted:true}),12,policy()).unwrap();
 let e=result.execution.as_mut().unwrap();e.version=4;e.status="cancelled".into();e.stop_requested=true;e.stop_confirmed=true;
 app.apply_authenticated_home_result(&request,&result,13).unwrap();let view=app.remote_executions(&s).unwrap().remove(0);
 assert_eq!(view.status,"cancelled");assert!(!view.cancel_rejected);assert!(!view.stop_pending);assert!(view.stop_confirmed);
}
