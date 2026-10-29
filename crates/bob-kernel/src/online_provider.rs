//! Legacy Online-compatible Provider read-only session catalog and controlled
//! continuation adapter (`BP3-02.a`). Never fabricates cross-node or
//! cross-harness native CLI session resumption.
use crate::{
    app::LocalApp,
    outbox::{digest, Error, Result},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineProviderSession {
    pub id: String,
    pub provider_node_id: String,
    pub provider_label: String,
    pub harness: String,
    pub native_session_id: String,
    pub project_path: String,
    pub title: String,
    pub summary_snippet: String,
    pub turn_count: i64,
    pub write_active: bool,
    pub resumable: bool,
    pub bound_conversation_id: Option<String>,
    pub continuation_mode: Option<String>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineContinuationBinding {
    pub provider_session_id: String,
    pub conversation_id: String,
    pub continuation_mode: String,
    pub native_session_resume: bool,
    pub status_reason: String,
}

impl LocalApp {
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_online_provider_session(
        &mut self,
        provider_node_id: &str,
        provider_label: &str,
        harness: &str,
        native_session_id: &str,
        project_path: &str,
        title: &str,
        summary_snippet: &str,
        turn_count: i64,
        write_active: bool,
        resumable: bool,
        now: i64,
    ) -> Result<OnlineProviderSession> {
        if [
            provider_node_id,
            provider_label,
            harness,
            native_session_id,
            project_path,
            title,
        ]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 512)
            || summary_snippet.len() > 64 * 1024
            || turn_count < 0
        {
            return Err(Error::InvalidTransition);
        }
        let id = format!(
            "online-session:{}",
            digest(format!("{provider_node_id}:{harness}:{native_session_id}").as_bytes())
        );
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO online_provider_sessions(id,provider_node_id,provider_label,harness,native_session_id,project_path,title,summary_snippet,turn_count,write_active,resumable,bound_conversation_id,continuation_mode,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,NULL,NULL,?12)
             ON CONFLICT(provider_node_id,harness,native_session_id) DO UPDATE SET
               provider_label=excluded.provider_label,
               project_path=excluded.project_path,
               title=excluded.title,
               summary_snippet=excluded.summary_snippet,
               turn_count=excluded.turn_count,
               write_active=excluded.write_active,
               resumable=excluded.resumable,
               updated_at=excluded.updated_at",
            params![
                id,
                provider_node_id.trim(),
                provider_label.trim(),
                harness.trim(),
                native_session_id.trim(),
                project_path.trim(),
                title.trim(),
                summary_snippet.trim(),
                turn_count,
                i64::from(write_active),
                i64::from(resumable),
                now
            ],
        )?;
        let session = query_online_session(&tx, &id)?;
        tx.commit()?;
        Ok(session)
    }

    pub fn online_provider_sessions(&self) -> Result<Vec<OnlineProviderSession>> {
        let mut stmt = self.store.db.prepare(
            "SELECT id, provider_node_id, provider_label, harness, native_session_id, project_path, title, summary_snippet, turn_count, write_active, resumable, bound_conversation_id, continuation_mode, updated_at
             FROM online_provider_sessions
             ORDER BY updated_at DESC, rowid ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(OnlineProviderSession {
                id: r.get(0)?,
                provider_node_id: r.get(1)?,
                provider_label: r.get(2)?,
                harness: r.get(3)?,
                native_session_id: r.get(4)?,
                project_path: r.get(5)?,
                title: r.get(6)?,
                summary_snippet: r.get(7)?,
                turn_count: r.get(8)?,
                write_active: r.get::<_, i64>(9)? != 0,
                resumable: r.get::<_, i64>(10)? != 0,
                bound_conversation_id: r.get(11)?,
                continuation_mode: r.get(12)?,
                updated_at: r.get(13)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Bind an Online Provider session into a Bob conversation with explicit
    /// continuation truth (`native_resume_verified` only when same node, same
    /// harness, same project path, resumable=true, and not currently write-active;
    /// otherwise `bounded_history_context`). Rejects concurrent write takeover
    /// when `write_active` is true and `request_native_resume` is requested.
    #[allow(clippy::too_many_arguments)]
    pub fn bind_online_provider_session(
        &mut self,
        provider_session_id: &str,
        conversation_id: &str,
        target_node_id: &str,
        target_harness: &str,
        target_project_path: &str,
        request_native_resume: bool,
        now: i64,
    ) -> Result<OnlineContinuationBinding> {
        if provider_session_id.trim().is_empty() || conversation_id.trim().is_empty() {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let session = query_online_session(&tx, provider_session_id)?;
        if session.write_active && request_native_resume {
            return Err(Error::InvalidTransition);
        }
        let same_scope = session.provider_node_id == target_node_id.trim()
            && session.harness == target_harness.trim()
            && session.project_path == target_project_path.trim();
        let (continuation_mode, native_session_resume, status_reason) =
            if request_native_resume && session.resumable && same_scope && !session.write_active {
                (
                    "native_resume_verified".to_owned(),
                    true,
                    format!(
                        "已验证同节点 ({})、同工作区与同 Harness ({})，支持原生会话续接。",
                        session.provider_label, session.harness
                    ),
                )
            } else {
                let reason = if session.write_active {
                    format!(
                        "原节点 ({}) 会话仍在活动中，已降级为受控历史摘要续接以避免双写冲突。",
                        session.provider_label
                    )
                } else if !same_scope {
                    format!(
                        "目标执行环境与原节点 ({}/{}) 不同，已显式降级为有界历史上下文续接（非原生会话恢复）。",
                        session.provider_label, session.harness
                    )
                } else {
                    format!(
                        "已从在线节点 ({}) 导入有界历史摘要，后续发送将按受控上下文续接执行。",
                        session.provider_label
                    )
                };
                ("bounded_history_context".to_owned(), false, reason)
            };

        tx.execute(
            "INSERT OR IGNORE INTO conversations(id,project_id,title,acceptance,version,created_at) VALUES(?1,NULL,?2,'local',1,?3)",
            params![conversation_id, session.title, now],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO conversation_view_states(origin_device_id,conversation_id,draft,draft_revision,scroll_anchor_message_id,scroll_offset,last_read_message_id,updated_at) VALUES(?1,?2,'',0,NULL,0,NULL,?3)",
            params![self.origin, conversation_id, now],
        )?;

        if !session.summary_snippet.is_empty() {
            let seed_msg_id = format!(
                "online-seed:{}",
                digest(format!("{provider_session_id}:{conversation_id}").as_bytes())
            );
            let content = json!({
                "text": session.summary_snippet,
                "source": "online_provider_summary",
                "provider_node_id": session.provider_node_id,
                "harness": session.harness,
                "native_session_id": session.native_session_id,
                "continuation_mode": continuation_mode,
                "native_session_resume": native_session_resume,
            })
            .to_string();
            tx.execute(
                "INSERT OR IGNORE INTO messages(id,conversation_id,origin_device_id,request_id,role,content_json,acceptance,created_at) VALUES(?1,?2,?3,NULL,'assistant',?4,'local',?5)",
                params![seed_msg_id, conversation_id, self.origin, content, now],
            )?;
        }

        tx.execute(
            "UPDATE online_provider_sessions SET bound_conversation_id=?2,continuation_mode=?3,updated_at=?4 WHERE id=?1",
            params![provider_session_id, conversation_id, continuation_mode, now],
        )?;
        tx.commit()?;
        Ok(OnlineContinuationBinding {
            provider_session_id: provider_session_id.to_owned(),
            conversation_id: conversation_id.to_owned(),
            continuation_mode,
            native_session_resume,
            status_reason,
        })
    }
}

fn query_online_session(
    db: &rusqlite::Connection,
    id: &str,
) -> Result<OnlineProviderSession> {
    db.query_row(
        "SELECT id, provider_node_id, provider_label, harness, native_session_id, project_path, title, summary_snippet, turn_count, write_active, resumable, bound_conversation_id, continuation_mode, updated_at
         FROM online_provider_sessions
         WHERE id=?1",
        [id],
        |r| {
            Ok(OnlineProviderSession {
                id: r.get(0)?,
                provider_node_id: r.get(1)?,
                provider_label: r.get(2)?,
                harness: r.get(3)?,
                native_session_id: r.get(4)?,
                project_path: r.get(5)?,
                title: r.get(6)?,
                summary_snippet: r.get(7)?,
                turn_count: r.get(8)?,
                write_active: r.get::<_, i64>(9)? != 0,
                resumable: r.get::<_, i64>(10)? != 0,
                bound_conversation_id: r.get(11)?,
                continuation_mode: r.get(12)?,
                updated_at: r.get(13)?,
            })
        },
    )
    .optional()?
    .ok_or(Error::InvalidTransition)
}
