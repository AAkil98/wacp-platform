use sqlx::FromRow;

use crate::DbPool;

#[derive(Debug, Clone, FromRow)]
pub struct ProfileRow {
    pub id: String,
    pub version: i64,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<String>,
    pub role_ref: String,
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_temperature: Option<f64>,
    pub llm_max_tokens: Option<i64>,
    pub autonomy: String,
    pub tool_allowlist: Option<String>,
    pub tool_denylist: Option<String>,
    pub budget_max_cost_micros: Option<i64>,
    pub budget_max_tokens: Option<i64>,
    pub budget_max_wall_time_ms: Option<i64>,
    pub budget_warning_threshold: Option<f64>,
    pub owner_user_id: String,
    pub visibility: String,
    pub is_current: bool,
    pub created_at: String,
    pub deleted_at: Option<String>,
}

/// Get the current (live) version of a profile.
pub async fn get_current(pool: &DbPool, id: &str) -> Result<Option<ProfileRow>, sqlx::Error> {
    sqlx::query_as::<_, ProfileRow>(
        "SELECT * FROM profiles WHERE id = ? AND is_current = 1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Get a specific version (for history, session assignment FK).
pub async fn get_version(
    pool: &DbPool,
    id: &str,
    version: i64,
) -> Result<Option<ProfileRow>, sqlx::Error> {
    sqlx::query_as::<_, ProfileRow>("SELECT * FROM profiles WHERE id = ? AND version = ?")
        .bind(id)
        .bind(version)
        .fetch_optional(pool)
        .await
}

/// List live profiles visible to the given user.
/// Admins see all; others see own + shared.
pub async fn list_visible(
    pool: &DbPool,
    viewer_user_id: &str,
    is_admin: bool,
    role_ref: Option<&str>,
    search: Option<&str>,
    limit: i64,
    after_cursor: Option<&str>,
) -> Result<Vec<ProfileRow>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT * FROM profiles WHERE is_current = 1 AND deleted_at IS NULL",
    );
    let mut binds: Vec<String> = Vec::new();

    if !is_admin {
        sql.push_str(" AND (owner_user_id = ? OR visibility = 'shared')");
        binds.push(viewer_user_id.to_string());
    }
    if let Some(role) = role_ref {
        sql.push_str(" AND role_ref = ?");
        binds.push(role.to_string());
    }
    if let Some(q) = search {
        sql.push_str(" AND (name LIKE ? OR description LIKE ?)");
        let pattern = format!("%{q}%");
        binds.push(pattern.clone());
        binds.push(pattern);
    }
    if let Some(cursor) = after_cursor {
        sql.push_str(" AND name > ?");
        binds.push(cursor.to_string());
    }
    sql.push_str(" ORDER BY name ASC LIMIT ?");

    let mut query = sqlx::query_as::<_, ProfileRow>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(limit);
    query.fetch_all(pool).await
}

/// List all versions of a profile (history view).
pub async fn list_versions(pool: &DbPool, id: &str) -> Result<Vec<ProfileRow>, sqlx::Error> {
    sqlx::query_as::<_, ProfileRow>(
        "SELECT * FROM profiles WHERE id = ? ORDER BY version DESC",
    )
    .bind(id)
    .fetch_all(pool)
    .await
}

/// Check name uniqueness for a user (excluding soft-deleted and self).
pub async fn name_exists_for_user(
    pool: &DbPool,
    name: &str,
    owner_user_id: &str,
    exclude_id: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let mut sql = String::from(
        "SELECT COUNT(*) FROM profiles
         WHERE name = ? AND owner_user_id = ? AND is_current = 1 AND deleted_at IS NULL",
    );
    let mut binds = vec![name.to_string(), owner_user_id.to_string()];

    if let Some(eid) = exclude_id {
        sql.push_str(" AND id != ?");
        binds.push(eid.to_string());
    }

    let mut query = sqlx::query_as::<_, (i64,)>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    let row = query.fetch_one(pool).await?;
    Ok(row.0 > 0)
}

/// Insert a new profile (version 1, is_current = true).
pub async fn insert_profile(pool: &DbPool, row: &ProfileRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO profiles (id, version, name, description, tags, role_ref,
         llm_provider, llm_model, llm_temperature, llm_max_tokens, autonomy,
         tool_allowlist, tool_denylist, budget_max_cost_micros, budget_max_tokens,
         budget_max_wall_time_ms, budget_warning_threshold, owner_user_id, visibility,
         is_current, created_at, deleted_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(row.version)
    .bind(&row.name)
    .bind(&row.description)
    .bind(&row.tags)
    .bind(&row.role_ref)
    .bind(&row.llm_provider)
    .bind(&row.llm_model)
    .bind(row.llm_temperature)
    .bind(row.llm_max_tokens)
    .bind(&row.autonomy)
    .bind(&row.tool_allowlist)
    .bind(&row.tool_denylist)
    .bind(row.budget_max_cost_micros)
    .bind(row.budget_max_tokens)
    .bind(row.budget_max_wall_time_ms)
    .bind(row.budget_warning_threshold)
    .bind(&row.owner_user_id)
    .bind(&row.visibility)
    .bind(row.is_current)
    .bind(&row.created_at)
    .bind(&row.deleted_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a new version: unset is_current on old, insert new row.
/// Must be called within a transaction.
pub async fn create_new_version(
    pool: &DbPool,
    id: &str,
    new_row: &ProfileRow,
) -> Result<(), sqlx::Error> {
    // Unset current on all existing versions
    sqlx::query("UPDATE profiles SET is_current = 0 WHERE id = ? AND is_current = 1")
        .bind(id)
        .execute(pool)
        .await?;

    insert_profile(pool, new_row).await
}

/// Soft-delete a profile (all versions).
pub async fn soft_delete(pool: &DbPool, id: &str, now: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE profiles SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Get the max version number for a profile.
pub async fn max_version(pool: &DbPool, id: &str) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT MAX(version) FROM profiles WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}
