use sqlx::FromRow;

use crate::DbPool;

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub username_lower: String,
    pub display_name: String,
    pub password_hash: String,
    pub console_role: String,
    pub must_change_password: bool,
    pub disabled_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_by_id(pool: &DbPool, id: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn get_by_username(
    pool: &DbPool,
    username: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE username_lower = LOWER(?)")
        .bind(username)
        .fetch_optional(pool)
        .await
}

pub async fn list_users(
    pool: &DbPool,
    include_disabled: bool,
    limit: i64,
    after_cursor: Option<&str>,
) -> Result<Vec<UserRow>, sqlx::Error> {
    let mut sql = String::from("SELECT * FROM users WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();

    if !include_disabled {
        sql.push_str(" AND disabled_at IS NULL");
    }
    if let Some(cursor) = after_cursor {
        sql.push_str(" AND username_lower > ?");
        binds.push(cursor.to_string());
    }
    sql.push_str(" ORDER BY username_lower ASC LIMIT ?");

    let mut query = sqlx::query_as::<_, UserRow>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(limit);
    query.fetch_all(pool).await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_user(
    pool: &DbPool,
    id: &str,
    username: &str,
    display_name: &str,
    password_hash: &str,
    console_role: &str,
    must_change_password: bool,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, username, username_lower, display_name, password_hash, console_role, must_change_password, created_at, updated_at)
         VALUES (?, ?, LOWER(?), ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(username)
    .bind(username)
    .bind(display_name)
    .bind(password_hash)
    .bind(console_role)
    .bind(must_change_password)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_password(
    pool: &DbPool,
    id: &str,
    password_hash: &str,
    now: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE users SET password_hash = ?, must_change_password = 0, updated_at = ? WHERE id = ?",
    )
    .bind(password_hash)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_role(
    pool: &DbPool,
    id: &str,
    console_role: &str,
    now: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE users SET console_role = ?, updated_at = ? WHERE id = ?")
        .bind(console_role)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn disable_user(pool: &DbPool, id: &str, now: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE users SET disabled_at = ?, updated_at = ? WHERE id = ? AND disabled_at IS NULL",
    )
    .bind(now)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn enable_user(pool: &DbPool, id: &str, now: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE users SET disabled_at = NULL, updated_at = ? WHERE id = ? AND disabled_at IS NOT NULL",
    )
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_must_change_password(
    pool: &DbPool,
    id: &str,
    now: &str,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("UPDATE users SET must_change_password = 1, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Count active (non-disabled) admins.
pub async fn count_active_admins(pool: &DbPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM users WHERE console_role = 'admin' AND disabled_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Count total users (for bootstrap detection).
pub async fn count_users(pool: &DbPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}
