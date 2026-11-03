//! Read-only multi-harness remote directory & session history reader (AGY, Claude, Pi, Codex, DSH/Control-Plane).
//! No model execution or private credential decoding is performed here.
use std::{fs, io::{Read, Write}, path::Path, process::{Command, Stdio}, thread, time::{Duration, Instant}};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all="camelCase")]
struct HostGrant {
    connection_id: String,
    ssh_target: String,
    #[serde(default)]
    agy_data_dir: String,
    #[serde(default)]
    claude_data_dir: Option<String>,
    #[serde(default)]
    pi_data_dir: Option<String>,
    #[serde(default)]
    codex_data_dir: Option<String>,
    #[serde(default)]
    control_plane_dbs: Vec<String>,
    #[serde(default)]
    harnesses: Vec<String>,
}

impl HostGrant {
    fn has_multi(&self) -> bool {
        self.claude_data_dir.is_some()
            || self.pi_data_dir.is_some()
            || self.codex_data_dir.is_some()
            || !self.control_plane_dbs.is_empty()
    }
}

fn grants(data: &Path) -> Result<Vec<HostGrant>, &'static str> {
    let p=data.join("device_harnesses.json");
    if !p.exists() { return Ok(vec![]) }
    if fs::metadata(&p).map_err(|_|"AGY_CONFIG_UNAVAILABLE")?.len()>65536 { return Err("AGY_CONFIG_INVALID") }
    serde_json::from_slice(&fs::read(p).map_err(|_|"AGY_CONFIG_UNAVAILABLE")?).map_err(|_|"AGY_CONFIG_INVALID")
}

pub fn owns_session(id: &str) -> bool { id.starts_with("agy:") }

pub fn owns_readable_session(id: &str) -> bool {
    id.starts_with("agy:")
        || id.starts_with("claude-jsonl:")
        || id.starts_with("pi:")
        || id.starts_with("codex:")
        || id.starts_with("cp:")
}

pub fn list(data: &Path, connection: &str) -> Result<Vec<Value>, &'static str> {
    use sha2::{Sha256,Digest};
    let config=data.join("device_harnesses.json");
    if !config.exists() { return Ok(vec![]) }
    let raw=fs::read(&config).map_err(|_|"AGY_CONFIG_UNAVAILABLE")?;
    if raw.len()>65536 { return Err("AGY_CONFIG_INVALID") }
    let mut hash=Sha256::new();hash.update(connection.as_bytes());hash.update(&raw);
    let path=data.join(format!("agy-directory-{:x}.json",hash.finalize()));
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let cached=fs::metadata(&path).ok().filter(|m|m.len()<=2*1024*1024).and_then(|_|fs::read(&path).ok()).and_then(|b|serde_json::from_slice::<Value>(&b).ok());
    let age=cached.as_ref().and_then(|v|v["at"].as_u64()).map(|t|now.saturating_sub(t));
    if age.is_some_and(|a|a<60) {
        if let Some(rows)=cached.as_ref().and_then(|v|v["rows"].as_array()) {return Ok(rows.clone())}
    }
    if age.is_some_and(|a|a<7*24*3600) {
        if let Some(rows)=cached.as_ref().and_then(|v|v["rows"].as_array()) {
            let stale_rows: Vec<Value> = rows.iter().cloned().map(|mut v|{if age.unwrap_or(0)>300 {v["directoryStale"]=json!(true);}v}).collect();
            let data_buf = data.to_path_buf();
            let conn_str = connection.to_owned();
            let path_buf = path.clone();
            let rows_clone = rows.clone();
            let _ = fs::write(&path, serde_json::to_vec(&json!({"at":now,"rows":rows_clone})).unwrap_or_default());
            thread::spawn(move || {
                if let Ok(fresh) = fetch_list(&data_buf, &conn_str) {
                    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                    let _ = fs::write(path_buf, serde_json::to_vec(&json!({"at":stamp,"rows":fresh})).unwrap_or_default());
                }
            });
            return Ok(stale_rows);
        }
    }
    match fetch_list(data,connection) {
        Ok(rows)=>{let _=fs::write(path,serde_json::to_vec(&json!({"at":now,"rows":rows})).unwrap_or_default());Ok(rows)},
        Err(e)=>Err(e)
    }
}

fn compact_python(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let spaces = line.len() - line.trim_start_matches(' ').len();
        for _ in 0..(spaces / 4) {
            out.push(' ');
        }
        out.push_str(line.trim_start_matches(' '));
        out.push('\n');
    }
    out
}

fn run_ssh_python(ssh_target: &str, arg_str: &str, timeout_secs: u64) -> Result<Vec<u8>, &'static str> {
    if ssh_target.is_empty() || ssh_target.starts_with('-') || !ssh_target.bytes().all(|c|c.is_ascii_alphanumeric()||b"-_.@".contains(&c)) {
        return Err("AGY_CONFIG_INVALID");
    }
    let encoded=STANDARD.encode(arg_str.as_bytes());
    let compacted=compact_python(include_str!("agy_index_probe.py"));
    let remote_cmd=format!("python3 -c \"import sys,base64;s=sys.stdin.read();sys.argv=['probe',base64.b64decode('{encoded}').decode('utf-8')];exec(s,{{'__name__':'__main__','__BOB_PROBE_SRC__':s}})\"");
    let mut command=Command::new("ssh");
    command.args(["-o","BatchMode=yes","-o","ConnectTimeout=15","-o","ServerAliveInterval=5","-o","ServerAliveCountMax=2","--",ssh_target,&remote_cmd])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child=command.spawn().map_err(|_|"AGY_HOST_UNAVAILABLE")?;
    if let Some(mut stdin)=child.stdin.take() {
        let _=stdin.write_all(compacted.as_bytes());
    }
    let stdout=child.stdout.take().ok_or("AGY_HOST_UNAVAILABLE")?;
    let reader=thread::spawn(move || { let mut b=Vec::new();stdout.take(2*1024*1024+1).read_to_end(&mut b).map(|_|b) });
    let started=Instant::now();
    let success=loop {
        match child.try_wait() {
            Ok(Some(s))=>break s.success(),
            Ok(None) if started.elapsed()<Duration::from_secs(timeout_secs)=>thread::sleep(Duration::from_millis(40)),
            _=>{let _=child.kill();let _=child.wait();return Err("AGY_HOST_TIMEOUT")}
        }
    };
    let b=reader.join().map_err(|_|"AGY_INDEX_UNAVAILABLE")?.map_err(|_|"AGY_INDEX_UNAVAILABLE")?;
    if !success || b.len()>2*1024*1024 { return Err("AGY_INDEX_UNAVAILABLE") }
    Ok(b)
}

fn fetch_list(data: &Path, connection: &str) -> Result<Vec<Value>, &'static str> {
    let all=grants(data)?;
    let Some(g)=all.iter().find(|g|g.connection_id==connection) else { return Ok(vec![]) };
    if !g.agy_data_dir.is_empty() && !g.agy_data_dir.starts_with('/') { return Err("AGY_CONFIG_INVALID") }
    let arg_str = if g.has_multi() {
        serde_json::to_string(&json!({
            "mode": "list",
            "agyDataDir": g.agy_data_dir,
            "claudeDataDir": g.claude_data_dir,
            "piDataDir": g.pi_data_dir,
            "codexDataDir": g.codex_data_dir,
            "controlPlaneDbs": g.control_plane_dbs,
            "harnesses": g.harnesses,
        })).map_err(|_|"AGY_CONFIG_INVALID")?
    } else {
        if g.agy_data_dir.is_empty() { return Ok(vec![]) }
        g.agy_data_dir.clone()
    };
    let b = run_ssh_python(&g.ssh_target, &arg_str, 35)?;
    let v:Value=serde_json::from_slice(&b).map_err(|_|"AGY_INDEX_INVALID")?;
    let rows=v["sessions"].as_array().ok_or("AGY_INDEX_INVALID")?;
    rows.iter().map(|s|normalize(connection,s)).collect()
}

pub fn read_page(data: &Path, connection: &str, session_id: &str, after: Option<i64>) -> Result<Value, &'static str> {
    use sha2::{Sha256,Digest};
    if !owns_readable_session(session_id) {
        return Err("DSH_SESSION_NOT_FOUND");
    }
    let all=grants(data)?;
    let Some(g)=all.iter().find(|g|g.connection_id==connection) else { return Err("DSH_CONNECTION_REQUIRED") };
    let mut hash=Sha256::new();
    hash.update(connection.as_bytes());
    hash.update(b":");
    hash.update(session_id.as_bytes());
    let cache_path=data.join(format!("remote-page-{:x}.json",hash.finalize()));
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Some(cached)=fs::read(&cache_path).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()) {
        let age=cached["at"].as_u64().map(|t|now.saturating_sub(t)).unwrap_or(9999);
        let has_running=cached["page"]["request_results"].as_array().is_some_and(|arr|arr.iter().any(|r|r["status"].as_str()==Some("running")));
        let ttl=if has_running || after.is_some() { 2 } else { 15 };
        if age<ttl {
            if let Some(mut page)=cached.get("page").cloned() {
                let cursor=page["cursor"].as_i64().unwrap_or(0);
                if after.is_some_and(|a|a>=cursor) && !has_running {
                    page["not_modified"]=json!(true);
                    page["messages"]=json!([]);
                }
                return Ok(page);
            }
        }
    }
    let arg_str=serde_json::to_string(&json!({
        "mode": "page",
        "sessionId": session_id,
        "agyDataDir": g.agy_data_dir,
        "claudeDataDir": g.claude_data_dir,
        "piDataDir": g.pi_data_dir,
        "codexDataDir": g.codex_data_dir,
        "controlPlaneDbs": g.control_plane_dbs,
    })).map_err(|_|"AGY_CONFIG_INVALID")?;
    let b=run_ssh_python(&g.ssh_target, &arg_str, 35)?;
    let raw:Value=serde_json::from_slice(&b).map_err(|_|"DSH_HISTORY_UNAVAILABLE")?;
    let msgs=raw["messages"].as_array().cloned().unwrap_or_default();
    let req_results=raw["request_results"].as_array().cloned().unwrap_or_default();
    let cursor=msgs.len() as i64;
    let harness=raw["harness"].as_str().unwrap_or("dsh");
    let workspace=raw["workspace"].as_str().unwrap_or("");
    let mut page=json!({
        "address": { "connection_id": connection, "session_id": session_id },
        "harness": harness,
        "workspace": workspace,
        "cursor": cursor,
        "before": if msgs.is_empty() { 0 } else { 1 },
        "has_more": false,
        "not_modified": false,
        "replacements": [],
        "messages": msgs,
        "request_results": req_results,
    });
    let _=fs::write(&cache_path, serde_json::to_vec(&json!({"at":now,"page":page})).unwrap_or_default());
    let has_running=req_results.iter().any(|r|r["status"].as_str()==Some("running"));
    if after.is_some_and(|a|a>=cursor) && !has_running {
        page["not_modified"]=json!(true);
        page["messages"]=json!([]);
    }
    Ok(page)
}

pub fn prompt_session(data: &Path, connection: &str, session_id: &str, text: &str, request_id: &str) -> Result<Value, &'static str> {
    use sha2::{Sha256,Digest};
    if !owns_readable_session(session_id) {
        return Err("DSH_SESSION_NOT_FOUND");
    }
    let all=grants(data)?;
    let Some(g)=all.iter().find(|g|g.connection_id==connection) else { return Err("DSH_CONNECTION_REQUIRED") };
    let arg_str=serde_json::to_string(&json!({
        "mode": "prompt",
        "sessionId": session_id,
        "text": text,
        "requestId": request_id,
        "agyDataDir": g.agy_data_dir,
        "claudeDataDir": g.claude_data_dir,
        "piDataDir": g.pi_data_dir,
        "codexDataDir": g.codex_data_dir,
        "controlPlaneDbs": g.control_plane_dbs,
    })).map_err(|_|"AGY_CONFIG_INVALID")?;
    let b=run_ssh_python(&g.ssh_target, &arg_str, 35)?;
    let res:Value=serde_json::from_slice(&b).map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
    if res.get("error").is_some() {
        return Err("DSH_RECEIPT_UNKNOWN");
    }
    let mut hash=Sha256::new();
    hash.update(connection.as_bytes());
    hash.update(b":");
    hash.update(session_id.as_bytes());
    let cache_path=data.join(format!("remote-page-{:x}.json",hash.finalize()));
    let _=fs::remove_file(cache_path);
    Ok(res)
}

pub fn inspect_receipt(data: &Path, connection: &str, session_id: &str, text: &str, request_id: &str) -> Result<Value, &'static str> {
    if !owns_readable_session(session_id) {
        return Err("DSH_SESSION_NOT_FOUND");
    }
    let all=grants(data)?;
    let Some(g)=all.iter().find(|g|g.connection_id==connection) else { return Err("DSH_CONNECTION_REQUIRED") };
    let arg_str=serde_json::to_string(&json!({
        "mode": "receipt",
        "sessionId": session_id,
        "text": text,
        "requestId": request_id,
        "claudeDataDir": g.claude_data_dir,
        "piDataDir": g.pi_data_dir,
        "codexDataDir": g.codex_data_dir,
        "controlPlaneDbs": g.control_plane_dbs,
    })).map_err(|_|"AGY_CONFIG_INVALID")?;
    let b=run_ssh_python(&g.ssh_target, &arg_str, 35)?;
    let res:Value=serde_json::from_slice(&b).map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
    Ok(res)
}

fn normalize(connection: &str, s: &Value)->Result<Value,&'static str>{
    if let Some(explicit_id)=s.get("id").and_then(Value::as_str) {
        if explicit_id.is_empty() || explicit_id.contains("..") { return Err("AGY_INDEX_INVALID") }
        let native_id=s["nativeSessionId"].as_str().unwrap_or(explicit_id);
        let harness=s["harness"].as_str().unwrap_or("dsh");
        let tool_label=s["toolLabel"].as_str().unwrap_or("DSH");
        let source_kind=s["sourceKind"].as_str().unwrap_or("remote_native");
        let uri=s["workspaceUris"].as_array().and_then(|x|x.first()).and_then(Value::as_str).unwrap_or("");
        let readable=s["historyReadable"].as_bool().unwrap_or(true);
        return Ok(json!({
            "id":explicit_id,
            "nativeSessionId":native_id,
            "connectionId":connection,
            "harness":harness,
            "toolLabel":tool_label,
            "source_kind":source_kind,
            "title":s["title"].as_str().filter(|x|!x.is_empty()).unwrap_or(native_id),
            "workspace":uri.strip_prefix("file://").unwrap_or(uri),
            "updated_at":s["updatedAt"],
            "historyReadable":readable,
            "historyReason":s["historyReason"],
            "readOnlyDirectory":!readable
        }));
    }
    let id=s["nativeSessionId"].as_str().ok_or("AGY_INDEX_INVALID")?;
    if id.len()!=36 || !id.bytes().all(|b|b.is_ascii_hexdigit()||b==b'-') { return Err("AGY_INDEX_INVALID") }
    let uri=s["workspaceUris"].as_array().and_then(|x|x.first()).and_then(Value::as_str).unwrap_or("");
    Ok(json!({"id":format!("agy:{id}"),"nativeSessionId":id,"connectionId":connection,
        "harness":"agy","toolLabel":"AGY","source_kind":"agy_native",
        "title":s["title"].as_str().filter(|x|!x.is_empty()).unwrap_or(id),
        "workspace":uri.strip_prefix("file://").unwrap_or(uri),"updated_at":s["updatedAt"],
        "historyReadable":false,"historyReason":s["historyReason"],"readOnlyDirectory":true}))
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn identity_and_unavailable_history_are_explicit(){
  let s=json!({"nativeSessionId":"00000000-0000-0000-0000-000000000001","title":"Project","workspaceUris":["file:///work/project"],"updatedAt":"2026-11-02","historyReason":"AGY_HISTORY_CODEC_UNSUPPORTED"});
  let v=normalize("device",&s).unwrap();assert!(owns_session(v["id"].as_str().unwrap()));assert_eq!(v["workspace"],"/work/project");assert_eq!(v["historyReadable"],false);
  assert!(normalize("device",&json!({"nativeSessionId":"../unsafe"})).is_err());
 }
 #[test] fn multi_harness_normalization_preserves_tool_badges(){
  let s=json!({"id":"pi:01a099bb","nativeSessionId":"01a099bb","harness":"pi","toolLabel":"Pi","sourceKind":"pi_native","title":"Snake D-pad","workspaceUris":["/home/ubuntu/snake"],"updatedAt":1790000000000i64,"historyReadable":true});
  let v=normalize("node-vps3",&s).unwrap();
  assert!(owns_readable_session(v["id"].as_str().unwrap()));
  assert_eq!(v["harness"],"pi");
  assert_eq!(v["toolLabel"],"Pi");
  assert_eq!(v["historyReadable"],true);
 }
 #[test] fn unconfigured_device_is_empty(){let d=tempfile::tempdir().unwrap();assert!(list(d.path(),"none").unwrap().is_empty())}
}
