use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Abstraction over the system clock for testability.
pub trait TimeSource: Send + Sync {
    /// Current time in microseconds since Unix epoch.
    fn now_us(&self) -> u64;
}

/// Production time source using `std::time::SystemTime`.
pub struct SystemTimeSource;

impl TimeSource for SystemTimeSource {
    fn now_us(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_micros() as u64
    }
}

/// Test time source with manual control.
pub struct ManualTimeSource {
    now: AtomicU64,
}

impl ManualTimeSource {
    /// Create at the given microsecond timestamp.
    pub fn new(initial_us: u64) -> Self {
        Self {
            now: AtomicU64::new(initial_us),
        }
    }

    /// Set the current time.
    pub fn set(&self, us: u64) {
        self.now.store(us, Ordering::SeqCst);
    }

    /// Advance time by the given number of microseconds.
    pub fn advance(&self, delta_us: u64) {
        self.now.fetch_add(delta_us, Ordering::SeqCst);
    }
}

impl TimeSource for ManualTimeSource {
    fn now_us(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}
