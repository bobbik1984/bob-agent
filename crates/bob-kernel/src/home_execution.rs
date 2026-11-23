//! Explicit Home-approved read-only execution of a durably received message.
//! This is not a transport RPC. Approval objects must come from trusted local
//! user/policy code; remote payloads cannot deserialize or manufacture them.
use crate::{
    app::LocalApp,
    home_auth::{authorize, HomeMessageContext},
    local_execution::{create_execution, owned_executions, LocalExecution, RUNNER},
    outbox::{digest, AppendMessage, Error, Result},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::json;

pub struct ReadOnlyExecutionGrant {
    request_hash: String,
    workspace_ref: String,
    home_device_id: String,
    expires_at: i64,
    approved_at: i64,
}
impl LocalApp {
    /// Trusted local approval seam. Caller must have explicit read-only model
    /// execution authorization; receiving or pairing is insufficient.
    pub fn approve_remote_readonly(
        &self,
        request: &AppendMessage,
        workspace_ref: &str,
        expires_at: i64,
        now: i64,
    ) -> Result<ReadOnlyExecutionGrant> {
        if request.home_device_id != self.origin
            || request.origin_device_id == self.origin
            || workspace_ref.trim().is_empty()
            || workspace_ref.len() > 8192
            || now < 0
            || expires_at <= now
            || expires_at.saturating_sub(now) > 10 * 60 * 1000
        {
            return Err(Error::InvalidTransition);
        }
        Ok(ReadOnlyExecutionGrant {
            request_hash: digest(&serde_json::to_vec(request)?),
            workspace_ref: workspace_ref.into(),
            home_device_id: self.origin.clone(),
            expires_at,
            approved_at: now,
        })
    }
    /// Recheck immediately before releasing the native worker gate. Revocation,
    /// expiry and stale authority must stop GO, even after preparation succeeded.
    pub fn recheck_remote_readonly(
        &mut self,
        ctx: &HomeMessageContext,
        request: &AppendMessage,
        grant: &ReadOnlyExecutionGrant,
        now: i64,
    ) -> Result<()> {
        check_grant(&self.origin, request, &grant.workspace_ref, grant, now)?;
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        authorize(&tx, ctx, request, now, false)?;
        tx.commit()?;
        Ok(())
    }
    pub fn prepare_remote_readonly(
        &mut self,
        ctx: &HomeMessageContext,
        request: &AppendMessage,
        workspace_ref: &str,
        grant: &ReadOnlyExecutionGrant,
        now: i64,
    ) -> Result<LocalExecution> {
        check_grant(&self.origin, request, workspace_ref, grant, now)?;
        let tx = self
            .store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result =
            prepare_remote_in_tx(&self.origin, &tx, ctx, request, workspace_ref, grant, now)?;
        tx.commit()?;
        Ok(result)
    }
}
pub(crate) fn check_grant(
    home: &str,
    request: &AppendMessage,
    workspace_ref: &str,
    grant: &ReadOnlyExecutionGrant,
    now: i64,
) -> Result<()> {
    if grant.home_device_id != home
        || request.home_device_id != home
        || request.origin_device_id == home
        || grant.request_hash != digest(&serde_json::to_vec(request)?)
        || grant.workspace_ref != workspace_ref
        || now < grant.approved_at
        || now >= grant.expires_at
    {
        return Err(Error::InvalidTransition);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::{PairingInvitation, ProvenPairingPeer};
    const PIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    fn fixture(
        receive: bool,
    ) -> (
        tempfile::TempDir,
        LocalApp,
        AppendMessage,
        HomeMessageContext,
    ) {
        let d = tempfile::tempdir().unwrap();
        let mut a = LocalApp::open(&d.path().join("db")).unwrap();
        let h = a.origin.clone();
        a.store
            .issue_pairing_invitation(
                &PairingInvitation {
                    id: "invite".into(),
                    home_device_id: h.clone(),
                    home_epoch: 1,
                    home_identity_sha256: PIN.into(),
                    expires_at: 10000,
                },
                &[1; 32],
                1,
            )
            .unwrap();
        a.store
            .propose_pairing_peer(
                "invite",
                &[1; 32],
                &ProvenPairingPeer {
                    device_id: "phone".into(),
                    identity_sha256: PIN.into(),
                    public_key_ref: "phone-key".into(),
                },
                2,
            )
            .unwrap();
        a.store
            .confirm_pairing_peer("invite", "phone", PIN, "phone-session", 20000, true, 3)
            .unwrap();
        let ctx = a
            .store
            .authenticate_home_session(&h, 1, PIN, "phone-session", 4)
            .unwrap();
        let r = AppendMessage {
            request_id: "remote-request".into(),
            message_id: "phone-message".into(),
            session_id: "phone-session".into(),
            origin_device_id: "phone".into(),
            home_device_id: h,
            home_epoch: 1,
            content: json!({"text":"what is 2+3?"}),
        };
        if receive {
            a.store.receive_message(&ctx, &r, 5).unwrap();
        }
        (d, a, r, ctx)
    }
    #[test]
    fn remote_source_and_reply_authority_are_preserved_without_fake_outbox() {
        let (d, mut a, r, ctx) = fixture(true);
        let grant = a
            .approve_remote_readonly(&r, "ephemeral/remote", 1000, 6)
            .unwrap();
        let e = a
            .prepare_remote_readonly(&ctx, &r, "ephemeral/remote", &grant, 7)
            .unwrap();
        assert!(e.can_start);
        assert!(
            !a.prepare_remote_readonly(&ctx, &r, "ephemeral/remote", &grant, 8)
                .unwrap()
                .can_start
        );
        assert_eq!(
            a.store
                .db
                .query_row("SELECT count(*) FROM intent_outbox", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        a.record_local_owner(&e.id, e.owner_job.as_deref().unwrap(), 8)
            .unwrap();
        a.recheck_remote_readonly(&ctx, &r, &grant, 9).unwrap();
        a.mark_local_started(&e.id, 9).unwrap();
        a.finish_local_execution(
            &e.id,
            include_str!("../tests/fixtures/codex-reply-probe.jsonl"),
            Some(0),
            None,
            true,
            10,
        )
        .unwrap();
        let result = a.store.lookup_home_result(&ctx, &r, 11).unwrap();
        assert_eq!(result.execution.unwrap().status, "completed");
        let (origin,session,acceptance):(String,String,String)=a.store.db.query_row("SELECT origin_device_id,conversation_id,acceptance FROM messages WHERE role='assistant'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(origin, a.origin);
        assert_eq!(session, r.session_id);
        assert_eq!(acceptance, "home");
        drop(a);
        let a = LocalApp::open(&d.path().join("db")).unwrap();
        assert_eq!(a.managed_executions().unwrap().len(), 1);
        assert_eq!(a.managed_executions().unwrap()[0].status, "completed");
    }
    #[test]
    fn approval_is_exact_expiring_and_receipt_is_required() {
        let (_d, mut a, r, ctx) = fixture(false);
        let grant = a.approve_remote_readonly(&r, "temp", 100, 6).unwrap();
        assert!(a
            .prepare_remote_readonly(&ctx, &r, "temp", &grant, 7)
            .is_err());
        a.store.receive_message(&ctx, &r, 7).unwrap();
        assert!(a
            .prepare_remote_readonly(&ctx, &r, "other", &grant, 8)
            .is_err());
        let mut changed = r.clone();
        changed.content = json!({"text":"changed"});
        assert!(a
            .prepare_remote_readonly(&ctx, &changed, "temp", &grant, 8)
            .is_err());
        assert!(a
            .prepare_remote_readonly(&ctx, &r, "temp", &grant, 100)
            .is_err());
        assert_eq!(a.managed_executions().unwrap().len(), 0);
    }
    #[test]
    fn revoke_between_prepare_and_worker_release_blocks_start_authorization() {
        let (_d, mut a, r, ctx) = fixture(true);
        let grant = a.approve_remote_readonly(&r, "temp", 100, 6).unwrap();
        let e = a
            .prepare_remote_readonly(&ctx, &r, "temp", &grant, 7)
            .unwrap();
        a.store
            .revoke_home_device(&r.home_device_id, 1, "phone", 1, 8)
            .unwrap();
        assert!(a.recheck_remote_readonly(&ctx, &r, &grant, 9).is_err());
        a.finish_local_execution(&e.id, "", None, Some("spawn_failed"), true, 10)
            .unwrap();
        assert_eq!(a.managed_executions().unwrap()[0].status, "failed");
    }
    #[test]
    fn remote_restart_is_unknown_and_owned_cleanup_never_replays() {
        let (d, mut a, r, ctx) = fixture(true);
        let grant = a.approve_remote_readonly(&r, "temp", 100, 6).unwrap();
        let e = a
            .prepare_remote_readonly(&ctx, &r, "temp", &grant, 7)
            .unwrap();
        a.mark_local_started(&e.id, 8).unwrap();
        drop(a);
        let mut a = LocalApp::open(&d.path().join("db")).unwrap();
        assert_eq!(a.managed_executions().unwrap()[0].status, "unknown");
        assert!(
            !a.prepare_remote_readonly(&ctx, &r, "temp", &grant, 9)
                .unwrap()
                .can_start
        );
        a.reconcile_local_execution(&e.id, false, 10).unwrap();
        assert_eq!(a.managed_executions().unwrap()[0].status, "unknown");
        a.reconcile_local_execution(&e.id, true, 11).unwrap();
        assert_eq!(a.managed_executions().unwrap()[0].status, "failed");
    }
    #[test]
    fn remote_cancel_timeout_and_completion_recovery_use_shared_lifecycle() {
        for reason in ["cancelled", "timeout", "restore"] {
            let (d, mut a, r, ctx) = fixture(true);
            let grant = a.approve_remote_readonly(&r, "temp", 100, 6).unwrap();
            let e = a
                .prepare_remote_readonly(&ctx, &r, "temp", &grant, 7)
                .unwrap();
            a.mark_local_started(&e.id, 8).unwrap();
            if reason == "cancelled" {
                a.request_local_stop(&e.id, 9).unwrap();
            }
            if reason == "restore" {
                drop(a);
                a = LocalApp::open(&d.path().join("db")).unwrap();
                a.restore_local_completion(
                    &e.id,
                    include_str!("../tests/fixtures/codex-reply-probe.jsonl"),
                    Some(0),
                    None,
                    10,
                )
                .unwrap();
            } else {
                a.finish_local_execution(&e.id, "", None, Some(reason), true, 10)
                    .unwrap();
            }
            let status = if reason == "restore" {
                "completed"
            } else if reason == "cancelled" {
                "cancelled"
            } else {
                "failed"
            };
            assert_eq!(a.managed_executions().unwrap()[0].status, status);
        }
    }
    #[test]
    fn atomic_home_accept_prepare_rolls_back_and_duplicate_does_not_replay() {
        let (_dir, mut app, request, ctx) = fixture(false);
        let grant = app
            .approve_remote_readonly(&request, "empty", 9000, 10)
            .unwrap();
        assert!(app
            .host_accept_and_prepare(&ctx, &request, "wrong-workspace", &grant, 11)
            .is_err());
        assert!(app
            .store
            .lookup_message_receipt(&ctx, &request, 12)
            .unwrap()
            .is_none());
        let messages: i64 = app
            .store
            .db
            .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(messages, 0);
        let (receipt, execution) = app
            .host_accept_and_prepare(&ctx, &request, "empty", &grant, 13)
            .unwrap();
        assert!(receipt.accepted);
        assert!(execution.unwrap().can_start);
        let (receipt, execution) = app
            .host_accept_and_prepare(&ctx, &request, "empty", &grant, 14)
            .unwrap();
        assert!(receipt.accepted);
        assert!(execution.is_none());
        assert_eq!(app.managed_executions().unwrap().len(), 1);
    }
}

pub(crate) fn prepare_remote_in_tx(
    origin: &str,
    tx: &rusqlite::Transaction<'_>,
    ctx: &HomeMessageContext,
    request: &AppendMessage,
    workspace_ref: &str,
    grant: &ReadOnlyExecutionGrant,
    now: i64,
) -> Result<LocalExecution> {
    check_grant(origin, request, workspace_ref, grant, now)?;
    let hash = digest(&serde_json::to_vec(request)?);
    authorize(tx, ctx, request, now, false)?;
    let received:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM inbox_receipts WHERE request_id=?1 AND origin_device_id=?2 AND home_epoch=?3 AND payload_sha256=?4 AND outcome='accepted' AND effect_ref=?5)",params![request.request_id,request.origin_device_id,request.home_epoch,hash,request.message_id],|r|r.get(0))?;
    if !received {
        return Err(Error::InvalidTransition);
    }
    let exact_content = serde_json::to_string(&request.content)?;
    let source:Option<(String,String,Option<String>)>=tx.query_row("SELECT m.id,m.content_json,c.project_id FROM messages m JOIN conversations c ON c.id=m.conversation_id WHERE m.id=?1 AND m.request_id=?2 AND m.conversation_id=?3 AND m.origin_device_id=?4 AND m.content_json=?5 AND m.role='user' AND m.acceptance='home' AND c.acceptance='home' AND c.archived_at IS NULL",params![request.message_id,request.request_id,request.session_id,request.origin_device_id,exact_content],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let (message, content, project) = source.ok_or(Error::InvalidTransition)?;
    if let Some(prior) = owned_executions(tx, origin)?
        .into_iter()
        .find(|e| e.request_id == request.request_id)
    {
        return Ok(prior);
    }
    let blocked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM executions e JOIN runner_dispatches d ON d.execution_id=e.id WHERE d.runner_instance_id=?1 AND e.status IN ('pending','dispatching','running','paused','unknown'))",[RUNNER],|r|r.get(0))?;
    if blocked {
        return Err(Error::InvalidTransition);
    }
    let approval = json!({"request_sha256":grant.request_hash,"approved_at":grant.approved_at,"expires_at":grant.expires_at,"source":"trusted_home_readonly_policy","origin_device_id":request.origin_device_id,"session_id":request.session_id,"home_epoch":request.home_epoch});
    let result = create_execution(
        tx,
        &request.request_id,
        workspace_ref,
        origin,
        &request.session_id,
        message,
        content,
        project,
        Some(&approval),
        now,
    )?;
    Ok(result)
}
