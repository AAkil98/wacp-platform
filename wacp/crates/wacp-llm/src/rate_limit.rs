use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token bucket rate limiter with sliding window.
pub struct RateLimiter {
    config: RateLimitConfig,
    state: Mutex<RateLimitState>,
}

/// Rate limit configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window. 0 = unlimited.
    pub max_requests_per_window: u32,
    /// Maximum tokens per window. 0 = unlimited.
    pub max_tokens_per_window: u32,
    /// Window duration.
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests_per_window: 0,
            max_tokens_per_window: 0,
            window: Duration::from_secs(60),
        }
    }
}

struct RateLimitState {
    /// Timestamps of recent requests within the window.
    request_log: VecDeque<Instant>,
    /// (timestamp, token_count) of recent requests.
    token_log: VecDeque<(Instant, u32)>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            state: Mutex::new(RateLimitState {
                request_log: VecDeque::new(),
                token_log: VecDeque::new(),
            }),
        }
    }

    /// Check if a request with estimated token count is allowed.
    /// Returns Ok(()) if allowed, or the duration to wait if rate limited.
    pub fn check(&self, estimated_tokens: u32) -> Result<(), Duration> {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let cutoff = now - self.config.window;

        // Prune old entries
        while state.request_log.front().is_some_and(|t| *t < cutoff) {
            state.request_log.pop_front();
        }
        while state.token_log.front().is_some_and(|(t, _)| *t < cutoff) {
            state.token_log.pop_front();
        }

        // Check request limit
        if self.config.max_requests_per_window > 0
            && state.request_log.len() >= self.config.max_requests_per_window as usize
        {
            let oldest = state.request_log.front().unwrap();
            let wait = self.config.window - now.duration_since(*oldest);
            return Err(wait);
        }

        // Check token limit
        if self.config.max_tokens_per_window > 0 {
            let total_tokens: u32 = state.token_log.iter().map(|(_, t)| t).sum();
            if total_tokens.saturating_add(estimated_tokens) > self.config.max_tokens_per_window {
                let oldest = state.token_log.front().map(|(t, _)| t).unwrap_or(&now);
                let wait = self.config.window - now.duration_since(*oldest);
                return Err(wait);
            }
        }

        Ok(())
    }

    /// Record a completed request with actual token usage.
    pub fn record(&self, tokens: u32) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        state.request_log.push_back(now);
        state.token_log.push_back((now, tokens));
    }

    /// Check if rate limiting is configured (any limit > 0).
    pub fn is_enabled(&self) -> bool {
        self.config.max_requests_per_window > 0 || self.config.max_tokens_per_window > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_always_passes() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        assert!(!limiter.is_enabled());
        for _ in 0..1000 {
            assert!(limiter.check(1000).is_ok());
            limiter.record(1000);
        }
    }

    #[test]
    fn request_limit_under_threshold() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 5,
            max_tokens_per_window: 0,
            window: Duration::from_secs(60),
        });
        for _ in 0..5 {
            assert!(limiter.check(0).is_ok());
            limiter.record(0);
        }
    }

    #[test]
    fn request_limit_at_threshold() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 3,
            max_tokens_per_window: 0,
            window: Duration::from_secs(60),
        });
        for _ in 0..3 {
            limiter.check(0).unwrap();
            limiter.record(0);
        }
        // 4th request should be rate limited
        let wait = limiter.check(0).unwrap_err();
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_secs(60));
    }

    #[test]
    fn token_limit_under_threshold() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 0,
            max_tokens_per_window: 1000,
            window: Duration::from_secs(60),
        });
        limiter.check(500).unwrap();
        limiter.record(500);
        limiter.check(400).unwrap();
        limiter.record(400);
    }

    #[test]
    fn token_limit_at_threshold() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 0,
            max_tokens_per_window: 1000,
            window: Duration::from_secs(60),
        });
        limiter.check(800).unwrap();
        limiter.record(800);
        // 300 more would exceed 1000
        let wait = limiter.check(300).unwrap_err();
        assert!(wait > Duration::ZERO);
    }

    #[test]
    fn window_slides_restores_capacity() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 2,
            max_tokens_per_window: 0,
            window: Duration::from_millis(50),
        });
        limiter.check(0).unwrap();
        limiter.record(0);
        limiter.check(0).unwrap();
        limiter.record(0);

        // At limit
        assert!(limiter.check(0).is_err());

        // Wait for window to expire
        std::thread::sleep(Duration::from_millis(60));

        // Should pass again
        assert!(limiter.check(0).is_ok());
    }

    #[test]
    fn both_limits_enforced_independently() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 10, // generous request limit
            max_tokens_per_window: 100,  // tight token limit
            window: Duration::from_secs(60),
        });
        // 2 requests, 50 tokens each = 100 total
        limiter.check(50).unwrap();
        limiter.record(50);
        limiter.check(50).unwrap();
        limiter.record(50);
        // 3rd request with 1 token exceeds token limit
        assert!(limiter.check(1).is_err());
    }

    #[test]
    fn is_enabled_with_request_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 10,
            ..Default::default()
        });
        assert!(limiter.is_enabled());
    }

    #[test]
    fn is_enabled_with_token_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_tokens_per_window: 1000,
            ..Default::default()
        });
        assert!(limiter.is_enabled());
    }

    #[test]
    fn record_zero_tokens_repeatedly() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 0,
            max_tokens_per_window: 100,
            window: Duration::from_secs(60),
        });
        for _ in 0..50 {
            limiter.check(0).unwrap();
            limiter.record(0);
        }
        // Token total is 0, should still pass
        limiter.check(100).unwrap();
    }

    #[test]
    fn single_request_exceeds_token_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests_per_window: 0,
            max_tokens_per_window: 100,
            window: Duration::from_secs(60),
        });
        // First request with more tokens than the limit
        let result = limiter.check(200);
        assert!(result.is_err());
    }
}
