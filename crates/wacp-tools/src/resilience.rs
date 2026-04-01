use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use crate::handler::ToolError;

// ---------------------------------------------------------------------------
// Circuit Breaker
// ---------------------------------------------------------------------------

/// Per-tool circuit breaker: closed → open → half-open → closed.
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Mutex<BreakerState>,
}

/// Circuit breaker configuration.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failures within window to trip the breaker.
    pub failure_threshold: u32,
    /// Window for counting failures.
    pub failure_window: Duration,
    /// How long the breaker stays open before probing.
    pub cooldown: Duration,
    /// Whether the circuit breaker is enabled.
    pub enabled: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            failure_window: Duration::from_secs(60),
            cooldown: Duration::from_secs(30),
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerStatus {
    Closed,
    Open,
    HalfOpen,
}

struct BreakerState {
    status: BreakerStatus,
    /// Timestamps of recent failures (within failure_window).
    failures: Vec<Instant>,
    /// When the breaker opened (for cooldown calculation).
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Mutex::new(BreakerState {
                status: BreakerStatus::Closed,
                failures: Vec::new(),
                opened_at: None,
            }),
        }
    }

    /// Check if the breaker allows an invocation. Returns Ok(()) or Err(Unavailable).
    pub fn check(&self) -> Result<(), ToolError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut state = self.state.lock().unwrap();
        let now = Instant::now();

        match state.status {
            BreakerStatus::Closed => Ok(()),
            BreakerStatus::Open => {
                // Check if cooldown has elapsed → transition to half-open
                if let Some(opened_at) = state.opened_at {
                    if now.duration_since(opened_at) >= self.config.cooldown {
                        state.status = BreakerStatus::HalfOpen;
                        return Ok(()); // allow one probe
                    }
                }
                Err(ToolError::unavailable(format!(
                    "circuit breaker open, cooldown remaining: {:?}",
                    state
                        .opened_at
                        .map(|t| self.config.cooldown.saturating_sub(now.duration_since(t)))
                        .unwrap_or_default()
                )))
            }
            BreakerStatus::HalfOpen => {
                // Only one probe allowed — reject additional calls while probing.
                // In practice, the first caller that passed check() is the probe.
                // Subsequent callers during half-open are rejected.
                Err(ToolError::unavailable(
                    "circuit breaker half-open, probe in progress",
                ))
            }
        }
    }

    /// Record a successful invocation.
    pub fn record_success(&self) {
        if !self.config.enabled {
            return;
        }

        let mut state = self.state.lock().unwrap();
        match state.status {
            BreakerStatus::HalfOpen => {
                // Probe succeeded → close the breaker
                state.status = BreakerStatus::Closed;
                state.failures.clear();
                state.opened_at = None;
            }
            BreakerStatus::Closed => {
                // Success in closed state — no action needed
            }
            BreakerStatus::Open => {
                // Should not happen (check() would reject), but no harm
            }
        }
    }

    /// Record a failed invocation. May trip the breaker.
    pub fn record_failure(&self) {
        if !self.config.enabled {
            return;
        }

        let mut state = self.state.lock().unwrap();
        let now = Instant::now();

        match state.status {
            BreakerStatus::Closed => {
                // Prune old failures outside the window
                let cutoff = now - self.config.failure_window;
                state.failures.retain(|t| *t >= cutoff);

                // Record this failure
                state.failures.push(now);

                // Check threshold
                if state.failures.len() >= self.config.failure_threshold as usize {
                    state.status = BreakerStatus::Open;
                    state.opened_at = Some(now);
                }
            }
            BreakerStatus::HalfOpen => {
                // Probe failed → back to open
                state.status = BreakerStatus::Open;
                state.opened_at = Some(now);
            }
            BreakerStatus::Open => {
                // Already open — no action
            }
        }
    }

    /// Current breaker status.
    pub fn status(&self) -> BreakerStatus {
        self.state.lock().unwrap().status
    }
}

// ---------------------------------------------------------------------------
// Concurrency Limiter
// ---------------------------------------------------------------------------

/// Per-tool concurrency limiter with bounded queue.
pub struct ConcurrencyLimiter {
    semaphore: Semaphore,
    config: ConcurrencyConfig,
    queued: AtomicU64,
}

/// Concurrency limiter configuration.
#[derive(Debug, Clone)]
pub struct ConcurrencyConfig {
    /// Maximum concurrent handler invocations. 0 = unlimited.
    pub max_concurrent: usize,
    /// Maximum queued invocations waiting for a permit. 0 = no queue (reject immediately).
    pub max_queued: usize,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            max_queued: 50,
        }
    }
}

/// RAII guard that releases the semaphore permit and decrements the queue count on drop.
#[derive(Debug)]
pub struct ConcurrencyPermit<'a> {
    _permit: tokio::sync::SemaphorePermit<'a>,
    queued: &'a AtomicU64,
    was_queued: bool,
}

impl Drop for ConcurrencyPermit<'_> {
    fn drop(&mut self) {
        if self.was_queued {
            self.queued.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl ConcurrencyLimiter {
    pub fn new(config: ConcurrencyConfig) -> Self {
        let permits = if config.max_concurrent == 0 {
            Semaphore::MAX_PERMITS
        } else {
            config.max_concurrent
        };
        Self {
            semaphore: Semaphore::new(permits),
            config,
            queued: AtomicU64::new(0),
        }
    }

    /// Try to acquire a permit. Returns immediately if one is available.
    /// Queues if permits are exhausted and queue has space.
    /// Returns Overloaded if queue is full.
    pub async fn acquire(&self) -> Result<ConcurrencyPermit<'_>, ToolError> {
        // Try immediate acquire
        match self.semaphore.try_acquire() {
            Ok(permit) => {
                return Ok(ConcurrencyPermit {
                    _permit: permit,
                    queued: &self.queued,
                    was_queued: false,
                });
            }
            Err(_) => {
                // No permit available — check queue capacity
                if self.config.max_concurrent == 0 {
                    // Unlimited — should never reach here, but handle gracefully
                    let permit = self.semaphore.acquire().await.map_err(|_| {
                        ToolError::internal("semaphore closed")
                    })?;
                    return Ok(ConcurrencyPermit {
                        _permit: permit,
                        queued: &self.queued,
                        was_queued: false,
                    });
                }

                let current_queued = self.queued.load(Ordering::Relaxed);
                if current_queued >= self.config.max_queued as u64 {
                    return Err(ToolError::overloaded(format!(
                        "concurrency limit reached ({} active, {} queued)",
                        self.config.max_concurrent, current_queued
                    )));
                }

                // Enter queue
                self.queued.fetch_add(1, Ordering::Relaxed);
                match self.semaphore.acquire().await {
                    Ok(permit) => Ok(ConcurrencyPermit {
                        _permit: permit,
                        queued: &self.queued,
                        was_queued: true,
                    }),
                    Err(_) => {
                        self.queued.fetch_sub(1, Ordering::Relaxed);
                        Err(ToolError::internal("semaphore closed"))
                    }
                }
            }
        }
    }

    /// Number of invocations currently waiting in the queue.
    pub fn queued_count(&self) -> u64 {
        self.queued.load(Ordering::Relaxed)
    }

    /// Number of available permits.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Circuit Breaker tests
    // -----------------------------------------------------------------------

    #[test]
    fn disabled_breaker_always_allows() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: false,
            ..Default::default()
        });
        // Record many failures — still allows
        for _ in 0..100 {
            cb.record_failure();
        }
        assert!(cb.check().is_ok());
    }

    #[test]
    fn closed_allows_invocations() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 3,
            ..Default::default()
        });
        assert_eq!(cb.status(), BreakerStatus::Closed);
        assert!(cb.check().is_ok());
    }

    #[test]
    fn trips_open_at_threshold() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 3,
            failure_window: Duration::from_secs(60),
            cooldown: Duration::from_secs(30),
        });

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.status(), BreakerStatus::Closed);

        cb.record_failure(); // 3rd failure → trips
        assert_eq!(cb.status(), BreakerStatus::Open);
    }

    #[test]
    fn open_rejects_invocations() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 1,
            ..Default::default()
        });
        cb.record_failure(); // trips immediately
        assert_eq!(cb.status(), BreakerStatus::Open);

        let err = cb.check().unwrap_err();
        assert_eq!(err.code, crate::handler::ToolErrorCode::Unavailable);
        assert!(err.retryable);
    }

    #[test]
    fn half_open_after_cooldown() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 1,
            cooldown: Duration::from_millis(0), // instant cooldown for testing
            ..Default::default()
        });
        cb.record_failure(); // trips
        assert_eq!(cb.status(), BreakerStatus::Open);

        // Cooldown is 0ms, so check() should transition to half-open
        assert!(cb.check().is_ok());
        assert_eq!(cb.status(), BreakerStatus::HalfOpen);
    }

    #[test]
    fn half_open_closes_on_success() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 1,
            cooldown: Duration::from_millis(0),
            ..Default::default()
        });
        cb.record_failure();

        cb.check().unwrap(); // transitions to half-open
        cb.record_success(); // probe succeeded → closed
        assert_eq!(cb.status(), BreakerStatus::Closed);
    }

    #[test]
    fn half_open_reopens_on_failure() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 1,
            cooldown: Duration::from_millis(0),
            ..Default::default()
        });
        cb.record_failure();

        cb.check().unwrap(); // transitions to half-open
        cb.record_failure(); // probe failed → back to open
        assert_eq!(cb.status(), BreakerStatus::Open);
    }

    #[test]
    fn half_open_rejects_extra_calls() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 1,
            cooldown: Duration::from_millis(0),
            ..Default::default()
        });
        cb.record_failure();

        cb.check().unwrap(); // first call → half-open
        assert!(cb.check().is_err()); // second call → rejected (probe in progress)
    }

    #[test]
    fn failures_outside_window_dont_trip_breaker() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 3,
            failure_window: Duration::from_millis(50), // very short window
            cooldown: Duration::from_secs(30),
        });

        // Record 2 failures
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.status(), BreakerStatus::Closed);

        // Wait for them to expire
        std::thread::sleep(Duration::from_millis(60));

        // This failure should NOT trip the breaker — previous 2 are expired
        cb.record_failure();
        assert_eq!(cb.status(), BreakerStatus::Closed); // still closed, only 1 recent failure
    }

    #[test]
    fn success_in_closed_clears_nothing() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 3,
            ..Default::default()
        });
        cb.record_failure();
        cb.record_failure();
        cb.record_success(); // does not clear failures
        cb.record_failure(); // 3rd failure in window → trips
        assert_eq!(cb.status(), BreakerStatus::Open);
    }

    // -----------------------------------------------------------------------
    // Concurrency Limiter tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn acquire_within_limit() {
        let limiter = ConcurrencyLimiter::new(ConcurrencyConfig {
            max_concurrent: 3,
            max_queued: 5,
        });
        let _p1 = limiter.acquire().await.unwrap();
        let _p2 = limiter.acquire().await.unwrap();
        let _p3 = limiter.acquire().await.unwrap();
        assert_eq!(limiter.available_permits(), 0);
    }

    #[tokio::test]
    async fn permit_released_on_drop() {
        let limiter = ConcurrencyLimiter::new(ConcurrencyConfig {
            max_concurrent: 1,
            max_queued: 0,
        });
        {
            let _p = limiter.acquire().await.unwrap();
            assert_eq!(limiter.available_permits(), 0);
        }
        // Permit dropped
        assert_eq!(limiter.available_permits(), 1);
    }

    #[tokio::test]
    async fn queue_full_returns_overloaded() {
        let limiter = ConcurrencyLimiter::new(ConcurrencyConfig {
            max_concurrent: 1,
            max_queued: 0, // no queue
        });
        let _p = limiter.acquire().await.unwrap(); // takes the only permit

        let err = limiter.acquire().await.unwrap_err();
        assert_eq!(err.code, crate::handler::ToolErrorCode::Overloaded);
    }

    #[tokio::test]
    async fn unlimited_concurrency() {
        let limiter = ConcurrencyLimiter::new(ConcurrencyConfig {
            max_concurrent: 0, // unlimited
            max_queued: 0,
        });
        let mut permits = vec![];
        for _ in 0..100 {
            permits.push(limiter.acquire().await.unwrap());
        }
        assert_eq!(permits.len(), 100);
    }

    #[tokio::test]
    async fn queued_invocation_proceeds_when_permit_freed() {
        let limiter = std::sync::Arc::new(ConcurrencyLimiter::new(ConcurrencyConfig {
            max_concurrent: 1,
            max_queued: 1,
        }));

        let p1 = limiter.acquire().await.unwrap();
        assert_eq!(limiter.available_permits(), 0);

        let limiter2 = limiter.clone();
        let handle = tokio::spawn(async move {
            // This will queue since p1 holds the only permit
            limiter2.acquire().await.unwrap();
        });

        // Give the spawned task time to queue
        tokio::task::yield_now().await;

        // Drop p1 → frees the permit → queued task proceeds
        drop(p1);
        handle.await.unwrap();
    }
}
