//! Password hashing with Argon2id (OWASP-recommended parameters).
//!
//! Spec: `wcon-auth` §7

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand::Rng;
use argon2::{Algorithm, Argon2, Params, Version};

use crate::error::ConsoleError;

/// OWASP-recommended Argon2id parameters.
///
/// - Memory: 19456 KiB (19 MiB)
/// - Iterations: 2
/// - Parallelism: 1
/// - Output: 32 bytes
fn argon2_instance() -> Argon2<'static> {
    let params = Params::new(19456, 2, 1, Some(32)).expect("valid Argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hash a password using Argon2id with OWASP parameters.
/// Returns a PHC-format string (e.g., `$argon2id$v=19$m=19456,t=2,p=1$...`).
pub fn hash_password(password: &str) -> Result<String, ConsoleError> {
    let salt = generate_salt()?;
    let hash = argon2_instance()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| ConsoleError::Internal(format!("password hashing failed: {e}")))?;
    Ok(hash.to_string())
}

/// Generate a 16-byte random salt as a base64-encoded SaltString.
fn generate_salt() -> Result<SaltString, ConsoleError> {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    SaltString::encode_b64(&bytes)
        .map_err(|e| ConsoleError::Internal(format!("salt encoding failed: {e}")))
}

/// Verify a password against a PHC-format hash string.
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch.
/// The underlying comparison is constant-time.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, ConsoleError> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| ConsoleError::Internal(format!("invalid password hash: {e}")))?;
    Ok(argon2_instance()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Minimum password strength requirements.
pub fn validate_password_strength(password: &str) -> Result<(), ConsoleError> {
    if password.len() < 12 {
        return Err(ConsoleError::PasswordTooWeak(
            "Password must be at least 12 characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_produces_phc_format() {
        let hash = hash_password("test-password-123!").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(hash.contains("m=19456,t=2,p=1"));
    }

    #[test]
    fn verify_correct_password() {
        let hash = hash_password("my-secure-password").unwrap();
        assert!(verify_password("my-secure-password", &hash).unwrap());
    }

    #[test]
    fn verify_wrong_password() {
        let hash = hash_password("my-secure-password").unwrap();
        assert!(!verify_password("wrong-password", &hash).unwrap());
    }

    #[test]
    fn different_hashes_for_same_password() {
        let h1 = hash_password("same-password").unwrap();
        let h2 = hash_password("same-password").unwrap();
        assert_ne!(h1, h2, "salt should differ");
        assert!(verify_password("same-password", &h1).unwrap());
        assert!(verify_password("same-password", &h2).unwrap());
    }

    #[test]
    fn weak_password_rejected() {
        let err = validate_password_strength("short").unwrap_err();
        assert!(matches!(err, ConsoleError::PasswordTooWeak(_)));
    }

    #[test]
    fn strong_password_accepted() {
        validate_password_strength("a-long-enough-pw").unwrap();
    }
}
