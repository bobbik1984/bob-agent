//! Actual web send path. Network work runs outside the global kernel lock.
use super::*;
use bob_kernel::direct_provider::DirectExecutionOutcome;
use bob_runtime::dsh_session::SessionAddress;

pub fn execute(state:&Arc<Mutex<NodeContext>>,args:&Value)->Result<Value,&'static str>{
 let request=args["requestId"].as_str().filter(|s|!s.is_empty()&&s.len()<=200).ok_or("REQUEST_ID_REQUIRED")?;
 let (prepared,profile,dir)={
  let mut ctx=state.lock().map_err(|_|"NODE_LOCK_POISONED")?;
  let assistant=ctx.app.assistant_conversation(now_millis()).map_err(|_|"ASSISTANT_OPEN_FAILED")?;
  let history=ctx.app.history(&assistant).map_err(|_|"ASSISTANT_OPEN_FAILED")?;
  if !history.iter().any(|m|m.role=="user"&&m.request_id.as_deref()==Some(request)){return Err("ASSISTANT_REQUEST_MISMATCH")}
  let prep=ctx.app.prepare_direct_execution(request,now_millis()).map_err(|_|"EXECUTION_PREPARE_FAILED")?;
  if !prep.can_start {
   let e=ctx.app.local_executions(&assistant).map_err(|_|"EXECUTION_READ_FAILED")?.into_iter().find(|e|e.id==prep.execution_id).ok_or("EXECUTION_READ_FAILED")?;
   return serde_json::to_value(e).map_err(|_|"EXECUTION_READ_FAILED");
  }
  let profile=ctx.app.direct_provider_profile().map_err(|_|"PROVIDER_READ_FAILED")?.ok_or("PROVIDER_UNCONFIGURED")?;
  (prep,profile,ctx.runtime.data_dir.clone())
 };
 let work=(||->Result<String,&'static str>{
  let runtime=ModelHubRuntime::with_file_vault(dir)?;
  let hub=runtime.load_hub_state();
  let nodes:Vec<_>=hub["remoteNodes"].as_array().map(|n|n.iter().filter(|n|n["enabled"]==true).collect()).unwrap_or_default();
  let mut complete=nodes.len()<=4;
  let mut candidates=Vec::new();
  for node in nodes.iter().take(4){
   let Some(id)=node["id"].as_str() else{complete=false;continue};
   match runtime.dsh_client(id).and_then(|c|c.list()){
    Ok(list)=>if let Some(items)=list.as_array(){for item in items{
      if candidates.len()>=100 {complete=false;break}
      candidates.push(json!({"connection_id":id,"session_id":item["id"],"title":item["title"].as_str().unwrap_or("").chars().take(120).collect::<String>(),"device":node["name"],"workspace":item["workspace"]}));
    }}else{complete=false},
    Err(_)=>complete=false,
   }
  }
  let user=prepared.messages.last().ok_or("ASSISTANT_REQUEST_MISMATCH")?.content.as_str();
  let prompt=bob_runtime::assistant::routing_prompt(user,&candidates,complete);
  let raw=runtime.invoke_direct_model(&profile,&prepared.messages,&prompt)?;
  let proposal=bob_runtime::assistant::parse_proposal(&raw,&candidates,complete)?;
  if proposal.kind=="dispatch" {
   if user.len()>32000{return Err("DSH_TEXT_REQUIRED")}
   let connection=proposal.connection_id.as_deref().ok_or("ASSISTANT_TARGET_UNVERIFIED")?;
   let session=proposal.session_id.as_deref().ok_or("ASSISTANT_TARGET_UNVERIFIED")?;
   let candidate=candidates.iter().find(|c|c["connection_id"]==connection&&c["session_id"]==session).ok_or("ASSISTANT_TARGET_UNVERIFIED")?;
   // Recheck enabled connection and native identity immediately before the only send.
   let client=runtime.dsh_client(connection)?;
   if !client.list()?.as_array().is_some_and(|items|items.iter().any(|s|s["id"]==session)){return Err("ASSISTANT_TARGET_UNVERIFIED")}
   state.lock().map_err(|_|"NODE_LOCK_POISONED")?.app.assistant_bind_dispatch(request,connection,session,candidate["title"].as_str().unwrap_or(""),now_millis()).map_err(|_|"ASSISTANT_LINK_SAVE_FAILED")?;
   let sent=bob_runtime::dsh_transport::dispatch_once(&runtime.data_dir,connection,session,user,request,||client.prompt(session,user,request));
   if let Ok(receipt)=sent {
    if receipt["status"]=="accepted"&&receipt["request_id"]==request&&receipt["session_id"]==session {
     state.lock().map_err(|_|"NODE_LOCK_POISONED")?.app.assistant_record_status(request,"accepted",now_millis()).map_err(|_|"ASSISTANT_LINK_SAVE_FAILED")?;
    }
   }
   // Unknown receipt stays unknown. Poll only observes; it never resends.
  }
  Ok(proposal.text)
 })();
 let outcome=match work{Ok(text)=>DirectExecutionOutcome::Completed{text,provider_reported_model:None,input_tokens:None,output_tokens:None},Err(reason)=>DirectExecutionOutcome::Failed{reason:reason.to_owned()}};
 let result=state.lock().map_err(|_|"NODE_LOCK_POISONED")?.app.finish_direct_execution(&prepared.execution_id,outcome,now_millis()).map_err(|_|"EXECUTION_SAVE_FAILED")?;
 serde_json::to_value(result).map_err(|_|"EXECUTION_READ_FAILED")
}

pub fn poll(state:&Arc<Mutex<NodeContext>>,args:&Value)->Result<Value,&'static str>{
 let conversation=args["sessionId"].as_str().ok_or("SESSION_REQUIRED")?;
 let (links,dir)={let ctx=state.lock().map_err(|_|"NODE_LOCK_POISONED")?;(ctx.app.assistant_links(conversation).map_err(|_|"ASSISTANT_LINK_READ_FAILED")?,ctx.runtime.data_dir.clone())};
 let runtime=ModelHubRuntime::with_file_vault(dir)?;
 let mut errors=Vec::new();
 for link in links.iter().filter(|l|matches!(l.status.as_str(),"accepted"|"unknown")).take(4){
  let result=runtime.dsh_client(&link.connection_id).and_then(|c|c.history(SessionAddress{connection_id:link.connection_id.clone(),session_id:link.native_session_id.clone()}));
  match result{
   Ok(history)=>{
    let matched:Vec<_>=history.request_results.iter().filter(|r|r.request_id==link.request_id).collect();
    if matched.len()!=1{continue}
    let r=matched[0];let mut ctx=state.lock().map_err(|_|"NODE_LOCK_POISONED")?;
    match r.status.as_str(){
     "completed" if !r.text.trim().is_empty()=>ctx.app.assistant_return_result(&link.request_id,&link.connection_id,&link.native_session_id,&r.text,&r.sequences,now_millis()).map_err(|_|"ASSISTANT_RESULT_SAVE_FAILED")?,
     "error"|"blocked"|"cancelled"=>ctx.app.assistant_record_status(&link.request_id,"failed",now_millis()).map_err(|_|"ASSISTANT_LINK_SAVE_FAILED")?,
     "ambiguous"=>ctx.app.assistant_record_status(&link.request_id,"ambiguous",now_millis()).map_err(|_|"ASSISTANT_LINK_SAVE_FAILED")?,
     _=>{},
    }
   },Err(code)=>errors.push(json!({"request_id":link.request_id,"code":code})),
  }
 }
 let links=state.lock().map_err(|_|"NODE_LOCK_POISONED")?.app.assistant_links(conversation).map_err(|_|"ASSISTANT_LINK_READ_FAILED")?;
 Ok(json!({"links":links,"errors":errors}))
}
