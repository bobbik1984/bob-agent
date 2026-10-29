//! Durable first-pairing state. Cryptographic proof belongs to the transport;
//! only trusted local Home UI may issue, confirm or cancel invitations.
use crate::outbox::{digest, Error, Result, Store};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;

pub const MAX_INVITATION_LIFETIME_MS: i64 = 10 * 60 * 1000;
#[derive(Clone, Debug, Serialize)]
pub struct PairingInvitation {
    pub id: String,
    pub home_device_id: String,
    pub home_epoch: i64,
    pub home_identity_sha256: String,
    pub expires_at: i64,
}
/// Identity must be obtained from a verified proof of possession (for example
/// TLS CertificateVerify), never trusted merely because it appears in JSON.
/// This is a trusted-host seam, not a remotely deserializable authorization.
pub struct ProvenPairingPeer {
    pub device_id: String,
    pub identity_sha256: String,
    pub public_key_ref: String,
}
#[derive(Debug, Serialize)]
pub struct PairingStatus {
    pub invitation: PairingInvitation,
    pub state: String,
    pub device_id: Option<String>,
    pub identity_sha256: Option<String>,
}
fn pin(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn id(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= 512
}
fn home(tx: &Transaction<'_>, h: &str, e: i64) -> Result<()> {
    let valid: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM home_authority WHERE device_id=?1 AND epoch=?2)",
        params![h, e],
        |r| r.get(0),
    )?;
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidTransition)
    }
}
pub(crate) fn migrate(tx: &Transaction<'_>, now: i64) -> Result<()> {
    let schema = include_str!("../migrations/0004.sql");
    let hash = digest(schema.replace("\r\n", "\n").as_bytes());
    let previous: Option<String> = tx
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=4",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match previous {
        Some(v) if v != hash => return Err(Error::SchemaMismatch),
        Some(_) => {}
        None => {
            tx.execute_batch(schema)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES(4,?1,?2)",
                params![hash, now],
            )?;
        }
    }
    Ok(())
}
impl Store {
    /// Trusted local UI lookup. Never expose candidate fingerprints to an
    /// unauthenticated network caller. Expiry/stale authority cannot look ready.
    pub fn pairing_status(&self, invitation_id: &str, now: i64) -> Result<PairingStatus> {
        let mut status: PairingStatus=self.db.query_row("SELECT id,home_device_id,home_epoch,home_identity_sha256,expires_at,state,device_id,identity_sha256 FROM home_pairing_invitations WHERE id=?1",[invitation_id],|r|Ok(PairingStatus{
            invitation:PairingInvitation{id:r.get(0)?,home_device_id:r.get(1)?,home_epoch:r.get(2)?,home_identity_sha256:r.get(3)?,expires_at:r.get(4)?},
            state:r.get(5)?,device_id:r.get(6)?,identity_sha256:r.get(7)?
        }))?;
        let current: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM home_authority WHERE device_id=?1 AND epoch=?2)",
            params![
                status.invitation.home_device_id,
                status.invitation.home_epoch
            ],
            |r| r.get(0),
        )?;
        if !current {
            status.state = "stale_authority".into();
        } else if matches!(status.state.as_str(), "open" | "pending")
            && status.invitation.expires_at <= now
        {
            status.state = "expired".into();
        }
        Ok(status)
    }
    /// IDs and challenges must be fresh cryptographic random values generated
    /// by the trusted host. Only the challenge digest is persisted here.
    pub fn issue_pairing_invitation(
        &mut self,
        invitation: &PairingInvitation,
        challenge: &[u8],
        now: i64,
    ) -> Result<()> {
        if !id(&invitation.id)
            || !pin(&invitation.home_identity_sha256)
            || challenge.len() != 32
            || now < 0
            || invitation.expires_at <= now
            || invitation.expires_at.saturating_sub(now) > MAX_INVITATION_LIFETIME_MS
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        home(&tx, &invitation.home_device_id, invitation.home_epoch)?;
        tx.execute("INSERT INTO home_pairing_invitations(id,home_device_id,home_epoch,home_identity_sha256,challenge_sha256,created_at,expires_at,state) VALUES(?1,?2,?3,?4,?5,?6,?7,'open')", params![invitation.id,invitation.home_device_id,invitation.home_epoch,invitation.home_identity_sha256,digest(challenge),now,invitation.expires_at])?;
        tx.commit()?;
        Ok(())
    }
    /// Invoked only after transport verifies the peer's private-key possession
    /// and binds this challenge to that authenticated connection. This stages
    /// a candidate; it grants no access until local user confirmation.
    pub fn propose_pairing_peer(
        &mut self,
        invitation_id: &str,
        challenge: &[u8],
        peer: &ProvenPairingPeer,
        now: i64,
    ) -> Result<()> {
        if challenge.len() != 32
            || !id(&peer.device_id)
            || !pin(&peer.identity_sha256)
            || !id(&peer.public_key_ref)
        {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row:Option<(String,i64,String)>=tx.query_row("SELECT home_device_id,home_epoch,challenge_sha256 FROM home_pairing_invitations WHERE id=?1 AND state='open' AND created_at<=?2 AND expires_at>?2",params![invitation_id,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (h, e, hash) = row.ok_or(Error::InvalidTransition)?;
        home(&tx, &h, e)?;
        if peer.device_id == h || hash != digest(challenge) {
            return Err(Error::IdentityConflict);
        }
        tx.execute("UPDATE home_pairing_invitations SET state='pending',device_id=?2,identity_sha256=?3,public_key_ref=?4 WHERE id=?1",params![invitation_id,peer.device_id,peer.identity_sha256,peer.public_key_ref])?;
        tx.commit()?;
        Ok(())
    }
    /// Trusted local user confirmation must include the exact displayed peer
    /// fingerprint. Pairing and its initial message-only grant commit together.
    /// This never grants permission to execute a model or arbitrary tools.
    #[allow(clippy::too_many_arguments)]
    pub fn confirm_pairing_peer(
        &mut self,
        invitation_id: &str,
        expected_device: &str,
        expected_pin: &str,
        conversation: &str,
        grant_expires_at: i64,
        allow_cancel: bool,
        now: i64,
    ) -> Result<i64> {
        self.confirm_pairing_peer_policy(invitation_id, expected_device, expected_pin, conversation, grant_expires_at, allow_cancel, now, false)
    }
    /// Trusted QR-host consent only: a fresh proof may recover the same revoked
    /// identity. Changing a key, Home or epoch still requires explicit recovery.
    #[allow(clippy::too_many_arguments)]
    pub fn confirm_scanned_pairing_peer(&mut self, invitation_id: &str, expected_device: &str, expected_pin: &str, conversation: &str, grant_expires_at: i64, allow_cancel: bool, now: i64) -> Result<i64> {
        self.confirm_pairing_peer_policy(invitation_id, expected_device, expected_pin, conversation, grant_expires_at, allow_cancel, now, true)
    }
    #[allow(clippy::too_many_arguments)]
    fn confirm_pairing_peer_policy(&mut self, invitation_id: &str, expected_device: &str, expected_pin: &str, conversation: &str, grant_expires_at: i64, allow_cancel: bool, now: i64, recover_same_revoked_key: bool) -> Result<i64> {
        if !id(conversation) || grant_expires_at <= now || !pin(expected_pin) {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row:Option<(String,i64,String,String,String)>=tx.query_row("SELECT home_device_id,home_epoch,device_id,identity_sha256,public_key_ref FROM home_pairing_invitations WHERE id=?1 AND state='pending' AND created_at<=?2 AND expires_at>?2",params![invitation_id,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let (h, e, d, p, k) = row.ok_or(Error::InvalidTransition)?;
        home(&tx, &h, e)?;
        if d != expected_device || p != expected_pin {
            return Err(Error::IdentityConflict);
        }
        // Existing paired/revoked identities require explicit recovery, not a
        // new invitation silently reviving access or replacing a key.
        let existing: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT trust_state,public_key_ref FROM devices WHERE id=?1",
                [&d],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let old_auth:Option<(i64,String,String,i64,String,i64)>=tx.query_row("SELECT revision,identity_sha256,home_device_id,home_epoch,paired_key_ref,revoked FROM home_device_authorizations WHERE device_id=?1",[&d],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        let recovering = recover_same_revoked_key && existing.as_ref().is_some_and(|(_,key)| key.as_deref()==Some(k.as_str())) && old_auth.as_ref().is_some_and(|a|a.1==p && a.2==h && a.3==e && a.4==k && a.5==1);
        if existing.as_ref().is_some_and(|(state,key)|state!="unpaired" || key.as_ref().is_some_and(|key|key!=&k)) && !recovering { return Err(Error::IdentityConflict); }
        if old_auth.is_some() && !recovering { return Err(Error::IdentityConflict); }
        let revision=if recovering {old_auth.as_ref().unwrap().0.checked_add(1).ok_or(Error::InvalidTransition)?} else {1};
        tx.execute("INSERT INTO devices(id,label,public_key_ref,trust_state,created_at) VALUES(?1,?1,?2,'paired',?3) ON CONFLICT(id) DO UPDATE SET public_key_ref=excluded.public_key_ref,trust_state='paired'",params![d,k,now])?;
        tx.execute(
            "INSERT INTO home_device_authorizations VALUES(?1,?2,?3,?4,?5,?6,0,?7) ON CONFLICT(device_id) DO UPDATE SET revision=excluded.revision,revoked=0,updated_at=excluded.updated_at",
            params![d, h, e, k, p, revision, now],
        )?;
        tx.execute(
            "INSERT INTO home_conversation_grants VALUES(?1,?2,?3,?4)",
            params![d, conversation, grant_expires_at, allow_cancel],
        )?;
        tx.execute(
            "UPDATE home_pairing_invitations SET state='consumed',confirmed_at=?2 WHERE id=?1",
            params![invitation_id, now],
        )?;
        tx.commit()?;
        Ok(revision)
    }
    pub fn cancel_pairing_invitation(&mut self, invitation_id: &str) -> Result<()> {
        if self.db.execute("UPDATE home_pairing_invitations SET state='cancelled' WHERE id=?1 AND state IN ('open','pending')",[invitation_id])?!=1 { return Err(Error::InvalidTransition); }
        Ok(())
    }
}

// Native callers share LocalApp's single Store owner; never open a second Store
// for listener or status operations (open also performs interrupted recovery).
impl crate::app::LocalApp {
    pub fn pairing_status(&self, invitation_id:&str, now:i64)->Result<PairingStatus> {
        let status=self.store.pairing_status(invitation_id,now)?;
        if status.invitation.home_device_id!=self.origin {return Err(Error::IdentityConflict);}
        Ok(status)
    }
    pub fn issue_pairing_invitation(&mut self, invitation:&PairingInvitation, challenge:&[u8], now:i64)->Result<()> {
        if invitation.home_device_id!=self.origin {return Err(Error::IdentityConflict);}
        self.store.issue_pairing_invitation(invitation,challenge,now)
    }
    pub fn propose_pairing_peer(&mut self, invitation_id:&str, challenge:&[u8], peer:&ProvenPairingPeer, now:i64)->Result<()> {
        self.pairing_status(invitation_id,now)?;
        self.store.propose_pairing_peer(invitation_id,challenge,peer,now)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn confirm_pairing_peer(&mut self, invitation_id:&str, expected_device:&str, expected_pin:&str, conversation:&str, grant_expires_at:i64, allow_cancel:bool, now:i64)->Result<i64> {
        self.pairing_status(invitation_id,now)?;
        self.store.confirm_pairing_peer(invitation_id,expected_device,expected_pin,conversation,grant_expires_at,allow_cancel,now)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn confirm_scanned_pairing_peer(&mut self, invitation_id:&str, expected_device:&str, expected_pin:&str, conversation:&str, grant_expires_at:i64, allow_cancel:bool, now:i64)->Result<i64> {
        self.pairing_status(invitation_id,now)?;
        self.store.confirm_scanned_pairing_peer(invitation_id,expected_device,expected_pin,conversation,grant_expires_at,allow_cancel,now)
    }
    pub fn cancel_pairing_invitation(&mut self, invitation_id:&str)->Result<()> {
        self.pairing_status(invitation_id,0)?;
        self.store.cancel_pairing_invitation(invitation_id)
    }
    pub fn authenticate_home_session(&mut self, home_id:&str, epoch:i64, pin:&str, session:&str, now:i64)->Result<crate::home_auth::HomeMessageContext> {
        self.authenticate_home_peer(home_id,epoch,pin,session,now)
    }
}
