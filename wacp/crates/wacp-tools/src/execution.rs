use std::panic::AssertUnwindSafe;
use std::time::Duration;

use futures::FutureExt;
use tokio_util::sync::CancellationToken;

use crate::descriptor::Capability;
use crate::handler::{ToolContext, ToolError, ToolHandler};

/// Framework-level execution defaults.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// Default timeout when neither invocation nor capability specifies one.
    pub default_timeout_ms: u64,
    /// Maximum timeout — caps everything.
    pub max_timeout_ms: u64,
    /// Maximum result size in bytes.
    pub max_result_bytes: usize,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30_000,
            max_timeout_ms: 300_000,
            max_result_bytes: 1_048_576, // 1 MB
        }
    }
}

/// Per-invocation options provided by the caller.
#[derive(Debug, Clone, Default)]
pub struct ExecutionOptions {
    /// Override timeout for this invocation.
    pub timeout_ms: Option<u64>,
    /// Workspace context.
    pub workspace_id: Option<wacp_types::WorkspaceId>,
    /// External cancellation token.
    pub cancellation_token: Option<CancellationToken>,
}

/// Execute a handler with full mediation: validate → timeout → invoke → check result.
pub async fn execute(
    handler: &dyn ToolHandler,
    capability: &Capability,
    tool_name: &str,
    config: &serde_json::Value,
    framework_config: &ExecutionConfig,
    args: serde_json::Value,
    opts: ExecutionOptions,
) -> Result<serde_json::Value, ToolError> {
    // 1. Input validation
    validate_input(&args, &capability.input_schema)?;

    // 2. Resolve timeout
    let timeout_ms = resolve_timeout(opts.timeout_ms, capability.timeout_ms, framework_config);

    // 3. Build context
    let cancel = opts.cancellation_token.unwrap_or_default();
    let ctx = ToolContext {
        tool_name: tool_name.to_string(),
        capability_name: capability.name.clone(),
        workspace_id: opts.workspace_id,
        cancellation_token: cancel.clone(),
        config: config.clone(),
        timeout_ms,
    };

    // 4. Invoke with timeout + panic catching
    let result = invoke_with_timeout(handler, &ctx, args, timeout_ms, &cancel).await?;

    // 5. Check result size
    check_result_size(&result, framework_config.max_result_bytes)?;

    Ok(result)
}

/// Validate invocation args against the capability's input schema.
fn validate_input(args: &serde_json::Value, schema: &serde_json::Value) -> Result<(), ToolError> {
    jsonschema::validate(schema, args)
        .map_err(|e| ToolError::validation(format!("input validation failed: {e}")))
}

/// Resolve the effective timeout from the three-level hierarchy.
fn resolve_timeout(
    invocation: Option<u64>,
    capability: Option<u64>,
    config: &ExecutionConfig,
) -> u64 {
    let base = invocation
        .or(capability)
        .unwrap_or(config.default_timeout_ms);
    base.min(config.max_timeout_ms)
}

/// Invoke the handler with timeout enforcement and panic catching.
async fn invoke_with_timeout(
    handler: &dyn ToolHandler,
    ctx: &ToolContext,
    args: serde_json::Value,
    timeout_ms: u64,
    cancel: &CancellationToken,
) -> Result<serde_json::Value, ToolError> {
    let duration = Duration::from_millis(timeout_ms);

    if cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }

    let handler_future = async {
        // Catch panics from both future creation AND future execution.
        // AssertUnwindSafe + FutureExt::catch_unwind catches panics during .await.
        let future = handler.execute(ctx, args);
        let result = AssertUnwindSafe(future).catch_unwind().await;
        match result {
            Ok(inner) => inner,
            Err(panic_info) => {
                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "handler panicked".to_string()
                };
                // Truncate to 4096 bytes
                let truncated = if msg.len() > 4096 {
                    format!("{}...", &msg[..4093])
                } else {
                    msg
                };
                Err(ToolError::internal(truncated))
            }
        }
    };

    tokio::select! {
        result = handler_future => result,
        () = tokio::time::sleep(duration) => {
            cancel.cancel();
            Err(ToolError::timeout(format!(
                "handler exceeded timeout of {timeout_ms}ms"
            )))
        }
        () = cancel.cancelled() => {
            Err(ToolError::cancelled())
        }
    }
}

/// Check that the serialized result does not exceed the size limit.
fn check_result_size(result: &serde_json::Value, max_bytes: usize) -> Result<(), ToolError> {
    let size = serde_json::to_vec(result).map(|v| v.len()).unwrap_or(0);

    if size > max_bytes {
        return Err(ToolError::execution(
            format!("result size ({size} bytes) exceeds maximum ({max_bytes} bytes)"),
            false,
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
