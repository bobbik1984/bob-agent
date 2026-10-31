//! Durable links to an external conversation; no duplicate task state machine.
use crate::{app::LocalApp,outbox::{digest,Error,Result}};
use rusqlite::{params,OptionalExtension,Transaction,TransactionBehavior};
use serde::{Serialize,Deserialize};
use serde_json::json;
const SCHEMA:&str=include_str!("../migrations/0009.sql");
pub(crate) fn migrate(tx:&Transaction<'_>,now:i64)->Result<()> {
 let hash=digest(SCHEMA.replace("\r\n","\n").as_bytes());
 let old:Option<String>=tx.query_row("SELECT checksum FROM schema_migrations WHERE version=9",[],|r|r.get(0)).optional()?;
 match old {Some(v) if v!=hash=>Err(Error::SchemaMismatch),Some(_)=>Ok(()),None=>{tx.execute_batch(SCHEMA)?;tx.execute("INSERT INTO schema_migrations VALUES(9,?1,?2)",params![hash,now])?;Ok(())}}
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct AssistantDispatchLink {
 pub request_id:String,pub connection_id:String,pub native_session_id:String,
 pub target_title:String,pub status:String,pub result_sequences:Vec<i64>,pub created_at:i64,
}
impl LocalApp {
 pub fn assistant_bind_dispatch(&mut self,request:&str,connection:&str,session:&str,title:&str,now:i64)->Result<()> {
  if [request,connection,session].iter().any(|v|v.is_empty()||v.len()>256)||title.len()>512{return Err(Error::InvalidTransition)}
  let tx=self.store.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let owner:Option<String>=tx.query_row("SELECT conversation_id FROM messages WHERE request_id=?1 AND role='user' AND origin_device_id=?2",params![request,self.origin],|r|r.get(0)).optional()?;
  if owner.as_deref()!=Some(format!("assistant:{}",self.origin).as_str()){return Err(Error::InvalidTransition)}
  let old:Option<(String,String)>=tx.query_row("SELECT connection_id,native_session_id FROM assistant_dispatch_links WHERE request_id=?1",[request],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
  if let Some((c,s))=old {if c!=connection||s!=session{return Err(Error::IdentityConflict)}}
  else {tx.execute("INSERT INTO assistant_dispatch_links(request_id,connection_id,native_session_id,target_title,status,created_at,updated_at) VALUES(?1,?2,?3,?4,'unknown',?5,?5)",params![request,connection,session,title,now])?;}
  tx.commit()?;Ok(())
 }
 pub fn assistant_links(&self,conversation:&str)->Result<Vec<AssistantDispatchLink>> {
  self.ensure_active_conversation(conversation)?;
  let mut statement=self.store.db.prepare("SELECT l.request_id,l.connection_id,l.native_session_id,l.target_title,l.status,l.result_sequences_json,l.created_at FROM assistant_dispatch_links l JOIN messages m ON m.request_id=l.request_id WHERE m.conversation_id=?1 ORDER BY l.created_at")?;
  let rows=statement.query_map([conversation],|r|Ok(AssistantDispatchLink{request_id:r.get(0)?,connection_id:r.get(1)?,native_session_id:r.get(2)?,target_title:r.get(3)?,status:r.get(4)?,result_sequences:serde_json::from_str(&r.get::<_,String>(5)?).unwrap_or_default(),created_at:r.get(6)?}))?;
  rows.collect::<std::result::Result<Vec<_>,_>>().map_err(Into::into)
 }
 pub fn assistant_record_status(&mut self,request:&str,status:&str,now:i64)->Result<()> {
  if !matches!(status,"accepted"|"failed"|"ambiguous"){return Err(Error::InvalidTransition)}
  self.store.db.execute("UPDATE assistant_dispatch_links SET status=?2,updated_at=?3 WHERE request_id=?1 AND status IN ('unknown','accepted')",params![request,status,now])?;Ok(())
 }
 /// Caller must authenticate exact native request and complete turn; source text
 /// is preserved as external evidence, never as human approval or verification.
 pub fn assistant_return_result(&mut self,request:&str,connection:&str,session:&str,text:&str,sequences:&[i64],now:i64)->Result<()> {
  if text.trim().is_empty()||text.len()>256*1024||sequences.is_empty()||sequences.iter().any(|v|*v<0){return Err(Error::InvalidTransition)}
  let tx=self.store.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let row:Option<(String,String,String,String,String)>=tx.query_row("SELECT l.connection_id,l.native_session_id,l.status,m.conversation_id,m.id FROM assistant_dispatch_links l JOIN messages m ON m.request_id=l.request_id WHERE l.request_id=?1",[request],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
  let Some((c,s,status,conversation,user_message))=row else{return Err(Error::InvalidTransition)};
  if c!=connection||s!=session{return Err(Error::IdentityConflict)}
  let reply_id=format!("assistant-native:{}",digest(request.as_bytes()));
  let payload=json!({"text":text,"source":"native_session_result","reply_to":user_message,"connection_id":connection,"native_session_id":session,"native_request_id":request,"sequences":sequences});
  let encoded=serde_json::to_string(&payload)?;
  if status=="completed" {
   let old:String=tx.query_row("SELECT content_json FROM messages WHERE id=?1",[&reply_id],|r|r.get(0))?;
   return if old==encoded {Ok(())}else{Err(Error::IdentityConflict)};
  }
  if !matches!(status.as_str(),"accepted"|"unknown"){return Err(Error::InvalidTransition)}
  let evidence=format!("assistant-native-evidence:{}",digest(request.as_bytes()));
  tx.execute("INSERT INTO messages(id,conversation_id,origin_device_id,role,content_json,acceptance,created_at) VALUES(?1,?2,?3,'assistant',?4,'local',?5)",params![reply_id,conversation,self.origin,encoded,now])?;
  tx.execute("INSERT INTO evidence(id,source_kind,source_ref,content_sha256,captured_at,verification_scope) VALUES(?1,'native_session_result',?2,?3,?4,'authenticated_request_and_turn_only')",params![evidence,serde_json::to_string(&json!({"connection_id":connection,"session_id":session,"request_id":request,"sequences":sequences}))?,digest(encoded.as_bytes()),now])?;
  tx.execute("INSERT INTO evidence_links(evidence_id,object_kind,object_id,relation) VALUES(?1,'message',?2,'observed')",params![evidence,reply_id])?;
  tx.execute("UPDATE assistant_dispatch_links SET status='completed',result_sequences_json=?2,updated_at=?3 WHERE request_id=?1",params![request,serde_json::to_string(sequences)?,now])?;
  tx.commit()?;Ok(())
 }
}
