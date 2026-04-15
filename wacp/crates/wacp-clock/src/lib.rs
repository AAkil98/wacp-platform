//! WACP — Hybrid logical clock.
//!
//! 80-bit HLC timestamp: 64-bit physical (microseconds since epoch) + 16-bit logical counter.
//! Three generation rules: local event, send, receive.

mod time_source;
mod timestamp;

pub use time_source::{ManualTimeSource, SystemTimeSource, TimeSource};
pub use timestamp::Timestamp;

/// Hybrid Logical Clock, generic over the physical time source.
pub struct Clock<T: TimeSource = SystemTimeSource> {
    last: Timestamp,
    time_source: T,
}

impl Clock<SystemTimeSource> {
    /// Create a clock backed by the system clock.
    pub fn system() -> Self {
        Self {
            last: Timestamp::ZERO,
            time_source: SystemTimeSource,
        }
    }
}

impl<T: TimeSource> Clock<T> {
    /// Create a clock with the given time source.
    pub fn new(time_source: T) -> Self {
        Self {
            last: Timestamp::ZERO,
            time_source,
        }
    }

    /// Create a clock initialized to a known timestamp (for recovery).
    pub fn with_initial(time_source: T, initial: Timestamp) -> Self {
        Self {
            last: initial,
            time_source,
        }
    }

    /// Generate a timestamp for a local event.
    ///
    /// Rule 1: If physical time advanced, reset logical to 0.
    /// Otherwise, increment logical.
    pub fn now(&mut self) -> Timestamp {
        let physical = self.time_source.now_us();

        self.last = if physical > self.last.physical_us() {
            Timestamp::new(physical, 0)
        } else {
            Timestamp::new(
                self.last.physical_us(),
                self.last.logical().saturating_add(1),
            )
        };

        self.last
    }

    /// Generate a timestamp for an outgoing message.
    /// Equivalent to `now()`.
    pub fn send(&mut self) -> Timestamp {
        self.now()
    }

    /// Generate a timestamp incorporating a remote timestamp.
    ///
    /// Rule 3: Take max of physical times, then resolve logical counter.
    /// Guarantees the result is strictly greater than both local and remote.
    pub fn recv(&mut self, remote: Timestamp) -> Timestamp {
        let local_physical = self.time_source.now_us();
        let max_physical = local_physical
            .max(self.last.physical_us())
            .max(remote.physical_us());

        let logical = if max_physical > self.last.physical_us()
            && max_physical > remote.physical_us()
        {
            // Physical advanced beyond both — reset logical.
            0
        } else if max_physical == self.last.physical_us() && max_physical == remote.physical_us() {
            // All three tied — take max of both logicals + 1.
            self.last.logical().max(remote.logical()).saturating_add(1)
        } else if max_physical == self.last.physical_us() {
            // Local physical is the max — increment local logical.
            self.last.logical().saturating_add(1)
        } else {
            // Remote physical is the max — increment remote logical.
            remote.logical().saturating_add(1)
        };

        self.last = Timestamp::new(max_physical, logical);
        self.last
    }

    /// Current clock head (read-only).
    pub fn last(&self) -> Timestamp {
        self.last
    }
}

#[cfg(test)]
mod tests;
