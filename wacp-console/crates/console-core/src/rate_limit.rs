//! Rate limiting — per-IP and per-account sliding windows.
//!
//! Spec: `wcon-auth` §9

use console_db::queries::login_attempts;
use console_db::DbPool;

use crate::error::ConsoleError;

/// Per-IP: 20 failed attempts within 15 minutes → 429.
const MAX_FAILED_PER_IP: i64 = 20;
/// Per-account: 5 failed attempts within 15 minutes → ACCOUNT_LOCKED.
const MAX_FAILED_PER_ACCOUNT: i64 = 5;
/// Window size in minutes.
const WINDOW_MINUTES: i64 = 15;

/// Check rate limits before attempting login.
/// Returns Ok(()) if allowed, Err if rate-limited.
pub async fn check_login_rate_limit(
    pool: &DbPool,
    ip: &str,
    username: &str,
) -> Result<(), ConsoleError> {
    let window_start = (chrono::Utc::now() - chrono::Duration::minutes(WINDOW_MINUTES)).to_rfc3339();

    // Check per-IP limit
    let ip_count = login_attempts::count_failed_by_ip(pool, ip, &window_start)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?;

    if ip_count >= MAX_FAILED_PER_IP {
        return Err(ConsoleError::RateLimited);
    }

    // Check per-account limit
    let account_count =
        login_attempts::count_failed_by_username(pool, username, &window_start)
            .await
            .map_err(|e| ConsoleError::Database(e.to_string()))?;

    if account_count >= MAX_FAILED_PER_ACCOUNT {
        return Err(ConsoleError::AccountLocked);
    }

    Ok(())
}

/// Record a login attempt (success or failure).
pub async fn record_login_attempt(
    pool: &DbPool,
    ip: &str,
    username: &str,
    success: bool,
) -> Result<(), ConsoleError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    login_attempts::record_attempt(pool, &id, ip, username, &now, success)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))
}

/// Garbage-collect login attempts older than 24 hours.
pub async fn cleanup_old_attempts(pool: &DbPool) -> Result<u64, ConsoleError> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
    login_attempts::cleanup_before(pool, &cutoff)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use console_db::create_test_pool;

    #[tokio::test]
    async fn allows_under_limit() {
        let pool = create_test_pool().await.unwrap();
        check_login_rate_limit(&pool, "10.0.0.1", "admin").await.unwrap();
    }

    #[tokio::test]
    async fn locks_account_after_threshold() {
        let pool = create_test_pool().await.unwrap();

        for _ in 0..MAX_FAILED_PER_ACCOUNT {
            record_login_attempt(&pool, "10.0.0.1", "admin", false).await.unwrap();
        }

        let err = check_login_rate_limit(&pool, "10.0.0.1", "admin").await.unwrap_err();
        assert!(matches!(err, ConsoleError::AccountLocked));
    }

    #[tokio::test]
    async fn rate_limits_ip_after_threshold() {
        let pool = create_test_pool().await.unwrap();

        // Use different usernames to avoid account lock
        for i in 0..MAX_FAILED_PER_IP {
            record_login_attempt(&pool, "10.0.0.1", &format!("user{i}"), false)
                .await
                .unwrap();
        }

        let err = check_login_rate_limit(&pool, "10.0.0.1", "newuser").await.unwrap_err();
        assert!(matches!(err, ConsoleError::RateLimited));
    }
}
