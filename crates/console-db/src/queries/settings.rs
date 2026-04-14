use sqlx::FromRow;

use crate::DbPool;

#[derive(Debug, Clone, FromRow)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

pub async fn get_setting(pool: &DbPool, key: &str) -> Result<Option<SettingRow>, sqlx::Error> {
    sqlx::query_as::<_, SettingRow>("SELECT key, value, updated_at FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
}

pub async fn get_all_settings(pool: &DbPool) -> Result<Vec<SettingRow>, sqlx::Error> {
    sqlx::query_as::<_, SettingRow>("SELECT key, value, updated_at FROM settings ORDER BY key")
        .fetch_all(pool)
        .await
}

pub async fn upsert_setting(
    pool: &DbPool,
    key: &str,
    value: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_setting(pool: &DbPool, key: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
