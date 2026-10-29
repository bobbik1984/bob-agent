//! Pure single-flight state machine. A transport adapter owns I/O and shared
//! waiters; all joiners receive the same generation ticket, never a skipped send.
use std::collections::HashMap;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct WatchKey {
    pub device: String,
    pub service_instance: String,
    pub auth_generation: u64,
    pub session: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Typing,
    Send,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Start(u64),
    Join(u64),
    Ready(u64),
    Cooldown,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Dormant,
    Connecting,
    Ready,
}
struct Entry {
    generation: u64,
    phase: Phase,
    started: u64,
    touched: u64,
}
pub struct Prewarm {
    entries: HashMap<WatchKey, Entry>,
    generation: u64,
    cooldown_ms: u64,
    idle_ms: u64,
}
impl Prewarm {
    pub fn new(cooldown_ms: u64, idle_ms: u64) -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            cooldown_ms,
            idle_ms,
        }
    }
    pub fn request(&mut self, key: WatchKey, trigger: Trigger, now: u64) -> Decision {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.touched = now;
            match entry.phase {
                Phase::Connecting => return Decision::Join(entry.generation),
                Phase::Ready => return Decision::Ready(entry.generation),
                Phase::Dormant
                    if trigger == Trigger::Typing
                        && now.saturating_sub(entry.started) < self.cooldown_ms =>
                {
                    return Decision::Cooldown
                }
                _ => {}
            }
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("generation exhausted");
        self.entries.insert(
            key,
            Entry {
                generation: self.generation,
                phase: Phase::Connecting,
                started: now,
                touched: now,
            },
        );
        Decision::Start(self.generation)
    }
    /// Authentication + watch subscription + initial snapshot must all succeed.
    /// An empty snapshot is valid; opening a socket alone is not readiness.
    pub fn initial_snapshot(&mut self, key: &WatchKey, generation: u64) -> bool {
        match self.entries.get_mut(key) {
            Some(entry) if entry.generation == generation && entry.phase == Phase::Connecting => {
                entry.phase = Phase::Ready;
                true
            }
            _ => false,
        }
    }
    pub fn failed(&mut self, key: &WatchKey, generation: u64) -> bool {
        match self.entries.get_mut(key) {
            Some(entry) if entry.generation == generation => {
                entry.phase = Phase::Dormant;
                true
            }
            _ => false,
        }
    }
    /// Returns tickets whose transport must be closed/cancelled by the adapter.
    pub fn expire_idle(&mut self, now: u64) -> Vec<(WatchKey, u64)> {
        let mut expired = Vec::new();
        for (key, entry) in &mut self.entries {
            if entry.phase != Phase::Dormant && now.saturating_sub(entry.touched) >= self.idle_ms {
                entry.phase = Phase::Dormant;
                expired.push((key.clone(), entry.generation));
            }
        }
        expired
    }
}
