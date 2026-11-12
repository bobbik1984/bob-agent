//! Scoped Bot todo operations share the personal schedule ledger and sync outbox.
use crate::{app::LocalApp,outbox::{digest,Error,Result}};
use rusqlite::{params,OptionalExtension,Transaction};
use serde_json::{json,Value};
use crate::personal_ops::ScheduleTiming;
const SCHEMA:&str=include_str!("../migrations/0013.sql");
pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    let hash = digest(SCHEMA.replace("\r\n", "\n").as_bytes());
    let old: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=13",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match old {
        Some(v) if v != hash => return Err(Error::SchemaMismatch),
        Some(_) => (),
        None => {
            tx.execute_batch(SCHEMA)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(13,?1,?2)",
                params![hash, now],
            )?;
        }
    }
    let schema=include_str!("../migrations/0014.sql");
    let hash=digest(schema.replace("\r\n","\n").as_bytes());
    let old:Option<String>=tx.query_row("SELECT checksum FROM schema_migrations WHERE version=14",[],|r|r.get(0)).optional()?;
    match old {Some(v) if v!=hash=>return Err(Error::SchemaMismatch),Some(_)=>(),None=>{
        tx.execute_batch(schema)?;
        tx.execute("INSERT INTO schema_migrations VALUES(14,?1,?2)",params![hash,now])?;
    }}
    Ok(())
}


impl LocalApp {
    pub fn create_bot_todo(&mut self, actor:&str, request_id:&str, allowed:&[String], project:Option<&str>,
        title:&str, start_at:Option<i64>, due_at:Option<i64>, notes:&str, now:i64) -> Result<Value> {
        self.create_bot_todo_timed(actor,request_id,allowed,project,title,start_at,due_at,notes,ScheduleTiming::default(),now)
    }
    pub fn create_bot_todo_timed(&mut self, actor:&str, request_id:&str, allowed:&[String], project:Option<&str>,
        title:&str,start_at:Option<i64>,due_at:Option<i64>,notes:&str,timing:ScheduleTiming,now:i64)->Result<Value>{
        timing.validate(start_at,due_at)?;
        if actor.is_empty() || actor.len()>128 || request_id.is_empty() || request_id.len()>128
            || project.is_some_and(|id|!allowed.iter().any(|p|p==id)) || title.trim().is_empty() || title.len()>256
            || notes.len()>16384 || start_at.is_some_and(|v|v<=0) || due_at.is_some_and(|v|v<=0)
            || start_at.zip(due_at).is_some_and(|(s,d)|d<s) {return Err(Error::InvalidTransition);}
        // Preserve receipt hashes for old requests without a deadline.
        let payload=digest(&serde_json::to_vec(&if !timing.is_empty(){json!(["timed",project,title.trim(),start_at,due_at,notes.trim(),timing])} else if due_at.is_none() {
            json!([project,title.trim(),start_at,notes.trim()])
        } else {json!([project,title.trim(),start_at,notes.trim(),due_at])})?);
        self.store.db.execute_batch("SAVEPOINT bot_todo_create")?;
        let result=(|| {
            let active:bool=self.store.db.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND status='active')",[project],|r|r.get(0))?;
            if project.is_some() && !active {return Err(Error::InvalidTransition);}
            let old:Option<(String,String)>=self.store.db.query_row(
                "SELECT payload_sha256,response_json FROM bot_todo_receipts WHERE actor=?1 AND request_id=?2",
                params![actor,request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((hash,response))=old {
                if hash!=payload {return Err(Error::IdentityConflict);}
                let mut value:Value=serde_json::from_str(&response)?;value["replayed"]=json!(true);return Ok(value);
            }
            let id=format!("bot-todo-{}",digest(&serde_json::to_vec(&json!([actor,request_id]))?));
            let item=self.create_schedule_item_with_timing(&id,project,None,title,"todo",start_at.unwrap_or(0),due_at,None,notes,timing,now)?;
            let mut response=json!({"todo":item,"created_by":actor,"request_id":request_id,"replayed":false});
            response["todo"]["start_at"]=json!(start_at);
            response["todo"]["due_at"]=json!(due_at);
            self.store.db.execute("INSERT INTO bot_todo_receipts VALUES(?1,?2,?3,?4,?5)",params![actor,request_id,payload,response.to_string(),now])?;
            Ok(response)
        })();
        match result {
            Ok(v)=>{self.store.db.execute_batch("RELEASE bot_todo_create")?;Ok(v)}
            Err(e)=>{self.store.db.execute_batch("ROLLBACK TO bot_todo_create; RELEASE bot_todo_create")?;Err(e)}
        }
    }
}

impl LocalApp {
    /// Scope, CAS, original patch receipt, state and sync are one atomic mutation.
    pub fn update_bot_todo(&mut self,actor:&str,allowed:&[String],unassigned:bool,a:&Value,now:i64)->Result<Value>{
        let id=a["id"].as_str().ok_or(Error::InvalidTransition)?;
        let request=a["request_id"].as_str().ok_or(Error::InvalidTransition)?;
        let expected=a["expected_version"].as_i64().filter(|v|*v>0 && *v<i64::MAX).ok_or(Error::InvalidTransition)?;
        if actor.is_empty() || request.is_empty() || request.len()>128 {return Err(Error::InvalidTransition);}
        let hash=digest(&serde_json::to_vec(&json!(["update",a]))?);
        self.store.db.execute_batch("SAVEPOINT bot_todo_update")?;
        let result=(||{
            let old=self.list_schedule_items()?.into_iter().find(|v|v.id==id && v.kind=="todo").ok_or(Error::InvalidTransition)?;
            let permitted=|p:Option<&str>|p.map_or(unassigned,|id|allowed.iter().any(|p|p==id));
            if !permitted(old.project_id.as_deref()){return Err(Error::InvalidTransition);}
            let project=match a.get("project_id"){None=>old.project_id.as_deref(),Some(Value::Null)=>None,Some(Value::String(v))=>Some(v.as_str()),_=>return Err(Error::InvalidTransition)};
            if !permitted(project){return Err(Error::InvalidTransition);}
            let prior:Option<(String,String)>=self.store.db.query_row("SELECT payload_sha256,response_json FROM bot_todo_receipts WHERE actor=?1 AND request_id=?2",params![actor,request],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((digest,response))=prior {if digest!=hash{return Err(Error::IdentityConflict);}let mut v:Value=serde_json::from_str(&response)?;v["replayed"]=json!(true);return Ok(v);}
            if old.version!=expected{return Err(Error::InvalidTransition);}
            if let Some(p)=project {let active:bool=self.store.db.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND status='active')",[p],|r|r.get(0))?;if !active{return Err(Error::InvalidTransition);}}
            let title=match a.get("title"){None=>old.title.as_str(),Some(Value::String(v))=>v,_=>return Err(Error::InvalidTransition)};
            let notes=match a.get("notes"){None=>old.notes.as_str(),Some(Value::String(v))=>v,_=>return Err(Error::InvalidTransition)};
            if notes.len()>16384{return Err(Error::InvalidTransition);}
            let time=|k:&str,prior:Option<i64>|->Result<Option<i64>>{match a.get(k){None=>Ok(prior),Some(Value::Null)=>Ok(None),Some(v)=>v.as_i64().filter(|v|*v>0).map(Some).ok_or(Error::InvalidTransition)}};
            let mut start=time("start_at",if old.start_at==0{None}else{Some(old.start_at)})?;
            let mut due=time("due_at",old.end_at)?;
            let mut timing:ScheduleTiming=match a.get("timing"){None=>old.timing.clone(),Some(v)=>serde_json::from_value(v.clone())?};
            if a.get("timing").is_some(){
                if timing.start_date.is_some() && a.get("start_at").is_none(){start=None;}
                if timing.due_date.is_some() && a.get("due_at").is_none(){due=None;}
            } else {
                if a.get("start_at").is_some(){timing.start_date=None;}
                if a.get("due_at").is_some(){timing.due_date=None;}
            }
            timing.validate(start,due)?;
            let item=self.edit_todo_item_with_timing(id,expected,title,project,start,due,notes,Some(timing),now)?;
            let mut v=json!({"todo":item,"updated_by":actor,"request_id":request,"replayed":false});
            v["todo"]["start_at"]=json!(start);v["todo"]["due_at"]=json!(due);
            self.store.db.execute("INSERT INTO bot_todo_receipts VALUES(?1,?2,?3,?4,?5)",params![actor,request,hash,v.to_string(),now])?;
            Ok(v)
        })();
        match result{Ok(v)=>{self.store.db.execute_batch("RELEASE bot_todo_update")?;Ok(v)},Err(e)=>{self.store.db.execute_batch("ROLLBACK TO bot_todo_update; RELEASE bot_todo_update")?;Err(e)}}
    }
}
