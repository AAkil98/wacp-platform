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

    // --- Pre-cancelled token ---

    #[tokio::test]
    async fn pre_cancelled_token_returns_cancelled_immediately() {
        let cap = test_capability();
        let cancel = CancellationToken::new();
        cancel.cancel(); // cancel BEFORE execute

        let result = execute(
            &echo_handler(),
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

    // --- Result size boundary ---

    #[tokio::test]
    async fn result_exactly_at_limit_passes() {
        let cap = test_capability();
        // Measure the exact serialized size
        let test_value = json!({"v": 1});
        let size = serde_json::to_vec(&test_value).unwrap().len();
        let config = ExecutionConfig {
            max_result_bytes: size, // exactly at limit
            ..default_config()
        };
        let handler =
            |_ctx: &ToolContext, _args: serde_json::Value| async move { Ok(json!({"v": 1})) };
        let result = execute(
            &handler,
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
    async fn result_one_byte_over_limit_fails() {
        let cap = test_capability();
        let test_value = json!({"v": 1});
        let size = serde_json::to_vec(&test_value).unwrap().len();
        let config = ExecutionConfig {
            max_result_bytes: size - 1, // one byte under → fail
            ..default_config()
        };
        let handler =
            |_ctx: &ToolContext, _args: serde_json::Value| async move { Ok(json!({"v": 1})) };
        let result = execute(
            &handler,
            &cap,
            "test",
            &json!({}),
            &config,
            json!({"value": 1}),
            default_opts(),
        )
        .await;
        assert_eq!(result.unwrap_err().code, ToolErrorCode::ExecutionFailed);
    }

    // --- Context population ---

    #[tokio::test]
    async fn context_populated_correctly() {
        let cap = test_capability();
        let context_checker = |ctx: &ToolContext, _args: serde_json::Value| {
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

    // =========================================================================
    // Branch-coverage tests: select! arms, cancel orderings, concurrency
    // =========================================================================

    // --- Cancel-before-start (F1 guard, line ~106) ---

    #[tokio::test]
    async fn pre_cancelled_token_never_invokes_handler() {
        let cap = test_capability();
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count2 = call_count.clone();

        let counting_handler = move |_ctx: &ToolContext, _args: serde_json::Value| {
            let c = call_count2.clone();
            async move {
                c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(json!({"should_not_reach": true}))
            }
        };

        let cancel = CancellationToken::new();
        cancel.cancel(); // cancel BEFORE execute

        let result = execute(
            &counting_handler,
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
        // Handler must NOT have been called — the early guard returned before select!
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    // --- Cancel mid-flight (select! cancel arm) ---

    #[tokio::test]
    async fn cancel_mid_flight_returns_cancelled() {
        let cap = test_capability();
        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();

        // Cancel after a short delay while handler is sleeping
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel2.cancel();
        });

        let result = execute(
            &slow_handler(5_000),
            &cap,
            "test",
            &json!({}),
            &ExecutionConfig {
                default_timeout_ms: 60_000, // very large so cancel wins
                ..default_config()
            },
            json!({"value": 1}),
            ExecutionOptions {
                cancellation_token: Some(cancel),
                ..default_opts()
            },
        )
        .await;

        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::Cancelled);
        assert!(!err.retryable);
    }

    // --- Timeout mid-flight (select! timeout arm) ---

    #[tokio::test]
    async fn timeout_mid_flight_returns_timeout_error() {
        let cap = test_capability();
        // Handler sleeps 500ms, timeout at 50ms
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

        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::Timeout);
        assert!(!err.retryable);
        assert!(err.message.contains("50ms"));
    }

    // --- Timeout arm also cancels the token ---

    #[tokio::test]
    async fn timeout_cancels_the_token() {
        let cap = test_capability();
        let cancel = CancellationToken::new();
        let cancel_probe = cancel.clone();

        let result = execute(
            &slow_handler(500),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            ExecutionOptions {
                timeout_ms: Some(50),
                cancellation_token: Some(cancel),
                ..default_opts()
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, ToolErrorCode::Timeout);
        // After timeout, the token should be cancelled (line 139)
        assert!(cancel_probe.is_cancelled());
    }

    // --- Cancel vs timeout race (both fire ~simultaneously) ---

    #[tokio::test]
    async fn cancel_vs_timeout_race_returns_one_of_two() {
        let cap = test_capability();

        // Run the race multiple times to exercise non-determinism
        for _ in 0..10 {
            let cancel = CancellationToken::new();
            let cancel2 = cancel.clone();

            // Cancel after ~25ms, timeout at 25ms — both fire around the same time
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(25)).await;
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
                    timeout_ms: Some(25),
                    cancellation_token: Some(cancel),
                    ..default_opts()
                },
            )
            .await;

            let err = result.unwrap_err();
            // Either Cancelled or Timeout is acceptable
            assert!(
                err.code == ToolErrorCode::Cancelled || err.code == ToolErrorCode::Timeout,
                "expected Cancelled or Timeout, got {:?}",
                err.code
            );
        }
    }

    // --- Handler returns Ok: normal success path ---

    #[tokio::test]
    async fn handler_returns_ok_propagated() {
        let cap = test_capability();
        let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            Ok(json!({"result": "success", "code": 200}))
        };
        let result = execute(
            &handler,
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            default_opts(),
        )
        .await
        .unwrap();
        assert_eq!(result["result"], "success");
        assert_eq!(result["code"], 200);
    }

    // --- Handler returns Err: error propagated correctly ---

    #[tokio::test]
    async fn handler_returns_retryable_error_propagated() {
        let cap = test_capability();
        let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            Err(ToolError::execution("transient failure", true))
        };
        let result = execute(
            &handler,
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
        assert!(err.message.contains("transient failure"));
    }

    #[tokio::test]
    async fn handler_returns_non_retryable_error_propagated() {
        let cap = test_capability();
        let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            Err(ToolError::execution("permanent failure", false))
        };
        let result = execute(
            &handler,
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
        assert!(!err.retryable);
        assert!(err.message.contains("permanent failure"));
    }

    // --- Panic with String payload (vs &str, vs non-string) ---

    #[tokio::test]
    async fn handler_panic_with_string_payload() {
        let cap = test_capability();
        let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            panic!("{}", String::from("owned string panic"));
        };
        let result = execute(
            &handler,
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
        assert!(err.message.contains("owned string panic"));
    }

    // --- No cancellation token provided (uses default) ---

    #[tokio::test]
    async fn no_cancellation_token_uses_default() {
        let cap = test_capability();
        // ExecutionOptions with cancellation_token = None
        let result = execute(
            &echo_handler(),
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            ExecutionOptions {
                cancellation_token: None,
                ..default_opts()
            },
        )
        .await;
        assert!(result.is_ok());
    }

    // --- Concurrent executions: one cancel doesn't affect others ---

    #[tokio::test]
    async fn concurrent_executions_isolated_cancellation() {
        let cap = test_capability();

        let cancel_a = CancellationToken::new();
        let cancel_a2 = cancel_a.clone();
        let cancel_b = CancellationToken::new();

        // Cancel task A after 20ms
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancel_a2.cancel();
        });

        // Task A: will be cancelled
        let cap_a = cap.clone();
        let handle_a = tokio::spawn(async move {
            let handler = slow_handler(5_000);
            execute(
                &handler,
                &cap_a,
                "test_a",
                &json!({}),
                &ExecutionConfig {
                    default_timeout_ms: 60_000,
                    ..ExecutionConfig::default()
                },
                json!({"value": 1}),
                ExecutionOptions {
                    cancellation_token: Some(cancel_a),
                    ..ExecutionOptions::default()
                },
            )
            .await
        });

        // Task B: should complete normally (short handler, different token)
        let cap_b = cap.clone();
        let handle_b = tokio::spawn(async move {
            let handler = slow_handler(50);
            execute(
                &handler,
                &cap_b,
                "test_b",
                &json!({}),
                &ExecutionConfig {
                    default_timeout_ms: 60_000,
                    ..ExecutionConfig::default()
                },
                json!({"value": 2}),
                ExecutionOptions {
                    cancellation_token: Some(cancel_b),
                    ..ExecutionOptions::default()
                },
            )
            .await
        });

        let result_a = handle_a.await.unwrap();
        let result_b = handle_b.await.unwrap();

        // Task A was cancelled
        assert_eq!(result_a.unwrap_err().code, ToolErrorCode::Cancelled);
        // Task B completed successfully
        assert!(result_b.is_ok());
    }

    // --- Handler that cooperatively checks cancellation ---

    #[tokio::test]
    async fn handler_cooperative_cancellation_via_context() {
        let cap = test_capability();
        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();

        // Handler checks ctx.is_cancelled() in a loop
        let cooperative_handler = |ctx: &ToolContext, _args: serde_json::Value| {
            let token = ctx.cancellation_token.clone();
            async move {
                for i in 0..100 {
                    if token.is_cancelled() {
                        return Err(ToolError::cancelled());
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    if i == 99 {
                        return Ok(json!({"done": true}));
                    }
                }
                Ok(json!({"done": true}))
            }
        };

        // Cancel after 50ms — handler should detect within its loop
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel2.cancel();
        });

        let result = execute(
            &cooperative_handler,
            &cap,
            "test",
            &json!({}),
            &ExecutionConfig {
                default_timeout_ms: 60_000,
                ..default_config()
            },
            json!({"value": 1}),
            ExecutionOptions {
                cancellation_token: Some(cancel),
                ..default_opts()
            },
        )
        .await;

        let err = result.unwrap_err();
        assert_eq!(err.code, ToolErrorCode::Cancelled);
    }

    // --- Timeout hierarchy: invocation timeout capped by max ---

    #[test]
    fn invocation_timeout_capped_by_max() {
        let config = ExecutionConfig {
            default_timeout_ms: 30_000,
            max_timeout_ms: 5_000,
            ..default_config()
        };
        // invocation requests 10_000, but max is 5_000
        assert_eq!(resolve_timeout(Some(10_000), None, &config), 5_000);
    }

    #[test]
    fn capability_timeout_capped_by_max() {
        let config = ExecutionConfig {
            default_timeout_ms: 30_000,
            max_timeout_ms: 5_000,
            ..default_config()
        };
        // capability requests 8_000, but max is 5_000
        assert_eq!(resolve_timeout(None, Some(8_000), &config), 5_000);
    }

    // --- Context timeout reflects resolved timeout ---

    #[tokio::test]
    async fn context_receives_resolved_timeout() {
        let cap = Capability {
            timeout_ms: Some(7_000),
            ..test_capability()
        };
        let handler = |ctx: &ToolContext, _args: serde_json::Value| {
            let timeout = ctx.timeout_ms;
            async move { Ok(json!({"timeout": timeout})) }
        };
        let config = ExecutionConfig {
            default_timeout_ms: 30_000,
            max_timeout_ms: 300_000,
            ..default_config()
        };
        let result = execute(
            &handler,
            &cap,
            "test",
            &json!({}),
            &config,
            json!({"value": 1}),
            // invocation timeout not set, capability is 7000
            default_opts(),
        )
        .await
        .unwrap();
        assert_eq!(result["timeout"], 7_000);
    }

    // --- Multiple concurrent executions, all succeed independently ---

    #[tokio::test]
    async fn multiple_concurrent_all_succeed() {
        let cap = test_capability();
        let mut handles = Vec::new();

        for i in 0..5 {
            let cap = cap.clone();
            handles.push(tokio::spawn(async move {
                let handler =
                    |_ctx: &ToolContext, args: serde_json::Value| async move { Ok(args) };
                execute(
                    &handler,
                    &cap,
                    "test",
                    &json!({}),
                    &ExecutionConfig::default(),
                    json!({"value": i}),
                    ExecutionOptions::default(),
                )
                .await
            }));
        }

        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await.unwrap().unwrap();
            assert_eq!(result["value"], i);
        }
    }

    // --- Workspace ID flows through to context ---

    #[tokio::test]
    async fn workspace_id_flows_to_context() {
        let cap = test_capability();
        let handler = |ctx: &ToolContext, _args: serde_json::Value| {
            let ws = ctx.workspace_id.clone();
            async move { Ok(json!({"has_ws": ws.is_some()})) }
        };
        let result = execute(
            &handler,
            &cap,
            "test",
            &json!({}),
            &default_config(),
            json!({"value": 1}),
            ExecutionOptions {
                workspace_id: Some(wacp_types::WorkspaceId::new("ws-test-123")),
                ..default_opts()
            },
        )
        .await
        .unwrap();
        assert_eq!(result["has_ws"], true);
    }
}
