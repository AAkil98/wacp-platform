use std::future::Future;
use std::pin::Pin;

use tokio_util::sync::CancellationToken;
use wacp_types::WorkspaceId;

/// The handler trait. Tool authors implement this.
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
    pub tool_name: String,
    pub capability_name: String,
    pub workspace_id: Option<WorkspaceId>,
    pub cancellation_token: CancellationToken,
    pub config: serde_json::Value,
    pub timeout_ms: u64,
}

/// Structured tool error.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}: {message}")]
pub struct ToolError {
    pub code: ToolErrorCode,
    pub message: String,
    pub retryable: bool,
}

/// Error category — determines caller behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorCode {
    ValidationFailed,
    Timeout,
    ExecutionFailed,
    InternalError,
    Unavailable,
    Overloaded,
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
