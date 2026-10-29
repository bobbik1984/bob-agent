//! Trusted local administration and authenticated-transport authorization seam.
//! This module does not perform a handshake, pair devices, or authorize execution.
use crate::outbox::{AppendMessage, Error, Result, Store};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

/// Only produced from a persisted grant and an identity observed by the trusted
/// transport. Never Deserialize this context or populate identity from a payload.
#[derive(Clone)]
pub struct HomeMessageContext {
    pub(crate) local_home_device_id: String,
    pub(crate) authenticated_origin_device_id: String,
    pub(crate) authorized_conversation_id: String,
    identity_sha256: String,
    epoch: i64,
    revision: i64,
}
impl HomeMessageContext {
    pub fn origin_device_id(&self) -> &str {
        &self.authenticated_origin_device_id
    }
    pub fn authorization_revision(&self) -> i64 {
        self.revision
    }
}
fn fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn home(tx: &Transaction<'_>, id: &str, epoch: i64) -> Result<()> {
    let valid: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM home_authority WHERE device_id=?1 AND epoch=?2)",
        params![id, epoch],
        |r| r.get(0),
    )?;
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidTransition)
    }
}
pub(crate) fn authorize(
    tx: &Transaction<'_>,
    ctx: &HomeMessageContext,
    request: &AppendMessage,
    now: i64,
    cancel: bool,
) -> Result<()> {
    if ctx.local_home_device_id != request.home_device_id
        || ctx.authenticated_origin_device_id != request.origin_device_id
        || ctx.authorized_conversation_id != request.session_id
        || ctx.epoch != request.home_epoch
        || [
            &request.request_id,
            &request.session_id,
            &request.message_id,
            &request.origin_device_id,
            &request.home_device_id,
        ]
        .iter()
        .any(|v| v.trim().is_empty())
    {
        return Err(Error::InvalidTransition);
    }
    home(tx, &ctx.local_home_device_id, ctx.epoch)?;
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM home_device_authorizations a JOIN devices d ON d.id=a.device_id JOIN home_conversation_grants g ON g.device_id=a.device_id WHERE a.device_id=?1 AND a.home_device_id=?2 AND a.home_epoch=?3 AND a.identity_sha256=?4 AND a.revision=?5 AND a.revoked=0 AND d.trust_state='paired' AND length(d.public_key_ref)>0 AND d.public_key_ref=a.paired_key_ref AND g.conversation_id=?6 AND g.expires_at>?7 AND (?8=0 OR g.allow_cancel=1) AND NOT EXISTS(SELECT 1 FROM conversations c WHERE c.id=g.conversation_id AND c.archived_at IS NOT NULL))",params![ctx.authenticated_origin_device_id,ctx.local_home_device_id,ctx.epoch,ctx.identity_sha256,ctx.revision,ctx.authorized_conversation_id,now,cancel],|r|r.get(0))?;
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidTransition)
    }
}
impl Store {
    /// Trusted local administration ONLY, never expose as a remote RPC. Pairing
    /// must already exist. expected_revision=0 creates, subsequent changes CAS.
    /// A revision change invalidates all open sessions for this device. Other
    /// exact-conversation grants remain; re-authentication observes the new state.
    #[allow(clippy::too_many_arguments)]
    pub fn grant_home_conversation(
        &mut self,
        home_id: &str,
        epoch: i64,
        device: &str,
        identity_sha256: &str,
        conversation: &str,
        expected_revision: i64,
        expires_at: i64,
        allow_cancel: bool,
        now: i64,
    ) -> Result<i64> {
        if !fingerprint(identity_sha256)
            || conversation.trim().is_empty()
            || expires_at <= now
            || expected_revision < 0
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        home(&tx, home_id, epoch)?;
        let paired:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM devices WHERE id=?1 AND trust_state='paired' AND length(public_key_ref)>0)",[device],|r|r.get(0))?;
        if !paired {
            return Err(Error::InvalidTransition);
        }
        let old:Option<(i64,String,i64,String)>=tx.query_row("SELECT revision,identity_sha256,home_epoch,home_device_id FROM home_device_authorizations WHERE device_id=?1",[device],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if old.as_ref().map(|v| v.0).unwrap_or(0) != expected_revision
            || old.as_ref().is_some_and(|v| v.1 != identity_sha256)
        {
            return Err(Error::IdentityConflict);
        }
        let revision = expected_revision
            .checked_add(1)
            .ok_or(Error::InvalidTransition)?;
        tx.execute("INSERT INTO home_device_authorizations VALUES(?1,?2,?3,(SELECT public_key_ref FROM devices WHERE id=?1),?4,?5,0,?6) ON CONFLICT(device_id) DO UPDATE SET home_device_id=excluded.home_device_id,home_epoch=excluded.home_epoch,paired_key_ref=excluded.paired_key_ref,revision=excluded.revision,revoked=0,updated_at=excluded.updated_at",params![device,home_id,epoch,identity_sha256,revision,now])?;
        // A changed authority epoch must not revive grants from the old Home.
        if old.as_ref().is_some_and(|v| v.2 != epoch || v.3 != home_id) {
            tx.execute(
                "DELETE FROM home_conversation_grants WHERE device_id=?1",
                [device],
            )?;
        }
        tx.execute("INSERT INTO home_conversation_grants VALUES(?1,?2,?3,?4) ON CONFLICT(device_id,conversation_id) DO UPDATE SET expires_at=excluded.expires_at,allow_cancel=excluded.allow_cancel",params![device,conversation,expires_at,allow_cancel])?;
        tx.commit()?;
        Ok(revision)
    }
    /// Revokes all conversation permissions; does not stop already running tools.
    pub fn revoke_home_device(
        &mut self,
        home_id: &str,
        epoch: i64,
        device: &str,
        expected_revision: i64,
        now: i64,
    ) -> Result<i64> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        home(&tx, home_id, epoch)?;
        let revision = expected_revision
            .checked_add(1)
            .ok_or(Error::InvalidTransition)?;
        if tx.execute("UPDATE home_device_authorizations SET revoked=1,revision=?4,updated_at=?5 WHERE device_id=?1 AND home_device_id=?2 AND home_epoch=?3 AND revision=?6",params![device,home_id,epoch,revision,now,expected_revision])?!=1 {return Err(Error::IdentityConflict)}
        tx.execute(
            "DELETE FROM home_conversation_grants WHERE device_id=?1",
            [device],
        )?;
        tx.commit()?;
        Ok(revision)
    }
    /// observed_identity_sha256 MUST come from authenticated TLS/transport peer,
    /// never an HTTP/JSON field. The kernel cannot prove caller transport honesty.
    /// This SHA256 pin may be the chosen transport's certificate fingerprint.
    pub fn authenticate_home_session(
        &mut self,
        home_id: &str,
        epoch: i64,
        observed_identity_sha256: &str,
        conversation: &str,
        now: i64,
    ) -> Result<HomeMessageContext> {
        if !fingerprint(observed_identity_sha256) {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let found:Option<(String,i64)>=tx.query_row("SELECT device_id,revision FROM home_device_authorizations WHERE identity_sha256=?1",[observed_identity_sha256],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (device, revision) = found.ok_or(Error::InvalidTransition)?;
        let ctx = HomeMessageContext {
            local_home_device_id: home_id.into(),
            authenticated_origin_device_id: device,
            authorized_conversation_id: conversation.into(),
            identity_sha256: observed_identity_sha256.into(),
            epoch,
            revision,
        };
        let binding = AppendMessage {
            request_id: "authorization-check".into(),
            session_id: conversation.into(),
            message_id: "authorization-check".into(),
            origin_device_id: ctx.authenticated_origin_device_id.clone(),
            home_device_id: home_id.into(),
            home_epoch: epoch,
            content: serde_json::Value::Null,
        };
        authorize(&tx, &ctx, &binding, now, false)?;
        tx.commit()?;
        Ok(ctx)
    }
}
