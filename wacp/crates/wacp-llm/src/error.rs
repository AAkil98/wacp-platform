use serde::Serialize;

/// LLM adapter error with two-axis classification.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{origin}/{persistence}: {message}")]
pub struct LlmError {
    pub origin: ErrorOrigin,
    pub persistence: ErrorPersistence,
    /// Provider's error code (e.g., "rate_limit_error").
    pub code: Option<String>,
    /// Human-readable message (credentials stripped).
    pub message: String,
    /// HTTP status code, if applicable.
    pub status: Option<u16>,
    /// Whether retry might succeed (derived from persistence).
    pub retryable: bool,
}

/// Error origin — where did the error come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorOrigin {
    /// Malformed request (missing field, invalid JSON).
    Structural,
    /// Provider-side error (model not found, content filter, server error).
    Provider,
    /// Network error (timeout, DNS, TLS).
    Transport,
    /// Inference error (context exceeded).
    Compute,
}

/// Error persistence — will retry help.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorPersistence {
    /// Likely to succeed on retry (429, 503, timeout).
    Transient,
    /// Will not succeed on retry (401, 404, content policy).
    Permanent,
    /// Cannot determine.
    Unknown,
}

impl LlmError {
    /// Classify an HTTP status code into origin + persistence.
    pub fn from_status(status: u16, message: impl Into<String>) -> Self {
        let (origin, persistence) = classify_status(status);
        Self {
            origin,
            persistence,
            code: None,
            message: message.into(),
            status: Some(status),
            retryable: persistence == ErrorPersistence::Transient,
        }
    }

    /// Create a transport error (connection, DNS, etc.).
    pub fn transport(message: impl Into<String>, transient: bool) -> Self {
        let persistence = if transient {
            ErrorPersistence::Transient
        } else {
            ErrorPersistence::Permanent
        };
        Self {
            origin: ErrorOrigin::Transport,
            persistence,
            code: None,
            message: message.into(),
            status: None,
            retryable: transient,
        }
    }

    /// Create a structural error (invalid request).
    pub fn structural(message: impl Into<String>) -> Self {
        Self {
            origin: ErrorOrigin::Structural,
            persistence: ErrorPersistence::Permanent,
            code: None,
            message: message.into(),
            status: None,
            retryable: false,
        }
    }

    /// Create a compute error (context length exceeded).
    pub fn compute(message: impl Into<String>) -> Self {
        Self {
            origin: ErrorOrigin::Compute,
            persistence: ErrorPersistence::Permanent,
            code: None,
            message: message.into(),
            status: None,
            retryable: false,
        }
    }
}

fn classify_status(status: u16) -> (ErrorOrigin, ErrorPersistence) {
    match status {
        400 => (ErrorOrigin::Structural, ErrorPersistence::Permanent),
        401 => (ErrorOrigin::Provider, ErrorPersistence::Permanent),
        403 => (ErrorOrigin::Provider, ErrorPersistence::Permanent),
        404 => (ErrorOrigin::Provider, ErrorPersistence::Permanent),
        408 => (ErrorOrigin::Transport, ErrorPersistence::Transient),
        429 => (ErrorOrigin::Provider, ErrorPersistence::Transient),
        500 => (ErrorOrigin::Provider, ErrorPersistence::Transient),
        502 => (ErrorOrigin::Transport, ErrorPersistence::Transient),
        503 => (ErrorOrigin::Provider, ErrorPersistence::Transient),
        _ => (ErrorOrigin::Provider, ErrorPersistence::Unknown),
    }
}

impl std::fmt::Display for ErrorOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Structural => write!(f, "structural"),
            Self::Provider => write!(f, "provider"),
            Self::Transport => write!(f, "transport"),
            Self::Compute => write!(f, "compute"),
        }
    }
}

impl std::fmt::Display for ErrorPersistence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transient => write!(f, "transient"),
            Self::Permanent => write!(f, "permanent"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_400_structural_permanent() {
        let err = LlmError::from_status(400, "bad request");
        assert_eq!(err.origin, ErrorOrigin::Structural);
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
        assert!(!err.retryable);
    }

    #[test]
    fn classify_401_provider_permanent() {
        let err = LlmError::from_status(401, "invalid api key");
        assert_eq!(err.origin, ErrorOrigin::Provider);
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
        assert!(!err.retryable);
    }

    #[test]
    fn classify_429_provider_transient() {
        let err = LlmError::from_status(429, "rate limited");
        assert_eq!(err.origin, ErrorOrigin::Provider);
        assert_eq!(err.persistence, ErrorPersistence::Transient);
        assert!(err.retryable);
    }

    #[test]
    fn classify_500_provider_transient() {
        let err = LlmError::from_status(500, "server error");
        assert_eq!(err.origin, ErrorOrigin::Provider);
        assert_eq!(err.persistence, ErrorPersistence::Transient);
        assert!(err.retryable);
    }

    #[test]
    fn classify_502_transport_transient() {
        let err = LlmError::from_status(502, "bad gateway");
        assert_eq!(err.origin, ErrorOrigin::Transport);
        assert_eq!(err.persistence, ErrorPersistence::Transient);
        assert!(err.retryable);
    }

    #[test]
    fn classify_503_provider_transient() {
        let err = LlmError::from_status(503, "overloaded");
        assert!(err.retryable);
    }

    #[test]
    fn classify_unknown_status() {
        let err = LlmError::from_status(418, "i'm a teapot");
        assert_eq!(err.origin, ErrorOrigin::Provider);
        assert_eq!(err.persistence, ErrorPersistence::Unknown);
        assert!(!err.retryable); // unknown is not retryable
    }

    #[test]
    fn transport_error_transient() {
        let err = LlmError::transport("connection refused", true);
        assert_eq!(err.origin, ErrorOrigin::Transport);
        assert!(err.retryable);
    }

    #[test]
    fn transport_error_permanent() {
        let err = LlmError::transport("TLS certificate invalid", false);
        assert_eq!(err.origin, ErrorOrigin::Transport);
        assert!(!err.retryable);
    }

    #[test]
    fn structural_error() {
        let err = LlmError::structural("missing model field");
        assert_eq!(err.origin, ErrorOrigin::Structural);
        assert!(!err.retryable);
    }

    #[test]
    fn compute_error() {
        let err = LlmError::compute("context length exceeded: 200k > 128k");
        assert_eq!(err.origin, ErrorOrigin::Compute);
        assert!(!err.retryable);
    }

    #[test]
    fn error_display_format() {
        let err = LlmError::from_status(429, "rate limited");
        assert_eq!(err.to_string(), "provider/transient: rate limited");
    }

    #[test]
    fn all_status_codes_classified() {
        // Verify every mapped status returns expected classification
        let cases: Vec<(u16, ErrorOrigin, ErrorPersistence)> = vec![
            (400, ErrorOrigin::Structural, ErrorPersistence::Permanent),
            (401, ErrorOrigin::Provider, ErrorPersistence::Permanent),
            (403, ErrorOrigin::Provider, ErrorPersistence::Permanent),
            (404, ErrorOrigin::Provider, ErrorPersistence::Permanent),
            (408, ErrorOrigin::Transport, ErrorPersistence::Transient),
            (429, ErrorOrigin::Provider, ErrorPersistence::Transient),
            (500, ErrorOrigin::Provider, ErrorPersistence::Transient),
            (502, ErrorOrigin::Transport, ErrorPersistence::Transient),
            (503, ErrorOrigin::Provider, ErrorPersistence::Transient),
        ];
        for (status, expected_origin, expected_persistence) in cases {
            let err = LlmError::from_status(status, "test");
            assert_eq!(
                err.origin, expected_origin,
                "status {status} origin mismatch"
            );
            assert_eq!(
                err.persistence, expected_persistence,
                "status {status} persistence mismatch"
            );
        }
    }
}
