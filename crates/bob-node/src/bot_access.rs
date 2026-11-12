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
    #[serde(default)]
    pub no_expiry: bool,
    pub revoked: bool,
    pub projects: Vec<String>,
    #[serde(default)]
    pub all_projects: bool,
    pub sessions: Vec<Session>,
    pub organize: bool,
    #[serde(default)]
    pub todos_read: bool,
    #[serde(default)]
    pub todos_create: bool,
    #[serde(default)]
    pub todos_unassigned: bool,
    #[serde(default)]
    pub todos_update: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session { pub connection_id: String, pub harness: String, pub session_id: String }

fn authorize(dir: &Path, header: Option<&str>, now: i64) -> Result<Grant, &'static str> {
    let token = header.and_then(|h| h.strip_prefix("Bearer ")).filter(|s| s.starts_with("bob_bot_"))
        .ok_or("BOT_AUTH_REQUIRED")?;
    let grants: Vec<Grant> = serde_json::from_slice(&fs::read(dir.join("bot-grants.json")).map_err(|_| "BOT_AUTH_REQUIRED")?)
        .map_err(|_| "BOT_AUTH_REQUIRED")?;
    grants.into_iter().find(|g| g.token_hash == hash_token(token) && !g.revoked && (g.no_expiry || g.expires_at > now))
        .ok_or("BOT_AUTH_REQUIRED")
}
fn audit(dir: &Path, user: &str, action: &str, outcome: &str, now: i64) -> Result<(), &'static str> {
    let mut f = fs::OpenOptions::new().create(true).append(true).open(dir.join("bot-audit.jsonl")).map_err(|_| "BOT_AUDIT_UNAVAILABLE")?;
    writeln!(f, "{}", json!({"at":now,"actor":user,"action":action,"outcome":outcome})).map_err(|_| "BOT_AUDIT_UNAVAILABLE")?;
    f.sync_data().map_err(|_| "BOT_AUDIT_UNAVAILABLE")
}
pub fn handle(ctx: &mut NodeContext, header: Option<&str>, body: &Value, now: i64) -> (u16, Value) {
    let mut g=match authorize(&ctx.runtime.data_dir,header,now) {Ok(g)=>g,Err(e)=>return (401,json!({"error":e}))};
    // Expand only authenticated owner-granted scope, freshly on each request.
    if g.all_projects {
        g.projects=match ctx.app.catalog_snapshot() {
            Ok(c)=>c.projects.into_iter().map(|p|p.id).collect(),
            Err(_)=>return (503,json!({"error":"BOT_READ_FAILED"})),
        };
    }
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
            if g.todos_read {tools.push(json!({"name":"bob_todos","description":"List authorized todos: omit project_id for all, null for independent todos, or an ID for a project. At most 100 items; paginate with offset.","inputSchema":{"type":"object","properties":{"project_id":{"type":["string","null"]},"offset":{"type":"integer","minimum":0}},"additionalProperties":false}}));}
            if g.todos_create {tools.push(json!({"name":"bob_todo_create","description":"Create a todo. Project, start and deadline are optional; infer dates from user intent; date-only uses timing.start_date/due_date, precise times use start_at/due_at; preserve original wording and interpretation. Retry with the same request_id and identical payload. Times are Unix milliseconds.","inputSchema":{"type":"object","properties":{"project_id":{"type":["string","null"]},"request_id":{"type":"string","maxLength":128},"title":{"type":"string","maxLength":256},"start_at":{"type":["integer","null"],"minimum":1},"due_at":{"type":["integer","null"],"minimum":1},"notes":{"type":"string","maxLength":16384},"timing":timing_schema()},"required":["request_id","title"],"additionalProperties":false}}));}
            if g.todos_update {tools.push(json!({"name":"bob_todo_update","description":"Patch an existing authorized todo, never create another. Read latest id/version first. Omitted fields are preserved, null timestamps clear; timing replaces date precision/provenance. Retry identical request_id/payload; stale versions are refused. No deletion or node execution.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"request_id":{"type":"string","maxLength":128},"expected_version":{"type":"integer","minimum":1},"project_id":{"type":["string","null"]},"title":{"type":"string","maxLength":256},"start_at":{"type":["integer","null"],"minimum":1},"due_at":{"type":["integer","null"],"minimum":1},"notes":{"type":"string","maxLength":16384},"timing":timing_schema()},"required":["id","request_id","expected_version"],"additionalProperties":false}}));}
            Ok(json!({"tools":tools}))
        }
        "tools/call" => {
            let name=body["params"]["name"].as_str().unwrap_or("");
            // Log only fixed tool identifiers, never attacker text or request payloads.
            let logged=if matches!(name,"bob_projects"|"bob_project_sessions"|"bob_associate_session"|"bob_todos"|"bob_todo_create"|"bob_todo_update"){name}else{"denied_tool"};
            if audit(&ctx.runtime.data_dir,&g.username,logged,"attempt",now).is_err(){return (503,json!({"error":"BOT_AUDIT_UNAVAILABLE"}));}
            let access=crate::project_service::ProjectAccess {
                projects:g.projects.clone(),
                sessions:g.sessions.iter().map(|s|(s.connection_id.clone(),s.harness.clone(),s.session_id.clone())).collect(),
                organize:g.organize,
            };
            let empty=json!({});
            let args=body["params"].get("arguments").unwrap_or(&empty);
            let out=if matches!(name,"bob_todos"|"bob_todo_create"|"bob_todo_update") {
                crate::todo_service::execute(ctx,&g.username,&g.projects,g.todos_read,g.todos_create,g.todos_update,g.todos_unassigned,name,args,now)
            } else {crate::project_service::execute(ctx,&access,name,args,now)};
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
        let grant=Grant{username:"grok_bot".into(),token_hash:hash_token(TOKEN),expires_at:expires,revoked,no_expiry:false,all_projects:false,
            projects:vec!["allowed".into(),"other-allowed".into()],
            sessions:vec![Session{connection_id:"test-node".into(),harness:"dsh".into(),session_id:"session-a".into()}],organize,todos_read:false,todos_create:false,todos_unassigned:false,todos_update:false};
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
    fn allow_todos(p:&Path) {
        let mut grants:Vec<Grant>=serde_json::from_slice(&fs::read(p.join("bot-grants.json")).unwrap()).unwrap();
        grants[0].todos_read=true;grants[0].todos_create=true;
        fs::write(p.join("bot-grants.json"),serde_json::to_vec(&grants).unwrap()).unwrap();
    }
    fn todo_args()->Value {json!({"project_id":"allowed","request_id":"request-1","title":"Review","start_at":1000,"notes":"Check result"})}
    fn value(r:Value)->Value {serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap()}
    #[test] fn todo_retry_survives_restart_and_does_not_duplicate() {
        let (d,mut c)=setup();allow_todos(d.path());
        let first=value(rpc(&mut c,"bob_todo_create",todo_args()));
        assert_eq!(first["replayed"],false);
        assert_eq!(c.app.list_schedule_items().unwrap().len(),1);
        drop(c);
        let cfg=BobNodeConfig{bind_addr:"127.0.0.1:0".into(),data_dir:d.path().into(),static_dir:None,legacy_db_path:None};
        let mut c=NodeContext::open(&cfg).unwrap();
        let again=value(rpc(&mut c,"bob_todo_create",todo_args()));
        assert_eq!(again["todo"]["id"],first["todo"]["id"]);assert_eq!(again["replayed"],true);
        assert_eq!(c.app.list_schedule_items().unwrap().len(),1);
        let mut altered=todo_args();altered["title"]=json!("Changed");
        assert_eq!(rpc(&mut c,"bob_todo_create",altered)["result"]["isError"],true);
    }
    #[test] fn todo_scope_defaults_and_revocation_are_enforced() {
        let (d,mut c)=setup();assert_eq!(rpc(&mut c,"bob_todo_create",todo_args())["result"]["isError"],true);
        allow_todos(d.path());let mut a=todo_args();a["project_id"]=json!("private");
        assert_eq!(rpc(&mut c,"bob_todo_create",a)["result"]["isError"],true);
        assert_eq!(rpc(&mut c,"bob_todos",json!({"project_id":"private"}))["result"]["isError"],true);
        assert!(c.app.list_schedule_items().unwrap().is_empty());
        save(d.path(),false,true,10000);
        assert_eq!(rpc(&mut c,"bob_todo_create",todo_args())["error"],"BOT_AUTH_REQUIRED");
    }
    #[test] fn todo_reads_filter_personal_and_other_project_items() {
        let (d,mut c)=setup();allow_todos(d.path());
        c.app.create_schedule_item_with_kind("private-todo",Some("private"),None,"Secret","todo",1000,None,None,"Private",1).unwrap();
        value(rpc(&mut c,"bob_todo_create",todo_args()));
        let list=value(rpc(&mut c,"bob_todos",json!({"project_id":"allowed"})));
        assert_eq!(list["todos"].as_array().unwrap().len(),1);assert!(!list.to_string().contains("Secret"));
        assert_eq!(rpc(&mut c,"bob_todos",json!({"project_id":"allowed","offset":-1}))["result"]["isError"],true);
    }

    #[test] fn explicit_non_expiring_grant_still_revokes() {
        let (d,mut c)=setup();
        let mut grants:Vec<Grant>=serde_json::from_slice(&fs::read(d.path().join("bot-grants.json")).unwrap()).unwrap();
        grants[0].expires_at=0;grants[0].no_expiry=true;
        fs::write(d.path().join("bot-grants.json"),serde_json::to_vec(&grants).unwrap()).unwrap();
        assert_eq!(rpc(&mut c,"bob_projects",json!({}))["result"]["isError"],false);
        grants[0].revoked=true;
        fs::write(d.path().join("bot-grants.json"),serde_json::to_vec(&grants).unwrap()).unwrap();
        assert_eq!(rpc(&mut c,"bob_projects",json!({}))["error"],"BOT_AUTH_REQUIRED");
    }

    #[test] fn all_projects_includes_new_projects_without_execution() {
        let (d,mut c)=setup();allow_todos(d.path());
        let path=d.path().join("bot-grants.json");
        let mut grants:Vec<Grant>=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        grants[0].all_projects=true;grants[0].organize=false;grants[0].sessions.clear();
        fs::write(&path,serde_json::to_vec(&grants).unwrap()).unwrap();
        c.app.create_project("future","Future",11).unwrap();
        assert!(rpc(&mut c,"bob_projects",json!({})).to_string().contains("future"));
        let mut a=todo_args();a["project_id"]=json!("future");
        assert_eq!(rpc(&mut c,"bob_todo_create",a)["result"]["isError"],false);
        assert_eq!(rpc(&mut c,"app_dsh_prompt",json!({}))["result"]["isError"],true);
        assert_eq!(rpc(&mut c,"bob_associate_session",args())["result"]["isError"],true);
    }

    #[test] fn independent_todo_dates_and_filters() {
        let (d,mut c)=setup();allow_todos(d.path());
        let a=json!({"request_id":"independent","title":"No project or dates"});
        assert_eq!(rpc(&mut c,"bob_todo_create",a.clone())["result"]["isError"],true);
        let path=d.path().join("bot-grants.json");
        let mut grants:Vec<Grant>=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        grants[0].todos_unassigned=true;
        fs::write(&path,serde_json::to_vec(&grants).unwrap()).unwrap();
        let first=value(rpc(&mut c,"bob_todo_create",a.clone()));
        assert!(first["todo"]["project_id"].is_null());assert!(first["todo"]["start_at"].is_null());
        assert!(first["todo"]["due_at"].is_null());
        let mut due=a.clone();due["request_id"]=json!("deadline");due["due_at"]=json!(2000);
        let second=value(rpc(&mut c,"bob_todo_create",due.clone()));assert_eq!(second["todo"]["due_at"],2000);
        let mut both=due.clone();both["request_id"]=json!("both");both["start_at"]=json!(1000);
        assert_eq!(rpc(&mut c,"bob_todo_create",both.clone())["result"]["isError"],false);
        both["request_id"]=json!("invalid");both["start_at"]=json!(3000);
        assert_eq!(rpc(&mut c,"bob_todo_create",both)["result"]["isError"],true);
        value(rpc(&mut c,"bob_todo_create",todo_args()));
        assert_eq!(value(rpc(&mut c,"bob_todos",json!({})))["todos"].as_array().unwrap().len(),4);
        assert_eq!(value(rpc(&mut c,"bob_todos",json!({"project_id":null})))["todos"].as_array().unwrap().len(),3);
        assert_eq!(value(rpc(&mut c,"bob_todos",json!({"project_id":"allowed"})))["todos"].as_array().unwrap().len(),1);
        assert_eq!(value(rpc(&mut c,"bob_todo_create",a))["replayed"],true);
        let id=second["todo"]["id"].as_str().unwrap();
        let edited=c.app.edit_todo_item(id,1,"Edited",None,Some(1500),Some(2000),"",12).unwrap();
        assert_eq!(edited.end_at,Some(2000));
        let cleared=c.app.edit_todo_item(id,2,"Edited",None,None,None,"",13).unwrap();
        assert_eq!(cleared.start_at,0);assert_eq!(cleared.end_at,None);
        assert!(c.app.edit_todo_item(id,2,"Stale",None,None,None,"",14).is_err());
    }

    #[test] fn todo_semantic_dates_update_cas_and_scope() {
        let(d,mut c)=setup();allow_todos(d.path());
        let path=d.path().join("bot-grants.json");
        let mut g:Vec<Grant>=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        g[0].todos_unassigned=true;
        fs::write(&path,serde_json::to_vec(&g).unwrap()).unwrap();
        let created=value(rpc(&mut c,"bob_todo_create",json!({"request_id":"date-create","title":"Date-only","timing":{"due_date":"2026-11-15","time_zone":"Asia/Shanghai"}})));
        let id=created["todo"]["id"].clone();
        assert_eq!(created["todo"]["timing"]["due_date"],"2026-11-15");
        let a=json!({"id":id,"request_id":"date-update","expected_version":1,"timing":{"due_date":"2026-11-17","source_text":"next Tuesday"}});
        assert_eq!(rpc(&mut c,"bob_todo_update",a.clone())["result"]["isError"],true);
        g[0].todos_update=true;fs::write(&path,serde_json::to_vec(&g).unwrap()).unwrap();
        let updated=value(rpc(&mut c,"bob_todo_update",a.clone()));assert_eq!(updated["todo"]["version"],2);
        assert_eq!(value(rpc(&mut c,"bob_todo_update",a.clone()))["replayed"],true);
        let mut stale=a.clone();stale["request_id"]=json!("stale");assert_eq!(rpc(&mut c,"bob_todo_update",stale)["result"]["isError"],true);
        let mut denied=a.clone();denied["project_id"]=json!("private");denied["expected_version"]=json!(2);denied["request_id"]=json!("other-project");
        assert_eq!(rpc(&mut c,"bob_todo_update",denied)["result"]["isError"],true);
        let precise=value(rpc(&mut c,"bob_todo_update",json!({"id":id,"request_id":"precise","expected_version":2,"due_at":2000000})));
        assert!(precise["todo"]["timing"]["due_date"].is_null());assert_eq!(precise["todo"]["due_at"],2000000);
        let bad=json!({"id":id,"request_id":"bad-date","expected_version":3,"timing":{"due_date":"2026-02-30"}});
        assert_eq!(rpc(&mut c,"bob_todo_update",bad)["result"]["isError"],true);
        let dual=json!({"id":id,"request_id":"double-date","expected_version":3,"due_at":1000,"timing":{"due_date":"2026-11-15"}});
        assert_eq!(rpc(&mut c,"bob_todo_update",dual)["result"]["isError"],true);
        let cleared=value(rpc(&mut c,"bob_todo_update",json!({"id":id,"request_id":"clear","expected_version":3,"due_at":null,"timing":{}})));
        assert!(cleared["todo"]["due_at"].is_null());assert_eq!(c.app.list_schedule_items().unwrap().len(),1);
        assert_eq!(c.app.list_schedule_items().unwrap()[0].notes,"");
        drop(c);let cfg=BobNodeConfig{bind_addr:"127.0.0.1:0".into(),data_dir:d.path().into(),static_dir:None,legacy_db_path:None};
        let mut c=NodeContext::open(&cfg).unwrap();assert_eq!(value(rpc(&mut c,"bob_todo_update",a))["replayed"],true);
        g[0].revoked=true;fs::write(path,serde_json::to_vec(&g).unwrap()).unwrap();
        assert_eq!(rpc(&mut c,"bob_todo_update",json!({}))["error"],"BOT_AUTH_REQUIRED");
    }

}

fn timing_schema()->Value {json!({"type":"object","description":"Date-only ISO YYYY-MM-DD remains all-day, not fabricated midnight. Precise timestamps and date-only of the same boundary are mutually exclusive. Interpret relative words using original message reference_at and user time_zone. Record assumptions in inference_note.","properties":{"start_date":{"type":["string","null"]},"due_date":{"type":["string","null"]},"time_zone":{"type":["string","null"],"maxLength":64},"source_text":{"type":["string","null"],"maxLength":4096},"reference_at":{"type":["integer","null"]},"inference_note":{"type":["string","null"],"maxLength":4096}},"additionalProperties":false})}
