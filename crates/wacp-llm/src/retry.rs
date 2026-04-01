use std::time::Duration;

use rand::Rng;

use crate::error::{ErrorPersistence, LlmError};

/// Retry configuration.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retry attempts (not counting the first try). Default: 2.
    pub max_retries: u32,
    /// Base delay before first retry. Default: 1000ms.
    pub base_delay_ms: u64,
    /// Backoff strategy. Default: Exponential.
    pub backoff: BackoffStrategy,
    /// Whether to honor provider's Retry-After header. Default: true.
    pub honor_retry_after: bool,
    /// Maximum total retry duration. Default: 30_000ms.
    pub max_retry_duration_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            base_delay_ms: 1_000,
            backoff: BackoffStrategy::Exponential,
            honor_retry_after: true,
            max_retry_duration_ms: 30_000,
        }
    }
}

/// Backoff strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffStrategy {
    /// Same delay every time.
    Fixed,
    /// Delay doubles each retry: base, base*2, base*4, ...
    Exponential,
}

/// Calculate the delay for a given retry attempt.
pub fn compute_delay(config: &RetryConfig, attempt: u32) -> Duration {
    let base = match config.backoff {
        BackoffStrategy::Fixed => config.base_delay_ms,
        BackoffStrategy::Exponential => {
            config.base_delay_ms.saturating_mul(2u64.saturating_pow(attempt))
        }
    };

    // Cap at a reasonable maximum (1 hour) before jitter to prevent overflow
    let base = base.min(3_600_000);

    // Add jitter: ±25%
    let jitter_range = base / 4;
    let jitter = if jitter_range > 0 {
        let mut rng = rand::rng();
        rng.random_range(0..=jitter_range * 2) as i64 - jitter_range as i64
    } else {
        0
    };

    let delay = (base as i64 + jitter).max(1) as u64;
    Duration::from_millis(delay)
}

/// Determine if an error should be retried.
pub fn should_retry(error: &LlmError) -> bool {
    error.persistence == ErrorPersistence::Transient
}

/// Apply retry-after header if configured and present.
pub fn apply_retry_after(
    computed: Duration,
    retry_after_ms: Option<u64>,
    config: &RetryConfig,
) -> Duration {
    if config.honor_retry_after {
        if let Some(after_ms) = retry_after_ms {
            return computed.max(Duration::from_millis(after_ms));
        }
    }
    computed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff_doubles() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            backoff: BackoffStrategy::Exponential,
            ..Default::default()
        };
        // Attempt 0: ~1000ms, attempt 1: ~2000ms, attempt 2: ~4000ms
        let d0 = compute_delay(&config, 0);
        let d1 = compute_delay(&config, 1);
        let d2 = compute_delay(&config, 2);

        // With ±25% jitter: 750–1250, 1500–2500, 3000–5000
        assert!(d0.as_millis() >= 750 && d0.as_millis() <= 1250);
        assert!(d1.as_millis() >= 1500 && d1.as_millis() <= 2500);
        assert!(d2.as_millis() >= 3000 && d2.as_millis() <= 5000);
    }

    #[test]
    fn fixed_backoff_constant() {
        let config = RetryConfig {
            base_delay_ms: 500,
            backoff: BackoffStrategy::Fixed,
            ..Default::default()
        };
        for attempt in 0..5 {
            let delay = compute_delay(&config, attempt);
            // With ±25% jitter: 375–625
            assert!(delay.as_millis() >= 375 && delay.as_millis() <= 625);
        }
    }

    #[test]
    fn should_retry_transient() {
        let err = LlmError::from_status(429, "rate limited");
        assert!(should_retry(&err));
    }

    #[test]
    fn should_not_retry_permanent() {
        let err = LlmError::from_status(401, "invalid api key");
        assert!(!should_retry(&err));
    }

    #[test]
    fn should_not_retry_unknown() {
        let err = LlmError::from_status(418, "teapot");
        assert!(!should_retry(&err));
    }

    #[test]
    fn retry_after_honored_when_larger() {
        let computed = Duration::from_millis(1000);
        let result = apply_retry_after(
            computed,
            Some(5000),
            &RetryConfig { honor_retry_after: true, ..Default::default() },
        );
        assert_eq!(result, Duration::from_millis(5000));
    }

    #[test]
    fn retry_after_ignored_when_smaller() {
        let computed = Duration::from_millis(5000);
        let result = apply_retry_after(
            computed,
            Some(1000),
            &RetryConfig { honor_retry_after: true, ..Default::default() },
        );
        assert_eq!(result, Duration::from_millis(5000)); // computed is larger
    }

    #[test]
    fn retry_after_ignored_when_disabled() {
        let computed = Duration::from_millis(1000);
        let result = apply_retry_after(
            computed,
            Some(5000),
            &RetryConfig { honor_retry_after: false, ..Default::default() },
        );
        assert_eq!(result, Duration::from_millis(1000)); // retry-after ignored
    }

    #[test]
    fn retry_after_none_returns_computed() {
        let computed = Duration::from_millis(1000);
        let result = apply_retry_after(computed, None, &RetryConfig::default());
        assert_eq!(result, Duration::from_millis(1000));
    }

    #[test]
    fn zero_base_delay_no_panic() {
        let config = RetryConfig {
            base_delay_ms: 0,
            ..Default::default()
        };
        let delay = compute_delay(&config, 0);
        // 0 base → 0 jitter range → jitter = 0 → capped at min 1ms
        assert!(delay.as_millis() <= 1);
    }

    #[test]
    fn large_attempt_saturates() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            backoff: BackoffStrategy::Exponential,
            ..Default::default()
        };
        // 2^63 would overflow — should saturate
        let delay = compute_delay(&config, 63);
        assert!(delay.as_millis() > 0); // didn't panic
    }
}
