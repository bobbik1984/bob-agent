//! One cancellable timer per active transport call; no network or executor dependency.
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
struct State {
    stopped: bool,
    waker: Option<Waker>,
}
struct Timer {
    shared: Arc<(Mutex<State>, Condvar)>,
    deadline: Instant,
}
impl Drop for Timer {
    fn drop(&mut self) {
        let (m, c) = &*self.shared;
        m.lock().unwrap().stopped = true;
        c.notify_all();
    }
}
struct Bounded<F> {
    future: Pin<Box<F>>,
    timer: Timer,
}
impl<F: Future> Future for Bounded<F> {
    type Output = Option<F::Output>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if Instant::now() >= self.timer.deadline {
            return Poll::Ready(None);
        }
        {
            let (m, _) = &*self.timer.shared;
            m.lock().unwrap().waker = Some(cx.waker().clone());
        }
        // Timer may have fired before this poll registered its waker.
        if Instant::now() >= self.timer.deadline {
            return Poll::Ready(None);
        }
        match self.future.as_mut().poll(cx) {
            Poll::Ready(x) => Poll::Ready(Some(x)),
            Poll::Pending => Poll::Pending,
        }
    }
}
pub(crate) fn bounded<F: Future>(
    future: F,
    duration: Duration,
) -> impl Future<Output = Option<F::Output>> {
    let now = Instant::now();
    let mut deadline = now.checked_add(duration).unwrap_or(now);
    let shared = Arc::new((
        Mutex::new(State {
            stopped: false,
            waker: None,
        }),
        Condvar::new(),
    ));
    let worker = shared.clone();
    let worker_deadline = deadline;
    let spawned = std::thread::Builder::new()
        .name("bob-transport-deadline".into())
        .spawn(move || {
            let deadline = worker_deadline;
            let (m, c) = &*worker;
            let mut state = m.lock().unwrap();
            while !state.stopped && Instant::now() < deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                state = c.wait_timeout(state, remaining).unwrap().0;
            }
            let waker = if state.stopped {
                None
            } else {
                state.waker.take()
            };
            drop(state);
            if let Some(w) = waker {
                w.wake();
            }
        });
    if spawned.is_err() {
        deadline = Instant::now();
    }
    Bounded {
        future: Box::pin(future),
        timer: Timer { shared, deadline },
    }
}
