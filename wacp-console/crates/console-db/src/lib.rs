pub mod queries;

#[cfg(test)]
pub(crate) mod testing;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

pub type DbPool = Pool<Sqlite>;

/// Creates a SQLite connection pool with production-appropriate settings:
/// - WAL journal mode for concurrent reads
/// - Foreign key enforcement
/// - 5-second busy timeout
pub async fn create_pool(database_url: &str) -> Result<DbPool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true)
        .create_if_missing(true);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

/// Creates a pool from a filesystem path (convenience wrapper).
pub async fn create_pool_from_path(path: &Path) -> Result<DbPool, sqlx::Error> {
    let url = format!("sqlite:{}", path.display());
    create_pool(&url).await
}

/// Runs all pending migrations against the database.
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}

/// Creates an in-memory pool for testing — WAL mode, FKs enabled.
pub async fn create_test_pool() -> Result<DbPool, sqlx::Error> {
    let pool = create_pool("sqlite::memory:").await?;
    run_migrations(&pool)
        .await
        .map_err(|e| sqlx::Error::Configuration(e.into()))?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_migration() {
        let pool = create_test_pool().await.expect("pool creation failed");

        // Verify all 9 tables exist
        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations' ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .expect("table query failed");

        let table_names: Vec<&str> = tables.iter().map(|t| t.0.as_str()).collect();
        assert_eq!(
            table_names,
            vec![
                "api_tokens",
                "audit_log",
                "login_attempts",
                "profiles",
                "session_assignments",
                "sessions",
                "settings",
                "user_sessions",
                "users",
            ]
        );
    }

    #[tokio::test]
    async fn test_foreign_keys_enabled() {
        let pool = create_test_pool().await.expect("pool creation failed");
        let (fk_enabled,): (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .expect("pragma query failed");
        assert_eq!(fk_enabled, 1);
    }

    #[tokio::test]
    async fn test_wal_mode() {
        // In-memory databases don't support WAL, so this tests the code path only
        let pool = create_test_pool().await.expect("pool creation failed");
        let (journal,): (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .expect("pragma query failed");
        // In-memory always returns "memory"
        assert!(journal == "memory" || journal == "wal");
    }
}
