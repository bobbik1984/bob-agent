use bob_kernel::{
    prewarm::{Trigger, WatchKey},
    wake::*,
};
use std::{
    future::Future,
    pin::pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    task::{Context, Poll, Wake, Waker},
    thread,
    time::{Duration, Instant},
};

struct Signal {
    flag: Mutex<bool>,
    changed: Condvar,
}
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref()
    }
    fn wake_by_ref(self: &Arc<Self>) {
        *self.flag.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
fn block_on<T>(future: impl Future<Output = T>) -> T {
    let signal = Arc::new(Signal {
        flag: Mutex::new(false),
        changed: Condvar::new(),
    });
    let waker = Waker::from(signal.clone());
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        let mut flag = signal.flag.lock().unwrap();
        while !*flag {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "test future hung");
            flag = signal.changed.wait_timeout(flag, left).unwrap().0;
        }
        *flag = false;
    }
}
fn key() -> WatchKey {
    WatchKey {
        device: "peer".into(),
        service_instance: "service".into(),
        auth_generation: 1,
        session: "session".into(),
    }
}
struct Adapter {
    calls: AtomicUsize,
    closed: AtomicUsize,
    active: AtomicUsize,
    delay: Duration,
    fail: bool,
}
impl WakeAdapter for Adapter {
    fn connect(&self, _: &WatchKey, g: u64, c: Cancellation) -> Result<String, WakeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.active.fetch_add(1, Ordering::SeqCst);
        let end = Instant::now() + self.delay;
        while Instant::now() < end && !c.is_cancelled() {
            thread::sleep(Duration::from_millis(1));
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        if c.is_cancelled() {
            Err(WakeError::Cancelled)
        } else if self.fail {
            Err(WakeError::AdapterFailed)
        } else {
            Ok(format!("watch-{g}"))
        }
    }
    fn close(&self, _: &WatchKey, _: &str) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }
}
fn adapter(delay: u64, fail: bool) -> Arc<Adapter> {
    Arc::new(Adapter {
        calls: AtomicUsize::new(0),
        closed: AtomicUsize::new(0),
        active: AtomicUsize::new(0),
        delay: Duration::from_millis(delay),
        fail,
    })
}
fn coordinator(timeout: u64) -> WakeCoordinator {
    WakeCoordinator::new(
        Duration::from_millis(timeout),
        Duration::from_secs(15),
        Duration::from_secs(60),
        4,
    )
}
fn wait_clean(a: &Adapter) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while a.active.load(Ordering::SeqCst) > 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn typing_and_send_share_actual_future_and_watch() {
    let c = coordinator(1000);
    let a = adapter(30, false);
    let one = c.request(key(), Trigger::Typing, a.clone());
    let two = c.request(key(), Trigger::Send, a.clone());
    let first = thread::spawn(move || block_on(one));
    let second = thread::spawn(move || block_on(two));
    let first = first.join().unwrap().unwrap();
    assert_eq!(first, second.join().unwrap().unwrap());
    assert_eq!(a.calls.load(Ordering::SeqCst), 1);
    assert!(c.is_current(&key(), &first));
    c.cancel(&key());
    assert!(!c.is_current(&key(), &first));
    wait_coordinator(&c);
    assert_eq!(a.closed.load(Ordering::SeqCst), 1);
}
#[test]
fn failed_wake_releases_all_waiters_and_send_bypasses_typing_cooldown() {
    let c = coordinator(1000);
    let a = adapter(10, true);
    let x = c.request(key(), Trigger::Typing, a.clone());
    let y = c.request(key(), Trigger::Send, a.clone());
    assert_eq!(block_on(x), Err(WakeError::AdapterFailed));
    assert_eq!(block_on(y), Err(WakeError::AdapterFailed));
    assert_eq!(
        block_on(c.request(key(), Trigger::Typing, a.clone())),
        Err(WakeError::Cooldown)
    );
    let good = adapter(0, false);
    assert!(block_on(c.request(key(), Trigger::Send, good)).is_ok());
    c.cancel(&key());
}
#[test]
fn deadline_cancels_adapter_and_does_not_hang_waiter() {
    let c = coordinator(20);
    let a = adapter(1000, false);
    let start = Instant::now();
    assert_eq!(
        block_on(c.request(key(), Trigger::Send, a.clone())),
        Err(WakeError::Timeout)
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    wait_clean(&a);
    c.cancel(&key());
}
#[test]
fn new_auth_generation_invalidates_old_watch_and_waiter() {
    let c = coordinator(1000);
    let a = adapter(30, false);
    let old = c.request(key(), Trigger::Send, a.clone());
    let mut next = key();
    next.auth_generation = 2;
    let watch = block_on(c.request(next.clone(), Trigger::Send, a.clone())).unwrap();
    assert_eq!(block_on(old), Err(WakeError::Cancelled));
    assert_eq!(
        block_on(c.request(key(), Trigger::Send, a.clone())),
        Err(WakeError::Cancelled)
    );
    assert!(c.is_current(&next, &watch));
    assert!(!c.is_current(&key(), &watch));
    c.cancel(&next);
    wait_clean(&a);
}
#[test]
fn cancel_then_reconnect_rejects_old_generation() {
    let c = coordinator(1000);
    let a = adapter(50, false);
    let old = c.request(key(), Trigger::Send, a.clone());
    c.cancel(&key());
    assert_eq!(block_on(old), Err(WakeError::Cancelled));
    let watch = block_on(c.request(key(), Trigger::Send, a.clone())).unwrap();
    assert!(c.is_current(&key(), &watch));
    assert_eq!(a.calls.load(Ordering::SeqCst), 2);
    c.cancel(&key());
    wait_clean(&a);
}
#[test]
fn idle_empty_session_closes_subscription_and_sessions_do_not_share() {
    let c = WakeCoordinator::new(
        Duration::from_secs(1),
        Duration::from_secs(15),
        Duration::from_millis(20),
        4,
    );
    let a = adapter(0, false);
    let first = block_on(c.request(key(), Trigger::Send, a.clone())).unwrap();
    let mut other = key();
    other.session = "other".into();
    let second = block_on(c.request(other.clone(), Trigger::Send, a.clone())).unwrap();
    assert_ne!(first.generation, second.generation);
    thread::sleep(Duration::from_millis(30));
    c.expire_idle();
    assert!(!c.is_current(&key(), &first));
    assert!(!c.is_current(&other, &second));
    wait_coordinator(&c);
    assert_eq!(a.closed.load(Ordering::SeqCst), 2);
}

#[test]
fn timed_out_uncooperative_worker_keeps_capacity_and_late_watch_is_closed() {
    struct Held {
        release: Mutex<std::sync::mpsc::Receiver<()>>,
        closed: AtomicUsize,
    }
    impl WakeAdapter for Held {
        fn connect(&self, _: &WatchKey, _: u64, _: Cancellation) -> Result<String, WakeError> {
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            Ok("late-watch".into())
        }
        fn close(&self, _: &WatchKey, _: &str) {
            self.closed.fetch_add(1, Ordering::SeqCst);
        }
    }
    let c = WakeCoordinator::new(
        Duration::from_millis(20),
        Duration::ZERO,
        Duration::from_secs(60),
        1,
    );
    let (release, receiver) = std::sync::mpsc::channel();
    let held = Arc::new(Held {
        release: Mutex::new(receiver),
        closed: AtomicUsize::new(0),
    });
    assert_eq!(
        block_on(c.request(key(), Trigger::Send, held.clone())),
        Err(WakeError::Timeout)
    );
    assert_eq!(c.active_workers(), 1);
    assert_eq!(
        block_on(c.request(key(), Trigger::Send, adapter(0, false))),
        Err(WakeError::Capacity)
    );
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while c.active_workers() != 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(held.closed.load(Ordering::SeqCst), 1);
    let watch = block_on(c.request(key(), Trigger::Send, adapter(0, false))).unwrap();
    assert!(c.is_current(&key(), &watch));
    c.cancel(&key());
}

fn wait_coordinator(c: &WakeCoordinator) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while c.active_workers() != 0 {
        assert!(Instant::now() < deadline, "worker or cleanup did not exit");
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn discarded_future_still_closes_idle_watch_automatically() {
    let c = WakeCoordinator::new(
        Duration::from_secs(1),
        Duration::ZERO,
        Duration::from_millis(20),
        2,
    );
    let a = adapter(5, false);
    drop(c.request(key(), Trigger::Typing, a.clone()));
    let deadline = Instant::now() + Duration::from_secs(2);
    while a.closed.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    wait_coordinator(&c);
}
#[test]
fn adapter_panic_releases_waiter_and_capacity() {
    struct Panics;
    impl WakeAdapter for Panics {
        fn connect(&self, _: &WatchKey, _: u64, _: Cancellation) -> Result<String, WakeError> {
            panic!("fake adapter failure")
        }
        fn close(&self, _: &WatchKey, _: &str) {}
    }
    let c = coordinator(1000);
    assert_eq!(
        block_on(c.request(key(), Trigger::Send, Arc::new(Panics))),
        Err(WakeError::AdapterFailed)
    );
    wait_coordinator(&c);
    let watch = block_on(c.request(key(), Trigger::Send, adapter(0, false))).unwrap();
    assert!(c.is_current(&key(), &watch));
    c.cancel(&key());
    wait_coordinator(&c);
}
#[test]
fn blocking_cleanup_does_not_block_cancel_and_is_not_reported_finished() {
    struct SlowClose {
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl WakeAdapter for SlowClose {
        fn connect(&self, _: &WatchKey, _: u64, _: Cancellation) -> Result<String, WakeError> {
            Ok("watch".into())
        }
        fn close(&self, _: &WatchKey, _: &str) {
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
        }
    }
    let c = coordinator(1000);
    let (release, receive) = std::sync::mpsc::channel();
    let watch = block_on(c.request(
        key(),
        Trigger::Send,
        Arc::new(SlowClose {
            release: Mutex::new(receive),
        }),
    ))
    .unwrap();
    let start = Instant::now();
    c.cancel(&key());
    assert!(start.elapsed() < Duration::from_millis(100));
    assert!(!c.is_current(&key(), &watch));
    assert!(c.active_workers() > 0);
    release.send(()).unwrap();
    wait_coordinator(&c);
    assert!(!c.cleanup_failed());
}
