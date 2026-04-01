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
    let cancel = opts
        .cancellation_token
        .unwrap_or_else(CancellationToken::new);
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
fn validate_input(
    args: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), ToolError> {
    jsonschema::validate(schema, args).map_err(|e| {
        ToolError::validation(format!("input validation failed: {e}"))
    })
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
fn check_result_size(
    result: &serde_json::Value,
    max_bytes: usize,
) -> Result<(), ToolError> {
    let size = serde_json::to_vec(result)
        .map(|v| v.len())
        .unwrap_or(0);

    if size > max_bytes {
        return Err(ToolError::execution(
            format!(
                "result size ({size} bytes) exceeds maximum ({max_bytes} bytes)"
            ),
            false,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::ToolErrorCode;
    use serde_json::json;

    fn test_capability() -> Capability {
        Capability {
            name: "run".into(),
            description: "Run test".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "value": {"type": "integer"}
                },
                "required": ["value"]
            }),
            output_schema: json!({"type": "object"}),
            timeout_ms: None,
            idempotent: false,
            side_effects: false,
        }
    }

    fn echo_handler() -> impl ToolHandler {
        |_ctx: &ToolContext, args: serde_json::Value| async move { Ok(args) }
    }

    fn error_handler() -> impl ToolHandler {
        |_ctx: &ToolContext, _args: serde_json::Value| async move {
            Err(ToolError::execution("handler failed", true))
        }
    }

    fn slow_handler(ms: u64) -> impl ToolHandler {
        move |_ctx: &ToolContext, _args: serde_json::Value| async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            Ok(json!({"done": true}))
        }
    }

    fn default_config() -> ExecutionConfig {
        ExecutionConfig::default()
    }

    fn default_opts() -> ExecutionOptions {
        ExecutionOptions::default()
    }

    // --- Input validation ---

    #[tokio::test]
    async fn valid_input_passes() {
        let cap = test_capability();
        let result = execute(
            &echo_handler(),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 42}),
            default_opts(),
        )
        .await;
        assert_eq!(result.unwrap(), json!({"value": 42}));
    }

    #[tokio::test]
    async fn invalid_input_rejected() {
        let cap = test_capability();
        let result = execute(
            &echo_handler(),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": "not an integer"}),
            default_opts(),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::ValidationFailed);
        assert!(!err.retryable);
    }

    #[tokio::test]
    async fn missing_required_field_rejected() {
        let cap = test_capability();
        let result = execute(
            &echo_handler(),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({}),
            default_opts(),
        )
        .await;
        assert_eq!(result.unwrap_err().code, ToolErrorCode::ValidationFailed);
    }

    // --- Timeout ---

    #[tokio::test]
    async fn handler_within_timeout_succeeds() {
        let cap = test_capability();
        let config = ExecutionConfig {
            default_timeout_ms: 1_000,
            ..default_config()
        };
        let result = execute(
            &slow_handler(10),
            &cap,
            "test",
            &json!({}),
            &config,
            json!({"value": 1}),
            default_opts(),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handler_exceeding_timeout_fails() {
        let cap = test_capability();
        let result = execute(
            &slow_handler(500),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            ExecutionOptions {
                timeout_ms: Some(50),
                ..default_opts()
            },
        )
        .await;
        assert_eq!(result.unwrap_err().code, ToolErrorCode::Timeout);
    }

    // --- Timeout hierarchy ---

    #[test]
    fn timeout_invocation_overrides_capability() {
        let config = ExecutionConfig {
            default_timeout_ms: 30_000,
            max_timeout_ms: 300_000,
            ..default_config()
        };
        assert_eq!(resolve_timeout(Some(5_000), Some(10_000), &config), 5_000);
    }

    #[test]
    fn timeout_capability_overrides_default() {
        let config = ExecutionConfig {
            default_timeout_ms: 30_000,
            max_timeout_ms: 300_000,
            ..default_config()
        };
        assert_eq!(resolve_timeout(None, Some(10_000), &config), 10_000);
    }

    #[test]
    fn timeout_default_used_when_none_specified() {
        let config = ExecutionConfig {
            default_timeout_ms: 30_000,
            max_timeout_ms: 300_000,
            ..default_config()
        };
        assert_eq!(resolve_timeout(None, None, &config), 30_000);
    }

    #[test]
    fn timeout_capped_by_max() {
        let config = ExecutionConfig {
            default_timeout_ms: 30_000,
            max_timeout_ms: 100_000,
            ..default_config()
        };
        assert_eq!(resolve_timeout(Some(500_000), None, &config), 100_000);
    }

    // --- Cancellation ---

    #[tokio::test]
    async fn external_cancellation_stops_handler() {
        let cap = test_capability();
        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();

        // Cancel after 20ms
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancel2.cancel();
        });

        let result = execute(
            &slow_handler(5_000),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            ExecutionOptions {
                cancellation_token: Some(cancel),
                ..default_opts()
            },
        )
        .await;
        assert_eq!(result.unwrap_err().code, ToolErrorCode::Cancelled);
    }

    // --- Result size ---

    #[tokio::test]
    async fn result_within_limit_passes() {
        let cap = test_capability();
        let result = execute(
            &echo_handler(),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            default_opts(),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn result_exceeding_limit_fails() {
        let cap = test_capability();
        let large_handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            // Generate a result > 100 bytes
            Ok(json!({"data": "x".repeat(200)}))
        };
        let config = ExecutionConfig {
            max_result_bytes: 100,
            ..default_config()
        };
        let result = execute(
            &large_handler,
            &cap,
            "test",
            &json!({}),
            &config,
            json!({"value": 1}),
            default_opts(),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::ExecutionFailed);
        assert!(!err.retryable);
    }

    // --- Handler errors ---

    #[tokio::test]
    async fn handler_error_propagated() {
        let cap = test_capability();
        let result = execute(
            &error_handler(),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            default_opts(),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::ExecutionFailed);
        assert!(err.retryable);
    }

    // --- Handler panic ---

    #[tokio::test]
    async fn handler_panic_returns_internal_error() {
        let cap = test_capability();
        let panicking_handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            panic!("something went terribly wrong");
        };
        let result = execute(
            &panicking_handler,
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            default_opts(),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::InternalError);
        assert!(!err.retryable);
        assert!(err.message.contains("something went terribly wrong"));
    }

    #[tokio::test]
    async fn panic_message_truncated_at_4096() {
        let cap = test_capability();
        let long_panic_handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            panic!("{}", "x".repeat(8000));
        };
        let result = execute(
            &long_panic_handler,
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            default_opts(),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::InternalError);
        // 4093 chars + "..." = 4096
        assert!(err.message.len() <= 4096);
        assert!(err.message.ends_with("..."));
    }

    #[tokio::test]
    async fn handler_panic_without_message() {
        let cap = test_capability();
        let panicking_handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            std::panic::panic_any(42_i32); // non-string panic payload
        };
        let result = execute(
            &panicking_handler,
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            default_opts(),
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::InternalError);
        assert_eq!(err.message, "handler panicked");
    }

    // --- Context population ---

    #[tokio::test]
    async fn context_populated_correctly() {
        let cap = test_capability();
        let context_checker =
            |ctx: &ToolContext, _args: serde_json::Value| {
                let name = ctx.tool_name.clone();
                let cap_name = ctx.capability_name.clone();
                let ws = ctx.workspace_id.clone();
                async move {
                    Ok(json!({
                        "tool": name,
                        "cap": cap_name,
                        "has_ws": ws.is_some(),
                    }))
                }
            };
        let result = execute(
            &context_checker,
            &cap,
            "my_tool",
            &json!({"key": "val"}),
            &default_config(),
            json!({"value": 1}),
            default_opts(),
        )
        .await
        .unwrap();
        assert_eq!(result["tool"], "my_tool");
        assert_eq!(result["cap"], "run");
        assert_eq!(result["has_ws"], false);
    }
}
