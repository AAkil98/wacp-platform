use sqlx::FromRow;

use crate::DbPool;

#[derive(Debug, Clone, FromRow)]
pub struct SessionAssignmentRow {
    pub id: String,
    pub session_id: String,
    pub role_ref: String,
    pub stage_id: Option<String>,
    pub slot_position: i64,
    pub profile_id: Option<String>,
    pub profile_version: Option<i64>,
    pub workspace_id: Option<String>,
    pub budget_max_cost_micros: Option<i64>,
    pub budget_max_tokens: Option<i64>,
    pub budget_max_wall_time_ms: Option<i64>,
}

pub async fn list_by_session(
    pool: &DbPool,
    session_id: &str,
) -> Result<Vec<SessionAssignmentRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionAssignmentRow>(
        "SELECT * FROM session_assignments WHERE session_id = ? ORDER BY slot_position ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
}

pub async fn get_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<SessionAssignmentRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionAssignmentRow>("SELECT * FROM session_assignments WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn insert_assignment(
    pool: &DbPool,
    row: &SessionAssignmentRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO session_assignments (id, session_id, role_ref, stage_id, slot_position,
         profile_id, profile_version, workspace_id, budget_max_cost_micros,
         budget_max_tokens, budget_max_wall_time_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.session_id)
    .bind(&row.role_ref)
    .bind(&row.stage_id)
    .bind(row.slot_position)
    .bind(&row.profile_id)
    .bind(row.profile_version)
    .bind(&row.workspace_id)
    .bind(row.budget_max_cost_micros)
    .bind(row.budget_max_tokens)
    .bind(row.budget_max_wall_time_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// Replace all assignments for a session (bulk update during configuring).
pub async fn replace_assignments(
    pool: &DbPool,
    session_id: &str,
    rows: &[SessionAssignmentRow],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM session_assignments WHERE session_id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;

    for row in rows {
        insert_assignment(pool, row).await?;
    }
    Ok(())
}

pub async fn set_workspace_id(
    pool: &DbPool,
    id: &str,
    workspace_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE session_assignments SET workspace_id = ? WHERE id = ?")
        .bind(workspace_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Find non-terminal sessions that reference a given profile.
pub async fn find_active_sessions_for_profile(
    pool: &DbPool,
    profile_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT sa.session_id FROM session_assignments sa
         JOIN sessions s ON s.id = sa.session_id
         WHERE sa.profile_id = ? AND s.state NOT IN ('completed', 'failed', 'cancelled')",
    )
    .bind(profile_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn count_assigned(pool: &DbPool, session_id: &str) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM session_assignments
         WHERE session_id = ? AND profile_id IS NOT NULL",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}
