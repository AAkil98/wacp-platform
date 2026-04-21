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
async fn panic_message_exactly_4096_bytes_is_not_truncated() {
    // Boundary test for the `msg.len() > 4096` check in invoke_with_timeout
    // (execution.rs:126). A message of exactly 4096 bytes must pass through
    // unchanged — only lengths STRICTLY greater than 4096 get truncated to
    // "<4093-byte-prefix>..." Pins the `>` vs `>=` semantics.
    let cap = test_capability();
    let exact_panic_handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        panic!("{}", "x".repeat(4096));
    };
    let result = execute(
        &exact_panic_handler,
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
    assert_eq!(
        err.message.len(),
        4096,
        "4096-byte message length preserved"
    );
    assert!(
        !err.message.ends_with("..."),
        "4096-byte message must NOT be truncated — current message: ends_with('...')",
    );
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
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move { Ok(json!({"v": 1})) };
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
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move { Ok(json!({"v": 1})) };
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
            let handler = |_ctx: &ToolContext, args: serde_json::Value| async move { Ok(args) };
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
