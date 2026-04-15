//! Bootstrap flow — first-launch credential generation.
//!
//! Spec: `wcon-auth` §6

use console_db::DbPool;
use console_db::queries::users;
use rand::Rng;
use tracing::info;

use crate::error::ConsoleError;
use crate::password::hash_password;

/// Result of the bootstrap check.
pub enum BootstrapResult {
    /// Users already exist, no bootstrap needed.
    AlreadyBootstrapped,
    /// Bootstrap performed: admin created with this one-time credential.
    Bootstrapped { username: String, password: String },
}

/// Check if the database is empty and bootstrap if needed.
/// Creates an admin user with a random one-time password and `must_change_password = true`.
pub async fn bootstrap_if_needed(pool: &DbPool) -> Result<BootstrapResult, ConsoleError> {
    let count = users::count_users(pool)
        .await
        .map_err(|e| ConsoleError::Database(e.to_string()))?;

    if count > 0 {
        return Ok(BootstrapResult::AlreadyBootstrapped);
    }

    let username = "admin";
    let password = generate_credential();
    let password_hash = hash_password(&password)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    users::insert_user(
        pool,
        &id,
        username,
        "Administrator",
        &password_hash,
        "admin",
        true,
        &now,
    )
    .await
    .map_err(|e| ConsoleError::Database(e.to_string()))?;

    info!("bootstrap: admin user created");

    Ok(BootstrapResult::Bootstrapped {
        username: username.to_string(),
        password,
    })
}

/// Write the bootstrap token to the XDG state directory.
pub fn write_bootstrap_token(password: &str) -> Result<std::path::PathBuf, ConsoleError> {
    let state_dir = directories::ProjectDirs::from("", "", "wacp-console")
        .and_then(|dirs| dirs.state_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    std::fs::create_dir_all(&state_dir).ok();
    let path = state_dir.join("bootstrap-token");
    std::fs::write(&path, password)
        .map_err(|e| ConsoleError::Internal(format!("failed to write bootstrap token: {e}")))?;

    // Restrict permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();
    }

    Ok(path)
}

/// Generate a 256-bit random credential as a base64url string.
fn generate_credential() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use console_db::create_test_pool;

    #[tokio::test]
    async fn bootstrap_creates_admin_on_empty_db() {
        let pool = create_test_pool().await.unwrap();

        let result = bootstrap_if_needed(&pool).await.unwrap();
        let BootstrapResult::Bootstrapped { username, password } = result else {
            panic!("expected Bootstrapped");
        };

        assert_eq!(username, "admin");
        assert!(!password.is_empty());

        // Verify user exists
        let user = users::get_by_username(&pool, "admin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.console_role, "admin");
        assert!(user.must_change_password);

        // Second call should return AlreadyBootstrapped
        let result = bootstrap_if_needed(&pool).await.unwrap();
        assert!(matches!(result, BootstrapResult::AlreadyBootstrapped));
    }

    #[test]
    fn generate_credential_is_32_bytes_b64() {
        let cred = generate_credential();
        // 32 bytes → 43 chars base64url (no padding)
        assert_eq!(cred.len(), 43);
    }
}
