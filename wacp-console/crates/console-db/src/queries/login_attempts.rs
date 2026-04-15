use crate::DbPool;

pub async fn record_attempt(
    pool: &DbPool,
    id: &str,
    ip: &str,
    username: &str,
    attempted_at: &str,
    success: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO login_attempts (id, ip, username, attempted_at, success)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(ip)
    .bind(username)
    .bind(attempted_at)
    .bind(success)
    .execute(pool)
    .await?;
    Ok(())
}

/// Count failed attempts from a given IP within the window.
pub async fn count_failed_by_ip(pool: &DbPool, ip: &str, since: &str) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM login_attempts
         WHERE ip = ? AND attempted_at > ? AND success = 0",
    )
    .bind(ip)
    .bind(since)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Count failed attempts for a given username within the window.
pub async fn count_failed_by_username(
    pool: &DbPool,
    username: &str,
    since: &str,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM login_attempts
         WHERE username = ? AND attempted_at > ? AND success = 0",
    )
    .bind(username)
    .bind(since)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Delete all failed attempts for a specific username (admin unlock).
pub async fn clear_for_username(pool: &DbPool, username: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM login_attempts WHERE username = ? AND success = 0")
        .bind(username)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Delete attempts older than the given timestamp.
pub async fn cleanup_before(pool: &DbPool, before: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM login_attempts WHERE attempted_at < ?")
        .bind(before)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
