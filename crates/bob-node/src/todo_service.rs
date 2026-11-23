//! Shared application boundary for external transports. No arbitrary RPC forwarding.
use crate::NodeContext;
use serde_json::{json,Value};
pub(crate) fn execute(ctx:&mut NodeContext,actor:&str,projects:&[String],read:bool,create:bool,update:bool,unassigned:bool,name:&str,a:&Value,now:i64)->Result<Value,&'static str>{
    let obj=a.as_object().ok_or("BOT_ARGUMENTS_INVALID")?;
    let fields:&[&str]=match name {
        "bob_todos" if read=> &["project_id","offset"],
        "bob_todo_create" if create=> &["project_id","request_id","title","start_at","due_at","notes","timing"],
        "bob_todo_update" if update=> &["id","request_id","expected_version","project_id","title","start_at","due_at","notes","timing"],
        _=>return Err("BOT_SCOPE_DENIED"),
    };
    if obj.keys().any(|k|!fields.contains(&k.as_str())) {return Err("BOT_ARGUMENTS_INVALID");}
    if name=="bob_todo_update" {return ctx.app.update_bot_todo(actor,projects,unassigned,a,now).map_err(|_|"BOT_TODO_DENIED_OR_REQUEST_CONFLICT");}
    let project=match a.get("project_id") {
        None|Some(Value::Null)=>None,
        Some(Value::String(v)) if projects.contains(v)=>Some(v.as_str()),
        _=>return Err("BOT_SCOPE_DENIED"),
    };
    if project.is_none() && !unassigned && (name=="bob_todo_create" || a.get("project_id").is_some()) {
        return Err("BOT_SCOPE_DENIED");
    }
    if name=="bob_todos" {
        let offset=match a.get("offset") {None=>0,Some(v)=>v.as_u64().filter(|v|*v<=100000).ok_or("BOT_ARGUMENTS_INVALID")? as usize};
        let all=ctx.app.list_schedule_items().map_err(|_|"BOT_READ_FAILED")?;
        let mut rows=all.into_iter().filter(|r|r.kind=="todo" && match project {
            Some(id)=>r.project_id.as_deref()==Some(id),
            None if a.get("project_id").is_some()=>r.project_id.is_none() && unassigned,
            None=>r.project_id.as_ref().map_or(unassigned,|id|projects.contains(id)),
        }).collect::<Vec<_>>();
        rows.sort_by(|a,b|a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        let total=rows.len();let items=rows.into_iter().skip(offset).take(100).map(|r|json!({"id":r.id,"project_id":r.project_id,"title":r.title,"start_at":if r.start_at==0 {None}else{Some(r.start_at)},"due_at":r.end_at,"notes":r.notes,"status":r.status,"version":r.version,"updated_at":r.updated_at,"created_at":r.created_at,"timing":r.timing})).collect::<Vec<_>>();
        return Ok(json!({"todos":items,"next_offset":if offset.saturating_add(100)<total {Some(offset+100)}else{None}}));
    }
    let request=a["request_id"].as_str().ok_or("BOT_ARGUMENTS_INVALID")?;
    let title=a["title"].as_str().ok_or("BOT_ARGUMENTS_INVALID")?;
    let optional_time=|key:&str|->Result<Option<i64>,&'static str>{match a.get(key){
        None|Some(Value::Null)=>Ok(None),
        Some(v)=>v.as_i64().filter(|v|*v>0).map(Some).ok_or("BOT_ARGUMENTS_INVALID"),
    }};
    let start=optional_time("start_at")?;
    let due=optional_time("due_at")?;
    let notes=match a.get("notes"){None=>"",Some(v)=>v.as_str().ok_or("BOT_ARGUMENTS_INVALID")?};
    ctx.app.create_bot_todo_timed(actor,request,projects,project,title,start,due,notes,serde_json::from_value(a.get("timing").cloned().unwrap_or(json!({}))).map_err(|_|"BOT_ARGUMENTS_INVALID")?,now).map_err(|_|"BOT_TODO_DENIED_OR_REQUEST_CONFLICT")
}
