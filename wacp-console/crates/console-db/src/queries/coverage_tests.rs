//! Coverage sweep for `console-db` — audit §12.2 T11 / §13.7.5.
//!
//! Goals:
//! - Fill the branch/line coverage gaps identified at the §13.7.5 kickoff
//!   baseline (55.64 % region, 63.43 % line).
//! - Exercise every production query on its negative-path surfaces:
//!   dropped-connection errors, UNIQUE/FK/CHECK violations, SQLITE_BUSY under
//!   contention, and WHERE-guarded serialization between concurrent writers.
//!
//! Organized as sibling submodules, one per query module, so failures point
//! at the right surface without scrolling a 1k-line file.

use crate::queries::{
    api_tokens, audit_log, login_attempts, profiles, session_assignments, sessions, settings,
    user_sessions, users,
};
use crate::testing::{FaultyDb, closed_pool, parallel_writes};
use crate::{DbPool, create_test_pool};
use sqlx::error::ErrorKind;

// ---------------------------------------------------------------------------
// Shared fixture helpers.
// ---------------------------------------------------------------------------

const NOW: &str = "2026-04-16T00:00:00Z";

async fn seed_user(pool: &DbPool, id: &str, username: &str) {
    users::insert_user(pool, id, username, username, "$hash$", "admin", false, NOW)
        .await
        .expect("seed user");
}

fn sample_profile(id: &str, version: i64, owner: &str, name: &str) -> profiles::ProfileRow {
    profiles::ProfileRow {
        id: id.into(),
        version,
        name: name.into(),
        description: None,
        tags: None,
        role_ref: "developer".into(),
        llm_provider: "anthropic".into(),
        llm_model: "claude-sonnet-4-6".into(),
        llm_temperature: None,
        llm_max_tokens: None,
        autonomy: "autonomous".into(),
        tool_allowlist: None,
        tool_denylist: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
        budget_warning_threshold: None,
        owner_user_id: owner.into(),
        visibility: "private".into(),
        is_current: true,
        created_at: NOW.into(),
        deleted_at: None,
    }
}

fn sample_session(id: &str, owner: &str) -> sessions::SessionRow {
    sessions::SessionRow {
        id: id.into(),
        name: Some(format!("sess-{id}")),
        owner_user_id: owner.into(),
        vertical: "fixture-simple".into(),
        workflow: "standard-dev".into(),
        context: None,
        coordinator_workspace_id: None,
        state: "configuring".into(),
        created_at: NOW.into(),
        launched_at: None,
        closed_at: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    }
}

fn sample_assignment(
    id: &str,
    session_id: &str,
    profile_id: &str,
    profile_version: i64,
    slot: i64,
) -> session_assignments::SessionAssignmentRow {
    session_assignments::SessionAssignmentRow {
        id: id.into(),
        session_id: session_id.into(),
        role_ref: "developer".into(),
        stage_id: None,
        slot_position: slot,
        profile_id: Some(profile_id.into()),
        profile_version: Some(profile_version),
        workspace_id: None,
        budget_max_cost_micros: None,
        budget_max_tokens: None,
        budget_max_wall_time_ms: None,
    }
}

fn expect_kind(err: &sqlx::Error, want: ErrorKind) {
    let kind = err
        .as_database_error()
        .map(|e| e.kind())
        .unwrap_or_else(|| panic!("expected database error, got {err:?}"));
    assert_eq!(kind, want, "wrong error kind: got {kind:?} in {err:?}");
}

fn expect_pool_closed(err: &sqlx::Error) {
    assert!(
        matches!(err, sqlx::Error::PoolClosed),
        "expected PoolClosed, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// lib.rs: coverage for create_pool_from_path.
// ---------------------------------------------------------------------------

mod lib_rs {
    use crate::{create_pool_from_path, run_migrations};

    #[tokio::test]
    async fn create_pool_from_path_opens_and_migrates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.db");
        let pool = create_pool_from_path(&path).await.expect("pool");
        run_migrations(&pool).await.expect("migrate");
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}

// ---------------------------------------------------------------------------
// settings
// ---------------------------------------------------------------------------

mod settings_tests {
    use super::*;

    #[tokio::test]
    async fn get_all_settings_returns_rows_sorted() {
        let pool = create_test_pool().await.unwrap();
        settings::upsert_setting(&pool, "b", "2", NOW)
            .await
            .unwrap();
        settings::upsert_setting(&pool, "a", "1", NOW)
            .await
            .unwrap();
        let rows = settings::get_all_settings(&pool).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].key, "a");
        assert_eq!(rows[1].key, "b");
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(&settings::get_setting(&pool, "k").await.unwrap_err());
        expect_pool_closed(&settings::get_all_settings(&pool).await.unwrap_err());
        expect_pool_closed(
            &settings::upsert_setting(&pool, "k", "v", NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&settings::delete_setting(&pool, "k").await.unwrap_err());
    }

    #[tokio::test]
    async fn busy_pool_errors_on_write() {
        let db = FaultyDb::new().await;
        let lock = db.hold_write_lock().await;
        let err = settings::upsert_setting(&db.pool, "k", "v", NOW)
            .await
            .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5"),
            "expected SQLITE_BUSY, got {err:?}"
        );
        drop(lock);
        // Recovery: write succeeds after the lock is released.
        settings::upsert_setting(&db.pool, "k", "v", NOW)
            .await
            .expect("post-release");
    }

    #[tokio::test]
    async fn parallel_upsert_last_writer_wins() {
        let pool = create_test_pool().await.unwrap();
        let results = parallel_writes(&pool, 8, |p, i| async move {
            settings::upsert_setting(&p, "k", &format!("v{i}"), NOW).await
        })
        .await;
        // All 8 upserts must succeed (serialised by SQLite's writer lock).
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 8);
        // Value is whichever task won the race; just confirm one of the
        // candidates ended up persisted.
        let row = settings::get_setting(&pool, "k").await.unwrap().unwrap();
        assert!(row.value.starts_with('v'));
    }
}

// ---------------------------------------------------------------------------
// users
// ---------------------------------------------------------------------------

mod users_tests {
    use super::*;

    #[tokio::test]
    async fn list_users_honors_include_disabled_and_cursor() {
        let pool = create_test_pool().await.unwrap();
        for (i, name) in ["alice", "bob", "carol"].iter().enumerate() {
            users::insert_user(
                &pool,
                &format!("u{i}"),
                name,
                name,
                "$h$",
                "operator",
                false,
                NOW,
            )
            .await
            .unwrap();
        }
        users::disable_user(&pool, "u1", NOW).await.unwrap();

        // include_disabled=false skips bob
        let active = users::list_users(&pool, false, 10, None).await.unwrap();
        let names: Vec<&str> = active.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, vec!["alice", "carol"]);

        // include_disabled=true picks up all three
        let all = users::list_users(&pool, true, 10, None).await.unwrap();
        assert_eq!(all.len(), 3);

        // cursor: usernames > "alice" → bob, carol (include_disabled=true)
        let after = users::list_users(&pool, true, 10, Some("alice"))
            .await
            .unwrap();
        let after_names: Vec<&str> = after.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(after_names, vec!["bob", "carol"]);

        // limit respected
        let one = users::list_users(&pool, true, 1, None).await.unwrap();
        assert_eq!(one.len(), 1);
    }

    #[tokio::test]
    async fn update_role_and_set_must_change_password_paths() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        assert!(
            users::update_role(&pool, "u1", "operator", NOW)
                .await
                .unwrap()
        );
        assert_eq!(
            users::get_by_id(&pool, "u1")
                .await
                .unwrap()
                .unwrap()
                .console_role,
            "operator"
        );
        // missing id → false
        assert!(
            !users::update_role(&pool, "missing", "operator", NOW)
                .await
                .unwrap()
        );

        assert!(
            users::set_must_change_password(&pool, "u1", NOW)
                .await
                .unwrap()
        );
        assert!(
            users::get_by_id(&pool, "u1")
                .await
                .unwrap()
                .unwrap()
                .must_change_password
        );
        assert!(
            !users::set_must_change_password(&pool, "missing", NOW)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn update_password_and_disable_missing_id_returns_false() {
        let pool = create_test_pool().await.unwrap();
        assert!(
            !users::update_password(&pool, "missing", "$h$", NOW)
                .await
                .unwrap()
        );
        assert!(!users::disable_user(&pool, "missing", NOW).await.unwrap());
        assert!(!users::enable_user(&pool, "missing", NOW).await.unwrap());
    }

    #[tokio::test]
    async fn enable_is_no_op_when_already_enabled() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        // Not currently disabled — enable returns false.
        assert!(!users::enable_user(&pool, "u1", NOW).await.unwrap());
    }

    #[tokio::test]
    async fn unique_username_lower_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "admin").await;
        let err = users::insert_user(&pool, "u2", "ADMIN", "ADMIN", "$h$", "admin", false, NOW)
            .await
            .unwrap_err();
        expect_kind(&err, ErrorKind::UniqueViolation);
    }

    #[tokio::test]
    async fn check_role_violation_rejects_unknown_role() {
        let pool = create_test_pool().await.unwrap();
        let err = users::insert_user(&pool, "u1", "bad", "bad", "$h$", "hacker", false, NOW)
            .await
            .unwrap_err();
        expect_kind(&err, ErrorKind::CheckViolation);
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(&users::get_by_id(&pool, "u1").await.unwrap_err());
        expect_pool_closed(&users::get_by_username(&pool, "u").await.unwrap_err());
        expect_pool_closed(&users::list_users(&pool, false, 10, None).await.unwrap_err());
        expect_pool_closed(&users::count_users(&pool).await.unwrap_err());
        expect_pool_closed(&users::count_active_admins(&pool).await.unwrap_err());
        expect_pool_closed(
            &users::insert_user(&pool, "u", "u", "u", "$", "admin", false, NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &users::update_password(&pool, "u", "$", NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &users::update_role(&pool, "u", "admin", NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&users::disable_user(&pool, "u", NOW).await.unwrap_err());
        expect_pool_closed(&users::enable_user(&pool, "u", NOW).await.unwrap_err());
        expect_pool_closed(
            &users::set_must_change_password(&pool, "u", NOW)
                .await
                .unwrap_err(),
        );
    }

    #[tokio::test]
    async fn busy_on_write() {
        let db = FaultyDb::new().await;
        let lock = db.hold_write_lock().await;
        let err = users::insert_user(&db.pool, "u", "u", "u", "$", "admin", false, NOW)
            .await
            .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5")
        );
        drop(lock);
    }

    #[tokio::test]
    async fn parallel_disable_only_one_winner() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        let results = parallel_writes(&pool, 4, |p, _| async move {
            users::disable_user(&p, "u1", NOW).await
        })
        .await;
        let wins = results
            .into_iter()
            .filter(|r| matches!(r, Ok(true)))
            .count();
        assert_eq!(wins, 1, "exactly one disable_user should flip the row");
    }
}

// ---------------------------------------------------------------------------
// user_sessions
// ---------------------------------------------------------------------------

mod user_sessions_tests {
    use super::*;

    #[tokio::test]
    async fn delete_user_sessions_clears_and_counts() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        for i in 0..3 {
            user_sessions::insert_session(
                &pool,
                &format!("s{i}"),
                "u1",
                &format!("hash{i}"),
                "10.0.0.1",
                "test",
                NOW,
                "2026-04-17T00:00:00Z",
            )
            .await
            .unwrap();
        }
        let n = user_sessions::delete_user_sessions(&pool, "u1")
            .await
            .unwrap();
        assert_eq!(n, 3);
        // calling again on an empty set returns 0
        let n = user_sessions::delete_user_sessions(&pool, "u1")
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn cleanup_expired_deletes_only_stale() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        user_sessions::insert_session(
            &pool,
            "s1",
            "u1",
            "h1",
            "ip",
            "ua",
            NOW,
            "2026-04-15T00:00:00Z",
        )
        .await
        .unwrap();
        user_sessions::insert_session(
            &pool,
            "s2",
            "u1",
            "h2",
            "ip",
            "ua",
            NOW,
            "2026-05-01T00:00:00Z",
        )
        .await
        .unwrap();
        let n = user_sessions::cleanup_expired(&pool, "2026-04-16T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(n, 1);
        assert!(
            user_sessions::get_by_token_hash(&pool, "h1", NOW)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            user_sessions::get_by_token_hash(&pool, "h2", NOW)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn delete_missing_returns_false() {
        let pool = create_test_pool().await.unwrap();
        assert!(
            !user_sessions::delete_session(&pool, "missing")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn unique_token_hash_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        user_sessions::insert_session(
            &pool,
            "s1",
            "u1",
            "h1",
            "ip",
            "ua",
            NOW,
            "2026-04-20T00:00:00Z",
        )
        .await
        .unwrap();
        let err = user_sessions::insert_session(
            &pool,
            "s2",
            "u1",
            "h1",
            "ip",
            "ua",
            NOW,
            "2026-04-20T00:00:00Z",
        )
        .await
        .unwrap_err();
        expect_kind(&err, ErrorKind::UniqueViolation);
    }

    #[tokio::test]
    async fn fk_violation_missing_user() {
        let pool = create_test_pool().await.unwrap();
        let err = user_sessions::insert_session(
            &pool,
            "s1",
            "ghost",
            "h",
            "ip",
            "ua",
            NOW,
            "2026-04-20T00:00:00Z",
        )
        .await
        .unwrap_err();
        expect_kind(&err, ErrorKind::ForeignKeyViolation);
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(
            &user_sessions::get_by_token_hash(&pool, "h", NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &user_sessions::insert_session(&pool, "s", "u", "h", "ip", "ua", NOW, NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&user_sessions::delete_session(&pool, "s").await.unwrap_err());
        expect_pool_closed(
            &user_sessions::delete_user_sessions(&pool, "u")
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &user_sessions::cleanup_expired(&pool, NOW)
                .await
                .unwrap_err(),
        );
    }

    #[tokio::test]
    async fn busy_on_write() {
        let db = FaultyDb::new().await;
        seed_user(&db.pool, "u1", "a").await;
        let lock = db.hold_write_lock().await;
        let err = user_sessions::insert_session(
            &db.pool,
            "s1",
            "u1",
            "h1",
            "ip",
            "ua",
            NOW,
            "2026-04-20T00:00:00Z",
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5")
        );
        drop(lock);
    }
}

// ---------------------------------------------------------------------------
// api_tokens
// ---------------------------------------------------------------------------

mod api_tokens_tests {
    use super::*;

    #[tokio::test]
    async fn get_by_id_and_update_last_used_cover_both_paths() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        api_tokens::insert_token(&pool, "t1", "u1", "n", "h1", NOW, None)
            .await
            .unwrap();

        let tok = api_tokens::get_by_id(&pool, "t1").await.unwrap().unwrap();
        assert_eq!(tok.name, "n");
        assert!(
            api_tokens::get_by_id(&pool, "missing")
                .await
                .unwrap()
                .is_none()
        );

        api_tokens::update_last_used(&pool, "t1", NOW)
            .await
            .unwrap();
        let tok = api_tokens::get_by_id(&pool, "t1").await.unwrap().unwrap();
        assert_eq!(tok.last_used_at.as_deref(), Some(NOW));

        // update_last_used on a missing id is a no-op (no rows affected, no error).
        api_tokens::update_last_used(&pool, "missing", NOW)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_by_user_excludes_revoked_tokens() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        api_tokens::insert_token(&pool, "t1", "u1", "active", "h1", NOW, None)
            .await
            .unwrap();
        api_tokens::insert_token(&pool, "t2", "u1", "tobe-revoked", "h2", NOW, None)
            .await
            .unwrap();
        api_tokens::revoke_token(&pool, "t2", NOW).await.unwrap();
        let list = api_tokens::list_by_user(&pool, "u1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "t1");
    }

    #[tokio::test]
    async fn expired_and_revoked_tokens_return_none_by_hash() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        // expired
        api_tokens::insert_token(
            &pool,
            "t1",
            "u1",
            "expired",
            "h1",
            NOW,
            Some("2026-04-15T00:00:00Z"),
        )
        .await
        .unwrap();
        assert!(
            api_tokens::get_by_token_hash(&pool, "h1", NOW)
                .await
                .unwrap()
                .is_none()
        );

        // active then revoked
        api_tokens::insert_token(&pool, "t2", "u1", "ok", "h2", NOW, None)
            .await
            .unwrap();
        assert!(
            api_tokens::get_by_token_hash(&pool, "h2", NOW)
                .await
                .unwrap()
                .is_some()
        );
        api_tokens::revoke_token(&pool, "t2", NOW).await.unwrap();
        assert!(
            api_tokens::get_by_token_hash(&pool, "h2", NOW)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn revoke_missing_returns_false_and_is_idempotent() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        api_tokens::insert_token(&pool, "t1", "u1", "n", "h", NOW, None)
            .await
            .unwrap();
        assert!(api_tokens::revoke_token(&pool, "t1", NOW).await.unwrap());
        // second revoke is a no-op (WHERE revoked_at IS NULL)
        assert!(!api_tokens::revoke_token(&pool, "t1", NOW).await.unwrap());
        // missing id also returns false
        assert!(
            !api_tokens::revoke_token(&pool, "missing", NOW)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn unique_token_hash_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        api_tokens::insert_token(&pool, "t1", "u1", "a", "hash", NOW, None)
            .await
            .unwrap();
        let err = api_tokens::insert_token(&pool, "t2", "u1", "b", "hash", NOW, None)
            .await
            .unwrap_err();
        expect_kind(&err, ErrorKind::UniqueViolation);
    }

    #[tokio::test]
    async fn fk_user_missing_violation() {
        let pool = create_test_pool().await.unwrap();
        let err = api_tokens::insert_token(&pool, "t1", "ghost", "a", "h", NOW, None)
            .await
            .unwrap_err();
        expect_kind(&err, ErrorKind::ForeignKeyViolation);
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(
            &api_tokens::get_by_token_hash(&pool, "h", NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&api_tokens::get_by_id(&pool, "t").await.unwrap_err());
        expect_pool_closed(&api_tokens::list_by_user(&pool, "u").await.unwrap_err());
        expect_pool_closed(
            &api_tokens::insert_token(&pool, "t", "u", "n", "h", NOW, None)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&api_tokens::revoke_token(&pool, "t", NOW).await.unwrap_err());
        expect_pool_closed(
            &api_tokens::update_last_used(&pool, "t", NOW)
                .await
                .unwrap_err(),
        );
    }

    #[tokio::test]
    async fn busy_on_write() {
        let db = FaultyDb::new().await;
        seed_user(&db.pool, "u1", "a").await;
        let lock = db.hold_write_lock().await;
        let err = api_tokens::insert_token(&db.pool, "t1", "u1", "n", "h1", NOW, None)
            .await
            .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5")
        );
        drop(lock);
    }

    #[tokio::test]
    async fn parallel_revoke_only_one_winner() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "u1", "a").await;
        api_tokens::insert_token(&pool, "t1", "u1", "n", "h", NOW, None)
            .await
            .unwrap();
        let results = parallel_writes(&pool, 4, |p, _| async move {
            api_tokens::revoke_token(&p, "t1", NOW).await
        })
        .await;
        let wins = results
            .into_iter()
            .filter(|r| matches!(r, Ok(true)))
            .count();
        assert_eq!(wins, 1);
    }
}

// ---------------------------------------------------------------------------
// profiles
// ---------------------------------------------------------------------------

mod profiles_tests {
    use super::*;

    async fn seed_two_users_with_profiles(pool: &DbPool) {
        seed_user(pool, "alice", "alice").await;
        seed_user(pool, "bob", "bob").await;
        // alice: 2 profiles, one shared, one private
        let mut p = sample_profile("pa1", 1, "alice", "Alice Dev");
        p.visibility = "shared".into();
        profiles::insert_profile(pool, &p).await.unwrap();
        let mut p = sample_profile("pa2", 1, "alice", "Alice QA");
        p.role_ref = "qa".into();
        profiles::insert_profile(pool, &p).await.unwrap();
        // bob: 1 private
        let p = sample_profile("pb1", 1, "bob", "Bob Dev");
        profiles::insert_profile(pool, &p).await.unwrap();
    }

    #[tokio::test]
    async fn list_visible_admin_sees_all_with_role_and_search_filters() {
        let pool = create_test_pool().await.unwrap();
        seed_two_users_with_profiles(&pool).await;
        // admin view: role-agnostic
        let all = profiles::list_visible(&pool, "alice", true, None, None, 10, None)
            .await
            .unwrap();
        assert_eq!(all.len(), 3);
        // role filter
        let qa = profiles::list_visible(&pool, "alice", true, Some("qa"), None, 10, None)
            .await
            .unwrap();
        assert_eq!(qa.len(), 1);
        assert_eq!(qa[0].id, "pa2");
        // search by name substring
        let dev = profiles::list_visible(&pool, "alice", true, None, Some("Dev"), 10, None)
            .await
            .unwrap();
        assert_eq!(dev.len(), 2); // "Alice Dev" + "Bob Dev"
    }

    #[tokio::test]
    async fn list_visible_non_admin_scopes_to_own_plus_shared() {
        let pool = create_test_pool().await.unwrap();
        seed_two_users_with_profiles(&pool).await;
        // bob (non-admin): sees own private + alice's shared
        let bob_view = profiles::list_visible(&pool, "bob", false, None, None, 10, None)
            .await
            .unwrap();
        let ids: Vec<&str> = bob_view.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"pa1"), "shared must be visible: {ids:?}");
        assert!(ids.contains(&"pb1"), "own must be visible: {ids:?}");
        assert!(
            !ids.contains(&"pa2"),
            "alice private must be hidden: {ids:?}"
        );
    }

    #[tokio::test]
    async fn list_visible_cursor_paginates_by_name() {
        let pool = create_test_pool().await.unwrap();
        seed_two_users_with_profiles(&pool).await;
        // cursor after "Alice Dev" — returns profiles with name > "Alice Dev"
        let rest = profiles::list_visible(&pool, "alice", true, None, None, 10, Some("Alice Dev"))
            .await
            .unwrap();
        let names: Vec<&str> = rest.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Alice QA", "Bob Dev"]);
    }

    #[tokio::test]
    async fn name_exists_for_user_respects_exclude_id() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let p = sample_profile("pa1", 1, "alice", "dup");
        profiles::insert_profile(&pool, &p).await.unwrap();
        // Without exclude — dup exists.
        assert!(
            profiles::name_exists_for_user(&pool, "dup", "alice", None)
                .await
                .unwrap()
        );
        // Excluding the owning id — treated as absent.
        assert!(
            !profiles::name_exists_for_user(&pool, "dup", "alice", Some("pa1"))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn max_version_covers_present_case() {
        // Note: for a missing profile, MAX() over the empty set returns NULL;
        // sqlx decodes that into i64 = 0 (not None), so the function's
        // row.map(...) None branch is effectively unreachable. Covered in the
        // perf-opt doc §9 as a data-model drift finding. We test the
        // reachable present-case here.
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let p = sample_profile("p1", 1, "alice", "n");
        profiles::insert_profile(&pool, &p).await.unwrap();
        let p2 = profiles::ProfileRow {
            version: 7,
            is_current: false,
            ..p.clone()
        };
        profiles::insert_profile(&pool, &p2).await.unwrap();
        assert_eq!(profiles::max_version(&pool, "p1").await.unwrap(), Some(7));
    }

    #[tokio::test]
    async fn check_autonomy_violation_rejected() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let mut p = sample_profile("p1", 1, "alice", "n");
        p.autonomy = "godmode".into();
        let err = profiles::insert_profile(&pool, &p).await.unwrap_err();
        expect_kind(&err, ErrorKind::CheckViolation);
    }

    #[tokio::test]
    async fn check_visibility_violation_rejected() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let mut p = sample_profile("p1", 1, "alice", "n");
        p.visibility = "public".into();
        let err = profiles::insert_profile(&pool, &p).await.unwrap_err();
        expect_kind(&err, ErrorKind::CheckViolation);
    }

    #[tokio::test]
    async fn fk_owner_user_violation() {
        let pool = create_test_pool().await.unwrap();
        let p = sample_profile("p1", 1, "ghost", "n");
        let err = profiles::insert_profile(&pool, &p).await.unwrap_err();
        expect_kind(&err, ErrorKind::ForeignKeyViolation);
    }

    #[tokio::test]
    async fn pk_duplicate_version_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let p = sample_profile("p1", 1, "alice", "n");
        profiles::insert_profile(&pool, &p).await.unwrap();
        // Same (id, version) pair — PK collision.
        let err = profiles::insert_profile(&pool, &p).await.unwrap_err();
        expect_kind(&err, ErrorKind::UniqueViolation);
    }

    #[tokio::test]
    async fn soft_delete_already_deleted_returns_false() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let p = sample_profile("p1", 1, "alice", "n");
        profiles::insert_profile(&pool, &p).await.unwrap();
        assert!(profiles::soft_delete(&pool, "p1", NOW).await.unwrap());
        assert!(!profiles::soft_delete(&pool, "p1", NOW).await.unwrap());
    }

    #[tokio::test]
    async fn get_version_missing_returns_none() {
        let pool = create_test_pool().await.unwrap();
        assert!(
            profiles::get_version(&pool, "p1", 1)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(&profiles::get_current(&pool, "p").await.unwrap_err());
        expect_pool_closed(&profiles::get_version(&pool, "p", 1).await.unwrap_err());
        expect_pool_closed(
            &profiles::list_visible(&pool, "u", false, None, None, 10, None)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&profiles::list_versions(&pool, "p").await.unwrap_err());
        expect_pool_closed(
            &profiles::name_exists_for_user(&pool, "n", "u", None)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&profiles::max_version(&pool, "p").await.unwrap_err());
        let row = sample_profile("p", 1, "u", "n");
        expect_pool_closed(&profiles::insert_profile(&pool, &row).await.unwrap_err());
        expect_pool_closed(
            &profiles::create_new_version(&pool, "p", &row)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&profiles::soft_delete(&pool, "p", NOW).await.unwrap_err());
    }

    #[tokio::test]
    async fn busy_on_write() {
        let db = FaultyDb::new().await;
        seed_user(&db.pool, "alice", "alice").await;
        let lock = db.hold_write_lock().await;
        let p = sample_profile("p1", 1, "alice", "n");
        let err = profiles::insert_profile(&db.pool, &p).await.unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5")
        );
        drop(lock);
    }

    #[tokio::test]
    async fn parallel_soft_delete_only_one_winner() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let p = sample_profile("p1", 1, "alice", "n");
        profiles::insert_profile(&pool, &p).await.unwrap();
        let results = parallel_writes(&pool, 4, |p, _| async move {
            profiles::soft_delete(&p, "p1", NOW).await
        })
        .await;
        let wins = results
            .into_iter()
            .filter(|r| matches!(r, Ok(true)))
            .count();
        assert_eq!(wins, 1);
    }
}

// ---------------------------------------------------------------------------
// sessions
// ---------------------------------------------------------------------------

mod sessions_tests {
    use super::*;

    async fn seed_sessions_for(pool: &DbPool, owner: &str, ids: &[&str]) {
        for id in ids {
            let mut row = sample_session(id, owner);
            row.created_at = format!(
                "2026-04-1{}T00:00:00Z",
                ids.iter().position(|x| x == id).unwrap()
            );
            sessions::insert_session(pool, &row).await.unwrap();
        }
    }

    #[tokio::test]
    async fn list_by_owner_with_state_and_cursor() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        seed_user(&pool, "bob", "bob").await;
        seed_sessions_for(&pool, "alice", &["s1", "s2", "s3"]).await;
        seed_sessions_for(&pool, "bob", &["s4"]).await;
        sessions::transition_state(&pool, "s2", "configuring", "validating", NOW)
            .await
            .unwrap();

        // alice has 3 sessions; bob's s4 must not leak in.
        let alice_all = sessions::list_by_owner(&pool, "alice", None, 10, None)
            .await
            .unwrap();
        assert_eq!(alice_all.len(), 3);
        assert!(alice_all.iter().all(|s| s.owner_user_id == "alice"));

        // filter by state "validating" -> only s2
        let v = sessions::list_by_owner(&pool, "alice", Some("validating"), 10, None)
            .await
            .unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "s2");

        // cursor: pick sessions created before "2026-04-11T00:00:00Z" — i.e. s1 only
        let page = sessions::list_by_owner(&pool, "alice", None, 10, Some("2026-04-11T00:00:00Z"))
            .await
            .unwrap();
        let ids: Vec<&str> = page.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s1"]);

        // limit honored
        let one = sessions::list_by_owner(&pool, "alice", None, 1, None)
            .await
            .unwrap();
        assert_eq!(one.len(), 1);
    }

    #[tokio::test]
    async fn list_all_admin_sees_everyone() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        seed_user(&pool, "bob", "bob").await;
        seed_sessions_for(&pool, "alice", &["s1"]).await;
        seed_sessions_for(&pool, "bob", &["s2"]).await;
        let all = sessions::list_all(&pool, None, 10, None).await.unwrap();
        assert_eq!(all.len(), 2);
        // with state filter
        let cfg = sessions::list_all(&pool, Some("configuring"), 10, None)
            .await
            .unwrap();
        assert_eq!(cfg.len(), 2);
        // with cursor — no matches before the very first
        let cursor = sessions::list_all(&pool, None, 10, Some("2025-01-01T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(cursor.len(), 0);
    }

    #[tokio::test]
    async fn update_context_only_in_configuring() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        seed_sessions_for(&pool, "alice", &["s1"]).await;
        assert!(
            sessions::update_context(&pool, "s1", "{\"k\":1}")
                .await
                .unwrap()
        );
        // move out of configuring, then update must fail
        sessions::transition_state(&pool, "s1", "configuring", "validating", NOW)
            .await
            .unwrap();
        assert!(
            !sessions::update_context(&pool, "s1", "{\"k\":2}")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn set_coordinator_workspace_id_covers_missing_and_present() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        seed_sessions_for(&pool, "alice", &["s1"]).await;
        assert!(
            sessions::set_coordinator_workspace_id(&pool, "s1", "ws1")
                .await
                .unwrap()
        );
        assert!(
            !sessions::set_coordinator_workspace_id(&pool, "missing", "ws1")
                .await
                .unwrap()
        );
        let s = sessions::get_by_id(&pool, "s1").await.unwrap().unwrap();
        assert_eq!(s.coordinator_workspace_id.as_deref(), Some("ws1"));
    }

    #[tokio::test]
    async fn list_active_and_count_by_state() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        seed_sessions_for(&pool, "alice", &["s1", "s2"]).await;
        sessions::transition_state(&pool, "s1", "configuring", "validating", NOW)
            .await
            .unwrap();
        sessions::transition_state(&pool, "s1", "validating", "launching", NOW)
            .await
            .unwrap();
        sessions::transition_state(&pool, "s1", "launching", "active", NOW)
            .await
            .unwrap();
        let active = sessions::list_active(&pool).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "s1");
        assert_eq!(
            sessions::count_by_state(&pool, "configuring")
                .await
                .unwrap(),
            1
        );
        assert_eq!(sessions::count_by_state(&pool, "active").await.unwrap(), 1);
        assert_eq!(
            sessions::count_by_state(&pool, "completed").await.unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn transition_to_completed_and_failed_sets_closed_at() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        seed_sessions_for(&pool, "alice", &["s1", "s2"]).await;
        // walk s1 to active → completed
        for (from, to) in [
            ("configuring", "validating"),
            ("validating", "launching"),
            ("launching", "active"),
            ("active", "completed"),
        ] {
            assert!(
                sessions::transition_state(&pool, "s1", from, to, NOW)
                    .await
                    .unwrap()
            );
        }
        let s = sessions::get_by_id(&pool, "s1").await.unwrap().unwrap();
        assert_eq!(s.state, "completed");
        assert!(s.closed_at.is_some());

        // walk s2 to launching → failed
        for (from, to) in [
            ("configuring", "validating"),
            ("validating", "launching"),
            ("launching", "failed"),
        ] {
            assert!(
                sessions::transition_state(&pool, "s2", from, to, NOW)
                    .await
                    .unwrap()
            );
        }
        let s = sessions::get_by_id(&pool, "s2").await.unwrap().unwrap();
        assert_eq!(s.state, "failed");
        assert!(s.closed_at.is_some());
    }

    #[tokio::test]
    async fn cancel_missing_or_terminal_returns_none() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        seed_sessions_for(&pool, "alice", &["s1"]).await;
        // non-existent session → None
        assert!(
            sessions::cancel(&pool, "missing", NOW)
                .await
                .unwrap()
                .is_none()
        );
        // walk s1 to completed
        for (from, to) in [
            ("configuring", "validating"),
            ("validating", "launching"),
            ("launching", "active"),
            ("active", "completed"),
        ] {
            sessions::transition_state(&pool, "s1", from, to, NOW)
                .await
                .unwrap();
        }
        // already-terminal → None
        assert!(sessions::cancel(&pool, "s1", NOW).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn cancel_from_configuring_returns_prev_state() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        seed_sessions_for(&pool, "alice", &["s1"]).await;
        let prev = sessions::cancel(&pool, "s1", NOW).await.unwrap();
        assert_eq!(prev.as_deref(), Some("configuring"));
    }

    #[tokio::test]
    async fn check_state_violation_rejected() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let mut row = sample_session("s1", "alice");
        row.state = "paused".into(); // not in CHECK list
        let err = sessions::insert_session(&pool, &row).await.unwrap_err();
        expect_kind(&err, ErrorKind::CheckViolation);
    }

    #[tokio::test]
    async fn fk_owner_violation() {
        let pool = create_test_pool().await.unwrap();
        let row = sample_session("s1", "ghost");
        let err = sessions::insert_session(&pool, &row).await.unwrap_err();
        expect_kind(&err, ErrorKind::ForeignKeyViolation);
    }

    #[tokio::test]
    async fn pk_duplicate_id_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let row = sample_session("s1", "alice");
        sessions::insert_session(&pool, &row).await.unwrap();
        let err = sessions::insert_session(&pool, &row).await.unwrap_err();
        expect_kind(&err, ErrorKind::UniqueViolation);
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(&sessions::get_by_id(&pool, "s").await.unwrap_err());
        expect_pool_closed(
            &sessions::list_by_owner(&pool, "u", None, 10, None)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&sessions::list_all(&pool, None, 10, None).await.unwrap_err());
        let row = sample_session("s", "u");
        expect_pool_closed(&sessions::insert_session(&pool, &row).await.unwrap_err());
        expect_pool_closed(
            &sessions::transition_state(&pool, "s", "configuring", "validating", NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&sessions::cancel(&pool, "s", NOW).await.unwrap_err());
        expect_pool_closed(
            &sessions::set_coordinator_workspace_id(&pool, "s", "w")
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &sessions::update_context(&pool, "s", "{}")
                .await
                .unwrap_err(),
        );
        expect_pool_closed(&sessions::list_active(&pool).await.unwrap_err());
        expect_pool_closed(&sessions::count_by_state(&pool, "active").await.unwrap_err());
    }

    #[tokio::test]
    async fn busy_on_write() {
        let db = FaultyDb::new().await;
        seed_user(&db.pool, "alice", "alice").await;
        let lock = db.hold_write_lock().await;
        let row = sample_session("s1", "alice");
        let err = sessions::insert_session(&db.pool, &row).await.unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5")
        );
        drop(lock);
    }

    #[tokio::test]
    async fn parallel_transition_only_one_winner() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let row = sample_session("s1", "alice");
        sessions::insert_session(&pool, &row).await.unwrap();

        let results = parallel_writes(&pool, 4, |p, _| async move {
            sessions::transition_state(&p, "s1", "configuring", "validating", NOW).await
        })
        .await;
        let wins = results
            .into_iter()
            .filter(|r| matches!(r, Ok(true)))
            .count();
        assert_eq!(wins, 1);
    }
}

// ---------------------------------------------------------------------------
// session_assignments
// ---------------------------------------------------------------------------

mod session_assignments_tests {
    use super::*;

    async fn seed_session_with_profile(pool: &DbPool) {
        seed_user(pool, "alice", "alice").await;
        let p = sample_profile("p1", 1, "alice", "n");
        profiles::insert_profile(pool, &p).await.unwrap();
        let s = sample_session("s1", "alice");
        sessions::insert_session(pool, &s).await.unwrap();
    }

    #[tokio::test]
    async fn insert_list_and_get_by_id() {
        let pool = create_test_pool().await.unwrap();
        seed_session_with_profile(&pool).await;
        let row = sample_assignment("a1", "s1", "p1", 1, 0);
        session_assignments::insert_assignment(&pool, &row)
            .await
            .unwrap();
        let list = session_assignments::list_by_session(&pool, "s1")
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        let got = session_assignments::get_by_id(&pool, "a1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.role_ref, "developer");
        assert!(
            session_assignments::get_by_id(&pool, "missing")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn list_by_session_orders_by_slot_position() {
        let pool = create_test_pool().await.unwrap();
        seed_session_with_profile(&pool).await;
        for (id, slot) in [("a1", 2), ("a2", 0), ("a3", 1)] {
            let r = sample_assignment(id, "s1", "p1", 1, slot);
            session_assignments::insert_assignment(&pool, &r)
                .await
                .unwrap();
        }
        let list = session_assignments::list_by_session(&pool, "s1")
            .await
            .unwrap();
        let slots: Vec<i64> = list.iter().map(|r| r.slot_position).collect();
        assert_eq!(slots, vec![0, 1, 2]);
    }

    #[tokio::test]
    async fn replace_assignments_deletes_then_inserts() {
        let pool = create_test_pool().await.unwrap();
        seed_session_with_profile(&pool).await;
        // initial: 2 rows
        for (id, slot) in [("a1", 0), ("a2", 1)] {
            let r = sample_assignment(id, "s1", "p1", 1, slot);
            session_assignments::insert_assignment(&pool, &r)
                .await
                .unwrap();
        }
        assert_eq!(
            session_assignments::count_assigned(&pool, "s1")
                .await
                .unwrap(),
            2
        );

        let replacement = vec![
            sample_assignment("b1", "s1", "p1", 1, 0),
            sample_assignment("b2", "s1", "p1", 1, 1),
            sample_assignment("b3", "s1", "p1", 1, 2),
        ];
        session_assignments::replace_assignments(&pool, "s1", &replacement)
            .await
            .unwrap();
        assert_eq!(
            session_assignments::count_assigned(&pool, "s1")
                .await
                .unwrap(),
            3
        );
        let ids: Vec<String> = session_assignments::list_by_session(&pool, "s1")
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["b1", "b2", "b3"]);
    }

    #[tokio::test]
    async fn set_workspace_id_handles_missing_and_present() {
        let pool = create_test_pool().await.unwrap();
        seed_session_with_profile(&pool).await;
        let r = sample_assignment("a1", "s1", "p1", 1, 0);
        session_assignments::insert_assignment(&pool, &r)
            .await
            .unwrap();
        assert!(
            session_assignments::set_workspace_id(&pool, "a1", "w1")
                .await
                .unwrap()
        );
        assert!(
            !session_assignments::set_workspace_id(&pool, "missing", "w1")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn find_active_sessions_for_profile_filters_out_terminal() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let p = sample_profile("p1", 1, "alice", "n");
        profiles::insert_profile(&pool, &p).await.unwrap();
        // s1 active, s2 completed
        let mut s1 = sample_session("s1", "alice");
        s1.state = "active".into();
        sessions::insert_session(&pool, &s1).await.unwrap();
        let mut s2 = sample_session("s2", "alice");
        s2.state = "completed".into();
        sessions::insert_session(&pool, &s2).await.unwrap();

        let a = sample_assignment("a1", "s1", "p1", 1, 0);
        session_assignments::insert_assignment(&pool, &a)
            .await
            .unwrap();
        let b = sample_assignment("a2", "s2", "p1", 1, 0);
        session_assignments::insert_assignment(&pool, &b)
            .await
            .unwrap();

        let active = session_assignments::find_active_sessions_for_profile(&pool, "p1")
            .await
            .unwrap();
        assert_eq!(active, vec!["s1".to_string()]);
    }

    #[tokio::test]
    async fn count_assigned_matches_inserted_row_count() {
        // The production query's `WHERE profile_id IS NOT NULL` clause is
        // defensive: the session_assignments schema enforces NOT NULL on
        // profile_id. So the "unassigned row" branch the query guards against
        // is unreachable against the current schema. See perf-opt doc §9 for
        // the spec-vs-schema drift note. We test the reachable count path here.
        let pool = create_test_pool().await.unwrap();
        seed_session_with_profile(&pool).await;
        for (id, slot) in [("a1", 0), ("a2", 1), ("a3", 2)] {
            let r = sample_assignment(id, "s1", "p1", 1, slot);
            session_assignments::insert_assignment(&pool, &r)
                .await
                .unwrap();
        }
        assert_eq!(
            session_assignments::count_assigned(&pool, "s1")
                .await
                .unwrap(),
            3
        );
    }

    #[tokio::test]
    async fn not_null_violation_when_profile_id_is_none() {
        // Covers the drift: the ProfileRow struct allows None, but the schema
        // rejects it. Exercising the rejection documents the invariant.
        let pool = create_test_pool().await.unwrap();
        seed_session_with_profile(&pool).await;
        let mut r = sample_assignment("a1", "s1", "p1", 1, 0);
        r.profile_id = None;
        r.profile_version = None;
        let err = session_assignments::insert_assignment(&pool, &r)
            .await
            .unwrap_err();
        expect_kind(&err, ErrorKind::NotNullViolation);
    }

    #[tokio::test]
    async fn fk_session_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let p = sample_profile("p1", 1, "alice", "n");
        profiles::insert_profile(&pool, &p).await.unwrap();
        let row = sample_assignment("a1", "ghost-session", "p1", 1, 0);
        let err = session_assignments::insert_assignment(&pool, &row)
            .await
            .unwrap_err();
        expect_kind(&err, ErrorKind::ForeignKeyViolation);
    }

    #[tokio::test]
    async fn fk_profile_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        let s = sample_session("s1", "alice");
        sessions::insert_session(&pool, &s).await.unwrap();
        let row = sample_assignment("a1", "s1", "ghost-profile", 1, 0);
        let err = session_assignments::insert_assignment(&pool, &row)
            .await
            .unwrap_err();
        expect_kind(&err, ErrorKind::ForeignKeyViolation);
    }

    #[tokio::test]
    async fn unique_slot_position_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_session_with_profile(&pool).await;
        let a = sample_assignment("a1", "s1", "p1", 1, 0);
        session_assignments::insert_assignment(&pool, &a)
            .await
            .unwrap();
        let dup = sample_assignment("a2", "s1", "p1", 1, 0);
        let err = session_assignments::insert_assignment(&pool, &dup)
            .await
            .unwrap_err();
        expect_kind(&err, ErrorKind::UniqueViolation);
    }

    #[tokio::test]
    async fn cascade_delete_on_session_drop() {
        let pool = create_test_pool().await.unwrap();
        seed_session_with_profile(&pool).await;
        let a = sample_assignment("a1", "s1", "p1", 1, 0);
        session_assignments::insert_assignment(&pool, &a)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE id = 's1'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            session_assignments::get_by_id(&pool, "a1")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(
            &session_assignments::list_by_session(&pool, "s")
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &session_assignments::get_by_id(&pool, "a")
                .await
                .unwrap_err(),
        );
        let r = sample_assignment("a", "s", "p", 1, 0);
        expect_pool_closed(
            &session_assignments::insert_assignment(&pool, &r)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &session_assignments::replace_assignments(&pool, "s", &[])
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &session_assignments::set_workspace_id(&pool, "a", "w")
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &session_assignments::find_active_sessions_for_profile(&pool, "p")
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &session_assignments::count_assigned(&pool, "s")
                .await
                .unwrap_err(),
        );
    }

    #[tokio::test]
    async fn busy_on_write() {
        let db = FaultyDb::new().await;
        seed_user(&db.pool, "alice", "alice").await;
        let p = sample_profile("p1", 1, "alice", "n");
        profiles::insert_profile(&db.pool, &p).await.unwrap();
        let s = sample_session("s1", "alice");
        sessions::insert_session(&db.pool, &s).await.unwrap();
        let lock = db.hold_write_lock().await;
        let row = sample_assignment("a1", "s1", "p1", 1, 0);
        let err = session_assignments::insert_assignment(&db.pool, &row)
            .await
            .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5")
        );
        drop(lock);
    }
}

// ---------------------------------------------------------------------------
// audit_log
// ---------------------------------------------------------------------------

mod audit_log_tests {
    use super::*;

    async fn seed_entries(pool: &DbPool) {
        seed_user(pool, "alice", "alice").await;
        seed_user(pool, "bob", "bob").await;
        let entries: [(&str, &str, &str, &str, &str); 5] = [
            ("e1", "alice", "2026-04-10T00:00:00Z", "auth.login", "user"),
            (
                "e2",
                "alice",
                "2026-04-11T00:00:00Z",
                "profile.create",
                "profile",
            ),
            ("e3", "bob", "2026-04-12T00:00:00Z", "auth.login", "user"),
            (
                "e4",
                "bob",
                "2026-04-13T00:00:00Z",
                "session.launch",
                "session",
            ),
            (
                "e5",
                "alice",
                "2026-04-14T00:00:00Z",
                "profile.update",
                "profile",
            ),
        ];
        for (id, user, ts, action, target_kind) in entries {
            audit_log::insert_entry(
                pool,
                id,
                user,
                ts,
                action,
                target_kind,
                &format!("t-{id}"),
                None,
                "10.0.0.1",
                "test",
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn filter_by_user_action_target_kind() {
        let pool = create_test_pool().await.unwrap();
        seed_entries(&pool).await;

        let alice = audit_log::list_entries(&pool, Some("alice"), None, None, None, None, 50, None)
            .await
            .unwrap();
        assert_eq!(alice.len(), 3);

        let logins =
            audit_log::list_entries(&pool, None, Some("auth.login"), None, None, None, 50, None)
                .await
                .unwrap();
        assert_eq!(logins.len(), 2);

        let profiles =
            audit_log::list_entries(&pool, None, None, Some("profile"), None, None, 50, None)
                .await
                .unwrap();
        assert_eq!(profiles.len(), 2);
    }

    #[tokio::test]
    async fn filter_by_since_until_and_cursor_pagination() {
        let pool = create_test_pool().await.unwrap();
        seed_entries(&pool).await;

        // since: 2026-04-12 -> e3, e4, e5
        let since = audit_log::list_entries(
            &pool,
            None,
            None,
            None,
            Some("2026-04-12T00:00:00Z"),
            None,
            50,
            None,
        )
        .await
        .unwrap();
        assert_eq!(since.len(), 3);

        // until: 2026-04-11 -> e1, e2
        let until = audit_log::list_entries(
            &pool,
            None,
            None,
            None,
            None,
            Some("2026-04-11T00:00:00Z"),
            50,
            None,
        )
        .await
        .unwrap();
        assert_eq!(until.len(), 2);

        // cursor pagination: order is DESC, page 1 limit 2 -> e5, e4; cursor = e4.ts
        let page1 = audit_log::list_entries(&pool, None, None, None, None, None, 2, None)
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].id, "e5");
        assert_eq!(page1[1].id, "e4");
        let cursor = &page1[1].timestamp;
        let page2 = audit_log::list_entries(&pool, None, None, None, None, None, 2, Some(cursor))
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].id, "e3");
        assert_eq!(page2[1].id, "e2");
    }

    #[tokio::test]
    async fn combined_filters_compose() {
        let pool = create_test_pool().await.unwrap();
        seed_entries(&pool).await;
        let rows = audit_log::list_entries(
            &pool,
            Some("alice"),
            Some("profile.update"),
            Some("profile"),
            Some("2026-04-13T00:00:00Z"),
            Some("2026-04-15T00:00:00Z"),
            50,
            None,
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "e5");
    }

    #[tokio::test]
    async fn fk_user_violation() {
        let pool = create_test_pool().await.unwrap();
        let err = audit_log::insert_entry(
            &pool,
            "e1",
            "ghost",
            NOW,
            "auth.login",
            "user",
            "x",
            None,
            "ip",
            "ua",
        )
        .await
        .unwrap_err();
        expect_kind(&err, ErrorKind::ForeignKeyViolation);
    }

    #[tokio::test]
    async fn pk_duplicate_id_violation() {
        let pool = create_test_pool().await.unwrap();
        seed_user(&pool, "alice", "alice").await;
        audit_log::insert_entry(
            &pool,
            "e1",
            "alice",
            NOW,
            "auth.login",
            "user",
            "x",
            None,
            "ip",
            "ua",
        )
        .await
        .unwrap();
        let err = audit_log::insert_entry(
            &pool,
            "e1",
            "alice",
            NOW,
            "auth.login",
            "user",
            "x",
            None,
            "ip",
            "ua",
        )
        .await
        .unwrap_err();
        expect_kind(&err, ErrorKind::UniqueViolation);
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(&audit_log::count_entries(&pool).await.unwrap_err());
        expect_pool_closed(
            &audit_log::list_entries(&pool, None, None, None, None, None, 10, None)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &audit_log::insert_entry(&pool, "e", "u", NOW, "a", "k", "t", None, "ip", "ua")
                .await
                .unwrap_err(),
        );
    }

    #[tokio::test]
    async fn busy_on_write() {
        let db = FaultyDb::new().await;
        seed_user(&db.pool, "alice", "alice").await;
        let lock = db.hold_write_lock().await;
        let err = audit_log::insert_entry(
            &db.pool,
            "e1",
            "alice",
            NOW,
            "auth.login",
            "user",
            "alice",
            None,
            "ip",
            "ua",
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5")
        );
        drop(lock);
    }
}

// ---------------------------------------------------------------------------
// login_attempts
// ---------------------------------------------------------------------------

mod login_attempts_tests {
    use super::*;

    async fn seed_attempts(pool: &DbPool) {
        for (i, success) in [false, false, true, false, false].iter().enumerate() {
            login_attempts::record_attempt(
                pool,
                &format!("la{i}"),
                "10.0.0.1",
                "admin",
                &format!("2026-04-14T00:0{i}:00Z"),
                *success,
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn clear_for_username_only_failed_rows() {
        let pool = create_test_pool().await.unwrap();
        seed_attempts(&pool).await;
        let n = login_attempts::clear_for_username(&pool, "admin")
            .await
            .unwrap();
        assert_eq!(n, 4);
        // successful attempt should remain.
        let (remaining,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM login_attempts WHERE username = 'admin'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, 1);
    }

    #[tokio::test]
    async fn cleanup_before_deletes_only_stale() {
        let pool = create_test_pool().await.unwrap();
        seed_attempts(&pool).await;
        // cutoff at 2026-04-14T00:03:00Z -> first three rows removed
        let n = login_attempts::cleanup_before(&pool, "2026-04-14T00:03:00Z")
            .await
            .unwrap();
        assert_eq!(n, 3);
    }

    #[tokio::test]
    async fn check_success_violation_rejected() {
        let pool = create_test_pool().await.unwrap();
        // success must be 0 or 1 — sqlx encodes bool as 0/1, so go via raw SQL
        let err = sqlx::query(
            "INSERT INTO login_attempts (id, ip, username, attempted_at, success)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("la1")
        .bind("10.0.0.1")
        .bind("admin")
        .bind(NOW)
        .bind(7i64)
        .execute(&pool)
        .await
        .unwrap_err();
        expect_kind(&err, ErrorKind::CheckViolation);
    }

    #[tokio::test]
    async fn closed_pool_errors_on_read_and_write() {
        let pool = closed_pool().await;
        expect_pool_closed(
            &login_attempts::record_attempt(&pool, "id", "ip", "u", NOW, false)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &login_attempts::count_failed_by_ip(&pool, "ip", NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &login_attempts::count_failed_by_username(&pool, "u", NOW)
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &login_attempts::clear_for_username(&pool, "u")
                .await
                .unwrap_err(),
        );
        expect_pool_closed(
            &login_attempts::cleanup_before(&pool, NOW)
                .await
                .unwrap_err(),
        );
    }

    #[tokio::test]
    async fn busy_on_write() {
        let db = FaultyDb::new().await;
        let lock = db.hold_write_lock().await;
        let err = login_attempts::record_attempt(&db.pool, "la1", "10.0.0.1", "admin", NOW, false)
            .await
            .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("5")
        );
        drop(lock);
    }
}
