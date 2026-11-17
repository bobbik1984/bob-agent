
//! Persistent execution policy, independent of discovery and readable history.
use std::{path::Path,sync::{Mutex,OnceLock}};
use serde_json::{Value,json};
static WRITE:OnceLock<Mutex<()>>=OnceLock::new();
pub const TOOLS:[&str;6]=["dsh","codex","agy","claude","pi","copilot"];
pub fn read(data:&Path)->Result<Value,&'static str>{
 match std::fs::read(data.join("tool_policy.json")){
  Ok(b)=>{
   let p:Value=serde_json::from_slice(&b).map_err(|_|"TOOL_POLICY_INVALID")?;
   if !p.is_object() || !p["tools"].is_object() || !p["devices"].is_object()
      || !p["tools"].as_object().unwrap().values().all(Value::is_boolean)
      || !p["devices"].as_object().unwrap().values().all(|d|d.as_object().is_some_and(|m|m.values().all(Value::is_boolean))){
    return Err("TOOL_POLICY_INVALID");
   }
   Ok(p)
  },
  Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(json!({"revision":0,"tools":{},"devices":{}})),
  Err(_)=>Err("TOOL_POLICY_UNAVAILABLE")
 }
}
fn device_key(data:&Path,connection:&str)->String{
 if connection=="local"{return "local".into();}
 let hub=std::fs::read(data.join("model_hub_state.json")).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null);
 hub["remoteNodes"].as_array().and_then(|ns|ns.iter().find(|n|n["id"]==connection)).and_then(|n|n["deviceId"].as_str()).filter(|id|!id.is_empty()).unwrap_or(connection).to_owned()
}
pub fn configured(data:&Path,connection:&str,tool:&str)->Result<Option<bool>,&'static str>{
 let p=read(data)?;let key=device_key(data,connection);
 Ok(p["devices"][&key][tool].as_bool().or_else(||p["tools"][tool].as_bool()))
}
pub fn enabled(data:&Path,connection:&str,tool:&str)->bool{configured(data,connection,tool).map(|v|v.unwrap_or(true)).unwrap_or(false)}
pub fn require(data:&Path,connection:&str,tool:&str)->Result<(),&'static str>{
 if !TOOLS.contains(&tool){return Err("TOOL_POLICY_TOOL_INVALID")}
 if !enabled(data,connection,tool){return Err("TOOL_DISABLED")} Ok(())
}
pub fn session_tool(s:&str)->&str{
 if s.starts_with("claude"){ "claude" }else if s.starts_with("codex:"){"codex"}else if s.starts_with("agy:"){"agy"}else if s.starts_with("pi:"){"pi"}else if s.starts_with("copilot:"){"copilot"}else{"dsh"}
}
pub fn set(data:&Path,connection:&str,tool:&str,value:bool,all:bool)->Result<Value,&'static str>{
 if !TOOLS.contains(&tool){return Err("TOOL_POLICY_TOOL_INVALID")}
 let _lock=WRITE.get_or_init(||Mutex::new(())).lock().map_err(|_|"TOOL_POLICY_UNAVAILABLE")?;
 let mut p=read(data)?;let key=device_key(data,connection);let connection=key.as_str();
 if all {
  p["tools"][tool]=json!(value);
  if all {if let Some(devices)=p["devices"].as_object_mut(){for device in devices.values_mut(){if let Some(obj)=device.as_object_mut(){obj.remove(tool);}}}}
 }else{
  if !p["devices"][connection].is_object(){p["devices"][connection]=json!({});}
  p["devices"][connection][tool]=json!(value);
 }
 p["revision"]=json!(p["revision"].as_i64().unwrap_or(0).saturating_add(1));
 std::fs::create_dir_all(data).map_err(|_|"TOOL_POLICY_WRITE_FAILED")?;
 let tmp=data.join("tool_policy.json.tmp");std::fs::write(&tmp,serde_json::to_vec(&p).map_err(|_|"TOOL_POLICY_INVALID")?).map_err(|_|"TOOL_POLICY_WRITE_FAILED")?;
 std::fs::rename(tmp,data.join("tool_policy.json")).map_err(|_|"TOOL_POLICY_WRITE_FAILED")?;
 Ok(p)
}
pub fn check_rpc(data:&Path,command:&str,args:&Value)->Result<(),&'static str>{
 if command=="app_native_permission" && args["optionId"].is_null(){return Ok(())}
 let connection=args["connectionId"].as_str().unwrap_or("local");
 let tool=if command=="app_native_setup"{
  if args["action"]=="check"{return Ok(())} args["harness"].as_str().unwrap_or("")
 }else if matches!(command,"app_dsh_prompt"|"app_dsh_create"|"app_dsh_answer"|"app_native_permission"){
  session_tool(args["sessionId"].as_str().unwrap_or(""))
 }else if matches!(command,"app_claude_prompt"|"app_claude_execute"){ "claude" }
 else{return Ok(())};
 require(data,connection,tool)
}
#[cfg(test)] mod tests{
 use super::*;
 #[test] fn disable_survives_reopen_and_preserves_read_operations(){
  let d=std::env::temp_dir().join(format!("policy-{}-{}",std::process::id(),crate::now_millis()));std::fs::create_dir_all(&d).unwrap();
  set(&d,"local","claude",false,true).unwrap();
  assert_eq!(require(&d,"local","claude"),Err("TOOL_DISABLED"));
  for command in ["app_dsh_prompt","app_native_permission"]{assert_eq!(check_rpc(&d,command,&json!({"sessionId":"claude-jsonl:abc","optionId":"allow"})),Err("TOOL_DISABLED"));}
  for command in ["app_dsh_page","app_dsh_history","app_native_cancel"]{assert!(check_rpc(&d,command,&json!({"sessionId":"claude-jsonl:abc"})).is_ok());}
  set(&d,"node-a","claude",true,false).unwrap();assert!(enabled(&d,"node-a","claude"));assert!(!enabled(&d,"node-b","claude"));
  set(&d,"local","claude",false,true).unwrap();assert!(!enabled(&d,"node-a","claude"));
  set(&d,"local","claude",true,true).unwrap();assert!(require(&d,"local","claude").is_ok());
  set(&d,"local","claude",false,false).unwrap();
  assert!(!enabled(&d,"local","claude"));assert!(enabled(&d,"node-b","claude"));
  std::fs::write(d.join("tool_policy.json"),b"[]").unwrap();assert!(!enabled(&d,"local","claude"));
  std::fs::write(d.join("tool_policy.json"),b"broken").unwrap();assert!(!enabled(&d,"local","claude"));
  std::fs::remove_dir_all(d).unwrap();
 }
}

pub fn acknowledge(data:&Path,connection:&str,tool:&str,enabled:bool,synced:bool)->Result<(),&'static str>{
 let _lock=WRITE.get_or_init(||Mutex::new(())).lock().map_err(|_|"TOOL_POLICY_UNAVAILABLE")?;
 let mut p=read(data)?;
 if !p["acknowledgements"].is_object(){p["acknowledgements"]=json!({});}
 if !p["acknowledgements"][connection].is_object(){p["acknowledgements"][connection]=json!({});}
 p["acknowledgements"][connection][tool]=json!({"enabled":enabled,"synced":synced,"revision":p["revision"],"observed_at":crate::now_millis()});
 let tmp=data.join("tool_policy.json.tmp");
 std::fs::write(&tmp,serde_json::to_vec(&p).map_err(|_|"TOOL_POLICY_INVALID")?).map_err(|_|"TOOL_POLICY_WRITE_FAILED")?;
 std::fs::rename(tmp,data.join("tool_policy.json")).map_err(|_|"TOOL_POLICY_WRITE_FAILED")
}
