//! Test-support helpers: fault-injection harnesses for query-layer tests.
//!
//! Gated behind `#[cfg(test)]` so the helpers compile only in test builds. If a
//! downstream crate ever needs them, promote to a `test-util` feature.
//!
//! Surfaces:
//! - [`FaultyDb`] — on-disk SQLite DB with a companion pool that can hold
//!   exclusive write locks on demand, driving `SQLITE_BUSY` into the
//!   production pool under test.
//! - [`closed_pool`] — a pool that has been closed; all subsequent operations
//!   return `sqlx::Error::PoolClosed`. Models "connection dropped" at the sqlx
//!   layer without requiring a mid-flight socket break.
//! - [`parallel_writes`] — fans out N copies of a write closure against the
//!   same pool so tests can assert last-writer-wins / WHERE-guard semantics.

#![allow(dead_code)] // helpers are consumed from sibling test modules; cfg(test) makes
// the dead-code lint misreport coverage during partial builds.

use crate::{DbPool, run_migrations};
use sqlx::Connection;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use std::future::Future;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tempfile::TempDir;

/// An on-disk SQLite database with migrations applied, plus a companion pool
/// the test can use to hold a reserved write lock.
///
/// The main [`pool`](Self::pool) is configured with a short `busy_timeout`
/// (50 ms) so contention surfaces as `SQLITE_BUSY` within the test window
/// instead of blocking up to the production default of 5 s.
pub struct FaultyDb {
    pub pool: DbPool,
    locker: DbPool,
    _dir: TempDir,
}

impl FaultyDb {
    /// Create a fresh file-backed DB, apply migrations, and open two pools
    /// against it: one for the code under test, one for the lock-holding
    /// companion.
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path: PathBuf = dir.path().join("console.db");
        let url = format!("sqlite:{}", db_path.display());

        let make_pool = |timeout: Duration| {
            let url = url.clone();
            async move {
                let options = SqliteConnectOptions::from_str(&url)
                    .expect("options")
                    .journal_mode(SqliteJournalMode::Wal)
                    .synchronous(SqliteSynchronous::Normal)
                    .busy_timeout(timeout)
                    .foreign_keys(true)
                    .create_if_missing(true);
                SqlitePoolOptions::new()
                    .max_connections(4)
                    .connect_with(options)
                    .await
                    .expect("pool")
            }
        };

        let pool = make_pool(Duration::from_millis(50)).await;
        let locker = make_pool(Duration::from_secs(5)).await;
        run_migrations(&pool).await.expect("migrate");

        Self {
            pool,
            locker,
            _dir: dir,
        }
    }

    /// Acquire and hold a reserved write lock via `BEGIN IMMEDIATE` on a
    /// detached connection. While the returned [`BusyLock`] is alive, any
    /// write on [`Self::pool`] returns `SQLITE_BUSY` after the 50 ms
    /// `busy_timeout`. Drop the lock to release — detaching ensures the
    /// underlying handle is closed (not returned to the pool still holding
    /// the transaction), so SQLite releases the reserved lock immediately.
    pub async fn hold_write_lock(&self) -> BusyLock {
        let pooled = self.locker.acquire().await.expect("locker acquire");
        let mut conn = pooled.detach();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut conn)
            .await
            .expect("begin immediate");
        BusyLock { conn: Some(conn) }
    }
}

/// Guard for a held SQLite reserved write lock.
///
/// Dropping the guard closes the underlying (detached) connection, which
/// releases the reserved lock. Prefer [`release`](Self::release) when a test
/// wants to observe the rollback result.
pub struct BusyLock {
    conn: Option<SqliteConnection>,
}

impl BusyLock {
    /// Explicitly roll back and close the connection. Tests that want to
    /// assert on the rollback error path use this; otherwise `drop(lock)` is
    /// sufficient.
    pub async fn release(mut self) -> Result<(), sqlx::Error> {
        if let Some(mut conn) = self.conn.take() {
            sqlx::query("ROLLBACK").execute(&mut conn).await?;
            conn.close().await?;
        }
        Ok(())
    }
}

/// Return a pool that has been closed. Every subsequent query returns
/// `sqlx::Error::PoolClosed`. Models "dropped connection" at the sqlx level —
/// the production caller observes the same error class whether the physical
/// socket dropped or the pool was shut down by the runtime.
pub async fn closed_pool() -> DbPool {
    let pool = crate::create_test_pool().await.expect("pool");
    pool.close().await;
    pool
}

/// Fan N copies of `make_fut(i)` against `pool` in parallel via `tokio::spawn`
/// and return the full result vector in spawn order.
///
/// Useful for asserting last-writer-wins semantics on WHERE-guarded updates:
/// e.g. two tasks each call `transition_state(from=A, to=B)` — exactly one
/// returns `Ok(true)`, the other `Ok(false)`.
pub async fn parallel_writes<T, F, Fut>(
    pool: &DbPool,
    n: usize,
    make_fut: F,
) -> Vec<Result<T, sqlx::Error>>
where
    T: Send + 'static,
    F: Fn(DbPool, usize) -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>> + Send + 'static,
{
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        let fut = make_fut(pool.clone(), i);
        handles.push(tokio::spawn(fut));
    }
    let mut out = Vec::with_capacity(n);
    for h in handles {
        out.push(h.await.expect("task join"));
    }
    out
}

// ---------------------------------------------------------------------------
// Self-tests for the harness itself.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod self_tests {
    use super::*;

    #[tokio::test]
    async fn faulty_db_boots_and_migrates() {
        let db = FaultyDb::new().await;
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(n, 9);
    }

    #[tokio::test]
    async fn write_lock_produces_busy_and_recovers_on_release() {
        let db = FaultyDb::new().await;
        let lock = db.hold_write_lock().await;

        // While lock is held, a write on the main pool should return BUSY
        // within ~50 ms.
        let err = sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('k', 'v', '2026-04-16')",
        )
        .execute(&db.pool)
        .await
        .unwrap_err();

        let code = err.as_database_error().and_then(|e| e.code());
        assert_eq!(
            code.as_deref(),
            Some("5"),
            "expected SQLITE_BUSY (code 5), got {err:?}"
        );

        drop(lock);

        // After release, the write succeeds.
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('k', 'v', '2026-04-16')",
        )
        .execute(&db.pool)
        .await
        .expect("post-release write");
    }

    #[tokio::test]
    async fn closed_pool_returns_pool_closed_error() {
        let pool = closed_pool().await;
        let err = sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .expect_err("closed pool must error");
        assert!(
            matches!(err, sqlx::Error::PoolClosed),
            "expected PoolClosed, got {err:?}"
        );
    }

    #[tokio::test]
    async fn parallel_writes_race_to_where_guarded_update() {
        // Seed a row, then race three parallel "transition configuring→launching".
        // Exactly one should report rows_affected>0.
        let pool = crate::create_test_pool().await.unwrap();
        let now = "2026-04-16T00:00:00Z";
        sqlx::query("INSERT INTO users (id, username, username_lower, display_name, password_hash, console_role, must_change_password, created_at, updated_at) VALUES ('u', 'u', 'u', 'u', '$', 'admin', 0, ?, ?)")
            .bind(now).bind(now).execute(&pool).await.unwrap();
        let row = crate::queries::sessions::SessionRow {
            id: "s".into(),
            name: None,
            owner_user_id: "u".into(),
            vertical: "v".into(),
            workflow: "w".into(),
            context: None,
            coordinator_workspace_id: None,
            state: "configuring".into(),
            created_at: now.into(),
            launched_at: None,
            closed_at: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        crate::queries::sessions::insert_session(&pool, &row)
            .await
            .unwrap();

        let results = parallel_writes(&pool, 3, |p, _| async move {
            crate::queries::sessions::transition_state(
                &p,
                "s",
                "configuring",
                "validating",
                "2026-04-16T00:00:00Z",
            )
            .await
        })
        .await;

        let wins = results
            .into_iter()
            .filter(|r| matches!(r, Ok(true)))
            .count();
        assert_eq!(wins, 1, "exactly one transition must win");
    }
}
