//! Read-only AGY directory attached to an existing device connection.
//! No model execution or private history decoding is performed here.
use std::{fs, io::{Read, Write}, path::Path, process::{Command, Stdio}, thread, time::{Duration, Instant}};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
struct HostGrant { connection_id: String, ssh_target: String, agy_data_dir: String }

fn grants(data: &Path) -> Result<Vec<HostGrant>, &'static str> {
    let p=data.join("device_harnesses.json");
    if !p.exists() { return Ok(vec![]) }
    if fs::metadata(&p).map_err(|_|"AGY_CONFIG_UNAVAILABLE")?.len()>65536 { return Err("AGY_CONFIG_INVALID") }
    serde_json::from_slice(&fs::read(p).map_err(|_|"AGY_CONFIG_UNAVAILABLE")?).map_err(|_|"AGY_CONFIG_INVALID")
}

pub fn owns_session(id: &str) -> bool { id.starts_with("agy:") }

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

fn fetch_list(data: &Path, connection: &str) -> Result<Vec<Value>, &'static str> {
    let all=grants(data)?;
    let Some(g)=all.iter().find(|g|g.connection_id==connection) else { return Ok(vec![]) };
    if g.ssh_target.is_empty() || g.ssh_target.starts_with('-') || !g.ssh_target.bytes().all(|c|c.is_ascii_alphanumeric()||b"-_.@".contains(&c)) || !g.agy_data_dir.starts_with('/') { return Err("AGY_CONFIG_INVALID") }
    let root=STANDARD.encode(g.agy_data_dir.as_bytes());
    let code=format!("import sys,base64\nsys.argv=['probe',base64.b64decode('{root}').decode('utf-8')]\n{}",include_str!("agy_index_probe.py"));
    let mut command=Command::new("ssh");
    command.args(["-o","BatchMode=yes","-o","ConnectTimeout=3","-o","ServerAliveInterval=5","-o","ServerAliveCountMax=2","--",&g.ssh_target,"python3 -"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
    let mut child=command.spawn().map_err(|_|"AGY_HOST_UNAVAILABLE")?;
    let mut stdin=child.stdin.take().ok_or("AGY_HOST_UNAVAILABLE")?;
    let writer=thread::spawn(move || stdin.write_all(code.as_bytes()));
    let stdout=child.stdout.take().ok_or("AGY_HOST_UNAVAILABLE")?;
    let reader=thread::spawn(move || { let mut b=Vec::new();stdout.take(2*1024*1024+1).read_to_end(&mut b).map(|_|b) });
    let started=Instant::now();
    let success=loop {
        match child.try_wait() {
            Ok(Some(s))=>break s.success(),
            Ok(None) if started.elapsed()<Duration::from_secs(6)=>thread::sleep(Duration::from_millis(40)),
            _=>{let _=child.kill();let _=child.wait();return Err("AGY_HOST_TIMEOUT")}
        }
    };
    writer.join().map_err(|_|"AGY_HOST_UNAVAILABLE")?.map_err(|_|"AGY_HOST_UNAVAILABLE")?;
    let b=reader.join().map_err(|_|"AGY_INDEX_UNAVAILABLE")?.map_err(|_|"AGY_INDEX_UNAVAILABLE")?;
    if !success || b.len()>2*1024*1024 { return Err("AGY_INDEX_UNAVAILABLE") }
    let v:Value=serde_json::from_slice(&b).map_err(|_|"AGY_INDEX_INVALID")?;
    let rows=v["sessions"].as_array().ok_or("AGY_INDEX_INVALID")?;
    rows.iter().map(|s|normalize(connection,s)).collect()
}

fn normalize(connection: &str, s: &Value)->Result<Value,&'static str>{
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
 #[test] fn unconfigured_device_is_empty(){let d=tempfile::tempdir().unwrap();assert!(list(d.path(),"none").unwrap().is_empty())}
}
