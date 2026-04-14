use sqlx::FromRow;

use crate::DbPool;

#[derive(Debug, Clone, FromRow)]
pub struct UserSessionRow {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub ip: String,
    pub user_agent: String,
    pub created_at: String,
    pub expires_at: String,
}

/// Look up a session by its token hash. Returns None if expired or missing.
pub async fn get_by_token_hash(
    pool: &DbPool,
    token_hash: &str,
    now: &str,
) -> Result<Option<UserSessionRow>, sqlx::Error> {
    sqlx::query_as::<_, UserSessionRow>(
        "SELECT * FROM user_sessions WHERE token_hash = ? AND expires_at > ?",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_session(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    token_hash: &str,
    ip: &str,
    user_agent: &str,
    created_at: &str,
    expires_at: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, token_hash, ip, user_agent, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(user_id)
    .bind(token_hash)
    .bind(ip)
    .bind(user_agent)
    .bind(created_at)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_session(pool: &DbPool, id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM user_sessions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete all sessions for a user (used on password change / session rotation).
pub async fn delete_user_sessions(pool: &DbPool, user_id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM user_sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Delete expired sessions.
pub async fn cleanup_expired(pool: &DbPool, now: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM user_sessions WHERE expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
