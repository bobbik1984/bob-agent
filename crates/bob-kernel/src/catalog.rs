//! Multi-project, multi-conversation directory catalog, per-session view state
//! (draft with revision CAS, scroll anchor/offset, unread tracking), and
//! device-grouped / project-grouped projection.
use crate::{
    app::LocalApp,
    outbox::{digest, Error, LocalAccepted, Result},
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;

const MIGRATION_0005: &str = include_str!("../migrations/0005.sql");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub conversation_count: i64,
    pub unread_count: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationViewState {
    pub conversation_id: String,
    pub draft: String,
    pub draft_revision: i64,
    pub scroll_anchor_message_id: Option<String>,
    pub scroll_offset: i64,
    pub last_read_message_id: Option<String>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceUiPreferences {
    pub selected_conversation_id: String,
    pub sidebar_group_mode: String,
    pub locale: String,
    pub theme: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationItem {
    pub id: String,
    pub project_id: Option<String>,
    pub project_title: Option<String>,
    pub title: String,
    pub acceptance: String,
    pub target_device_id: String,
    pub target_device_label: String,
    pub target_device_online: bool,
    pub status_reason: String,
    pub message_count: i64,
    pub unread_count: i64,
    pub has_running_execution: bool,
    pub last_message_preview: String,
    pub updated_at: i64,
    pub view_state: ConversationViewState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDirectoryGroup {
    pub device_id: String,
    pub label: String,
    pub role: String,
    pub availability_state: String,
    pub immediate_execution_available: bool,
    pub status_reason: String,
    pub conversations: Vec<ConversationItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    pub selected_conversation_id: String,
    pub default_session_id: String,
    pub preferences: DeviceUiPreferences,
    pub projects: Vec<ProjectItem>,
    pub conversations: Vec<ConversationItem>,
    pub standalone_conversations: Vec<ConversationItem>,
    pub device_groups: Vec<DeviceDirectoryGroup>,
}

pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    let hash = digest(MIGRATION_0005.replace("\r\n", "\n").as_bytes());
    let previous: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=5",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match previous {
        Some(v) if v != hash => return Err(Error::SchemaMismatch),
        Some(_) => {}
        None => {
            tx.execute_batch(MIGRATION_0005)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(5,?1,?2)",
                params![hash, now],
            )?;
        }
    }
    Ok(())
}

pub(crate) fn ensure_bootstrap_catalog_in_tx(
    tx: &Transaction<'_>,
    origin: &str,
    default_session: &str,
    now: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE conversations SET project_id=NULL WHERE project_id LIKE 'local-project:%' OR project_id LIKE 'direct-project:%'",
        [],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO conversations(id,project_id,title,acceptance,version,created_at) VALUES(?1,NULL,'','local',1,?2)",
        params![default_session, now],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO conversation_view_states(origin_device_id,conversation_id,draft,draft_revision,scroll_anchor_message_id,scroll_offset,last_read_message_id,updated_at) VALUES(?1,?2,'',0,NULL,0,NULL,?3)",
        params![origin, default_session, now],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO device_ui_preferences(origin_device_id,selected_conversation_id,sidebar_group_mode,locale,theme,updated_at) VALUES(?1,?2,'project','zh-CN','system',?3)",
        params![origin, default_session, now],
    )?;
    tx.execute(
        "UPDATE device_ui_preferences SET selected_conversation_id=?2 WHERE origin_device_id=?1 AND (selected_conversation_id IS NULL OR NOT EXISTS(SELECT 1 FROM conversations c WHERE c.id=device_ui_preferences.selected_conversation_id AND c.archived_at IS NULL))",
        params![origin, default_session],
    )?;
    Ok(())
}

fn valid_id(id: &str) -> bool {
    let trimmed = id.trim();
    !trimmed.is_empty() && trimmed.len() <= 128 && !trimmed.contains('\0')
}

impl LocalApp {
    pub(crate) fn ensure_active_conversation(&self, session_id: &str) -> Result<()> {
        if session_id.trim().is_empty() {
            return Err(Error::InvalidTransition);
        }
        let exists: bool = self.store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1 AND archived_at IS NULL)",
            [session_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::InvalidTransition);
        }
        Ok(())
    }

    pub fn selected_conversation(&self) -> Result<String> {
        let selected: Option<String> = self
            .store
            .db
            .query_row(
                "SELECT selected_conversation_id FROM device_ui_preferences WHERE origin_device_id=?1",
                [&self.origin],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(selected.unwrap_or_else(|| self.session.clone()))
    }

    pub fn accept_with_draft_revision(
        &mut self,
        request_id: &str,
        message_id: &str,
        session_id: &str,
        content: String,
        expected_draft_revision: Option<i64>,
        now: i64,
    ) -> Result<LocalAccepted> {
        let accepted = self.accept(request_id, message_id, session_id, content, now)?;
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO conversation_view_states(origin_device_id,conversation_id,draft,draft_revision,scroll_anchor_message_id,scroll_offset,last_read_message_id,updated_at) VALUES(?1,?2,'',0,NULL,0,?3,?4)",
            params![self.origin, session_id, message_id, now],
        )?;
        if let Some(expected_rev) = expected_draft_revision {
            tx.execute(
                "UPDATE conversation_view_states SET draft='',draft_revision=draft_revision+1,last_read_message_id=?3,updated_at=?4 WHERE origin_device_id=?1 AND conversation_id=?2 AND draft_revision=?5",
                params![self.origin, session_id, message_id, now, expected_rev],
            )?;
            tx.execute(
                "UPDATE conversation_view_states SET last_read_message_id=?3,updated_at=?4 WHERE origin_device_id=?1 AND conversation_id=?2 AND draft_revision!=?5",
                params![self.origin, session_id, message_id, now, expected_rev],
            )?;
        } else {
            tx.execute(
                "UPDATE conversation_view_states SET last_read_message_id=?3,updated_at=?4 WHERE origin_device_id=?1 AND conversation_id=?2",
                params![self.origin, session_id, message_id, now],
            )?;
        }
        tx.commit()?;
        Ok(accepted)
    }

    pub fn create_project(
        &mut self,
        project_id: &str,
        title: &str,
        now: i64,
    ) -> Result<ProjectItem> {
        let title = title.trim();
        if !valid_id(project_id)
            || project_id.starts_with("local-project:")
            || title.is_empty()
            || title.len() > 256
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT title FROM projects WHERE id=?1",
                [project_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old_title) = existing {
            if old_title != title {
                return Err(Error::IdentityConflict);
            }
        } else {
            tx.execute(
                "INSERT INTO projects(id,title,acceptance,status,version,created_at,updated_at) VALUES(?1,?2,'local','active',1,?3,?3)",
                params![project_id, title, now],
            )?;
        }
        tx.commit()?;
        let snapshot = self.catalog_snapshot()?;
        snapshot
            .projects
            .into_iter()
            .find(|p| p.id == project_id)
            .ok_or(Error::InvalidTransition)
    }

    pub fn create_conversation(
        &mut self,
        conversation_id: &str,
        project_id: Option<&str>,
        title: &str,
        now: i64,
    ) -> Result<ConversationItem> {
        self.create_conversation_impl(conversation_id, project_id, title, now, true)
    }

    /// Background imports must never change the user's active conversation.
    pub fn import_conversation(
        &mut self, conversation_id: &str, project_id: Option<&str>, title: &str, now: i64,
    ) -> Result<ConversationItem> {
        self.create_conversation_impl(conversation_id, project_id, title, now, false)
    }

    fn create_conversation_impl(
        &mut self, conversation_id: &str, project_id: Option<&str>, title: &str, now: i64,
        select: bool,
    ) -> Result<ConversationItem> {
        let title = title.trim();
        let project_id = project_id.map(str::trim).filter(|s| !s.is_empty());
        if !valid_id(conversation_id) || title.len() > 256 {
            return Err(Error::InvalidTransition);
        }
        if let Some(pid) = project_id {
            if !valid_id(pid) || pid.starts_with("local-project:") {
                return Err(Error::InvalidTransition);
            }
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(pid) = project_id {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND id NOT LIKE 'local-project:%' AND status!='archived')",
                [pid],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::InvalidTransition);
            }
        }
        let existing: Option<(Option<String>, String)> = tx
            .query_row(
                "SELECT project_id, title FROM conversations WHERE id=?1",
                [conversation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old_pid, old_title)) = existing {
            if old_pid.as_deref() != project_id || (!title.is_empty() && old_title != title) {
                return Err(Error::IdentityConflict);
            }
        } else {
            tx.execute(
                "INSERT INTO conversations(id,project_id,title,acceptance,version,created_at) VALUES(?1,?2,?3,'local',1,?4)",
                params![conversation_id, project_id, title, now],
            )?;
        }
        tx.execute(
            "INSERT OR IGNORE INTO conversation_view_states(origin_device_id,conversation_id,draft,draft_revision,scroll_anchor_message_id,scroll_offset,last_read_message_id,updated_at) VALUES(?1,?2,'',0,NULL,0,NULL,?3)",
            params![self.origin, conversation_id, now],
        )?;
        if select {
        tx.execute(
            "UPDATE device_ui_preferences SET selected_conversation_id=?2,updated_at=?3 WHERE origin_device_id=?1",
            params![self.origin, conversation_id, now],
        )?;
        }
        if let Some(pid) = project_id {
            tx.execute(
                "UPDATE projects SET updated_at=?2,version=version+1 WHERE id=?1",
                params![pid, now],
            )?;
        }
        tx.commit()?;
        let snapshot = self.catalog_snapshot()?;
        snapshot
            .conversations
            .into_iter()
            .find(|c| c.id == conversation_id)
            .ok_or(Error::InvalidTransition)
    }

    pub fn promote_conversation_to_project(
        &mut self,
        conversation_id: &str,
        project_id: &str,
        project_title: &str,
        now: i64,
    ) -> Result<CatalogSnapshot> {
        let cid = conversation_id.trim();
        let pid = project_id.trim();
        let ptitle = project_title.trim();
        if !valid_id(cid)
            || !valid_id(pid)
            || pid.starts_with("local-project:")
            || ptitle.is_empty()
            || ptitle.len() > 256
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing_conv: Option<(Option<String>, String)> = tx
            .query_row(
                "SELECT project_id, title FROM conversations WHERE id=?1 AND archived_at IS NULL",
                [cid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((current_pid, current_title)) = existing_conv else {
            return Err(Error::InvalidTransition);
        };
        if let Some(ref cur) = current_pid {
            if cur != pid {
                return Err(Error::IdentityConflict);
            }
        }
        tx.execute(
            "INSERT INTO projects(id,title,acceptance,status,version,created_at,updated_at)
             VALUES(?1,?2,'local','active',1,?3,?3)
             ON CONFLICT(id) DO UPDATE SET updated_at=?3",
            params![pid, ptitle, now],
        )?;
        let effective_title = if current_title.trim().is_empty() {
            ptitle.to_owned()
        } else {
            current_title
        };
        if current_pid.as_deref() != Some(pid) {
            tx.execute(
                "UPDATE conversations SET project_id=?2, title=?3, version=version+1 WHERE id=?1",
                params![cid, pid, effective_title],
            )?;
        }
        tx.execute(
            "UPDATE notes SET project_id=?2, version=version+1, updated_at=?3 WHERE conversation_id=?1 AND project_id IS NULL",
            params![cid, pid, now],
        )?;
        let source_msg_id: Option<String> = tx
            .query_row(
                "SELECT id FROM messages WHERE conversation_id=?1 ORDER BY created_at ASC, rowid ASC LIMIT 1",
                [cid],
                |r| r.get(0),
            )
            .optional()?;
        let task_id = format!("task-conv-{}", &digest(cid.as_bytes())[..24]);
        let constraints = json!({
            "conversation_id": cid,
            "promoted_from_standalone": true
        })
        .to_string();
        tx.execute(
            "INSERT INTO tasks(id,project_id,goal_id,milestone_id,source_message_id,title,status,constraints_json,version,created_at,updated_at)
             VALUES(?1,?2,NULL,NULL,?3,?4,'running',?5,1,?6,?6)
             ON CONFLICT(id) DO UPDATE SET project_id=?2, updated_at=?6",
            params![task_id, pid, source_msg_id, effective_title, constraints, now],
        )?;
        tx.execute(
            "UPDATE decisions SET project_id=?2, task_id=COALESCE(task_id,?3), version=version+1
             WHERE source_message_id IN (SELECT id FROM messages WHERE conversation_id=?1)",
            params![cid, pid, task_id],
        )?;
        tx.execute(
            "UPDATE device_ui_preferences SET selected_conversation_id=?2, updated_at=?3 WHERE origin_device_id=?1",
            params![self.origin, cid, now],
        )?;
        tx.commit()?;
        self.catalog_snapshot()
    }

    pub fn select_conversation(
        &mut self,
        conversation_id: &str,
        now: i64,
    ) -> Result<ConversationItem> {
        self.ensure_active_conversation(conversation_id)?;
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let latest_msg: Option<String> = tx
            .query_row(
                "SELECT id FROM messages WHERE conversation_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [conversation_id],
                |r| r.get(0),
            )
            .optional()?;
        tx.execute(
            "INSERT OR IGNORE INTO conversation_view_states(origin_device_id,conversation_id,draft,draft_revision,scroll_anchor_message_id,scroll_offset,last_read_message_id,updated_at) VALUES(?1,?2,'',0,NULL,0,?3,?4)",
            params![self.origin, conversation_id, latest_msg, now],
        )?;
        if let Some(msg_id) = latest_msg {
            tx.execute(
                "UPDATE conversation_view_states SET last_read_message_id=?3,updated_at=?4 WHERE origin_device_id=?1 AND conversation_id=?2",
                params![self.origin, conversation_id, msg_id, now],
            )?;
        }
        tx.execute(
            "UPDATE device_ui_preferences SET selected_conversation_id=?2,updated_at=?3 WHERE origin_device_id=?1",
            params![self.origin, conversation_id, now],
        )?;
        tx.commit()?;
        let snapshot = self.catalog_snapshot()?;
        snapshot
            .conversations
            .into_iter()
            .find(|c| c.id == conversation_id)
            .ok_or(Error::InvalidTransition)
    }

    pub fn save_conversation_view_state(
        &mut self,
        conversation_id: &str,
        draft: Option<&str>,
        scroll_anchor_message_id: Option<Option<&str>>,
        scroll_offset: Option<i64>,
        mark_read: bool,
        now: i64,
    ) -> Result<ConversationViewState> {
        self.ensure_active_conversation(conversation_id)?;
        if let Some(d) = draft {
            if d.len() > 256 * 1024 {
                return Err(Error::InvalidTransition);
            }
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO conversation_view_states(origin_device_id,conversation_id,draft,draft_revision,scroll_anchor_message_id,scroll_offset,last_read_message_id,updated_at) VALUES(?1,?2,'',0,NULL,0,NULL,?3)",
            params![self.origin, conversation_id, now],
        )?;
        if let Some(new_draft) = draft {
            let current_draft: String = tx.query_row(
                "SELECT draft FROM conversation_view_states WHERE origin_device_id=?1 AND conversation_id=?2",
                params![self.origin, conversation_id],
                |r| r.get(0),
            )?;
            if current_draft != new_draft {
                tx.execute(
                    "UPDATE conversation_view_states SET draft=?3,draft_revision=draft_revision+1,updated_at=?4 WHERE origin_device_id=?1 AND conversation_id=?2",
                    params![self.origin, conversation_id, new_draft, now],
                )?;
            }
        }
        if let Some(anchor) = scroll_anchor_message_id {
            let anchor = anchor.map(str::trim).filter(|s| !s.is_empty());
            tx.execute(
                "UPDATE conversation_view_states SET scroll_anchor_message_id=?3,updated_at=?4 WHERE origin_device_id=?1 AND conversation_id=?2",
                params![self.origin, conversation_id, anchor, now],
            )?;
        }
        if let Some(offset) = scroll_offset {
            tx.execute(
                "UPDATE conversation_view_states SET scroll_offset=?3,updated_at=?4 WHERE origin_device_id=?1 AND conversation_id=?2",
                params![self.origin, conversation_id, offset.max(0), now],
            )?;
        }
        if mark_read {
            let latest_msg: Option<String> = tx
                .query_row(
                    "SELECT id FROM messages WHERE conversation_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                    [conversation_id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(msg_id) = latest_msg {
                tx.execute(
                    "UPDATE conversation_view_states SET last_read_message_id=?3,updated_at=?4 WHERE origin_device_id=?1 AND conversation_id=?2",
                    params![self.origin, conversation_id, msg_id, now],
                )?;
            }
        }
        let state = query_view_state(&tx, &self.origin, conversation_id)?;
        tx.commit()?;
        Ok(state)
    }

    pub fn conversation_view_state(&self, conversation_id: &str) -> Result<ConversationViewState> {
        self.ensure_active_conversation(conversation_id)?;
        query_view_state(&self.store.db, &self.origin, conversation_id)
    }

    pub fn save_ui_preferences(
        &mut self,
        sidebar_group_mode: Option<&str>,
        locale: Option<&str>,
        theme: Option<&str>,
        now: i64,
    ) -> Result<DeviceUiPreferences> {
        if let Some(mode) = sidebar_group_mode {
            if !matches!(mode, "project" | "device") {
                return Err(Error::InvalidTransition);
            }
        }
        if let Some(loc) = locale {
            if !matches!(loc, "zh-CN" | "en-US") {
                return Err(Error::InvalidTransition);
            }
        }
        if let Some(th) = theme {
            if !matches!(th, "system" | "light" | "dark") {
                return Err(Error::InvalidTransition);
            }
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(mode) = sidebar_group_mode {
            tx.execute(
                "UPDATE device_ui_preferences SET sidebar_group_mode=?2,updated_at=?3 WHERE origin_device_id=?1",
                params![self.origin, mode, now],
            )?;
        }
        if let Some(loc) = locale {
            tx.execute(
                "UPDATE device_ui_preferences SET locale=?2,updated_at=?3 WHERE origin_device_id=?1",
                params![self.origin, loc, now],
            )?;
        }
        if let Some(th) = theme {
            tx.execute(
                "UPDATE device_ui_preferences SET theme=?2,updated_at=?3 WHERE origin_device_id=?1",
                params![self.origin, th, now],
            )?;
        }
        let prefs = query_preferences(&tx, &self.origin, &self.session)?;
        tx.commit()?;
        Ok(prefs)
    }

    pub fn ui_preferences(&self) -> Result<DeviceUiPreferences> {
        query_preferences(&self.store.db, &self.origin, &self.session)
    }

    pub fn set_device_availability(
        &mut self,
        device_id: &str,
        availability_state: &str,
        reason: &str,
        now: i64,
    ) -> Result<()> {
        if !valid_id(device_id)
            || !matches!(availability_state, "online" | "sleeping" | "offline")
            || reason.len() > 512
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO devices(id,label,trust_state,created_at) VALUES(?1,?1,'unpaired',?2)",
            params![device_id, now],
        )?;
        let ws_id = format!("device-workspace:{device_id}");
        tx.execute(
            "INSERT OR IGNORE INTO workspaces(id,device_id,directory_ref,policy_ref,version) VALUES(?1,?2,'device-default','default',1)",
            params![ws_id, device_id],
        )?;
        tx.execute(
            "INSERT INTO workspace_states(workspace_id,availability_state,availability_reason,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(workspace_id) DO UPDATE SET availability_state=excluded.availability_state,availability_reason=excluded.availability_reason,updated_at=excluded.updated_at",
            params![ws_id, availability_state, reason.trim(), now],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn catalog_snapshot(&self) -> Result<CatalogSnapshot> {
        let db = &self.store.db;
        let preferences = query_preferences(db, &self.origin, &self.session)?;
        let selected_conversation_id = preferences.selected_conversation_id.clone();
        let paired_home = self.paired_home()?;
        let default_target_device_id = paired_home
            .as_ref()
            .map(|(h, _, _)| h.clone())
            .unwrap_or_else(|| self.origin.clone());

        let mut conv_stmt = db.prepare(
            "SELECT c.id, c.project_id, p.title, c.title, c.acceptance, c.created_at
             FROM conversations c
             LEFT JOIN projects p ON p.id = c.project_id AND p.id NOT LIKE 'local-project:%' AND p.id NOT LIKE 'direct-project:%'
             WHERE c.archived_at IS NULL
             ORDER BY c.created_at ASC, c.rowid ASC",
        )?;
        let raw_convs: Vec<(String, Option<String>, Option<String>, String, String, i64)> =
            conv_stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get::<_, Option<String>>(1)?
                            .filter(|pid| !pid.starts_with("local-project:") && !pid.starts_with("direct-project:")),
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut conversations = Vec::with_capacity(raw_convs.len());
        for (cid, project_id, project_title, raw_title, acceptance, created_at) in raw_convs {
            let view_state = query_view_state(db, &self.origin, &cid).unwrap_or(
                ConversationViewState {
                    conversation_id: cid.clone(),
                    draft: String::new(),
                    draft_revision: 0,
                    scroll_anchor_message_id: None,
                    scroll_offset: 0,
                    last_read_message_id: None,
                    updated_at: created_at,
                },
            );
            let message_count: i64 = db.query_row(
                "SELECT count(*) FROM messages WHERE conversation_id=?1",
                [&cid],
                |r| r.get(0),
            )?;
            let last_msg: Option<(String, String, i64, i64)> = db
                .query_row(
                    "SELECT id, content_json, created_at, rowid FROM messages WHERE conversation_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                    [&cid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;
            let first_user_preview: Option<String> = db
                .query_row(
                    "SELECT json_extract(content_json,'$.text') FROM messages WHERE conversation_id=?1 AND role='user' ORDER BY created_at ASC, rowid ASC LIMIT 1",
                    [&cid],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();

            let last_read_rowid: i64 = match &view_state.last_read_message_id {
                Some(mid) => db
                    .query_row(
                        "SELECT rowid FROM messages WHERE id=?1 AND conversation_id=?2",
                        params![mid, cid],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or(0),
                None => 0,
            };
            let unread_count: i64 = if cid == selected_conversation_id {
                0
            } else {
                db.query_row(
                    "SELECT count(*) FROM messages WHERE conversation_id=?1 AND role='assistant' AND rowid>?2",
                    params![cid, last_read_rowid],
                    |r| r.get(0),
                )?
            };

            let local_running: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM executions e JOIN tasks t ON t.id=e.task_id JOIN messages m ON m.id=t.source_message_id WHERE m.conversation_id=?1 AND e.status IN ('pending','dispatching','running','paused','unknown'))",
                [&cid],
                |r| r.get(0),
            )?;
            let remote_running: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM domain_events e JOIN intent_outbox o ON o.request_id=e.request_id WHERE e.object_kind='remote_execution' AND o.session_id=?1 AND NOT EXISTS(SELECT 1 FROM domain_events newer WHERE newer.object_kind='remote_execution' AND newer.request_id=e.request_id AND newer.object_version>e.object_version) AND json_extract(e.payload_json,'$.status') IN ('pending','dispatching','running','paused','unknown'))",
                [&cid],
                |r| r.get(0),
            )?;
            let has_running_execution = local_running || remote_running;

            let target_device_id: String = db
                .query_row(
                    "SELECT target_device_id FROM intent_outbox WHERE session_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                    [&cid],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or_else(|| default_target_device_id.clone());

            let (target_device_label, availability_state, custom_reason) =
                device_status_info(db, &self.origin, &target_device_id, paired_home.is_some())?;
            let target_device_online = availability_state == "online";
            let status_reason = if !custom_reason.is_empty() {
                custom_reason
            } else if target_device_online {
                "当前执行设备在线，可直接发送并继续会话。".to_owned()
            } else {
                "原执行设备当前离线，历史消息与草稿可正常查看；新消息将保存在本地待唤醒发送，或切换至其他已同步副本工作区继续。".to_owned()
            };

            let last_message_preview = match &last_msg {
                Some((_, json_str, _, _)) => serde_json::from_str::<serde_json::Value>(json_str)
                    .ok()
                    .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(str::to_owned))
                    .unwrap_or_default()
                    .chars()
                    .take(60)
                    .collect(),
                None => String::new(),
            };
            let updated_at = last_msg
                .as_ref()
                .map(|(_, _, ts, _)| *ts)
                .unwrap_or(created_at);

            let display_title = if !raw_title.trim().is_empty() {
                raw_title
            } else if let Some(preview) = first_user_preview.filter(|s| !s.trim().is_empty()) {
                preview.trim().chars().take(28).collect()
            } else if cid == self.session {
                "默认会话".to_owned()
            } else {
                "新会话".to_owned()
            };

            conversations.push(ConversationItem {
                id: cid,
                project_id,
                project_title,
                title: display_title,
                acceptance,
                target_device_id,
                target_device_label,
                target_device_online,
                status_reason,
                message_count,
                unread_count,
                has_running_execution,
                last_message_preview,
                updated_at,
                view_state,
            });
        }

        let mut proj_stmt = db.prepare(
            "SELECT id, title, status, updated_at
             FROM projects
             WHERE id NOT LIKE 'local-project:%' AND id NOT LIKE 'direct-project:%' AND status != 'archived'
             ORDER BY updated_at DESC, rowid ASC",
        )?;
        let raw_projects: Vec<(String, String, String, i64)> = proj_stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut projects = Vec::with_capacity(raw_projects.len());
        for (pid, title, status, proj_updated_at) in raw_projects {
            let proj_convs: Vec<&ConversationItem> = conversations
                .iter()
                .filter(|c| c.project_id.as_deref() == Some(pid.as_str()))
                .collect();
            let conversation_count = proj_convs.len() as i64;
            let unread_count: i64 = proj_convs.iter().map(|c| c.unread_count).sum();
            let updated_at = proj_convs
                .iter()
                .map(|c| c.updated_at)
                .max()
                .unwrap_or(proj_updated_at);
            projects.push(ProjectItem {
                id: pid,
                title,
                status,
                conversation_count,
                unread_count,
                updated_at,
            });
        }

        let standalone_conversations: Vec<ConversationItem> = conversations
            .iter()
            .filter(|c| c.project_id.is_none())
            .cloned()
            .collect();

        let mut device_ids: Vec<String> = vec![self.origin.clone()];
        if let Some((home_id, _, _)) = &paired_home {
            if !device_ids.contains(home_id) {
                device_ids.insert(0, home_id.clone());
            }
        }
        for conv in &conversations {
            if !device_ids.contains(&conv.target_device_id) {
                device_ids.push(conv.target_device_id.clone());
            }
        }
        let mut extra_dev_stmt = db.prepare(
            "SELECT DISTINCT device_id FROM workspaces WHERE device_id IS NOT NULL ORDER BY device_id",
        )?;
        let extra_devs: Vec<String> = extra_dev_stmt
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for dev_id in extra_devs {
            if !device_ids.contains(&dev_id) {
                device_ids.push(dev_id);
            }
        }

        let mut device_groups = Vec::new();
        for dev_id in device_ids {
            let (label, availability_state, custom_reason) =
                device_status_info(db, &self.origin, &dev_id, paired_home.is_some())?;
            let role = if Some(dev_id.as_str())
                == paired_home.as_ref().map(|(h, _, _)| h.as_str())
            {
                "paired_home".to_owned()
            } else if dev_id == self.origin {
                "local".to_owned()
            } else {
                "replica".to_owned()
            };
            let immediate_execution_available = availability_state == "online";
            let status_reason = if !custom_reason.is_empty() {
                custom_reason
            } else if immediate_execution_available {
                "当前执行设备在线，可直接在此设备上继续会话或启动只读执行。".to_owned()
            } else {
                "该设备当前离线，立即执行已置灰；历史与草稿仍可查看，新消息将本地保存待唤醒，或切换至其他已同步副本工作区继续。".to_owned()
            };
            let dev_convs: Vec<ConversationItem> = conversations
                .iter()
                .filter(|c| c.target_device_id == dev_id)
                .cloned()
                .collect();
            device_groups.push(DeviceDirectoryGroup {
                device_id: dev_id,
                label,
                role,
                availability_state,
                immediate_execution_available,
                status_reason,
                conversations: dev_convs,
            });
        }

        Ok(CatalogSnapshot {
            selected_conversation_id,
            default_session_id: self.session.clone(),
            preferences,
            projects,
            conversations,
            standalone_conversations,
            device_groups,
        })
    }
}

fn query_view_state(
    db: &rusqlite::Connection,
    origin: &str,
    conversation_id: &str,
) -> Result<ConversationViewState> {
    let row: Option<ConversationViewState> = db
        .query_row(
            "SELECT conversation_id, draft, draft_revision, scroll_anchor_message_id, scroll_offset, last_read_message_id, updated_at
             FROM conversation_view_states
             WHERE origin_device_id=?1 AND conversation_id=?2",
            params![origin, conversation_id],
            |r| {
                Ok(ConversationViewState {
                    conversation_id: r.get(0)?,
                    draft: r.get(1)?,
                    draft_revision: r.get(2)?,
                    scroll_anchor_message_id: r.get(3)?,
                    scroll_offset: r.get(4)?,
                    last_read_message_id: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            },
        )
        .optional()?;
    Ok(row.unwrap_or_else(|| ConversationViewState {
        conversation_id: conversation_id.to_owned(),
        draft: String::new(),
        draft_revision: 0,
        scroll_anchor_message_id: None,
        scroll_offset: 0,
        last_read_message_id: None,
        updated_at: 0,
    }))
}

fn query_preferences(
    db: &rusqlite::Connection,
    origin: &str,
    default_session: &str,
) -> Result<DeviceUiPreferences> {
    let row: Option<DeviceUiPreferences> = db
        .query_row(
            "SELECT COALESCE(selected_conversation_id, ?2), sidebar_group_mode, locale, theme, updated_at
             FROM device_ui_preferences
             WHERE origin_device_id=?1",
            params![origin, default_session],
            |r| {
                Ok(DeviceUiPreferences {
                    selected_conversation_id: r.get(0)?,
                    sidebar_group_mode: r.get(1)?,
                    locale: r.get(2)?,
                    theme: r.get(3)?,
                    updated_at: r.get(4)?,
                })
            },
        )
        .optional()?;
    Ok(row.unwrap_or_else(|| DeviceUiPreferences {
        selected_conversation_id: default_session.to_owned(),
        sidebar_group_mode: "project".to_owned(),
        locale: "zh-CN".to_owned(),
        theme: "system".to_owned(),
        updated_at: 0,
    }))
}

fn device_status_info(
    db: &rusqlite::Connection,
    origin: &str,
    device_id: &str,
    has_paired_home: bool,
) -> Result<(String, String, String)> {
    let raw_label: Option<String> = db
        .query_row(
            "SELECT label FROM devices WHERE id=?1",
            [device_id],
            |r| r.get(0),
        )
        .optional()?;
    let label = match raw_label.as_deref() {
        Some("local") | None if device_id == origin => {
            if has_paired_home {
                "本机 (副本)".to_owned()
            } else {
                "本机 (Local)".to_owned()
            }
        }
        Some("Home") => "已配对电脑 (Home)".to_owned(),
        Some(l) if !l.trim().is_empty() && l != device_id => l.to_owned(),
        _ => {
            let short: String = device_id.chars().take(8).collect();
            format!("设备 {short}")
        }
    };
    let ws_state: Option<(String, String)> = db
        .query_row(
            "SELECT ws.availability_state, ws.availability_reason
             FROM workspace_states ws
             JOIN workspaces w ON w.id = ws.workspace_id
             WHERE w.device_id = ?1
             ORDER BY ws.updated_at DESC
             LIMIT 1",
            [device_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (availability_state, reason) = ws_state.unwrap_or_else(|| ("online".to_owned(), String::new()));
    Ok((label, availability_state, reason))
}
