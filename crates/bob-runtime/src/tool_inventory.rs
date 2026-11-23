//! Node-owned tool inventory; history never grants creation authority.
use std::{path::{Path,PathBuf},sync::{Mutex,OnceLock},collections::HashSet,time::Duration};
use serde_json::{Value,json};
static UPDATING: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
pub fn executable(tool:&str)->Option<PathBuf>{
 let home=std::env::var_os("BOB_LOCAL_HOME").or_else(||std::env::var_os("HOME")).or_else(||std::env::var_os("USERPROFILE")).map(PathBuf::from)?;
 let mut dirs=vec![home.join(".local/bin"),home.join(".npm-global/bin"),home.join(".cargo/bin")];
 if let Some(path)=std::env::var_os("PATH"){dirs.extend(std::env::split_paths(&path));}
 let aliases = crate::harness_registry::executable_aliases(tool);
 let names: Vec<&str> = if aliases.is_empty() { vec![tool] } else { aliases.to_vec() };
 for name in names {
  if let Some(found) = dirs.iter().flat_map(|d|{
   #[cfg(windows)] {vec![d.join(name),d.join(format!("{name}.exe")),d.join(format!("{name}.cmd"))]}
   #[cfg(not(windows))] {vec![d.join(name)]}
  }).find(|p|{
   if !p.is_file(){return false;}
   #[cfg(unix)] {use std::os::unix::fs::PermissionsExt; p.metadata().map(|m|m.permissions().mode() & 0o111 != 0).unwrap_or(false)}
   #[cfg(not(unix))] {true}
  }) {
   return Some(found);
  }
 }
 None
}
fn parse_codex_models_cache(bytes: &[u8]) -> Option<Vec<Value>> {
 let parsed: Value = serde_json::from_slice(bytes).ok()?;
 let mut items: Vec<(i64, Value)> = parsed["models"].as_array()?
  .iter()
  .filter(|m| m["visibility"].as_str() == Some("list"))
  .filter_map(|m| {
   let slug = m["slug"].as_str()?.trim();
   if slug.is_empty() { return None; }
   let label = m["display_name"].as_str().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(slug);
   let efforts: Vec<String> = m["supported_reasoning_levels"].as_array()
    .map(|arr| arr.iter().filter_map(|r| r["effort"].as_str().map(str::to_owned)).collect())
    .unwrap_or_default();
   let priority = m["priority"].as_i64().unwrap_or(9999);
   Some((priority, json!({"id": slug, "label": label, "efforts": efforts})))
  })
  .collect();
 if items.is_empty() { return None; }
 items.sort_by_key(|(p, _)| *p);
 Some(items.into_iter().map(|(_, v)| v).collect())
}
fn read_codex_models() -> Option<Vec<Value>> {
 let codex_home = std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| {
  std::env::var_os("BOB_LOCAL_HOME").or_else(|| std::env::var_os("HOME")).or_else(|| std::env::var_os("USERPROFILE")).map(|h| PathBuf::from(h).join(".codex"))
 })?;
 let bytes = std::fs::read(codex_home.join("models_cache.json")).ok()?;
 parse_codex_models_cache(&bytes)
}
fn read(data:&Path)->Value{std::fs::read(data.join("tool_inventory.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_else(||json!({"version":1,"observed_at":0,"tools":[]}))}
fn save(data:&Path,v:&Value){let p=data.join("tool_inventory.json.tmp");if let Ok(b)=serde_json::to_vec(v){if std::fs::write(&p,b).is_ok(){let _=std::fs::rename(p,data.join("tool_inventory.json"));}}}
pub fn refresh(data:&Path){
 let lock=UPDATING.get_or_init(||Mutex::new(HashSet::new()));
 {let mut active=lock.lock().unwrap();if !active.insert(data.to_path_buf()){return;}}
 let data=data.to_path_buf();std::thread::spawn(move||{
  let _=crate::agy_directory::ensure_local_grant_registered(&data,"local");
  let mut v=read(&data);let previous=v["tools"].as_array().cloned().unwrap_or_default();let mut tools=Vec::new();
  for tool in crate::harness_registry::INVENTORY_TOOL_IDS {
   let old=previous.iter().find(|x|x["id"]==tool);let now=crate::now_millis();
   let dispatchable = crate::harness_registry::is_dispatchable_tool(tool);
   let row=if !crate::tool_policy::enabled(&data,"local",tool){
    let mut x=old.cloned().unwrap_or_else(||json!({"id":tool,"installed":executable(tool).is_some(),"observed_at":0}));
    x["enabled"]=json!(false);x["can_create"]=json!(false);x["reason"]=json!("TOOL_DISABLED");x
   }else if tool=="dsh"{
    match crate::ModelHubRuntime::with_file_vault(data.clone()).and_then(|r|r.dsh_client("local")).and_then(|c|c.list()){
     Ok(_)=>json!({"id":tool,"installed":true,"auth_state":"authenticated","can_create":true,"observed_at":now,"stale":false}),
     Err(e)=>{let mut x=old.cloned().unwrap_or_else(||json!({"id":tool,"installed":null,"auth_state":"unknown","can_create":false,"observed_at":0}));x["stale"]=json!(true);x["reason"]=json!(e);x}
    }
   }else if executable(tool).is_none(){json!({"id":tool,"installed":false,"auth_state":"unknown","can_create":false,"observed_at":now,"stale":false})}
   else{
    match crate::agy_directory::probe_native_harness(&data,"local",tool,false){
     Ok(p) if matches!(p["reason"].as_str(),Some("PROBE_TIMEOUT"|"PROBE_UNAVAILABLE"))=>{
      let mut x=old.cloned().unwrap_or_else(||json!({"id":tool,"installed":true,"auth_state":"unknown","can_create":false,"observed_at":0}));
      x["stale"]=json!(true);x["reason"]=p["reason"].clone();x
     },
     Ok(p) if p["installed"]==true=>{
      let can_create = dispatchable
       && p["auth_state"]!="login_required"
       && (p["protocol_verified"]==true || (tool=="claude" && p["auth_state"]=="authenticated"))
       && p["reason"]!="NO_AVAILABLE_MODEL"
       && p["reason"]!="CONFIGURED_MODEL_UNAVAILABLE";
      let mut row=json!({"id":tool,"installed":true,"auth_state":p["auth_state"],"version":p["version"],"can_create":can_create,"reason":p["reason"],"observed_at":now,"stale":false});
      if tool=="codex" { if let Some(models)=read_codex_models() { row["models"]=json!(models); } }
      row
     },
     _=>{let mut x=old.cloned().unwrap_or_else(||json!({"id":tool,"installed":true,"auth_state":"unknown","can_create":false,"observed_at":0}));x["stale"]=json!(true);x["reason"]=json!("HARNESS_PROBE_UNCONFIRMED");x}
    }
   };
   tools.push(row);let mut merged=tools.clone();merged.extend(previous.iter().filter(|x|!tools.iter().any(|t|t["id"]==x["id"])).cloned());
   v["tools"]=json!(merged);v["observed_at"]=json!(now);save(&data,&v);
  }
  for custom in crate::harness_registry::load_custom_acp_registry(&data) {
   let now = crate::now_millis();
   let installed = executable(&custom.executable).is_some();
   let row = json!({
    "id": custom.id,
    "label": custom.label,
    "installed": installed,
    "auth_state": "unknown",
    "can_create": false,
    "custom_acp": true,
    "reason": if installed { "CUSTOM_ACP_DISCOVERED" } else { "EXECUTABLE_NOT_FOUND" },
    "observed_at": now,
    "stale": false
   });
   tools.push(row);
   let mut merged=tools.clone();merged.extend(previous.iter().filter(|x|!tools.iter().any(|t|t["id"]==x["id"])).cloned());
   v["tools"]=json!(merged);v["observed_at"]=json!(now);save(&data,&v);
  }
  lock.lock().unwrap().remove(&data);
 });
}
pub fn start_monitor(data:PathBuf){std::thread::spawn(move||loop{refresh(&data);std::thread::sleep(Duration::from_secs(120));});}
pub fn snapshot(data:&Path,force:bool)->Result<Value,&'static str>{
 let mut v=read(data);let age=crate::now_millis()-v["observed_at"].as_i64().unwrap_or(0);
 if force||age>120_000{refresh(data);}
 v["stale"]=json!(age>300_000);v["refreshing"]=json!(UPDATING.get_or_init(||Mutex::new(HashSet::new())).lock().unwrap().contains(data));
  if let Some(rows)=v["tools"].as_array_mut(){for row in rows{
   let id=row["id"].as_str().unwrap_or("").to_owned();let enabled=crate::tool_policy::enabled(data,"local",&id);
   row["global_enabled"]=crate::tool_policy::read(data)?["tools"][&id].as_bool().map(Value::Bool).unwrap_or(json!(true));
   row["enabled"]=json!(enabled);if !enabled{row["can_create"]=json!(false);row["reason"]=json!("TOOL_DISABLED");}
   if id=="codex" && row.get("models").and_then(Value::as_array).map(|a|a.is_empty()).unwrap_or(true) {
    if let Some(models)=read_codex_models() { row["models"]=json!(models); }
   }
  }}
 v["policy_revision"]=crate::tool_policy::read(data)?["revision"].clone();
 v["workspaces"]=serde_json::to_value(crate::host_tools::WorkspaceGrantStore::open(data)?.list_workspaces()).map_err(|_|"HOST_WORKSPACES_READ_FAILED")?;
 Ok(v)
}
pub fn authorize(data:&Path,args:&Value)->Result<Value,&'static str>{
 if args["connectionId"]!="local"{return Err("HARNESS_LOCAL_NODE_REQUIRED");}
 let path=args["path"].as_str().ok_or("HOST_PATH_EMPTY")?;
 if !Path::new(path).is_absolute(){return Err("HOST_PATH_EMPTY");}
 let mut store=crate::host_tools::WorkspaceGrantStore::open(data)?;
 let canonical=std::fs::canonicalize(path).map_err(|_|"HOST_DIR_NOT_FOUND")?;
 if store.list_workspaces().iter().any(|w|w.read_only && canonical.starts_with(&w.path)){return Err("HOST_WORKSPACE_READ_ONLY");}
 serde_json::to_value(store.authorize_workspace(path,"",false,crate::now_millis())?).map_err(|_|"HOST_WORKSPACES_WRITE_FAILED")
}

#[cfg(test)]
mod tests {
 use super::*;
 fn temp()->PathBuf {let p=std::env::temp_dir().join(format!("bob-inventory-test-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir_all(&p).unwrap();p}
 #[test] fn cached_inventory_is_independent_of_history(){
  let p=temp();save(&p,&json!({"version":1,"observed_at":crate::now_millis(),"tools":[{"id":"codex","installed":true,"can_create":true}]}));
  let v=snapshot(&p,false).unwrap();assert_eq!(v["tools"][0]["can_create"],true);assert_eq!(v["workspaces"],json!([]));assert!(!p.join("device_harnesses.json").exists());std::fs::remove_dir_all(p).unwrap();
 }
 #[test] fn workspace_authorization_needs_no_history_but_respects_readonly(){
  let p=temp();let work=p.join("work");std::fs::create_dir(&work).unwrap();
  assert!(authorize(&p,&json!({"connectionId":"remote","path":work})).is_err());
  assert!(authorize(&p,&json!({"connectionId":"local","path":"relative"})).is_err());
  authorize(&p,&json!({"connectionId":"local","path":work})).unwrap();
  let mut store=crate::host_tools::WorkspaceGrantStore::open(&p).unwrap();
  assert!(store.resolve_authorized_path(work.to_str().unwrap(),true).is_ok());
  store.authorize_workspace(work.to_str().unwrap(),"",true,crate::now_millis()).unwrap();
  assert_eq!(authorize(&p,&json!({"connectionId":"local","path":work})),Err("HOST_WORKSPACE_READ_ONLY"));
  std::fs::remove_dir_all(p).unwrap();
 }
 #[test] fn parse_codex_models_cache_filters_visible_and_sorts_by_priority(){
  let raw=br#"{"models":[{"slug":"hidden-m","visibility":"hide","priority":0},{"slug":"gpt-6.1-sol","display_name":"GPT-6.1-Sol","visibility":"list","priority":2,"supported_reasoning_levels":[{"effort":"low"},{"effort":"xhigh"}]},{"slug":"gpt-6.2-sol","display_name":"GPT-6.2-Sol","visibility":"list","priority":1,"supported_reasoning_levels":[{"effort":"low"},{"effort":"ultra"}]}]}"#;
  let models=parse_codex_models_cache(raw).unwrap();
  assert_eq!(models.len(),2);
  assert_eq!(models[0]["id"],"gpt-6.2-sol");
  assert_eq!(models[0]["label"],"GPT-6.2-Sol");
  assert_eq!(models[0]["efforts"],json!(["low","ultra"]));
  assert_eq!(models[1]["id"],"gpt-6.1-sol");
 }
}
