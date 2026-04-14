use sqlx::FromRow;

use crate::DbPool;

#[derive(Debug, Clone, FromRow)]
pub struct AuditLogRow {
    pub id: String,
    pub user_id: String,
    pub timestamp: String,
    pub action: String,
    pub target_kind: String,
    pub target_id: String,
    pub detail: Option<String>,
    pub ip: String,
    pub user_agent: String,
}

pub async fn insert_entry(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    timestamp: &str,
    action: &str,
    target_kind: &str,
    target_id: &str,
    detail: Option<&str>,
    ip: &str,
    user_agent: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_log (id, user_id, timestamp, action, target_kind, target_id, detail, ip, user_agent)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(user_id)
    .bind(timestamp)
    .bind(action)
    .bind(target_kind)
    .bind(target_id)
    .bind(detail)
    .bind(ip)
    .bind(user_agent)
    .execute(pool)
    .await?;
    Ok(())
}

/// Paginated audit log query with optional filters.
/// Uses cursor-based pagination: pass `before_timestamp` for the next page.
pub async fn list_entries(
    pool: &DbPool,
    user_id: Option<&str>,
    action: Option<&str>,
    target_kind: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    limit: i64,
    before_cursor: Option<&str>,
) -> Result<Vec<AuditLogRow>, sqlx::Error> {
    // Build dynamic query with optional filters.
    let mut sql = String::from(
        "SELECT id, user_id, timestamp, action, target_kind, target_id, detail, ip, user_agent
         FROM audit_log WHERE 1=1",
    );
    let mut binds: Vec<String> = Vec::new();

    if let Some(v) = user_id {
        sql.push_str(" AND user_id = ?");
        binds.push(v.to_string());
    }
    if let Some(v) = action {
        sql.push_str(" AND action = ?");
        binds.push(v.to_string());
    }
    if let Some(v) = target_kind {
        sql.push_str(" AND target_kind = ?");
        binds.push(v.to_string());
    }
    if let Some(v) = since {
        sql.push_str(" AND timestamp >= ?");
        binds.push(v.to_string());
    }
    if let Some(v) = until {
        sql.push_str(" AND timestamp <= ?");
        binds.push(v.to_string());
    }
    if let Some(cursor) = before_cursor {
        sql.push_str(" AND timestamp < ?");
        binds.push(cursor.to_string());
    }

    sql.push_str(" ORDER BY timestamp DESC LIMIT ?");

    let mut query = sqlx::query_as::<_, AuditLogRow>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(limit);
    query.fetch_all(pool).await
}

pub async fn count_entries(pool: &DbPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}
