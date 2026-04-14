use thiserror::Error;

/// Top-level error type for console-core operations.
#[derive(Debug, Error)]
pub enum ConsoleError {
    #[error("database error: {0}")]
    Database(String),

    #[error("not found: {entity} {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("validation error: {0}")]
    Validation(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}
