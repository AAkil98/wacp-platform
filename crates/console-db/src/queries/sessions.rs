use sqlx::FromRow;

use crate::DbPool;

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub id: String,
    pub name: Option<String>,
    pub owner_user_id: String,
    pub vertical: String,
    pub workflow: String,
    pub context: Option<String>,
    pub coordinator_workspace_id: Option<String>,
    pub state: String,
    pub created_at: String,
    pub launched_at: Option<String>,
    pub closed_at: Option<String>,
    pub budget_max_cost_micros: Option<i64>,
    pub budget_max_tokens: Option<i64>,
    pub budget_max_wall_time_ms: Option<i64>,
}

pub async fn get_by_id(pool: &DbPool, id: &str) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRow>("SELECT * FROM sessions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list_by_owner(
    pool: &DbPool,
    owner_user_id: &str,
    state: Option<&str>,
    limit: i64,
    after_cursor: Option<&str>,
) -> Result<Vec<SessionRow>, sqlx::Error> {
    let mut sql = String::from("SELECT * FROM sessions WHERE owner_user_id = ?");
    let mut binds = vec![owner_user_id.to_string()];

    if let Some(s) = state {
        sql.push_str(" AND state = ?");
        binds.push(s.to_string());
    }
    if let Some(cursor) = after_cursor {
        sql.push_str(" AND created_at < ?");
        binds.push(cursor.to_string());
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ?");

    let mut query = sqlx::query_as::<_, SessionRow>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(limit);
    query.fetch_all(pool).await
}

/// List all sessions (admin view).
pub async fn list_all(
    pool: &DbPool,
    state: Option<&str>,
    limit: i64,
    after_cursor: Option<&str>,
) -> Result<Vec<SessionRow>, sqlx::Error> {
    let mut sql = String::from("SELECT * FROM sessions WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();

    if let Some(s) = state {
        sql.push_str(" AND state = ?");
        binds.push(s.to_string());
    }
    if let Some(cursor) = after_cursor {
        sql.push_str(" AND created_at < ?");
        binds.push(cursor.to_string());
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ?");

    let mut query = sqlx::query_as::<_, SessionRow>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(limit);
    query.fetch_all(pool).await
}

pub async fn insert_session(pool: &DbPool, row: &SessionRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO sessions (id, name, owner_user_id, vertical, workflow, context,
         coordinator_workspace_id, state, created_at, launched_at, closed_at,
         budget_max_cost_micros, budget_max_tokens, budget_max_wall_time_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.name)
    .bind(&row.owner_user_id)
    .bind(&row.vertical)
    .bind(&row.workflow)
    .bind(&row.context)
    .bind(&row.coordinator_workspace_id)
    .bind(&row.state)
    .bind(&row.created_at)
    .bind(&row.launched_at)
    .bind(&row.closed_at)
    .bind(row.budget_max_cost_micros)
    .bind(row.budget_max_tokens)
    .bind(row.budget_max_wall_time_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// Transition session state. Returns false if current state doesn't match `from_state`.
pub async fn transition_state(
    pool: &DbPool,
    id: &str,
    from_state: &str,
    to_state: &str,
    now: &str,
) -> Result<bool, sqlx::Error> {
    // Determine which timestamp to set based on target state.
    let sql = match to_state {
        "active" => {
            "UPDATE sessions SET state = ?, launched_at = ? WHERE id = ? AND state = ?"
        }
        "completed" | "failed" | "cancelled" => {
            "UPDATE sessions SET state = ?, closed_at = ? WHERE id = ? AND state = ?"
        }
        _ => "UPDATE sessions SET state = ? WHERE id = ? AND state = ?",
    };

    let result = match to_state {
        "active" | "completed" | "failed" | "cancelled" => {
            sqlx::query(sql)
                .bind(to_state)
                .bind(now)
                .bind(id)
                .bind(from_state)
                .execute(pool)
                .await?
        }
        _ => {
            sqlx::query(sql)
                .bind(to_state)
                .bind(id)
                .bind(from_state)
                .execute(pool)
                .await?
        }
    };
    Ok(result.rows_affected() > 0)
}

/// Cancel from any non-terminal state.
pub async fn cancel(pool: &DbPool, id: &str, now: &str) -> Result<Option<String>, sqlx::Error> {
    // Get current state first to determine cleanup action.
    let session = get_by_id(pool, id).await?;
    let Some(session) = session else {
        return Ok(None);
    };

    match session.state.as_str() {
        "completed" | "failed" | "cancelled" => Ok(None), // already terminal
        _ => {
            let result = sqlx::query(
                "UPDATE sessions SET state = 'cancelled', closed_at = ?
                 WHERE id = ? AND state NOT IN ('completed', 'failed', 'cancelled')",
            )
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
            if result.rows_affected() > 0 {
                Ok(Some(session.state))
            } else {
                Ok(None)
            }
        }
    }
}

pub async fn set_coordinator_workspace_id(
    pool: &DbPool,
    id: &str,
    workspace_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE sessions SET coordinator_workspace_id = ? WHERE id = ?",
    )
    .bind(workspace_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_context(
    pool: &DbPool,
    id: &str,
    context: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE sessions SET context = ? WHERE id = ? AND state = 'configuring'",
    )
    .bind(context)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Find active sessions (for recovery on startup).
pub async fn list_active(pool: &DbPool) -> Result<Vec<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRow>("SELECT * FROM sessions WHERE state = 'active'")
        .fetch_all(pool)
        .await
}

pub async fn count_by_state(pool: &DbPool, state: &str) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE state = ?")
        .bind(state)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}
