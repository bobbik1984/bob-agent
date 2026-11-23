//! Runtime single-flight wakeup. Local durable acceptance never awaits this API.
//! Adapters own authentication, subscription and transport cleanup; deadlines
//! release waiters, while cooperative cancellation bounds adapter work.
use crate::prewarm::{Trigger, WatchKey};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WakeError {
    Cancelled,
    Timeout,
    Cooldown,
    Capacity,
    AdapterFailed,
    InvalidWatch,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyWatch {
    pub key: WatchKey,
    pub generation: u64,
    pub watch_id: String,
}
#[derive(Clone)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
pub trait WakeAdapter: Send + Sync + 'static {
    /// Return only after peer authentication, fresh watch and initial snapshot.
    /// Check cancellation during I/O and apply transport deadlines. Errors must
    /// be sanitized; no credentials or raw provider output enter this type.
    fn connect(
        &self,
        key: &WatchKey,
        generation: u64,
        cancel: Cancellation,
    ) -> Result<String, WakeError>;
    /// Idempotent, bounded cleanup of this exact subscription only.
    fn close(&self, key: &WatchKey, watch_id: &str);
}
struct Completion {
    result: Option<Result<ReadyWatch, WakeError>>,
    waiters: Vec<Waker>,
}
struct Flight {
    completion: Mutex<Completion>,
    changed: Condvar,
    cancellation: Cancellation,
}
impl Flight {
    fn new() -> Self {
        Self {
            completion: Mutex::new(Completion {
                result: None,
                waiters: vec![],
            }),
            changed: Condvar::new(),
            cancellation: Cancellation(Arc::new(AtomicBool::new(false))),
        }
    }
    fn result(&self) -> Option<Result<ReadyWatch, WakeError>> {
        self.completion.lock().unwrap().result.clone()
    }
    fn finish(&self, result: Result<ReadyWatch, WakeError>) -> bool {
        let waiters = {
            let mut state = self.completion.lock().unwrap();
            if state.result.is_some() {
                return false;
            }
            if result.is_err() {
                self.cancellation.0.store(true, Ordering::Release);
            }
            state.result = Some(result);
            std::mem::take(&mut state.waiters)
        };
        self.changed.notify_all();
        for waiter in waiters {
            waiter.wake();
        }
        true
    }
}
pub struct WakeFuture {
    flight: Arc<Flight>,
}
impl Future for WakeFuture {
    type Output = Result<ReadyWatch, WakeError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.flight.completion.lock().unwrap();
        if let Some(result) = &state.result {
            return Poll::Ready(result.clone());
        }
        if !state.waiters.iter().any(|w| w.will_wake(cx.waker())) {
            state.waiters.push(cx.waker().clone());
        }
        Poll::Pending
    }
}
struct Slot {
    generation: u64,
    started: Instant,
    touched: Instant,
    flight: Arc<Flight>,
    adapter: Arc<dyn WakeAdapter>,
}
struct State {
    generation: u64,
    entries: HashMap<WatchKey, Slot>,
    auth: HashMap<(String, String), u64>,
}
struct CloseJob {
    key: WatchKey,
    watch_id: String,
    adapter: Arc<dyn WakeAdapter>,
}
struct Cleanup {
    queue: VecDeque<CloseJob>,
    running: bool,
}
struct WorkerGuard(Arc<Shared>);
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.workers.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Shared {
    state: Mutex<State>,
    workers: AtomicUsize,
    cleanup_pending: AtomicUsize,
    cleanup_failed: AtomicBool,
    cleanup: Mutex<Cleanup>,
    max_workers: usize,
    timeout: Duration,
    cooldown: Duration,
    idle: Duration,
}
#[derive(Clone)]
pub struct WakeCoordinator {
    shared: Arc<Shared>,
}
impl WakeCoordinator {
    pub fn new(timeout: Duration, cooldown: Duration, idle: Duration, max_workers: usize) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    generation: 0,
                    entries: HashMap::new(),
                    auth: HashMap::new(),
                }),
                workers: AtomicUsize::new(0),
                cleanup_pending: AtomicUsize::new(0),
                cleanup_failed: AtomicBool::new(false),
                cleanup: Mutex::new(Cleanup {
                    queue: VecDeque::new(),
                    running: false,
                }),
                max_workers,
                timeout,
                cooldown,
                idle,
            }),
        }
    }
    fn immediate(error: WakeError) -> WakeFuture {
        let flight = Arc::new(Flight::new());
        flight.finish(Err(error));
        WakeFuture { flight }
    }
    pub fn request(
        &self,
        key: WatchKey,
        trigger: Trigger,
        adapter: Arc<dyn WakeAdapter>,
    ) -> WakeFuture {
        Self::start_cleanup(&self.shared);
        self.expire_idle();
        let scope = (key.device.clone(), key.service_instance.clone());
        let obsolete = {
            let mut state = self.shared.state.lock().unwrap();
            if state
                .auth
                .get(&scope)
                .is_some_and(|generation| *generation > key.auth_generation)
            {
                return Self::immediate(WakeError::Cancelled);
            }
            state.auth.insert(scope, key.auth_generation);
            let keys: Vec<_> = state
                .entries
                .keys()
                .filter(|k| {
                    k.device == key.device
                        && k.service_instance == key.service_instance
                        && k.auth_generation != key.auth_generation
                })
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|k| state.entries.remove(&k).map(|s| (k, s)))
                .collect::<Vec<_>>()
        };
        for (old_key, slot) in obsolete {
            self.close_slot(&old_key, slot);
        }
        let now = Instant::now();
        let mut state = self.shared.state.lock().unwrap();
        if state
            .auth
            .get(&(key.device.clone(), key.service_instance.clone()))
            != Some(&key.auth_generation)
        {
            return Self::immediate(WakeError::Cancelled);
        }
        if let Some(slot) = state.entries.get_mut(&key) {
            slot.touched = now;
            match slot.flight.result() {
                None | Some(Ok(_)) => {
                    return WakeFuture {
                        flight: slot.flight.clone(),
                    }
                }
                Some(Err(_))
                    if trigger == Trigger::Typing
                        && now.duration_since(slot.started) < self.shared.cooldown =>
                {
                    return Self::immediate(WakeError::Cooldown)
                }
                _ => {}
            }
        }
        // Count unfinished adapter calls, not just visible flights: a provider
        // ignoring cancellation cannot cause unlimited replacement threads.
        if self.active_workers() >= self.shared.max_workers
            || self.shared.cleanup_failed.load(Ordering::Acquire)
        {
            return Self::immediate(WakeError::Capacity);
        }
        // Ready subscriptions and their idle monitors also consume capacity.
        if !state.entries.contains_key(&key) && state.entries.len() >= self.shared.max_workers {
            return Self::immediate(WakeError::Capacity);
        }
        let Some(generation) = state.generation.checked_add(1) else {
            return Self::immediate(WakeError::Capacity);
        };
        state.generation = generation;
        let flight = Arc::new(Flight::new());
        state.entries.insert(
            key.clone(),
            Slot {
                generation,
                started: now,
                touched: now,
                flight: flight.clone(),
                adapter: adapter.clone(),
            },
        );
        self.shared.workers.fetch_add(1, Ordering::AcqRel);
        drop(state);
        let monitor_key = key.clone();
        let fallback_key = key.clone();
        let worker_flight = flight.clone();
        let shared = self.shared.clone();
        let worker = thread::Builder::new()
            .name("bob-wake".into())
            .spawn(move || {
                let _guard = WorkerGuard(shared.clone());
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    adapter.connect(&key, generation, worker_flight.cancellation.clone())
                }))
                .unwrap_or(Err(WakeError::AdapterFailed));
                let outcome = outcome.and_then(|watch_id| {
                    if watch_id.trim().is_empty() {
                        Err(WakeError::InvalidWatch)
                    } else {
                        Ok(ReadyWatch {
                            key: key.clone(),
                            generation,
                            watch_id,
                        })
                    }
                });
                // Never invoke user wakers under the coordinator lock. Concurrent
                // cancellation completes the flight or removes its current slot.
                let current = shared
                    .state
                    .lock()
                    .unwrap()
                    .entries
                    .get(&key)
                    .is_some_and(|s| s.generation == generation);
                let accepted = current && worker_flight.finish(outcome.clone());
                if !accepted {
                    if let Ok(watch) = outcome {
                        Self::enqueue_close(
                            &shared,
                            CloseJob {
                                key: key.clone(),
                                watch_id: watch.watch_id,
                                adapter: adapter.clone(),
                            },
                        );
                    }
                }
            });
        if worker.is_err() {
            self.shared.workers.fetch_sub(1, Ordering::AcqRel);
            flight.finish(Err(WakeError::AdapterFailed));
            return WakeFuture { flight };
        }
        let deadline_flight = flight.clone();
        let timeout = self.shared.timeout;
        let monitor = self.clone();
        if thread::Builder::new()
            .name("bob-wake-deadline".into())
            .spawn(move || {
                let state = deadline_flight.completion.lock().unwrap();
                let (state, _) = deadline_flight
                    .changed
                    .wait_timeout_while(state, timeout, |s| s.result.is_none())
                    .unwrap();
                let expired = state.result.is_none();
                drop(state);
                if expired {
                    deadline_flight.finish(Err(WakeError::Timeout));
                }
                // A discarded prewarm Future must still release an unused watch.
                loop {
                    if !matches!(deadline_flight.result(), Some(Ok(_))) {
                        break;
                    }
                    let remaining = {
                        let state = monitor.shared.state.lock().unwrap();
                        match state.entries.get(&monitor_key) {
                            Some(slot) if slot.generation == generation => {
                                monitor.shared.idle.saturating_sub(slot.touched.elapsed())
                            }
                            _ => break,
                        }
                    };
                    if remaining.is_zero() {
                        monitor.expire_idle();
                        break;
                    }
                    let guard = deadline_flight.completion.lock().unwrap();
                    if !matches!(guard.result, Some(Ok(_))) {
                        break;
                    }
                    let _ = deadline_flight
                        .changed
                        .wait_timeout(guard, remaining)
                        .unwrap();
                }
            })
            .is_err()
        {
            flight.finish(Err(WakeError::AdapterFailed));
            self.cancel(&fallback_key);
        }
        WakeFuture { flight }
    }
    /// Includes timed-out adapters that have not actually exited yet.
    pub fn active_workers(&self) -> usize {
        self.shared.workers.load(Ordering::Acquire)
            + self.shared.cleanup_pending.load(Ordering::Acquire)
    }
    pub fn cleanup_failed(&self) -> bool {
        self.shared.cleanup_failed.load(Ordering::Acquire)
    }
    fn enqueue_close(shared: &Arc<Shared>, job: CloseJob) {
        shared.cleanup_pending.fetch_add(1, Ordering::AcqRel);
        shared.cleanup.lock().unwrap().queue.push_back(job);
        Self::start_cleanup(shared);
    }
    fn start_cleanup(shared: &Arc<Shared>) {
        let mut cleanup = shared.cleanup.lock().unwrap();
        if cleanup.running || cleanup.queue.is_empty() {
            return;
        }
        cleanup.running = true;
        let work = shared.clone();
        if thread::Builder::new()
            .name("bob-watch-cleanup".into())
            .spawn(move || {
                loop {
                    let job = {
                        let mut cleanup = work.cleanup.lock().unwrap();
                        match cleanup.queue.pop_front() {
                            Some(job) => job,
                            None => {
                                cleanup.running = false;
                                break;
                            }
                        }
                    };
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        job.adapter.close(&job.key, &job.watch_id)
                    }))
                    .is_err()
                    {
                        // Do not claim a panicking cleanup completed successfully.
                        work.cleanup_failed.store(true, Ordering::Release);
                    }
                    work.cleanup_pending.fetch_sub(1, Ordering::AcqRel);
                }
            })
            .is_err()
        {
            cleanup.running = false;
        }
    }
    pub fn cancel(&self, key: &WatchKey) {
        let slot = self.shared.state.lock().unwrap().entries.remove(key);
        if let Some(slot) = slot {
            self.close_slot(key, slot);
        }
    }
    fn close_slot(&self, key: &WatchKey, slot: Slot) {
        slot.flight.cancellation.0.store(true, Ordering::Release);
        let (ready, waiters) = {
            let mut completion = slot.flight.completion.lock().unwrap();
            let ready = completion.result.replace(Err(WakeError::Cancelled));
            (ready, std::mem::take(&mut completion.waiters))
        };
        slot.flight.changed.notify_all();
        for waiter in waiters {
            waiter.wake();
        }
        if let Some(Ok(watch)) = ready {
            Self::enqueue_close(
                &self.shared,
                CloseJob {
                    key: key.clone(),
                    watch_id: watch.watch_id,
                    adapter: slot.adapter,
                },
            );
        }
    }
    /// Call from the host timer too: request also prunes idle sessions. Empty
    /// conversations have exactly the same expiry behavior as populated ones.
    pub fn expire_idle(&self) {
        let expired = {
            let mut state = self.shared.state.lock().unwrap();
            let now = Instant::now();
            let keys: Vec<_> = state
                .entries
                .iter()
                .filter(|(_, slot)| now.duration_since(slot.touched) >= self.shared.idle)
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter()
                .filter_map(|key| state.entries.remove(&key).map(|s| (key, s)))
                .collect::<Vec<_>>()
        };
        for (key, slot) in expired {
            self.close_slot(&key, slot);
        }
    }
    /// Re-check immediately before sending; the adapter must also fence the
    /// generation when writing bytes. A ReadyWatch is not an authority token.
    pub fn is_current(&self, key: &WatchKey, watch: &ReadyWatch) -> bool {
        if key != &watch.key {
            return false;
        }
        let state = self.shared.state.lock().unwrap();
        state.entries.get(&watch.key).is_some_and(|s| {
            s.generation == watch.generation
                && s.touched.elapsed() < self.shared.idle
                && s.flight.result() == Some(Ok(watch.clone()))
        })
    }
}
