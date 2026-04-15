#[cfg(test)]
mod tests {
    use crate::create_test_pool;
    use crate::queries::*;

    #[tokio::test]
    async fn test_settings_crud() {
        let pool = create_test_pool().await.unwrap();
        let now = "2026-04-14T00:00:00Z";

        // Empty initially
        assert!(settings::get_setting(&pool, "ui.theme").await.unwrap().is_none());
        assert!(settings::get_all_settings(&pool).await.unwrap().is_empty());

        // Upsert
        settings::upsert_setting(&pool, "ui.theme", "\"dark\"", now).await.unwrap();
        let row = settings::get_setting(&pool, "ui.theme").await.unwrap().unwrap();
        assert_eq!(row.value, "\"dark\"");

        // Update
        settings::upsert_setting(&pool, "ui.theme", "\"light\"", now).await.unwrap();
        let row = settings::get_setting(&pool, "ui.theme").await.unwrap().unwrap();
        assert_eq!(row.value, "\"light\"");

        // Delete
        assert!(settings::delete_setting(&pool, "ui.theme").await.unwrap());
        assert!(settings::get_setting(&pool, "ui.theme").await.unwrap().is_none());
        assert!(!settings::delete_setting(&pool, "nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_users_crud() {
        let pool = create_test_pool().await.unwrap();
        let now = "2026-04-14T00:00:00Z";

        // Insert
        users::insert_user(&pool, "u1", "Admin", "Admin User", "$hash$", "admin", true, now)
            .await
            .unwrap();

        // Get by ID
        let user = users::get_by_id(&pool, "u1").await.unwrap().unwrap();
        assert_eq!(user.username, "Admin");
        assert_eq!(user.console_role, "admin");
        assert!(user.must_change_password);

        // Get by username (case-insensitive)
        let user = users::get_by_username(&pool, "admin").await.unwrap().unwrap();
        assert_eq!(user.id, "u1");

        // Count
        assert_eq!(users::count_users(&pool).await.unwrap(), 1);
        assert_eq!(users::count_active_admins(&pool).await.unwrap(), 1);

        // Update password
        users::update_password(&pool, "u1", "$newhash$", now).await.unwrap();
        let user = users::get_by_id(&pool, "u1").await.unwrap().unwrap();
        assert_eq!(user.password_hash, "$newhash$");
        assert!(!user.must_change_password);

        // Disable
        users::disable_user(&pool, "u1", now).await.unwrap();
        let user = users::get_by_id(&pool, "u1").await.unwrap().unwrap();
        assert!(user.disabled_at.is_some());
        assert_eq!(users::count_active_admins(&pool).await.unwrap(), 0);

        // Enable
        users::enable_user(&pool, "u1", now).await.unwrap();
        assert_eq!(users::count_active_admins(&pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_user_sessions() {
        let pool = create_test_pool().await.unwrap();
        let now = "2026-04-14T00:00:00Z";
        let expires = "2026-04-15T00:00:00Z";

        users::insert_user(&pool, "u1", "admin", "Admin", "$h$", "admin", false, now)
            .await
            .unwrap();

        user_sessions::insert_session(&pool, "s1", "u1", "hashvalue", "127.0.0.1", "test-agent", now, expires)
            .await
            .unwrap();

        // Lookup by hash (not expired)
        let sess = user_sessions::get_by_token_hash(&pool, "hashvalue", now).await.unwrap();
        assert!(sess.is_some());

        // Lookup with expired timestamp
        let sess = user_sessions::get_by_token_hash(&pool, "hashvalue", "2026-04-16T00:00:00Z").await.unwrap();
        assert!(sess.is_none());

        // Delete
        assert!(user_sessions::delete_session(&pool, "s1").await.unwrap());
        assert!(user_sessions::get_by_token_hash(&pool, "hashvalue", now).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_api_tokens() {
        let pool = create_test_pool().await.unwrap();
        let now = "2026-04-14T00:00:00Z";

        users::insert_user(&pool, "u1", "admin", "Admin", "$h$", "admin", false, now)
            .await
            .unwrap();

        api_tokens::insert_token(&pool, "t1", "u1", "My Token", "tokenhash", now, None)
            .await
            .unwrap();

        // Lookup by hash
        let tok = api_tokens::get_by_token_hash(&pool, "tokenhash", now).await.unwrap();
        assert!(tok.is_some());

        // List by user
        let tokens = api_tokens::list_by_user(&pool, "u1").await.unwrap();
        assert_eq!(tokens.len(), 1);

        // Revoke
        assert!(api_tokens::revoke_token(&pool, "t1", now).await.unwrap());
        assert!(api_tokens::get_by_token_hash(&pool, "tokenhash", now).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_audit_log_append_only() {
        let pool = create_test_pool().await.unwrap();
        let now = "2026-04-14T00:00:00Z";

        users::insert_user(&pool, "u1", "admin", "Admin", "$h$", "admin", false, now)
            .await
            .unwrap();

        audit_log::insert_entry(&pool, "a1", "u1", now, "auth.login", "user", "u1", None, "127.0.0.1", "test-agent")
            .await
            .unwrap();

        let entries = audit_log::list_entries(&pool, None, None, None, None, None, 50, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "auth.login");

        assert_eq!(audit_log::count_entries(&pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_login_attempts_rate_limiting() {
        let pool = create_test_pool().await.unwrap();
        let window_start = "2026-04-13T23:59:00Z";

        // Record 3 failed attempts
        for i in 0..3 {
            login_attempts::record_attempt(
                &pool,
                &format!("la{i}"),
                "10.0.0.1",
                "admin",
                &format!("2026-04-14T00:0{i}:00Z"),
                false,
            )
            .await
            .unwrap();
        }

        assert_eq!(login_attempts::count_failed_by_ip(&pool, "10.0.0.1", window_start).await.unwrap(), 3);
        assert_eq!(login_attempts::count_failed_by_username(&pool, "admin", window_start).await.unwrap(), 3);
        assert_eq!(login_attempts::count_failed_by_ip(&pool, "10.0.0.2", window_start).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_profiles_versioning() {
        let pool = create_test_pool().await.unwrap();
        let now = "2026-04-14T00:00:00Z";

        users::insert_user(&pool, "u1", "admin", "Admin", "$h$", "admin", false, now)
            .await
            .unwrap();

        let row = profiles::ProfileRow {
            id: "p1".into(),
            version: 1,
            name: "My Profile".into(),
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
            owner_user_id: "u1".into(),
            visibility: "private".into(),
            is_current: true,
            created_at: now.into(),
            deleted_at: None,
        };
        profiles::insert_profile(&pool, &row).await.unwrap();

        // Get current
        let p = profiles::get_current(&pool, "p1").await.unwrap().unwrap();
        assert_eq!(p.version, 1);

        // Create new version
        let v2 = profiles::ProfileRow {
            version: 2,
            name: "My Profile v2".into(),
            ..row.clone()
        };
        profiles::create_new_version(&pool, "p1", &v2).await.unwrap();

        // Current should be v2
        let p = profiles::get_current(&pool, "p1").await.unwrap().unwrap();
        assert_eq!(p.version, 2);
        assert_eq!(p.name, "My Profile v2");

        // Old version still accessible
        let p = profiles::get_version(&pool, "p1", 1).await.unwrap().unwrap();
        assert_eq!(p.version, 1);
        assert!(!p.is_current);

        // Version history
        let versions = profiles::list_versions(&pool, "p1").await.unwrap();
        assert_eq!(versions.len(), 2);

        // Soft delete
        profiles::soft_delete(&pool, "p1", now).await.unwrap();
        assert!(profiles::get_current(&pool, "p1").await.unwrap().is_none());

        // Name uniqueness
        assert!(!profiles::name_exists_for_user(&pool, "My Profile v2", "u1", None).await.unwrap());
    }

    #[tokio::test]
    async fn test_sessions_state_machine() {
        let pool = create_test_pool().await.unwrap();
        let now = "2026-04-14T00:00:00Z";

        users::insert_user(&pool, "u1", "admin", "Admin", "$h$", "admin", false, now)
            .await
            .unwrap();

        let row = sessions::SessionRow {
            id: "sess1".into(),
            name: Some("Test Session".into()),
            owner_user_id: "u1".into(),
            vertical: "fixture-simple".into(),
            workflow: "standard-dev".into(),
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
        sessions::insert_session(&pool, &row).await.unwrap();

        // Transition: configuring → validating
        assert!(sessions::transition_state(&pool, "sess1", "configuring", "validating", now).await.unwrap());

        // Transition from wrong state fails
        assert!(!sessions::transition_state(&pool, "sess1", "configuring", "launching", now).await.unwrap());

        // validating → launching → active
        assert!(sessions::transition_state(&pool, "sess1", "validating", "launching", now).await.unwrap());
        assert!(sessions::transition_state(&pool, "sess1", "launching", "active", now).await.unwrap());

        let s = sessions::get_by_id(&pool, "sess1").await.unwrap().unwrap();
        assert_eq!(s.state, "active");
        assert!(s.launched_at.is_some());

        // Cancel from active
        let prev = sessions::cancel(&pool, "sess1", now).await.unwrap();
        assert_eq!(prev, Some("active".into()));

        let s = sessions::get_by_id(&pool, "sess1").await.unwrap().unwrap();
        assert_eq!(s.state, "cancelled");
        assert!(s.closed_at.is_some());

        // Cancel again is no-op
        assert!(sessions::cancel(&pool, "sess1", now).await.unwrap().is_none());
    }
}
