//! Multi-harness directory, history and authorized native-session transport.
//! Execution and approval remain bound to the host grant and original session.
use std::{fs, io::{Read, Write}, path::Path, process::{Command, Stdio}, thread, time::{Duration, Instant}};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all="camelCase")]
struct HostGrant {
    #[serde(alias = "connection_id")]
    connection_id: String,
    #[serde(alias = "ssh_target")]
    ssh_target: String,
    #[serde(default, alias = "agy_data_dir")]
    agy_data_dir: String,
    #[serde(default, alias = "claude_data_dir")]
    claude_data_dir: Option<String>,
    #[serde(default, alias = "pi_data_dir")]
    pi_data_dir: Option<String>,
    #[serde(default, alias = "codex_data_dir")]
    codex_data_dir: Option<String>,
    #[serde(default, alias = "control_plane_dbs")]
    control_plane_dbs: Vec<String>,
    #[serde(default)]
    harnesses: Vec<String>,
    #[serde(default, alias = "claude_tools")]
    claude_tools: Vec<String>,
}

impl HostGrant {
    fn has_multi(&self) -> bool {
        self.claude_data_dir.is_some()
            || self.pi_data_dir.is_some()
            || self.codex_data_dir.is_some()
            || !self.control_plane_dbs.is_empty()
    }
    fn is_local(&self) -> bool {
        matches!(
            self.ssh_target.as_str(),
            "local" | "self" | "localhost" | "127.0.0.1"
        )
    }
    fn effective_claude_tools(&self) -> Vec<String> {
        if !self.claude_tools.is_empty() {
            return self.claude_tools.clone();
        }
        if self.harnesses.iter().any(|h| h == "claude") {
            return vec![
                "Read".to_owned(),
                "Grep".to_owned(),
                "Glob".to_owned(),
                "Bash".to_owned(),
                "Edit".to_owned(),
                "Write".to_owned(),
                "AskUserQuestion".to_owned(),
                "WebFetch".to_owned(),
                "WebSearch".to_owned(),
            ];
        }
        Vec::new()
    }
}

pub fn discover_local_grant(connection_id: &str) -> Option<Value> {
    let home = std::env::var_os("BOB_LOCAL_HOME")
        .or_else(|| std::env::var_os("HOME"))
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)?;
    if !home.is_dir() {
        return None;
    }
    let agy_cli = home.join(".gemini").join("antigravity-cli");
    let agy_gui = home.join(".gemini").join("antigravity");
    let agy_dir = if agy_cli.is_dir() {
        agy_cli.to_string_lossy().replace('\\', "/")
    } else if agy_gui.is_dir() {
        agy_gui.to_string_lossy().replace('\\', "/")
    } else {
        String::new()
    };
    let claude_dir = home.join(".claude");
    let claude_opt = claude_dir
        .is_dir()
        .then(|| claude_dir.to_string_lossy().replace('\\', "/"));
    let pi_dir = home.join(".pi").join("agent").join("sessions");
    let pi_opt = pi_dir
        .is_dir()
        .then(|| pi_dir.to_string_lossy().replace('\\', "/"));
    let codex_dir = home.join(".codex");
    let codex_opt = codex_dir
        .is_dir()
        .then(|| codex_dir.to_string_lossy().replace('\\', "/"));

    let mut cp_dbs = Vec::new();
    let bob_state = home.join(".bob").join("state");
    if let Ok(entries) = fs::read_dir(&bob_state) {
        for e in entries.flatten() {
            let db = e.path().join("nexus_control_plane.sqlite");
            if db.is_file() {
                cp_dbs.push(db.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let opt_db = std::path::Path::new("/opt/bob-online/data/nexus_control_plane.sqlite");
    if opt_db.is_file() {
        cp_dbs.push(opt_db.to_string_lossy().into_owned());
    }

    let mut harnesses = Vec::new();
    if !cp_dbs.is_empty() {
        harnesses.push("dsh".to_owned());
    }
    if claude_opt.is_some() {
        harnesses.push("claude".to_owned());
    }
    if pi_opt.is_some() {
        harnesses.push("pi".to_owned());
    }
    if codex_opt.is_some() {
        harnesses.push("codex".to_owned());
    }
    if !agy_dir.is_empty() {
        harnesses.push("agy".to_owned());
    }
    if harnesses.is_empty() {
        return None;
    }
    let claude_tools: Vec<String> = if claude_opt.is_some() {
        vec![
            "Read".to_owned(),
            "Grep".to_owned(),
            "Glob".to_owned(),
            "Bash".to_owned(),
            "Edit".to_owned(),
            "Write".to_owned(),
            "AskUserQuestion".to_owned(),
            "WebFetch".to_owned(),
            "WebSearch".to_owned(),
        ]
    } else {
        Vec::new()
    };
    Some(json!({
        "connectionId": connection_id,
        "sshTarget": "local",
        "agyDataDir": agy_dir,
        "claudeDataDir": claude_opt,
        "piDataDir": pi_opt,
        "codexDataDir": codex_opt,
        "controlPlaneDbs": cp_dbs,
        "harnesses": harnesses,
        "claudeTools": claude_tools,
    }))
}

pub fn ensure_local_grant_registered(data: &Path, connection_id: &str) -> Option<Value> {
    if std::env::var("BOB_AUTO_DISCOVER_LOCAL").ok().as_deref() == Some("0") {
        return None;
    }
    let grant = discover_local_grant(connection_id)?;
    let p = data.join("device_harnesses.json");
    let mut list: Vec<Value> = fs::read(&p)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if let Some(idx) = list
        .iter()
        .position(|x| x.get("connectionId").and_then(Value::as_str) == Some(connection_id))
    {
        if list[idx].get("sshTarget").and_then(Value::as_str) == Some("local") {
            list[idx] = grant.clone();
            let _ = fs::write(&p, serde_json::to_vec_pretty(&list).unwrap_or_default());
        }
        return Some(list[idx].clone());
    }
    list.push(grant.clone());
    let _ = fs::write(&p, serde_json::to_vec_pretty(&list).unwrap_or_default());
    Some(grant)
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
        || id.starts_with("claude:")
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

pub fn run_local_probe(arg_str: &str, timeout_secs: u64) -> Result<Vec<u8>, &'static str> {
    let source = compact_python(include_str!("agy_index_probe.py"));
    run_local_python(arg_str, timeout_secs, &source)
}

fn run_local_python(arg_str: &str, timeout_secs: u64, source: &str) -> Result<Vec<u8>, &'static str> {
    let encoded = STANDARD.encode(arg_str.as_bytes());
    let compacted = source.to_owned();
    let py_code = format!(
        "import sys,base64;s=sys.stdin.read();sys.argv=['probe',base64.b64decode('{encoded}').decode('utf-8')];exec(s,{{'__name__':'__main__','__BOB_PROBE_SRC__':s}})"
    );
    let mut child = Command::new("python3")
        .args(["-c", &py_code])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .or_else(|_| {
            Command::new("python")
                .args(["-c", &py_code])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
        })
        .map_err(|_| "AGY_HOST_UNAVAILABLE")?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(compacted.as_bytes());
    }
    let stdout = child.stdout.take().ok_or("AGY_HOST_UNAVAILABLE")?;
    let reader = thread::spawn(move || {
        let mut b = Vec::new();
        stdout.take(4 * 1024 * 1024 + 1).read_to_end(&mut b).map(|_| b)
    });
    let started = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.success(),
            Ok(None) if started.elapsed() < Duration::from_secs(timeout_secs) => {
                thread::sleep(Duration::from_millis(40))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("AGY_HOST_TIMEOUT");
            }
        }
    };
    let b = reader
        .join()
        .map_err(|_| "AGY_INDEX_UNAVAILABLE")?
        .map_err(|_| "AGY_INDEX_UNAVAILABLE")?;
    if !success || b.len() > 4 * 1024 * 1024 {
        return Err("AGY_INDEX_UNAVAILABLE");
    }
    Ok(b)
}


/// Render bytes already authorized and read by a transport. No path or host comes
/// from the request; the same bounded renderer is used for native and DSH files.
pub(crate) fn render_document_bytes(bytes: &[u8], extension: &str) -> Result<Value, &'static str> {
    if !["pdf","docx","xlsx","pptx","doc","xls","ppt","odt","ods","odp"].contains(&extension) {
        return Err("DSH_FILE_TYPE_UNSUPPORTED");
    }
    if bytes.len() >= 2*1024*1024 { return Err("DSH_FILE_TOO_LARGE"); }
    // Carry document bytes over stdin, never in argv (size limit and visibility).
    let source = format!("{}\ntry:\n print(json.dumps(_document_preview(base64.b64decode('{}'), '{}', {})))\nexcept (ValueError,OSError) as exc:\n print(json.dumps({{'error':str(exc) if isinstance(exc,ValueError) else 'DSH_DOCUMENT_RENDER_FAILED'}}))\n",
        include_str!("agy_index_probe.py").replace("if __name__ == '__main__':", "if False:"),
        STANDARD.encode(bytes), extension, 2*1024*1024-bytes.len());
    let raw=run_local_python("{}",100,&source).map_err(|e|match e {
        "AGY_HOST_UNAVAILABLE"=>"DSH_DOCUMENT_RENDERER_UNAVAILABLE",
        "AGY_HOST_TIMEOUT"=>"DSH_DOCUMENT_RENDER_TIMEOUT",
        _=>"DSH_DOCUMENT_RENDER_FAILED",
    })?;
    let result:Value=serde_json::from_slice(&raw).map_err(|_|"DSH_DOCUMENT_RENDER_FAILED")?;
    if let Some(code)=result["error"].as_str() {
        return Err(match code {
            "DSH_DOCUMENT_RENDERER_UNAVAILABLE"=>"DSH_DOCUMENT_RENDERER_UNAVAILABLE",
            "DSH_DOCUMENT_SANDBOX_UNAVAILABLE"=>"DSH_DOCUMENT_SANDBOX_UNAVAILABLE",
            "DSH_DOCUMENT_RENDER_TIMEOUT"=>"DSH_DOCUMENT_RENDER_TIMEOUT",
            "DSH_DOCUMENT_PAGE_LIMIT"=>"DSH_DOCUMENT_PAGE_LIMIT",
            "DSH_FILE_TOO_LARGE"=>"DSH_FILE_TOO_LARGE",
            _=>"DSH_DOCUMENT_RENDER_FAILED",
        });
    }
    if !result["pages"].is_array() || result["page_count"].as_u64().is_none() {
        return Err("DSH_DOCUMENT_RENDER_FAILED");
    }
    Ok(result)
}

fn run_ssh_python(ssh_target: &str, arg_str: &str, timeout_secs: u64) -> Result<Vec<u8>, &'static str> {
    let source = compact_python(include_str!("agy_index_probe.py"));
    run_host_python(ssh_target, arg_str, timeout_secs, &source)
}

fn run_host_python(ssh_target: &str, arg_str: &str, timeout_secs: u64, source: &str) -> Result<Vec<u8>, &'static str> {
    if matches!(ssh_target, "local" | "self" | "localhost" | "127.0.0.1") {
        return run_local_python(arg_str, timeout_secs, source);
    }
    if ssh_target.is_empty() || ssh_target.starts_with('-') || !ssh_target.bytes().all(|c|c.is_ascii_alphanumeric()||b"-_.@".contains(&c)) {
        return Err("AGY_CONFIG_INVALID");
    }
    let encoded=STANDARD.encode(arg_str.as_bytes());
    let compacted=source.to_owned();
    let remote_cmd=format!("python3 -c \"import sys,base64;s=sys.stdin.read();sys.argv=['probe',base64.b64decode('{encoded}').decode('utf-8')];exec(s,{{'__name__':'__main__','__BOB_PROBE_SRC__':s}})\"");
    let mut command=Command::new("ssh");
    command.args(["-o","BatchMode=yes","-o","ConnectTimeout=15","-o","ServerAliveInterval=5","-o","ServerAliveCountMax=2","--",ssh_target,&remote_cmd])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child=command.spawn().map_err(|_|"AGY_HOST_UNAVAILABLE")?;
    if let Some(mut stdin)=child.stdin.take() {
        let _=stdin.write_all(compacted.as_bytes());
    }
    let stdout=child.stdout.take().ok_or("AGY_HOST_UNAVAILABLE")?;
    let reader=thread::spawn(move || { let mut b=Vec::new();stdout.take(4*1024*1024+1).read_to_end(&mut b).map(|_|b) });
    let started=Instant::now();
    let success=loop {
        match child.try_wait() {
            Ok(Some(s))=>break s.success(),
            Ok(None) if started.elapsed()<Duration::from_secs(timeout_secs)=>thread::sleep(Duration::from_millis(40)),
            _=>{let _=child.kill();let _=child.wait();return Err("AGY_HOST_TIMEOUT")}
        }
    };
    let b=reader.join().map_err(|_|"AGY_INDEX_UNAVAILABLE")?.map_err(|_|"AGY_INDEX_UNAVAILABLE")?;
    if !success || b.len()>4*1024*1024 { return Err("AGY_INDEX_UNAVAILABLE") }
    Ok(b)
}

/// Probe only tools explicitly granted to this connection, on that host.
/// Inputs never supply executable paths, shell text, credentials, or a new host.
pub fn probe_native_harness(data: &Path, connection: &str, tool: &str, discovery_only: bool) -> Result<Value, &'static str> {
    if !["codex", "pi", "agy", "claude"].contains(&tool) {
        return Err("HARNESS_TOOL_UNSUPPORTED");
    }
    let all = grants(data)?;
    let g = all.iter().find(|g| g.connection_id == connection).ok_or("HARNESS_HOST_GRANT_REQUIRED")?;
    if !g.harnesses.iter().any(|t| t == tool) {
        return Err("HARNESS_TOOL_NOT_GRANTED");
    }
    let args = serde_json::to_string(&json!({"tool": tool, "discovery_only": discovery_only}))
        .map_err(|_| "HARNESS_PROBE_INVALID")?;
    let bytes = run_host_python(&g.ssh_target, &args, 55, include_str!("harness_probe.py"))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "HARNESS_PROBE_INVALID")?;
    if value["schema_version"] != 1 || value["tool"] != tool || !value["installed"].is_boolean() {
        return Err("HARNESS_PROBE_INVALID");
    }
    Ok(value)
}

fn fetch_list(data: &Path, connection: &str) -> Result<Vec<Value>, &'static str> {
    let all=grants(data)?;
    let Some(g)=all.iter().find(|g|g.connection_id==connection) else { return Ok(vec![]) };
    if !g.is_local() && !g.agy_data_dir.is_empty() && !g.agy_data_dir.starts_with('/') { return Err("AGY_CONFIG_INVALID") }
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
        let has_running=cached["page"]["request_results"].as_array().is_some_and(|arr|arr.iter().any(|r|r["status"].as_str()==Some("running")))
            || cached["page"]["terminal_holder"]["busy"].as_bool() == Some(true);
        let ttl=if has_running || after.is_some() { 2 } else { 15 };
        if age<ttl {
            if let Some(mut page)=cached.get("page").cloned() {
                // Native history is a mutable bounded snapshot, not an append-only log.
                // A stable message count does not prove unchanged text (or window contents).
                page["not_modified"]=json!(false);
                page["history_snapshot"]=json!(true);
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
    let terminal_holder=raw.get("terminal_holder").cloned().unwrap_or(Value::Null);
    let cursor=msgs.len() as i64;
    let harness=raw["harness"].as_str().unwrap_or("dsh");
    let workspace=raw["workspace"].as_str().unwrap_or("");
    let page=json!({
        "address": { "connection_id": connection, "session_id": session_id },
        "harness": harness,
        "workspace": workspace,
        "cursor": cursor,
        "before": if msgs.is_empty() { 0 } else { 1 },
        "has_more": false,
        "not_modified": false,
        "history_snapshot": true,
        "replacements": [],
        "messages": msgs,
        "request_results": req_results,
        "terminal_holder": terminal_holder,
    });
    let _=fs::write(&cache_path, serde_json::to_vec(&json!({"at":now,"page":page})).unwrap_or_default());
    Ok(page)
}

pub fn prompt_session(data: &Path, connection: &str, session_id: &str, text: &str, request_id: &str) -> Result<Value, &'static str> {
    prompt_session_with_options(data, connection, session_id, text, request_id, false, None)
}

pub fn find_cached_workspace(data: &Path, connection: &str, session_id: &str) -> Option<String> {
    use sha2::{Sha256, Digest};
    let mut hash = Sha256::new();
    hash.update(connection.as_bytes());
    hash.update(b":");
    hash.update(session_id.as_bytes());
    let page_cache = data.join(format!("remote-page-{:x}.json", hash.finalize()));
    if let Ok(b) = fs::read(&page_cache) {
        if let Ok(v) = serde_json::from_slice::<Value>(&b) {
            if let Some(ws) = v.pointer("/page/workspace").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                return Some(ws.to_owned());
            }
        }
    }
    let p = data.join("device_harnesses.json");
    if let Ok(raw) = fs::read(&p) {
        let mut d_hash = Sha256::new();
        d_hash.update(connection.as_bytes());
        d_hash.update(&raw);
        let dir_cache = data.join(format!("agy-directory-{:x}.json", d_hash.finalize()));
        if let Ok(b) = fs::read(&dir_cache) {
            if let Ok(v) = serde_json::from_slice::<Value>(&b) {
                if let Some(rows) = v.get("rows").and_then(Value::as_array) {
                    for r in rows {
                        if r.get("id").and_then(Value::as_str) == Some(session_id) {
                            if let Some(ws) = r.get("workspace").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                                return Some(ws.to_owned());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn prompt_session_with_options(
    data: &Path,
    connection: &str,
    session_id: &str,
    text: &str,
    request_id: &str,
    takeover: bool,
    approval_mode_override: Option<&str>,
) -> Result<Value, &'static str> {
    prompt_session_with_details(data, connection, session_id, text, request_id, takeover, approval_mode_override, None)
}

pub fn prompt_session_with_details(
    data: &Path,
    connection: &str,
    session_id: &str,
    text: &str,
    request_id: &str,
    takeover: bool,
    approval_mode_override: Option<&str>,
    attachments: Option<&Value>,
) -> Result<Value, &'static str> {
    use sha2::{Sha256,Digest};
    if !owns_readable_session(session_id) {
        return Err("DSH_SESSION_NOT_FOUND");
    }
    let all=grants(data)?;
    let Some(g)=all.iter().find(|g|g.connection_id==connection) else { return Err("DSH_CONNECTION_REQUIRED") };
    require_native_harness_grant(g, session_id)?;
    let claude_tools = if !session_id.starts_with("claude-jsonl:") || approval_mode_override == Some("deny") {
        Vec::new()
    } else if approval_mode_override == Some("read_only") {
        g.effective_claude_tools()
            .into_iter()
            .filter(|t| matches!(t.as_str(), "Read" | "Grep" | "Glob" | "AskUserQuestion" | "WebFetch" | "WebSearch"))
            .collect()
    } else {
        g.effective_claude_tools()
    };
    let approval_mode = if approval_mode_override == Some("deny") {
        "deny"
    } else if session_id.starts_with("agy:") || (session_id.starts_with("claude-jsonl:") && !claude_tools.is_empty()) {
        "interactive"
    } else {
        "deny"
    };
    let arg_str=serde_json::to_string(&json!({
        "mode": "prompt",
        "approvalMode": approval_mode,
        "claudeTools": claude_tools,
        "takeover": takeover,
        "sessionId": session_id,
        "text": text,
        "requestId": request_id,
        "attachments": attachments,
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

fn require_native_harness_grant(grant: &HostGrant, session: &str) -> Result<(), &'static str> {
    let tool = match session.split_once(':').map(|(tool, _)| tool) {
        Some("claude-jsonl") => "claude",
        Some(tool @ ("codex" | "agy" | "pi" | "claude")) => tool,
        _ => return Err("HARNESS_EVENTS_UNSUPPORTED"),
    };
    if !grant.harnesses.iter().any(|allowed| allowed == tool) {
        return Err("HARNESS_TOOL_NOT_GRANTED");
    }
    Ok(())
}

pub fn native_permission(data: &Path, connection: &str, session: &str, request: &str, permission: &str, option: Option<&str>) -> Result<Value, &'static str> {
    use sha2::{Sha256,Digest};
    if !session.starts_with("agy:") && !session.starts_with("claude-jsonl:") { return Err("HARNESS_APPROVAL_UNSUPPORTED") }
    if permission.is_empty() || permission.len()>128 || option.is_some_and(|x|x.len()>128) { return Err("HARNESS_APPROVAL_INVALID") }
    let all=grants(data)?;
    let grant=all.iter().find(|g|g.connection_id==connection).ok_or("DSH_CONNECTION_REQUIRED")?;
    require_native_harness_grant(grant, session)?;
    let bytes=run_ssh_python(&grant.ssh_target,&json!({"mode":"permission","sessionId":session,"requestId":request,"permissionId":permission,"optionId":option,"claudeTools":grant.effective_claude_tools()}).to_string(),15)?;
    let res: Value = serde_json::from_slice(&bytes).map_err(|_|"HARNESS_APPROVAL_UNCONFIRMED")?;
    let mut hash=Sha256::new();
    hash.update(connection.as_bytes());
    hash.update(b":");
    hash.update(session.as_bytes());
    let cache_path=data.join(format!("remote-page-{:x}.json",hash.finalize()));
    let _=fs::remove_file(cache_path);
    Ok(res)
}

pub fn native_cancel(data: &Path, connection: &str, session: &str, request: &str) -> Result<Value, &'static str> {
    if !session.starts_with("codex:") && !session.starts_with("pi:") && !session.starts_with("agy:") && !session.starts_with("claude-jsonl:") { return Err("HARNESS_CANCEL_UNSUPPORTED") }
    let all=grants(data)?;
    let grant=all.iter().find(|g|g.connection_id==connection).ok_or("DSH_CONNECTION_REQUIRED")?;
    require_native_harness_grant(grant, session)?;
    let bytes=run_ssh_python(&grant.ssh_target,&json!({"mode":"cancel","sessionId":session,"requestId":request}).to_string(),15)?;
    serde_json::from_slice(&bytes).map_err(|_|"HARNESS_CANCEL_UNCONFIRMED")
}

pub(crate) fn preview_host_timeout_secs(path: &str) -> u64 {
    let ext=Path::new(path).extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
    // Office 45s + pdfinfo 10s + page rasterization 40s + process overhead.
    if ["pdf","docx","xlsx","pptx","doc","xls","ppt","odt","ods","odp"].contains(&ext.as_str()) {110} else {35}
}

pub fn preview_native_file(data: &Path, connection: &str, session: &str, path: &str, known: Option<&Value>) -> Result<Value, &'static str> {
    if !owns_readable_session(session) { return Err("DSH_SESSION_NOT_FOUND") }
    let all=grants(data)?;
    let g=all.iter().find(|g|g.connection_id==connection).ok_or("DSH_CONNECTION_REQUIRED")?;
    let tool=if session.starts_with("claude-jsonl:") {"claude"} else {session.split(':').next().unwrap_or("")};
    if !g.harnesses.iter().any(|h|h==tool) { return Err("HARNESS_TOOL_NOT_GRANTED") }
    let args=json!({"mode":"preview","sessionId":session,"path":path,"known":known,
        "agyDataDir":g.agy_data_dir,"claudeDataDir":g.claude_data_dir,"piDataDir":g.pi_data_dir,"codexDataDir":g.codex_data_dir});
    let bytes=run_ssh_python(&g.ssh_target,&args.to_string(),preview_host_timeout_secs(path))
        .map_err(|e|if e=="AGY_HOST_TIMEOUT" {"DSH_FILE_TIMEOUT"} else {e})?;
    let mut value:Value=serde_json::from_slice(&bytes).map_err(|_|"DSH_PREVIEW_UNAVAILABLE")?;
    if let Some(error)=value["error"].as_str() {
        return Err(match error {
            "DSH_FILE_TOO_LARGE"=>"DSH_FILE_TOO_LARGE",
            "DSH_DOCUMENT_RENDERER_UNAVAILABLE"=>"DSH_DOCUMENT_RENDERER_UNAVAILABLE",
            "DSH_DOCUMENT_SANDBOX_UNAVAILABLE"=>"DSH_DOCUMENT_SANDBOX_UNAVAILABLE",
            "DSH_DOCUMENT_RENDER_TIMEOUT"=>"DSH_DOCUMENT_RENDER_TIMEOUT",
            "DSH_DOCUMENT_RENDER_FAILED"=>"DSH_DOCUMENT_RENDER_FAILED",
            "DSH_DOCUMENT_PAGE_LIMIT"=>"DSH_DOCUMENT_PAGE_LIMIT",

            "DSH_FILE_CHANGED"=>"DSH_FILE_CHANGED",
            "DSH_FILE_NOT_REFERENCED"=>"DSH_FILE_NOT_REFERENCED",
            "DSH_FILE_OUTSIDE_WORKSPACE"=>"DSH_FILE_OUTSIDE_WORKSPACE",
            "DSH_FILE_PATH_REJECTED"=>"DSH_FILE_PATH_REJECTED",
            "DSH_FILE_TYPE_UNSUPPORTED"=>"DSH_FILE_TYPE_UNSUPPORTED",
            "DSH_SESSION_NOT_FOUND"=>"DSH_SESSION_NOT_FOUND",
            _=>"DSH_PREVIEW_UNAVAILABLE",
        });
    }
    if value["path"]!=path || value["session_id"]!=session {return Err("DSH_IDENTITY_MISMATCH")}
    value["connection_id"]=json!(connection);
    Ok(value)
}

pub fn native_events(data: &Path, connection: &str, session: &str, after: i64, epoch: Option<&str>) -> Result<Value, &'static str> {
    if !owns_readable_session(session) { return Err("HARNESS_EVENTS_UNSUPPORTED") }
    let all=grants(data)?;
    let grant=all.iter().find(|g|g.connection_id==connection).ok_or("DSH_CONNECTION_REQUIRED")?;
    require_native_harness_grant(grant, session)?;
    let args=json!({"mode":"events","sessionId":session,"after":after,"epoch":epoch,"observeExternal":session.starts_with("codex:") && grant.harnesses.iter().any(|h|h=="codex")});
    let bytes=run_ssh_python(&grant.ssh_target,&args.to_string(),15)?;
    serde_json::from_slice(&bytes).map_err(|_|"HARNESS_EVENTS_INVALID")
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
 #[test] fn dsh_document_bridge_bounds_before_process() {
  assert_eq!(render_document_bytes(b"", "exe").unwrap_err(),"DSH_FILE_TYPE_UNSUPPORTED");
  assert_eq!(render_document_bytes(&vec![0;2*1024*1024], "pdf").unwrap_err(),"DSH_FILE_TOO_LARGE");
 }
 #[test] fn dsh_document_bridge_real_pdf_when_renderer_installed() {
  if !Command::new("pdfinfo").arg("-v").output().is_ok_and(|v|v.status.success()) { return; }
  let bytes=STANDARD.decode("JVBERi0xLjQKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCAyMDAgMjAwXSAvUmVzb3VyY2VzIDw8ID4+IC9Db250ZW50cyA0IDAgUiA+PgplbmRvYmoKNCAwIG9iago8PCAvTGVuZ3RoIDI3ID4+CnN0cmVhbQowIDAgMSByZyAxMCAxMCAxMDAgMTAwIHJlIGYKZW5kc3RyZWFtCmVuZG9iagp4cmVmCjAgNQowMDAwMDAwMDAwIDY1NTM1IGYgCjAwMDAwMDAwMDkgMDAwMDAgbiAKMDAwMDAwMDA1OCAwMDAwMCBuIAowMDAwMDAwMTE1IDAwMDAwIG4gCjAwMDAwMDAyMTkgMDAwMDAgbiAKdHJhaWxlcgo8PCAvU2l6ZSA1IC9Sb290IDEgMCBSID4+CnN0YXJ0eHJlZgoyOTYKJSVFT0YK").unwrap();
  let result=render_document_bytes(&bytes,"pdf").unwrap();
  assert_eq!(result["page_count"],1);
  assert_eq!(result["pages"][0]["number"],1);
  let image=STANDARD.decode(result["pages"][0]["data"].as_str().unwrap()).unwrap();
  assert!(image.starts_with(&[0xff,0xd8]));
  assert_eq!(render_document_bytes(b"malformed", "pdf").unwrap_err(),"DSH_DOCUMENT_RENDER_FAILED");
 }
 #[test] fn unconfigured_device_is_empty(){let d=tempfile::tempdir().unwrap();assert!(list(d.path(),"none").unwrap().is_empty())}
}

#[cfg(test)]
mod native_grant_tests {
    use super::*;
    #[test]
    fn dsh_native_control_rejects_ungranted_tool_before_host_contact() {
        let data = tempfile::tempdir().unwrap();
        fs::write(data.path().join("device_harnesses.json"), serde_json::to_vec(&json!([
            {"connectionId":"test-node","sshTarget":"must-not-be-contacted.invalid","harnesses":["agy"]}
        ])).unwrap()).unwrap();
        assert_eq!(native_cancel(data.path(),"test-node","codex:12345678","request").unwrap_err(),"HARNESS_TOOL_NOT_GRANTED");
        assert_eq!(native_events(data.path(),"test-node","codex:12345678",0,None).unwrap_err(),"HARNESS_TOOL_NOT_GRANTED");
        assert_eq!(prompt_session(data.path(),"test-node","codex:12345678","read only","request").unwrap_err(),"HARNESS_TOOL_NOT_GRANTED");
    }
    #[test]
    fn dsh_native_grant_uses_exact_tool_and_claude_alias() {
        let grant: HostGrant = serde_json::from_value(json!({"connectionId":"test","sshTarget":"local","harnesses":["claude","agy"]})).unwrap();
        assert!(require_native_harness_grant(&grant,"claude-jsonl:12345678").is_ok());
        assert!(require_native_harness_grant(&grant,"agy:12345678").is_ok());
        assert_eq!(require_native_harness_grant(&grant,"pi:12345678"),Err("HARNESS_TOOL_NOT_GRANTED"));
        assert_eq!(require_native_harness_grant(&grant,"unknown:12345678"),Err("HARNESS_EVENTS_UNSUPPORTED"));
    }
}
