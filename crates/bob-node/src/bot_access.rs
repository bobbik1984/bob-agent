//! External project assistants: deny-by-default, no execution RPC forwarding.
use crate::{NodeContext, hash_token};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, io::Write, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub username: String,
    pub token_hash: String,
    pub expires_at: i64,
    pub revoked: bool,
    pub projects: Vec<String>,
    pub sessions: Vec<Session>,
    pub organize: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session { pub connection_id: String, pub harness: String, pub session_id: String }

fn authorize(dir: &Path, header: Option<&str>, now: i64) -> Result<Grant, &'static str> {
    let token = header.and_then(|h| h.strip_prefix("Bearer ")).filter(|s| s.starts_with("bob_bot_"))
        .ok_or("BOT_AUTH_REQUIRED")?;
    let grants: Vec<Grant> = serde_json::from_slice(&fs::read(dir.join("bot-grants.json")).map_err(|_| "BOT_AUTH_REQUIRED")?)
        .map_err(|_| "BOT_AUTH_REQUIRED")?;
    grants.into_iter().find(|g| g.token_hash == hash_token(token) && !g.revoked && g.expires_at > now)
        .ok_or("BOT_AUTH_REQUIRED")
}
fn audit(dir: &Path, user: &str, action: &str, outcome: &str, now: i64) -> Result<(), &'static str> {
    let mut f = fs::OpenOptions::new().create(true).append(true).open(dir.join("bot-audit.jsonl")).map_err(|_| "BOT_AUDIT_UNAVAILABLE")?;
    writeln!(f, "{}", json!({"at":now,"actor":user,"action":action,"outcome":outcome})).map_err(|_| "BOT_AUDIT_UNAVAILABLE")?;
    f.sync_data().map_err(|_| "BOT_AUDIT_UNAVAILABLE")
}
pub fn handle(ctx: &mut NodeContext, header: Option<&str>, body: &Value, now: i64) -> (u16, Value) {
    let g=match authorize(&ctx.runtime.data_dir,header,now) {Ok(g)=>g,Err(e)=>return (401,json!({"error":e}))};
    if body["jsonrpc"] != "2.0" || !body.is_object() {return (400,json!({"error":"BOT_BODY_INVALID"}));}
    let id=body.get("id").cloned().unwrap_or(Value::Null);
    let method=body["method"].as_str().unwrap_or("");
    let result=match method {
        "initialize" => Ok(json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"bob-project-assistant","version":"0.1.0"}})),
        "notifications/initialized" => return (202,json!({})),
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut tools=vec![
                json!({"name":"bob_projects","description":"Read explicitly granted project metadata. No node execution.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
                json!({"name":"bob_project_sessions","description":"Read granted existing project/session associations.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})];
            if g.organize {tools.push(json!({"name":"bob_associate_session","description":"Associate an explicitly granted session with a granted project using version CAS. Does not execute or send messages.","inputSchema":{"type":"object","properties":{"connection_id":{"type":"string"},"harness":{"type":"string"},"session_id":{"type":"string"},"project_id":{"type":"string"},"expected_version":{"type":"integer","minimum":0}},"required":["connection_id","harness","session_id","project_id","expected_version"],"additionalProperties":false}}));}
            Ok(json!({"tools":tools}))
        }
        "tools/call" => {
            let name=body["params"]["name"].as_str().unwrap_or("");
            // Log only fixed tool identifiers, never attacker text or request payloads.
            let logged=if matches!(name,"bob_projects"|"bob_project_sessions"|"bob_associate_session"){name}else{"denied_tool"};
            if audit(&ctx.runtime.data_dir,&g.username,logged,"attempt",now).is_err(){return (503,json!({"error":"BOT_AUDIT_UNAVAILABLE"}));}
            let access=crate::project_service::ProjectAccess {
                projects:g.projects.clone(),
                sessions:g.sessions.iter().map(|s|(s.connection_id.clone(),s.harness.clone(),s.session_id.clone())).collect(),
                organize:g.organize,
            };
            let empty=json!({});
            let args=body["params"].get("arguments").unwrap_or(&empty);
            let out=crate::project_service::execute(ctx,&access,name,args,now);
            let outcome=if out.is_ok(){"success"}else{"denied_or_conflict"};
            if audit(&ctx.runtime.data_dir,&g.username,logged,outcome,now).is_err(){return (503,json!({"error":"BOT_RECEIPT_UNKNOWN_CHECK_VERSION"}));}
            Ok(match out {Ok(v)=>json!({"content":[{"type":"text","text":v.to_string()}],"isError":false}),Err(e)=>json!({"content":[{"type":"text","text":e}],"isError":true})})
        }
        _ => Err("BOT_METHOD_DENIED"),
    };
    (200,match result {Ok(v)=>json!({"jsonrpc":"2.0","id":id,"result":v}),Err(e)=>json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":e}})})
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BobNodeConfig, spawn_node_server};
    use std::{io::{Read,Write},net::TcpStream};
    const TOKEN: &str="bob_bot_test_only_not_a_real_credential";
    fn setup() -> (tempfile::TempDir,NodeContext) {
        let d=tempfile::tempdir().unwrap();
        let c=BobNodeConfig{bind_addr:"127.0.0.1:0".into(),data_dir:d.path().into(),static_dir:None,legacy_db_path:None};
        let mut ctx=NodeContext::open(&c).unwrap();
        for id in ["allowed","other-allowed","private"] {ctx.app.create_project(id,id,1).unwrap();}
        save(d.path(),true,false,10000);
        (d,ctx)
    }
    fn save(p:&Path,organize:bool,revoked:bool,expires:i64) {
        let grant=Grant{username:"grok_bot".into(),token_hash:hash_token(TOKEN),expires_at:expires,revoked,
            projects:vec!["allowed".into(),"other-allowed".into()],
            sessions:vec![Session{connection_id:"test-node".into(),harness:"dsh".into(),session_id:"session-a".into()}],organize};
        fs::write(p.join("bot-grants.json"),serde_json::to_vec(&vec![grant]).unwrap()).unwrap();
    }
    fn rpc(ctx:&mut NodeContext,name:&str,args:Value)->Value {
        handle(ctx,Some(&format!("Bearer {TOKEN}")),&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),10).1
    }
    fn args()->Value {json!({"connection_id":"test-node","harness":"dsh","session_id":"session-a","project_id":"allowed","expected_version":0})}
    #[test] fn bot_read_projection_is_scoped() {
        let (_d,mut c)=setup();let r=rpc(&mut c,"bob_projects",json!({}));
        let s=r.to_string();assert!(s.contains("other-allowed"));assert!(!s.contains("private"));assert!(!s.contains("draft"));assert!(!s.contains(TOKEN));
    }
    #[test] fn bot_organize_persists_with_conflict_protection() {
        let (d,mut c)=setup();assert_eq!(rpc(&mut c,"bob_associate_session",args())["result"]["isError"],false);
        assert_eq!(rpc(&mut c,"bob_associate_session",args())["result"]["isError"],true);
        drop(c);let cfg=BobNodeConfig{bind_addr:"127.0.0.1:0".into(),data_dir:d.path().into(),static_dir:None,legacy_db_path:None};
        let mut c=NodeContext::open(&cfg).unwrap();assert!(rpc(&mut c,"bob_project_sessions",json!({})).to_string().contains("session-a"));
    }
    #[test] fn bot_readonly_cannot_organize() {
        let (d,mut c)=setup();save(d.path(),false,false,10000);
        assert_eq!(rpc(&mut c,"bob_associate_session",args())["result"]["isError"],true);
        assert_eq!(c.app.project_session_links().unwrap()["links"].as_array().unwrap().len(),0);
    }
    #[test] fn bot_scope_denies_other_project_and_session() {
        let (_d,mut c)=setup();let mut a=args();a["project_id"]=json!("private");
        assert_eq!(rpc(&mut c,"bob_associate_session",a)["result"]["isError"],true);
        let mut a=args();a["session_id"]=json!("secret");assert_eq!(rpc(&mut c,"bob_associate_session",a)["result"]["isError"],true);
    }
    #[test] fn bot_cannot_move_from_ungranted_project() {
        let (_d,mut c)=setup();c.app.set_project_session_link("test-node","dsh","session-a",Some("private"),0,1).unwrap();
        let mut a=args();a["expected_version"]=json!(1);assert_eq!(rpc(&mut c,"bob_associate_session",a)["result"]["isError"],true);
    }
    #[test] fn bot_revocation_expiry_and_missing_auth() {
        let (d,mut c)=setup();let b=json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
        assert_eq!(handle(&mut c,None,&b,10).0,401);
        save(d.path(),true,true,10000);assert_eq!(handle(&mut c,Some(&format!("Bearer {TOKEN}")),&b,10).0,401);
        save(d.path(),true,false,10);assert_eq!(handle(&mut c,Some(&format!("Bearer {TOKEN}")),&b,10).0,401);
    }
    #[test] fn bot_execution_and_argument_injection_denied() {
        let (_d,mut c)=setup();for name in ["app_dsh_prompt","app_subagent_execute","shell","app_remote_node_save"] {
            assert_eq!(rpc(&mut c,name,json!({}))["result"]["isError"],true);
        }
        assert_eq!(rpc(&mut c,"bob_projects",json!({"command":"anything"}))["result"]["isError"],true);
    }
    #[test] fn bot_http_token_cannot_use_owner_rpc() {
        let (d,c)=setup();drop(c);
        let server=spawn_node_server(BobNodeConfig{bind_addr:"127.0.0.1:0".into(),data_dir:d.path().into(),static_dir:None,legacy_db_path:None}).unwrap();
        let mut stream=TcpStream::connect(server.local_addr).unwrap();
        write!(stream,"POST /api/rpc/app_catalog HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
        let mut out=String::new();stream.read_to_string(&mut out).unwrap();assert!(out.starts_with("HTTP/1.1 403"));server.shutdown();
    }
    #[test] fn bot_audit_contains_no_credentials_or_payload() {
        let (d,mut c)=setup();rpc(&mut c,"bob_projects",json!({}));
        let audit=fs::read_to_string(d.path().join("bot-audit.jsonl")).unwrap();assert!(!audit.contains(TOKEN));assert!(audit.contains("grok_bot"));assert!(audit.contains("success"));
    }
}
