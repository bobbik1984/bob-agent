use crate::{app::LocalApp, outbox::{digest, Error, Result}};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};
const SCHEMA: &str = include_str!("../migrations/0010.sql");
pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    let hash = digest(SCHEMA.replace("\r\n", "\n").as_bytes());
    let old: Option<String> = tx.query_row("SELECT checksum FROM schema_migrations WHERE version=10", [], |r| r.get(0)).optional()?;
    match old {
        Some(v) if v != hash => return Err(Error::SchemaMismatch),
        Some(_) => (),
        None => {tx.execute_batch(SCHEMA)?; tx.execute("INSERT INTO schema_migrations VALUES(10,?1,?2)",params![hash,now])?;}
    }
    Ok(())
}
impl LocalApp {
    pub fn project_session_links(&self) -> Result<Value> {
        let mut stmt = self.store.db.prepare("SELECT l.connection_id,l.harness,l.native_session_id,l.project_id,l.owner_instance,l.version,p.title FROM project_session_links l LEFT JOIN projects p ON p.id=l.project_id ORDER BY l.connection_id,l.harness,l.native_session_id")?;
        let rows = stmt.query_map([], |r| Ok(json!({
            "connection_id":r.get::<_,String>(0)?,"harness":r.get::<_,String>(1)?,
            "session_id":r.get::<_,String>(2)?,"project_id":r.get::<_,Option<String>>(3)?,
            "owner_instance":r.get::<_,String>(4)?,"version":r.get::<_,i64>(5)?,
            "project_title":r.get::<_,Option<String>>(6)?
        })))?.collect::<std::result::Result<Vec<_>,_>>()?;
        Ok(json!({"owner_instance":self.origin,"links":rows}))
    }
    pub fn set_project_session_link(&mut self, connection: &str, harness: &str, session: &str, project: Option<&str>, expected: i64, now: i64) -> Result<Value> {
        if [connection,harness,session].iter().any(|s| s.is_empty() || s.len()>512 || s.contains('\0')) || expected<0 {return Err(Error::InvalidTransition);}
        let tx = self.store.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(p) = project {
            let exists: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND acceptance='local')",[p],|r|r.get(0))?;
            if !exists {return Err(Error::InvalidTransition);}
        }
        let old: Option<(String,i64)> = tx.query_row("SELECT owner_instance,version FROM project_session_links WHERE connection_id=?1 AND harness=?2 AND native_session_id=?3",params![connection,harness,session],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if old.as_ref().map(|o|o.1).unwrap_or(0)!=expected || old.as_ref().is_some_and(|o|o.0!=self.origin) {return Err(Error::IdentityConflict);}
        tx.execute("INSERT INTO project_session_links VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(connection_id,harness,native_session_id) DO UPDATE SET project_id=excluded.project_id,version=excluded.version,updated_at=excluded.updated_at",params![connection,harness,session,project,self.origin,expected+1,now])?;
        tx.commit()?;
        self.project_session_links()
    }
}
