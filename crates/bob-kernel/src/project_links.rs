use crate::{
    app::LocalApp,
    outbox::{digest, Error, Result},
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};

const SCHEMA: &str = include_str!("../migrations/0010.sql");

/// Authorization context supplied by a trusted entry point, never deserialized
/// from an external request. All writers use the same transaction below.
pub enum ProjectLinkAuthority<'a> {
    Owner,
    Scoped {
        organize: bool,
        projects: &'a [String],
        sessions: &'a [(String, String, String)],
    },
}


pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    let hash = digest(SCHEMA.replace("\r\n", "\n").as_bytes());
    let old: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=10",
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
                "INSERT INTO schema_migrations VALUES(10,?1,?2)",
                params![hash, now],
            )?;
        }
    }
    Ok(())
}

impl LocalApp {
    pub fn project_session_links(&self) -> Result<Value> {
        let mut stmt = self.store.db.prepare(
            "SELECT l.connection_id,l.harness,l.native_session_id,l.project_id,l.owner_instance,l.version,p.title
             FROM project_session_links l
             LEFT JOIN projects p ON p.id=l.project_id
             ORDER BY l.connection_id,l.harness,l.native_session_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "connection_id": r.get::<_, String>(0)?,
                    "harness": r.get::<_, String>(1)?,
                    "session_id": r.get::<_, String>(2)?,
                    "project_id": r.get::<_, Option<String>>(3)?,
                    "owner_instance": r.get::<_, String>(4)?,
                    "version": r.get::<_, i64>(5)?,
                    "project_title": r.get::<_, Option<String>>(6)?
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({ "owner_instance": self.origin, "links": rows }))
    }

    pub fn set_project_session_link(
        &mut self,
        connection: &str,
        harness: &str,
        session: &str,
        project: Option<&str>,
        expected: i64,
        now: i64,
    ) -> Result<Value> {
        self.set_project_session_link_authorized(
            ProjectLinkAuthority::Owner, connection, harness, session, project, expected, now,
        )
    }

    pub fn set_project_session_link_authorized(
        &mut self,
        authority: ProjectLinkAuthority<'_>,
        connection: &str,
        harness: &str,
        session: &str,
        project: Option<&str>,
        expected: i64,
        now: i64,
    ) -> Result<Value> {
        if let ProjectLinkAuthority::Scoped { organize, projects, sessions } = &authority {
            if !*organize
                || !project.is_some_and(|p| projects.iter().any(|id| id == p))
                || !sessions.iter().any(|(c,h,s)| c == connection && h == harness && s == session)
            { return Err(Error::InvalidTransition); }
        }
        if [connection, harness, session]
            .iter()
            .any(|s| s.is_empty() || s.len() > 512 || s.contains('\0'))
            || expected < 0 || expected == i64::MAX
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(p) = project {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND acceptance='local')",
                [p],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::InvalidTransition);
            }
        }
        let old: Option<(String, i64, Option<String>)> = tx
            .query_row(
                "SELECT owner_instance,version,project_id FROM project_session_links WHERE connection_id=?1 AND harness=?2 AND native_session_id=?3",
                params![connection, harness, session],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        // Check the source project under the same SQLite write lock as the CAS.
        if let ProjectLinkAuthority::Scoped { projects, .. } = &authority {
            if old.as_ref().and_then(|o| o.2.as_ref()).is_some_and(|p| !projects.contains(p)) {
                return Err(Error::InvalidTransition);
            }
        }
        if old.as_ref().map(|o| o.1).unwrap_or(0) != expected
            || old.as_ref().is_some_and(|o| o.0 != self.origin)
        {
            return Err(Error::IdentityConflict);
        }
        tx.execute(
            "INSERT INTO project_session_links VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(connection_id,harness,native_session_id)
             DO UPDATE SET project_id=excluded.project_id,version=excluded.version,updated_at=excluded.updated_at",
            params![
                connection,
                harness,
                session,
                project,
                self.origin,
                expected + 1,
                now
            ],
        )?;
        tx.commit()?;
        self.project_session_links()
    }

    pub fn export_projects_and_links(&self) -> Result<Value> {
        let mut proj_stmt = self.store.db.prepare(
            "SELECT id, title, status, version, created_at, updated_at
             FROM projects
             WHERE id NOT LIKE 'local-project:%' AND id NOT LIKE 'direct-project:%' AND status != 'archived'
             ORDER BY updated_at DESC, id ASC",
        )?;
        let projects = proj_stmt
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "title": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?,
                    "version": r.get::<_, i64>(3)?,
                    "created_at": r.get::<_, i64>(4)?,
                    "updated_at": r.get::<_, i64>(5)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut link_stmt = self.store.db.prepare(
            "SELECT l.connection_id, l.harness, l.native_session_id, l.project_id, l.owner_instance, l.version, l.updated_at, p.title
             FROM project_session_links l
             LEFT JOIN projects p ON p.id=l.project_id
             ORDER BY l.connection_id, l.harness, l.native_session_id",
        )?;
        let links = link_stmt
            .query_map([], |r| {
                Ok(json!({
                    "connection_id": r.get::<_, String>(0)?,
                    "harness": r.get::<_, String>(1)?,
                    "session_id": r.get::<_, String>(2)?,
                    "project_id": r.get::<_, Option<String>>(3)?,
                    "owner_instance": r.get::<_, String>(4)?,
                    "version": r.get::<_, i64>(5)?,
                    "updated_at": r.get::<_, i64>(6)?,
                    "project_title": r.get::<_, Option<String>>(7)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(json!({
            "projects": projects,
            "projectSessionLinks": links,
        }))
    }

    pub fn merge_projects_and_links(&mut self, incoming: &Value, now: i64) -> Result<()> {
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(projects) = incoming.get("projects").and_then(Value::as_array) {
            for p in projects {
                let id = p
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                let title = p
                    .get("title")
                    .or_else(|| p.get("name"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                if id.is_empty()
                    || id.len() > 128
                    || id.contains('\0')
                    || id.starts_with("local-project:")
                    || id.starts_with("direct-project:")
                    || title.is_empty()
                    || title.len() > 256
                {
                    continue;
                }
                let status = p
                    .get("status")
                    .and_then(Value::as_str)
                    .filter(|s| matches!(*s, "active" | "completed" | "archived"))
                    .unwrap_or("active");
                let in_ver = p.get("version").and_then(Value::as_i64).unwrap_or(1).max(1);
                let created_at = p
                    .get("created_at")
                    .or_else(|| p.get("createdAt"))
                    .and_then(Value::as_i64)
                    .unwrap_or(now);
                let updated_at = p
                    .get("updated_at")
                    .or_else(|| p.get("updatedAt"))
                    .and_then(Value::as_i64)
                    .unwrap_or(now);

                let existing: Option<(i64, i64)> = tx
                    .query_row(
                        "SELECT version, updated_at FROM projects WHERE id=?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                match existing {
                    None => {
                        tx.execute(
                            "INSERT INTO projects(id,title,acceptance,status,version,created_at,updated_at)
                             VALUES(?1,?2,'local',?3,?4,?5,?6)",
                            params![id, title, status, in_ver, created_at, updated_at],
                        )?;
                    }
                    Some((cur_ver, cur_updated)) => {
                        if in_ver > cur_ver || (in_ver == cur_ver && updated_at >= cur_updated) {
                            tx.execute(
                                "UPDATE projects SET title=?2, acceptance='local', status=?3, version=?4, updated_at=?5 WHERE id=?1",
                                params![
                                    id,
                                    title,
                                    status,
                                    in_ver.max(cur_ver),
                                    updated_at.max(cur_updated)
                                ],
                            )?;
                        } else {
                            tx.execute(
                                "UPDATE projects SET acceptance='local' WHERE id=?1 AND acceptance!='local'",
                                [id],
                            )?;
                        }
                    }
                }
            }
        }

        if let Some(links) = incoming
            .get("projectSessionLinks")
            .or_else(|| incoming.get("project_session_links"))
            .and_then(Value::as_array)
        {
            for l in links {
                let conn = l
                    .get("connection_id")
                    .or_else(|| l.get("connectionId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                let harness = l
                    .get("harness")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                let session = l
                    .get("session_id")
                    .or_else(|| l.get("sessionId"))
                    .or_else(|| l.get("native_session_id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                if [conn, harness, session]
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 512 || s.contains('\0'))
                {
                    continue;
                }
                let proj_opt = l
                    .get("project_id")
                    .or_else(|| l.get("projectId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                if let Some(pid) = proj_opt {
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                        [pid],
                        |r| r.get(0),
                    )?;
                    if !exists {
                        let ptitle = l
                            .get("project_title")
                            .or_else(|| l.get("projectTitle"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(pid);
                        tx.execute(
                            "INSERT OR IGNORE INTO projects(id,title,acceptance,status,version,created_at,updated_at)
                             VALUES(?1,?2,'local','active',1,?3,?3)",
                            params![pid, ptitle, now],
                        )?;
                    }
                }
                let in_ver = l.get("version").and_then(Value::as_i64).unwrap_or(1).max(1);
                let in_updated = l
                    .get("updated_at")
                    .or_else(|| l.get("updatedAt"))
                    .and_then(Value::as_i64)
                    .unwrap_or(now);
                let existing: Option<(i64, i64)> = tx
                    .query_row(
                        "SELECT version, updated_at FROM project_session_links WHERE connection_id=?1 AND harness=?2 AND native_session_id=?3",
                        params![conn, harness, session],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                match existing {
                    None => {
                        tx.execute(
                            "INSERT INTO project_session_links(connection_id,harness,native_session_id,project_id,owner_instance,version,updated_at)
                             VALUES(?1,?2,?3,?4,?5,?6,?7)",
                            params![conn, harness, session, proj_opt, self.origin, in_ver, in_updated],
                        )?;
                    }
                    Some((cur_ver, cur_updated)) => {
                        if in_ver > cur_ver || (in_ver == cur_ver && in_updated >= cur_updated) {
                            tx.execute(
                                "UPDATE project_session_links
                                 SET project_id=?4, owner_instance=?5, version=?6, updated_at=?7
                                 WHERE connection_id=?1 AND harness=?2 AND native_session_id=?3",
                                params![
                                    conn,
                                    harness,
                                    session,
                                    proj_opt,
                                    self.origin,
                                    in_ver.max(cur_ver),
                                    in_updated.max(cur_updated)
                                ],
                            )?;
                        }
                    }
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
}
