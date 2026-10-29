use bob_kernel::{
    dispatch::*,
    outbox::{AppendMessage, Store},
    prewarm::WatchKey,
    wake::*,
};
use serde_json::json;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn block_on<F: Future>(f: F) -> F::Output {
    let w = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(x) => return x,
            Poll::Pending => std::thread::park_timeout(Duration::from_secs(2)),
        }
    }
}
struct Adapter;
impl WakeAdapter for Adapter {
    fn connect(&self, _: &WatchKey, g: u64, _: Cancellation) -> Result<String, WakeError> {
        Ok(format!("watch-{g}"))
    }
    fn close(&self, _: &WatchKey, _: &str) {}
}
fn key() -> WatchKey {
    WatchKey {
        device: "home".into(),
        service_instance: "fixture".into(),
        auth_generation: 1,
        session: "s".into(),
    }
}
fn wake() -> WakeCoordinator {
    WakeCoordinator::new(
        Duration::from_secs(1),
        Duration::from_millis(15),
        Duration::from_secs(5),
        2,
    )
}
fn policy() -> DispatchPolicy {
    DispatchPolicy {
        retry_ms: 10,
        lease_ms: 100,
        max_attempts: 3,
    }
}
fn request(id: &str) -> AppendMessage {
    AppendMessage {
        request_id: id.into(),
        session_id: "s".into(),
        message_id: format!("m-{id}"),
        origin_device_id: "replica".into(),
        home_device_id: "home".into(),
        home_epoch: 1,
        content: json!({"text":id}),
    }
}
fn fixture(p: &std::path::Path) -> Store {
    let mut s = Store::open(p, 0).unwrap();
    s.initialize_local_identity("replica", "home", 1, 0)
        .unwrap();
    s
}
struct HomeTransport {
    store: Mutex<Store>,
    sent: AtomicUsize,
    looked: AtomicUsize,
}
impl Transport for HomeTransport {
    fn send<'a>(
        &'a self,
        job: &'a DispatchJob,
        _: &'a ReadyWatch,
    ) -> Pin<Box<dyn Future<Output = TransportResult> + Send + 'a>> {
        Box::pin(async move {
            self.sent.fetch_add(1, Ordering::SeqCst);
            let mut s = self.store.lock().unwrap();
            let ctx = s
                .authenticate_home_session("home", 1, &"a".repeat(64), "s", 5)
                .unwrap();
            if let WireIntent::Message(m) = &job.intent {
                s.receive_message(&ctx, m, 5).unwrap();
            } else {
                panic!("wrong fixture intent")
            };
            TransportResult::Uncertain // Home committed; simulate lost ACK.
        })
    }
    fn lookup<'a>(
        &'a self,
        job: &'a DispatchJob,
        _: &'a ReadyWatch,
    ) -> Pin<Box<dyn Future<Output = TransportResult> + Send + 'a>> {
        Box::pin(async move {
            self.looked.fetch_add(1, Ordering::SeqCst);
            let mut s = self.store.lock().unwrap();
            let ctx = s
                .authenticate_home_session("home", 1, &"a".repeat(64), "s", 5)
                .unwrap();
            let WireIntent::Message(m) = &job.intent else {
                panic!("wrong fixture intent")
            };
            TransportResult::Receipt(s.lookup_message_receipt(&ctx, m, 5).unwrap().unwrap())
        })
    }
}
#[test]
fn real_sqlite_home_commit_lost_ack_restart_lookup_no_resend() {
    let d = tempfile::tempdir().unwrap();
    let rp = d.path().join("r.sqlite");
    let hp = d.path().join("h.sqlite");
    let mut replica = fixture(&rp);
    let mut home = fixture(&hp);
    rusqlite::Connection::open(&hp)
        .unwrap()
        .execute(
            "UPDATE devices SET trust_state='paired',public_key_ref='fake-key' WHERE id='replica'",
            [],
        )
        .unwrap();
    home.grant_home_conversation("home", 1, "replica", &"a".repeat(64), "s", 0, 100, true, 1)
        .unwrap();
    let transport = HomeTransport {
        store: Mutex::new(home),
        sent: AtomicUsize::new(0),
        looked: AtomicUsize::new(0),
    };
    let wake = wake();
    replica.accept_message(&request("r1"), 1).unwrap();
    let job = replica.claim_next(2, policy()).unwrap().unwrap();
    let result = block_on(job.run(
        &wake,
        key(),
        Arc::new(Adapter),
        &transport,
        Duration::from_secs(1),
    ));
    replica.complete_job(&job, result, 3, policy()).unwrap();
    assert_eq!(replica.state("r1").unwrap(), "unknown_send");
    drop(replica);
    let mut replica = Store::open(&rp, 4).unwrap();
    let job = replica.claim_next(13, policy()).unwrap().unwrap();
    assert_eq!(job.kind, JobKind::Lookup);
    let result = block_on(job.run(
        &wake,
        key(),
        Arc::new(Adapter),
        &transport,
        Duration::from_secs(1),
    ));
    replica.complete_job(&job, result, 14, policy()).unwrap();
    assert_eq!(replica.state("r1").unwrap(), "peer_accepted");
    assert_eq!(transport.sent.load(Ordering::SeqCst), 1);
    assert_eq!(transport.looked.load(Ordering::SeqCst), 1);
    let count: i64 = rusqlite::Connection::open(&hp)
        .unwrap()
        .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    wake.cancel(&key());
}
#[test]
fn confirmed_zero_retries_with_backoff_and_budget() {
    let d = tempfile::tempdir().unwrap();
    let mut s = fixture(&d.path().join("r.sqlite"));
    s.accept_message(&request("r1"), 1).unwrap();
    for now in [2, 12, 22] {
        let job = s.claim_next(now, policy()).unwrap().unwrap();
        assert_eq!(job.kind, JobKind::Send);
        s.complete_job(&job, TransportResult::ConfirmedZero, now, policy())
            .unwrap();
        assert!(s.claim_next(now + 1, policy()).unwrap().is_none());
    }
    assert!(s.claim_next(32, policy()).unwrap().is_none());
    assert_eq!(s.state("r1").unwrap(), "needs_attention");
}
#[test]
fn unknown_normal_does_not_block_cancel_and_absence_never_replays() {
    let d = tempfile::tempdir().unwrap();
    let mut s = fixture(&d.path().join("r.sqlite"));
    s.accept_message(&request("r1"), 1).unwrap();
    let j = s.claim_next(2, policy()).unwrap().unwrap();
    s.complete_job(&j, TransportResult::Uncertain, 2, policy())
        .unwrap();
    s.accept_message(&request("r2"), 3).unwrap();
    s.queue_cancel(
        &CancelExecution {
            request_id: "cancel".into(),
            session_id: "s".into(),
            origin_device_id: "replica".into(),
            home_device_id: "home".into(),
            home_epoch: 1,
            execution_id: "exec".into(),
            expected_version: 1,
            permission_ref: "approved-operation".into(),
        },
        4,
    )
    .unwrap();
    let cancel = s.claim_next(5, policy()).unwrap().unwrap();
    assert_eq!(cancel.intent.request_id(), "cancel");
    s.complete_job(&cancel, TransportResult::ConfirmedZero, 5, policy())
        .unwrap();
    let q = s.claim_next(12, policy()).unwrap().unwrap();
    assert_eq!(q.kind, JobKind::Lookup);
    s.complete_job(&q, TransportResult::Absent, 12, policy())
        .unwrap();
    assert_eq!(s.state("r1").unwrap(), "unknown_send");
    assert_eq!(s.state("r2").unwrap(), "local_saved");
}
#[test]
fn abandoned_claim_is_queried_and_late_callback_rejected() {
    let d = tempfile::tempdir().unwrap();
    let mut s = fixture(&d.path().join("r.sqlite"));
    s.accept_message(&request("r1"), 1).unwrap();
    let old = s.claim_next(2, policy()).unwrap().unwrap();
    assert!(s.claim_next(3, policy()).unwrap().is_none());
    let lookup = s.claim_next(102, policy()).unwrap().unwrap();
    assert_eq!(lookup.kind, JobKind::Lookup);
    assert!(s
        .complete_job(&old, TransportResult::ConfirmedZero, 103, policy())
        .is_err());
}
struct Hanging;
impl Transport for Hanging {
    fn send<'a>(
        &'a self,
        _: &'a DispatchJob,
        _: &'a ReadyWatch,
    ) -> Pin<Box<dyn Future<Output = TransportResult> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
    fn lookup<'a>(
        &'a self,
        j: &'a DispatchJob,
        w: &'a ReadyWatch,
    ) -> Pin<Box<dyn Future<Output = TransportResult> + Send + 'a>> {
        self.send(j, w)
    }
}
#[test]
fn transport_timeout_is_unknown_not_zero() {
    let d = tempfile::tempdir().unwrap();
    let mut s = fixture(&d.path().join("r.sqlite"));
    s.accept_message(&request("r1"), 1).unwrap();
    let job = s.claim_next(2, policy()).unwrap().unwrap();
    let w = wake();
    let result = block_on(job.run(
        &w,
        key(),
        Arc::new(Adapter),
        &Hanging,
        Duration::from_millis(20),
    ));
    assert!(matches!(result, TransportResult::Uncertain));
    s.complete_job(&job, result, 3, policy()).unwrap();
    assert_eq!(s.state("r1").unwrap(), "unknown_send");
    w.cancel(&key());
}

#[test]
fn same_target_lane_has_one_active_job_but_control_is_independent() {
    let d = tempfile::tempdir().unwrap();
    let mut s = fixture(&d.path().join("r.sqlite"));
    s.accept_message(&request("r1"), 1).unwrap();
    let mut second = request("r2");
    second.session_id = "other".into();
    s.accept_message(&second, 2).unwrap();
    let _normal = s.claim_next(3, policy()).unwrap().unwrap();
    assert!(s.claim_next(4, policy()).unwrap().is_none());
    for id in ["cancel1", "cancel2"] {
        s.queue_cancel(
            &CancelExecution {
                request_id: id.into(),
                session_id: "s".into(),
                origin_device_id: "replica".into(),
                home_device_id: "home".into(),
                home_epoch: 1,
                execution_id: "exec".into(),
                expected_version: 1,
                permission_ref: "scope".into(),
            },
            5,
        )
        .unwrap();
    }
    let control = s.claim_next(6, policy()).unwrap().unwrap();
    assert_eq!(control.intent.request_id(), "cancel1");
    assert!(s.claim_next(7, policy()).unwrap().is_none());
}
#[test]
fn invalid_configuration_is_not_unknown_send() {
    let d = tempfile::tempdir().unwrap();
    let mut s = fixture(&d.path().join("r.sqlite"));
    s.accept_message(&request("r1"), 1).unwrap();
    let job = s.claim_next(2, policy()).unwrap().unwrap();
    let w = wake();
    let mut bad = key();
    bad.device = "wrong".into();
    let result = block_on(job.run(&w, bad, Arc::new(Adapter), &Hanging, Duration::from_secs(1)));
    assert!(matches!(result, TransportResult::InvalidConfiguration));
    s.complete_job(&job, result, 3, policy()).unwrap();
    assert_eq!(s.state("r1").unwrap(), "needs_attention");
    let next = request("r2");
    s.accept_message(&next, 4).unwrap();
    let job = s.claim_next(5, policy()).unwrap().unwrap();
    let result = block_on(job.run(&w, key(), Arc::new(Adapter), &Hanging, Duration::ZERO));
    assert!(matches!(result, TransportResult::InvalidConfiguration));
}
