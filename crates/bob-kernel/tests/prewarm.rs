use bob_kernel::prewarm::*;
fn key() -> WatchKey {
    WatchKey {
        device: "node".into(),
        service_instance: "service".into(),
        auth_generation: 1,
        session: "session".into(),
    }
}
#[test]
fn typing_and_send_join_one_generation_and_stale_snapshot_cannot_win() {
    let mut warm = Prewarm::new(15_000, 30_000);
    assert_eq!(warm.request(key(), Trigger::Typing, 0), Decision::Start(1));
    assert_eq!(warm.request(key(), Trigger::Send, 1), Decision::Join(1));
    assert!(warm.failed(&key(), 1));
    assert_eq!(warm.request(key(), Trigger::Typing, 2), Decision::Cooldown);
    assert_eq!(warm.request(key(), Trigger::Send, 3), Decision::Start(2));
    assert!(!warm.initial_snapshot(&key(), 1));
    assert!(warm.initial_snapshot(&key(), 2));
    assert_eq!(warm.request(key(), Trigger::Send, 4), Decision::Ready(2));
    assert_eq!(warm.expire_idle(30_004), vec![(key(), 2)]);
    assert!(!warm.initial_snapshot(&key(), 2));
}
#[test]
fn sessions_and_auth_generations_do_not_share_watch() {
    let mut warm = Prewarm::new(15_000, 30_000);
    warm.request(key(), Trigger::Typing, 0);
    let mut other = key();
    other.session = "other".into();
    assert_eq!(warm.request(other, Trigger::Send, 1), Decision::Start(2));
    let mut renewed = key();
    renewed.auth_generation = 2;
    assert_eq!(warm.request(renewed, Trigger::Send, 1), Decision::Start(3));
}
