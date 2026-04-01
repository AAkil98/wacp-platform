use std::future::Future;
use std::pin::Pin;

use tokio_util::sync::CancellationToken;
use wacp_types::WorkspaceId;

/// The handler trait. Tool authors implement this.
///
/// Handlers are async, take context + validated args, return result or error.
/// The blanket impl means any `async fn(&ToolContext, Value) -> Result<Value, ToolError>`
/// automatically implements ToolHandler.
pub trait ToolHandler: Send + Sync + 'static {
    fn execute(
        &self,
        ctx: &ToolContext,
        args: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>>;
}

/// Blanket impl: any async fn matching the signature is a ToolHandler.
impl<F, Fut> ToolHandler for F
where
    F: Fn(&ToolContext, serde_json::Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<serde_json::Value, ToolError>> + Send + 'static,
{
    fn execute(
        &self,
        ctx: &ToolContext,
        args: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>> {
        Box::pin(self(ctx, args))
    }
}

/// Invocation environment passed to every handler call.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Which tool is being invoked.
    pub tool_name: String,
    /// Which capability within the tool.
    pub capability_name: String,
    /// The workspace on whose behalf this invocation occurs. None in tests.
    pub workspace_id: Option<WorkspaceId>,
    /// Cancellation token. Check this in long-running handlers.
    pub cancellation_token: CancellationToken,
    /// The tool's validated deployment configuration.
    pub config: serde_json::Value,
    /// Effective timeout for this invocation (ms).
    pub timeout_ms: u64,
}

impl ToolContext {
    /// Check if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }
}

/// Structured tool error.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}: {message}")]
pub struct ToolError {
    pub code: ToolErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl ToolError {
    /// Create a validation error (not retryable).
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            code: ToolErrorCode::ValidationFailed,
            message: message.into(),
            retryable: false,
        }
    }

    /// Create a timeout error (not retryable).
    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            code: ToolErrorCode::Timeout,
            message: message.into(),
            retryable: false,
        }
    }

    /// Create an execution error with retryable flag.
    pub fn execution(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: ToolErrorCode::ExecutionFailed,
            message: message.into(),
            retryable,
        }
    }

    /// Create an internal error (not retryable).
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: ToolErrorCode::InternalError,
            message: message.into(),
            retryable: false,
        }
    }

    /// Create an unavailable error (circuit breaker open, retryable after cooldown).
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: ToolErrorCode::Unavailable,
            message: message.into(),
            retryable: true,
        }
    }

    /// Create an overloaded error (concurrency limit, retryable after backoff).
    pub fn overloaded(message: impl Into<String>) -> Self {
        Self {
            code: ToolErrorCode::Overloaded,
            message: message.into(),
            retryable: true,
        }
    }

    /// Create a cancelled error (not retryable).
    pub fn cancelled() -> Self {
        Self {
            code: ToolErrorCode::Cancelled,
            message: "invocation cancelled".into(),
            retryable: false,
        }
    }
}

/// Error category — determines caller behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolErrorCode {
    /// Input did not pass schema validation.
    ValidationFailed,
    /// Handler timed out.
    Timeout,
    /// Handler returned an error (tool-specific).
    ExecutionFailed,
    /// Handler panicked or child process crashed.
    InternalError,
    /// Tool is currently circuit-broken.
    Unavailable,
    /// Concurrency limit reached, queue full.
    Overloaded,
    /// Cancellation was requested and honored.
    Cancelled,
}

impl std::fmt::Display for ToolErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationFailed => write!(f, "validation_failed"),
            Self::Timeout => write!(f, "timeout"),
            Self::ExecutionFailed => write!(f, "execution_failed"),
            Self::InternalError => write!(f, "internal_error"),
            Self::Unavailable => write!(f, "unavailable"),
            Self::Overloaded => write!(f, "overloaded"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_context() -> ToolContext {
        ToolContext {
            tool_name: "test_tool".into(),
            capability_name: "run".into(),
            workspace_id: None,
            cancellation_token: CancellationToken::new(),
            config: json!({}),
            timeout_ms: 30_000,
        }
    }

    // --- ToolErrorCode display ---

    #[test]
    fn error_code_display() {
        assert_eq!(ToolErrorCode::ValidationFailed.to_string(), "validation_failed");
        assert_eq!(ToolErrorCode::Timeout.to_string(), "timeout");
        assert_eq!(ToolErrorCode::ExecutionFailed.to_string(), "execution_failed");
        assert_eq!(ToolErrorCode::InternalError.to_string(), "internal_error");
        assert_eq!(ToolErrorCode::Unavailable.to_string(), "unavailable");
        assert_eq!(ToolErrorCode::Overloaded.to_string(), "overloaded");
        assert_eq!(ToolErrorCode::Cancelled.to_string(), "cancelled");
    }

    // --- ToolError constructors ---

    #[test]
    fn error_constructors() {
        let e = ToolError::validation("bad input");
        assert_eq!(e.code, ToolErrorCode::ValidationFailed);
        assert!(!e.retryable);

        let e = ToolError::timeout("took too long");
        assert_eq!(e.code, ToolErrorCode::Timeout);
        assert!(!e.retryable);

        let e = ToolError::execution("api failed", true);
        assert_eq!(e.code, ToolErrorCode::ExecutionFailed);
        assert!(e.retryable);

        let e = ToolError::execution("bad data", false);
        assert!(!e.retryable);

        let e = ToolError::internal("panic");
        assert_eq!(e.code, ToolErrorCode::InternalError);
        assert!(!e.retryable);

        let e = ToolError::unavailable("circuit open");
        assert_eq!(e.code, ToolErrorCode::Unavailable);
        assert!(e.retryable);

        let e = ToolError::overloaded("queue full");
        assert_eq!(e.code, ToolErrorCode::Overloaded);
        assert!(e.retryable);

        let e = ToolError::cancelled();
        assert_eq!(e.code, ToolErrorCode::Cancelled);
        assert!(!e.retryable);
    }

    // --- ToolError display ---

    #[test]
    fn error_display_format() {
        let e = ToolError::validation("missing required field 'path'");
        assert_eq!(e.to_string(), "validation_failed: missing required field 'path'");
    }

    // --- ToolContext ---

    #[test]
    fn context_not_cancelled_initially() {
        let ctx = test_context();
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn context_cancelled_after_cancel() {
        let ctx = test_context();
        ctx.cancellation_token.cancel();
        assert!(ctx.is_cancelled());
    }

    // --- ToolHandler blanket impl ---

    #[tokio::test]
    async fn fn_handler_success() {
        let handler = |_ctx: &ToolContext, args: serde_json::Value| async move {
            Ok(json!({"echo": args}))
        };
        let ctx = test_context();
        let result = handler.execute(&ctx, json!({"x": 1})).await.unwrap();
        assert_eq!(result, json!({"echo": {"x": 1}}));
    }

    #[tokio::test]
    async fn fn_handler_error() {
        let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            Err(ToolError::execution("something broke", false))
        };
        let ctx = test_context();
        let err = handler.execute(&ctx, json!({})).await.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::ExecutionFailed);
    }

    #[tokio::test]
    async fn fn_handler_reads_context() {
        let handler = |ctx: &ToolContext, _args: serde_json::Value| {
            let name = ctx.tool_name.clone();
            async move { Ok(json!({"tool": name})) }
        };
        let ctx = test_context();
        let result = handler.execute(&ctx, json!({})).await.unwrap();
        assert_eq!(result, json!({"tool": "test_tool"}));
    }
}
