use thiserror::Error;

/// Top-level error type for console-core domain operations.
#[derive(Debug, Error)]
pub enum ConsoleError {
    #[error("database error: {0}")]
    Database(String),

    #[error("not found: {entity} {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("validation error: {message}")]
    Validation {
        message: String,
        violations: Vec<Violation>,
        warnings: Vec<Violation>,
    },

    #[error("unauthenticated")]
    Unauthenticated,

    #[error("account locked")]
    AccountLocked,

    #[error("password change required")]
    PasswordChangeRequired,

    #[error("csrf validation failed")]
    CsrfFailed,

    #[error("password too weak: {0}")]
    PasswordTooWeak(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("last admin: cannot remove the only admin")]
    LastAdmin,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("rate limited")]
    RateLimited,

    #[error("runtime error: {message}")]
    Runtime {
        message: String,
        grpc_status: Option<String>,
        service: Option<String>,
        method: Option<String>,
    },

    #[error("internal error: {0}")]
    Internal(String),
}

/// A single validation violation or warning.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Violation {
    pub field: Option<String>,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

impl ConsoleError {
    /// Convenience: single-violation validation error.
    pub fn validation(code: impl Into<String>, message: impl Into<String>) -> Self {
        let message_str = message.into();
        ConsoleError::Validation {
            message: message_str.clone(),
            violations: vec![Violation {
                field: None,
                code: code.into(),
                message: message_str,
                value: None,
            }],
            warnings: vec![],
        }
    }
}

impl ConsoleError {
    /// Wrap a database error string. Conversion from sqlx::Error
    /// lives in console-db to avoid leaking the sqlx dependency.
    pub fn database(msg: impl Into<String>) -> Self {
        ConsoleError::Database(msg.into())
    }
}
