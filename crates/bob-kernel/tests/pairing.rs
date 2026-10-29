use bob_kernel::{
    outbox::{Error, Store},
    pairing::{PairingInvitation, ProvenPairingPeer},
};
use rusqlite::Connection;
const PIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HOME: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
fn invitation() -> PairingInvitation {
    PairingInvitation {
        id: "invite".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        home_identity_sha256: HOME.into(),
        expires_at: 1000,
    }
}
fn peer() -> ProvenPairingPeer {
    ProvenPairingPeer {
        device_id: "phone".into(),
        identity_sha256: PIN.into(),
        public_key_ref: "vault:phone-public".into(),
    }
}
fn fixture(p: &std::path::Path) -> Store {
    let mut s = Store::open(p, 0).unwrap();
    s.initialize_local_identity("home", "home", 1, 0).unwrap();
    s.issue_pairing_invitation(&invitation(), &[7; 32], 1)
        .unwrap();
    s
}
#[test]
fn confirmation_is_atomic_durable_and_message_scope_only() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("db");
    let mut s = fixture(&p);
    s.propose_pairing_peer("invite", &[7; 32], &peer(), 2)
        .unwrap();
    assert!(s
        .authenticate_home_session("home", 1, PIN, "session", 3)
        .is_err());
    assert_eq!(
        s.confirm_pairing_peer("invite", "phone", PIN, "session", 2000, true, 3)
            .unwrap(),
        1
    );
    assert!(s
        .authenticate_home_session("home", 1, PIN, "session", 4)
        .is_ok());
    assert!(s
        .authenticate_home_session("home", 1, PIN, "different", 4)
        .is_err());
    drop(s);
    let mut s = Store::open(&p, 5).unwrap();
    assert!(s
        .authenticate_home_session("home", 1, PIN, "session", 6)
        .is_ok());
    assert!(s
        .confirm_pairing_peer("invite", "phone", PIN, "session", 2000, true, 6)
        .is_err());
    assert!(s
        .propose_pairing_peer("invite", &[7; 32], &peer(), 6)
        .is_err());
    let c = Connection::open(&p).unwrap();
    assert_eq!(
        c.query_row("SELECT count(*) FROM executions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row(
            "SELECT challenge_sha256 FROM home_pairing_invitations",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
        .len(),
        64
    );
}
#[test]
fn wrong_challenge_and_wrong_confirmation_identity_are_rejected_without_consuming() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = fixture(&dir.path().join("db"));
    assert!(s
        .propose_pairing_peer("invite", &[8; 32], &peer(), 2)
        .is_err());
    s.propose_pairing_peer("invite", &[7; 32], &peer(), 2)
        .unwrap();
    assert!(s
        .confirm_pairing_peer("invite", "other", PIN, "session", 2000, false, 3)
        .is_err());
    assert!(s
        .confirm_pairing_peer("invite", "phone", HOME, "session", 2000, false, 3)
        .is_err());
    s.confirm_pairing_peer("invite", "phone", PIN, "session", 2000, false, 3)
        .unwrap();
}
#[test]
fn expired_cancelled_and_clock_rollback_invitations_cannot_pair() {
    for mode in [
        "expired-proposal",
        "expired-confirm",
        "cancelled",
        "clock-rollback",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = fixture(&dir.path().join("db"));
        if mode == "expired-proposal" {
            assert!(s
                .propose_pairing_peer("invite", &[7; 32], &peer(), 1000)
                .is_err());
            continue;
        }
        if mode == "clock-rollback" {
            assert!(s
                .propose_pairing_peer("invite", &[7; 32], &peer(), 0)
                .is_err());
            continue;
        }
        s.propose_pairing_peer("invite", &[7; 32], &peer(), 2)
            .unwrap();
        if mode == "cancelled" {
            s.cancel_pairing_invitation("invite").unwrap();
        }
        let time = if mode == "expired-confirm" { 1000 } else { 3 };
        assert!(s
            .confirm_pairing_peer("invite", "phone", PIN, "session", 2000, false, time)
            .is_err());
    }
}
#[test]
fn authority_change_and_revoked_device_never_revive_implicitly() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("db");
    let mut s = fixture(&p);
    s.propose_pairing_peer("invite", &[7; 32], &peer(), 2)
        .unwrap();
    let c = Connection::open(&p).unwrap();
    c.execute("UPDATE home_authority SET epoch=2", []).unwrap();
    assert!(s
        .confirm_pairing_peer("invite", "phone", PIN, "session", 2000, false, 3)
        .is_err());
    c.execute("UPDATE home_authority SET epoch=1", []).unwrap();
    c.execute(
        "INSERT INTO devices VALUES('phone','phone',NULL,'revoked',0)",
        [],
    )
    .unwrap();
    assert!(s
        .confirm_pairing_peer("invite", "phone", PIN, "session", 2000, false, 3)
        .is_err());
    assert_eq!(
        c.query_row("SELECT state FROM home_pairing_invitations", [], |r| r
            .get::<_, String>(
            0
        ))
        .unwrap(),
        "pending"
    );
}
#[test]
fn failed_grant_rolls_back_device_and_invitation_and_schema_tampering_stops_open() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("db");
    let mut s = fixture(&p);
    s.propose_pairing_peer("invite", &[7; 32], &peer(), 2)
        .unwrap();
    let c = Connection::open(&p).unwrap();
    c.execute_batch("CREATE TRIGGER deny_grant BEFORE INSERT ON home_conversation_grants BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(s
        .confirm_pairing_peer("invite", "phone", PIN, "session", 2000, false, 3)
        .is_err());
    assert_eq!(
        c.query_row("SELECT count(*) FROM devices WHERE id='phone'", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT state FROM home_pairing_invitations", [], |r| r
            .get::<_, String>(
            0
        ))
        .unwrap(),
        "pending"
    );
    c.execute_batch("DROP TRIGGER deny_grant;").unwrap();
    s.confirm_pairing_peer("invite", "phone", PIN, "session", 2000, false, 3)
        .unwrap();
    drop(s);
    c.execute(
        "UPDATE schema_migrations SET checksum='bad' WHERE version=4",
        [],
    )
    .unwrap();
    assert!(matches!(Store::open(&p, 4), Err(Error::SchemaMismatch)));
}
#[test]
fn invitation_requires_bounded_lifetime_and_distinct_remote_device() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = fixture(&dir.path().join("db"));
    let mut long = invitation();
    long.id = "long".into();
    long.expires_at = i64::MAX;
    assert!(s.issue_pairing_invitation(&long, &[7; 32], 1).is_err());
    let mut p = peer();
    p.device_id = "home".into();
    assert!(s.propose_pairing_peer("invite", &[7; 32], &p, 2).is_err());
    assert!(s
        .issue_pairing_invitation(&invitation(), &[7; 31], 1)
        .is_err());
}

#[test]
fn pending_candidate_is_readable_after_restart_without_challenge_and_expiry_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("db");
    let mut s = fixture(&p);
    s.propose_pairing_peer("invite", &[7; 32], &peer(), 2)
        .unwrap();
    drop(s);
    let s = Store::open(&p, 3).unwrap();
    let status = s.pairing_status("invite", 3).unwrap();
    assert_eq!(status.state, "pending");
    assert_eq!(status.device_id.as_deref(), Some("phone"));
    assert_eq!(status.identity_sha256.as_deref(), Some(PIN));
    assert!(!serde_json::to_string(&status)
        .unwrap()
        .contains("challenge"));
    assert_eq!(s.pairing_status("invite", 1000).unwrap().state, "expired");
}
