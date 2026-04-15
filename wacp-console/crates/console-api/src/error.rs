use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use console_core::error::{ConsoleError, Violation};
use serde::Serialize;

/// API-layer error that maps domain errors to HTTP responses.
///
/// JSON shape: `{ "error": "<code>", "message": "<text>", "details": { ... } }`
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub body: ApiErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

impl From<ConsoleError> for ApiError {
    fn from(err: ConsoleError) -> Self {
        match err {
            ConsoleError::Database(msg) => ApiError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                body: ApiErrorBody {
                    error: "internal_error".into(),
                    message: "An internal error occurred".into(),
                    details: Some(serde_json::json!({ "cause": msg })),
                },
            },
            ConsoleError::NotFound { entity, id } => ApiError {
                status: StatusCode::NOT_FOUND,
                body: ApiErrorBody {
                    error: "not_found".into(),
                    message: format!("{entity} '{id}' not found"),
                    details: None,
                },
            },
            ConsoleError::Validation {
                message,
                violations,
                warnings,
            } => ApiError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                body: ApiErrorBody {
                    error: "validation_failed".into(),
                    message,
                    details: Some(serde_json::json!({
                        "violations": violations,
                        "warnings": warnings,
                    })),
                },
            },
            ConsoleError::Unauthenticated => ApiError {
                status: StatusCode::UNAUTHORIZED,
                body: ApiErrorBody {
                    error: "unauthenticated".into(),
                    message: "Authentication required".into(),
                    details: None,
                },
            },
            ConsoleError::AccountLocked => ApiError {
                status: StatusCode::UNAUTHORIZED,
                body: ApiErrorBody {
                    error: "account_locked".into(),
                    message: "Account is locked due to too many failed login attempts".into(),
                    details: None,
                },
            },
            ConsoleError::PasswordChangeRequired => ApiError {
                status: StatusCode::FORBIDDEN,
                body: ApiErrorBody {
                    error: "password_change_required".into(),
                    message: "You must change your password before continuing".into(),
                    details: None,
                },
            },
            ConsoleError::CsrfFailed => ApiError {
                status: StatusCode::FORBIDDEN,
                body: ApiErrorBody {
                    error: "csrf_validation_failed".into(),
                    message: "CSRF token missing or invalid".into(),
                    details: None,
                },
            },
            ConsoleError::PasswordTooWeak(reason) => ApiError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                body: ApiErrorBody {
                    error: "password_too_weak".into(),
                    message: reason,
                    details: None,
                },
            },
            ConsoleError::Forbidden(reason) => ApiError {
                status: StatusCode::FORBIDDEN,
                body: ApiErrorBody {
                    error: "forbidden".into(),
                    message: reason,
                    details: None,
                },
            },
            ConsoleError::LastAdmin => ApiError {
                status: StatusCode::FORBIDDEN,
                body: ApiErrorBody {
                    error: "last_admin".into(),
                    message: "Cannot remove the only admin account".into(),
                    details: None,
                },
            },
            ConsoleError::Conflict(msg) => ApiError {
                status: StatusCode::CONFLICT,
                body: ApiErrorBody {
                    error: "conflict".into(),
                    message: msg,
                    details: None,
                },
            },
            ConsoleError::RateLimited => ApiError {
                status: StatusCode::TOO_MANY_REQUESTS,
                body: ApiErrorBody {
                    error: "rate_limited".into(),
                    message: "Too many requests, please try again later".into(),
                    details: None,
                },
            },
            ConsoleError::Runtime {
                message,
                grpc_status,
                service,
                method,
            } => ApiError {
                status: StatusCode::BAD_GATEWAY,
                body: ApiErrorBody {
                    error: "runtime_error".into(),
                    message,
                    details: Some(serde_json::json!({
                        "grpc_status": grpc_status,
                        "service": service,
                        "method": method,
                    })),
                },
            },
            ConsoleError::Internal(msg) => ApiError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                body: ApiErrorBody {
                    error: "internal_error".into(),
                    message: msg,
                    details: None,
                },
            },
        }
    }
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::BAD_REQUEST,
            body: ApiErrorBody {
                error: "bad_request".into(),
                message: message.into(),
                details: None,
            },
        }
    }

    pub fn not_found(entity: &str, id: &str) -> Self {
        ApiError::from(ConsoleError::NotFound {
            entity: Box::leak(entity.to_string().into_boxed_str()),
            id: id.to_string(),
        })
    }

    pub fn validation(violations: Vec<Violation>) -> Self {
        let message = violations
            .first()
            .map(|v| v.message.clone())
            .unwrap_or_else(|| "Validation failed".into());
        ApiError::from(ConsoleError::Validation {
            message,
            violations,
            warnings: vec![],
        })
    }

    /// Map a `tonic::Status` returned by a runtime gRPC call into the API's
    /// HTTP error shape. Per spec `wcon-w4-highway-forwarding` §3.2:
    /// runtime-rejected actions surface as 4xx where the cause is structural
    /// (NotFound, FailedPrecondition, PermissionDenied), and as 5xx where the
    /// runtime is unreachable or unhealthy.
    pub fn from_tonic(s: tonic::Status, service: &'static str, method: &'static str) -> Self {
        let (status, code, message) = match s.code() {
            tonic::Code::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                s.message().to_string(),
            ),
            tonic::Code::FailedPrecondition => (
                StatusCode::CONFLICT,
                "conflict",
                s.message().to_string(),
            ),
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => (
                StatusCode::SERVICE_UNAVAILABLE,
                "runtime_unavailable",
                s.message().to_string(),
            ),
            tonic::Code::PermissionDenied => (
                StatusCode::FORBIDDEN,
                "forbidden",
                s.message().to_string(),
            ),
            _ => (
                StatusCode::BAD_GATEWAY,
                "runtime_error",
                format!("runtime: {}", s.message()),
            ),
        };
        ApiError {
            status,
            body: ApiErrorBody {
                error: code.into(),
                message,
                details: Some(serde_json::json!({
                    "grpc_status": format!("{:?}", s.code()),
                    "service": service,
                    "method": method,
                })),
            },
        }
    }

    /// Constructed when the gRPC pool has no live highway client (e.g., the
    /// runtime crashed and reconnect hasn't fired yet). Returns 503 to signal
    /// the caller should retry rather than re-issue with backoff.
    pub fn runtime_unavailable(service: &'static str, method: &'static str) -> Self {
        ApiError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            body: ApiErrorBody {
                error: "runtime_unavailable".into(),
                message: format!("{service}.{method} unavailable: pool has no live channel"),
                details: Some(serde_json::json!({
                    "service": service,
                    "method": method,
                })),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    #[tokio::test]
    async fn test_not_found_response() {
        let err = ApiError::from(ConsoleError::NotFound {
            entity: "user",
            id: "abc".into(),
        });
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "not_found");
        assert_eq!(json["message"], "user 'abc' not found");
    }

    #[tokio::test]
    async fn test_validation_response_has_violations() {
        let err = ApiError::from(ConsoleError::Validation {
            message: "Invalid profile".into(),
            violations: vec![Violation {
                field: Some("role".into()),
                code: "UNKNOWN_ROLE".into(),
                message: "Role 'foo' does not exist".into(),
                value: Some(serde_json::json!("foo")),
            }],
            warnings: vec![],
        });
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "validation_failed");
        assert_eq!(json["details"]["violations"][0]["code"], "UNKNOWN_ROLE");
    }

    #[tokio::test]
    async fn test_auth_errors_status_codes() {
        let cases: Vec<(ConsoleError, StatusCode, &str)> = vec![
            (
                ConsoleError::Unauthenticated,
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
            ),
            (
                ConsoleError::AccountLocked,
                StatusCode::UNAUTHORIZED,
                "account_locked",
            ),
            (
                ConsoleError::PasswordChangeRequired,
                StatusCode::FORBIDDEN,
                "password_change_required",
            ),
            (
                ConsoleError::CsrfFailed,
                StatusCode::FORBIDDEN,
                "csrf_validation_failed",
            ),
            (
                ConsoleError::Forbidden("nope".into()),
                StatusCode::FORBIDDEN,
                "forbidden",
            ),
            (ConsoleError::LastAdmin, StatusCode::FORBIDDEN, "last_admin"),
            (
                ConsoleError::RateLimited,
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
            ),
        ];

        for (err, expected_status, expected_code) in cases {
            let api_err = ApiError::from(err);
            assert_eq!(
                api_err.status, expected_status,
                "status for {expected_code}"
            );
            assert_eq!(api_err.body.error, expected_code);
        }
    }

    #[tokio::test]
    async fn test_runtime_error_has_grpc_details() {
        let err = ApiError::from(ConsoleError::Runtime {
            message: "connection refused".into(),
            grpc_status: Some("UNAVAILABLE".into()),
            service: Some("CoordinatorService".into()),
            method: Some("SubmitGoal".into()),
        });
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["details"]["service"], "CoordinatorService");
    }
}
