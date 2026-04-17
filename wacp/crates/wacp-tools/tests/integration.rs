//! Cross-module integration tests for the tool framework.
//!
//! These test the full pipeline: register → execute → verify result + resilience.

use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use wacp_tools::descriptor::Capability;
use wacp_tools::handler::ToolContext;
use wacp_tools::resilience::CircuitBreakerConfig;
use wacp_tools::*;

fn test_cap(name: &str, schema: serde_json::Value) -> Capability {
    Capability {
        name: name.into(),
        description: format!("{name} capability"),
        input_schema: schema,
        output_schema: json!({"type": "object"}),
        timeout_ms: None,
        idempotent: false,
        side_effects: false,
    }
}

fn test_desc(name: &str, caps: Vec<Capability>) -> ToolDescriptor {
    ToolDescriptor {
        name: name.into(),
        version: "1.0.0".into(),
        description: format!("{name} tool"),
        capabilities: caps,
        config_schema: None,
        tags: vec![],
    }
}

// ── Full lifecycle: register → execute → verify ─────────────────────────────

#[tokio::test]
async fn full_tool_lifecycle() {
    let handler = |_ctx: &ToolContext, args: serde_json::Value| async move {
        let x = args["x"].as_i64().unwrap();
        let y = args["y"].as_i64().unwrap();
        Ok(json!({"sum": x + y}))
    };

    let cap = test_cap(
        "add",
        json!({
            "type": "object",
            "properties": {"x": {"type": "integer"}, "y": {"type": "integer"}},
            "required": ["x", "y"]
        }),
    );

    let pkg = PackageBuilder::new(test_desc("math", vec![cap]))
        .handler("add", handler)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig::default());
    registry.register(pkg).await.unwrap();

    let result = registry
        .execute(
            "math",
            "add",
            json!({"x": 3, "y": 4}),
            ExecutionOptions::default(),
        )
        .await
        .unwrap();

    assert_eq!(result["sum"], 7);
}

// ── Input validation rejects before handler runs ────────────────────────────

#[tokio::test]
async fn input_validation_prevents_handler_execution() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count2 = call_count.clone();

    let handler = move |_ctx: &ToolContext, _args: serde_json::Value| {
        let c = call_count2.clone();
        async move {
            c.fetch_add(1, Ordering::Relaxed);
            Ok(json!({}))
        }
    };

    let cap = test_cap(
        "strict",
        json!({
            "type": "object",
            "properties": {"required_field": {"type": "string"}},
            "required": ["required_field"]
        }),
    );

    let pkg = PackageBuilder::new(test_desc("strict_tool", vec![cap]))
        .handler("strict", handler)
        .build()
        .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig::default());
    registry.register(pkg).await.unwrap();

    // Invalid input → handler never called
    let err = registry
        .execute(
            "strict_tool",
            "strict",
            json!({}),
            ExecutionOptions::default(),
        )
        .await
        .unwrap_err();

    assert_eq!(
        err.code,
        wacp_tools::handler::ToolErrorCode::ValidationFailed
    );
    assert_eq!(call_count.load(Ordering::Relaxed), 0);
}

// ── Config flows from registry to handler context ───────────────────────────

#[tokio::test]
async fn config_flows_through_to_handler_context() {
    let handler = |ctx: &ToolContext, _args: serde_json::Value| {
        let config = ctx.config.clone();
        async move { Ok(config) }
    };

    let mut desc = test_desc(
        "configured",
        vec![test_cap("run", json!({"type": "object"}))],
    );
    desc.config_schema = Some(json!({
        "type": "object",
        "properties": {"api_url": {"type": "string"}}
    }));

    let pkg = PackageBuilder::new(desc)
        .handler("run", handler)
        .build()
        .unwrap();

    let mut config = RegistryConfig::default();
    config.tool_configs.insert(
        "configured".into(),
        json!({"api_url": "https://api.example.com"}),
    );

    let mut registry = ToolRegistry::new(config);
    registry.register(pkg).await.unwrap();

    let result = registry
        .execute("configured", "run", json!({}), ExecutionOptions::default())
        .await
        .unwrap();

    assert_eq!(result["api_url"], "https://api.example.com");
}

// ── Circuit breaker + execution integration ─────────────────────────────────

#[tokio::test]
async fn circuit_breaker_blocks_after_repeated_failures() {
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        Err(wacp_tools::ToolError::execution("always fails", false))
    };

    let pkg = PackageBuilder::new(test_desc(
        "flaky",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig {
        default_circuit_breaker: CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 3,
            cooldown: Duration::from_secs(60),
            failure_window: Duration::from_secs(60),
        },
        ..RegistryConfig::default()
    });
    registry.register(pkg).await.unwrap();

    // 3 failures trip the breaker
    for _ in 0..3 {
        let _ = registry
            .execute("flaky", "run", json!({}), ExecutionOptions::default())
            .await;
    }

    // 4th call → circuit breaker blocks
    let err = registry
        .execute("flaky", "run", json!({}), ExecutionOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Unavailable);
}

// ── Timeout + cancellation integration ──────────────────────────────────────

#[tokio::test]
async fn timeout_cancels_long_running_handler() {
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(json!({"should_not_reach": true}))
    };

    let pkg = PackageBuilder::new(test_desc(
        "slow",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig {
        execution: ExecutionConfig {
            default_timeout_ms: 50,
            max_timeout_ms: 100,
            ..ExecutionConfig::default()
        },
        ..RegistryConfig::default()
    });
    registry.register(pkg).await.unwrap();

    let err = registry
        .execute("slow", "run", json!({}), ExecutionOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Timeout);
}

// ── Multi-capability tool ───────────────────────────────────────────────────

#[tokio::test]
async fn multi_capability_tool_routes_correctly() {
    let read_handler =
        |_ctx: &ToolContext, _args: serde_json::Value| async move { Ok(json!({"action": "read"})) };
    let write_handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        Ok(json!({"action": "write"}))
    };

    let pkg = PackageBuilder::new(test_desc(
        "file_tool",
        vec![
            test_cap("read", json!({"type": "object"})),
            test_cap("write", json!({"type": "object"})),
        ],
    ))
    .handler("read", read_handler)
    .handler("write", write_handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig::default());
    registry.register(pkg).await.unwrap();

    let r1 = registry
        .execute("file_tool", "read", json!({}), ExecutionOptions::default())
        .await
        .unwrap();
    let r2 = registry
        .execute("file_tool", "write", json!({}), ExecutionOptions::default())
        .await
        .unwrap();

    assert_eq!(r1["action"], "read");
    assert_eq!(r2["action"], "write");
}

// =========================================================================
// Branch-coverage integration tests: retry classifier, cancel isolation,
// select! arm orderings through the full registry stack.
// =========================================================================

// ── Retry classifier: Timeout counts as circuit-breaker failure ────────

#[tokio::test]
async fn timeout_counts_for_circuit_breaker() {
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(json!({"should_not_reach": true}))
    };

    let pkg = PackageBuilder::new(test_desc(
        "timeout_cb",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig {
        execution: ExecutionConfig {
            default_timeout_ms: 20,
            max_timeout_ms: 50,
            ..ExecutionConfig::default()
        },
        default_circuit_breaker: CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 2,
            cooldown: Duration::from_secs(60),
            failure_window: Duration::from_secs(60),
        },
        ..RegistryConfig::default()
    });
    registry.register(pkg).await.unwrap();

    // Two timeouts should trip the circuit breaker
    for _ in 0..2 {
        let err = registry
            .execute("timeout_cb", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Timeout);
    }

    // Third call: breaker should be open
    let err = registry
        .execute("timeout_cb", "run", json!({}), ExecutionOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Unavailable);
}

// ── Retry classifier: InternalError (panic) counts for circuit breaker ─

#[tokio::test]
async fn panic_counts_for_circuit_breaker() {
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        panic!("handler crashed");
    };

    let pkg = PackageBuilder::new(test_desc(
        "panicky_cb",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig {
        default_circuit_breaker: CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 2,
            cooldown: Duration::from_secs(60),
            failure_window: Duration::from_secs(60),
        },
        ..RegistryConfig::default()
    });
    registry.register(pkg).await.unwrap();

    // Two panics should trip the circuit breaker
    for _ in 0..2 {
        let err = registry
            .execute("panicky_cb", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::InternalError);
    }

    // Breaker open
    let err = registry
        .execute("panicky_cb", "run", json!({}), ExecutionOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Unavailable);
}

// ── Retry classifier: Cancelled does NOT count for circuit breaker ─────

#[tokio::test]
async fn cancelled_does_not_count_for_circuit_breaker() {
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(json!({"should_not_reach": true}))
    };

    let pkg = PackageBuilder::new(test_desc(
        "cancel_cb",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig {
        default_circuit_breaker: CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 2,
            cooldown: Duration::from_secs(60),
            failure_window: Duration::from_secs(60),
        },
        ..RegistryConfig::default()
    });
    registry.register(pkg).await.unwrap();

    // Five cancellations should NOT trip the circuit breaker
    for _ in 0..5 {
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel(); // pre-cancelled

        let err = registry
            .execute(
                "cancel_cb",
                "run",
                json!({}),
                ExecutionOptions {
                    cancellation_token: Some(cancel),
                    ..ExecutionOptions::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Cancelled);
    }

    // Breaker should still be closed — verify the 6th call is NOT Unavailable.
    // We use another pre-cancelled call; the key is the error code stays Cancelled
    // (not Unavailable, which would mean the breaker tripped).
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let err = registry
        .execute(
            "cancel_cb",
            "run",
            json!({}),
            ExecutionOptions {
                cancellation_token: Some(cancel),
                ..ExecutionOptions::default()
            },
        )
        .await
        .unwrap_err();
    // Still Cancelled, NOT Unavailable — breaker is still closed
    assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Cancelled);
}

// ── Retry exhaustion: N failures then breaker blocks ───────────────────

#[tokio::test]
async fn retry_exhaustion_at_exact_threshold() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count2 = call_count.clone();

    let handler = move |_ctx: &ToolContext, _args: serde_json::Value| {
        let c = call_count2.clone();
        async move {
            c.fetch_add(1, Ordering::Relaxed);
            Err(wacp_tools::ToolError::execution("always fails", false))
        }
    };

    let pkg = PackageBuilder::new(test_desc(
        "exhaust_tool",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let threshold = 5u32;
    let mut registry = ToolRegistry::new(RegistryConfig {
        default_circuit_breaker: CircuitBreakerConfig {
            enabled: true,
            failure_threshold: threshold,
            cooldown: Duration::from_secs(60),
            failure_window: Duration::from_secs(60),
        },
        ..RegistryConfig::default()
    });
    registry.register(pkg).await.unwrap();

    // Exactly `threshold` failures
    for i in 0..threshold {
        let err = registry
            .execute(
                "exhaust_tool",
                "run",
                json!({}),
                ExecutionOptions::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(
            err.code,
            wacp_tools::handler::ToolErrorCode::ExecutionFailed,
            "call {i} should be ExecutionFailed, not breaker"
        );
    }

    assert_eq!(call_count.load(Ordering::Relaxed), threshold);

    // Next call: breaker is open, handler is NOT called
    let err = registry
        .execute(
            "exhaust_tool",
            "run",
            json!({}),
            ExecutionOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Unavailable);
    assert_eq!(
        call_count.load(Ordering::Relaxed),
        threshold,
        "handler should not be called when breaker is open"
    );
}

// ── Retry success on Nth attempt (half-open probe) ─────────────────────

#[tokio::test]
async fn retry_success_on_nth_attempt_resets_breaker() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count2 = call_count.clone();

    // Fails first 3 calls, succeeds from call 4 onward
    let handler = move |_ctx: &ToolContext, _args: serde_json::Value| {
        let count = call_count2.fetch_add(1, Ordering::Relaxed);
        async move {
            if count < 3 {
                Err(wacp_tools::ToolError::execution("transient", true))
            } else {
                Ok(json!({"recovered": true}))
            }
        }
    };

    let pkg = PackageBuilder::new(test_desc(
        "recovering",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig {
        default_circuit_breaker: CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 3,
            cooldown: Duration::from_millis(0), // instant cooldown for test
            failure_window: Duration::from_secs(60),
        },
        ..RegistryConfig::default()
    });
    registry.register(pkg).await.unwrap();

    // 3 failures trip the breaker
    for _ in 0..3 {
        let _ = registry
            .execute("recovering", "run", json!({}), ExecutionOptions::default())
            .await;
    }

    // Cooldown is 0 -> half-open -> probe call (4th) succeeds -> closes breaker
    let result = registry
        .execute("recovering", "run", json!({}), ExecutionOptions::default())
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap()["recovered"], true);

    // Breaker is closed again — 5th call also succeeds
    let result = registry
        .execute("recovering", "run", json!({}), ExecutionOptions::default())
        .await;
    assert!(result.is_ok());
}

// ── Concurrent executions: one cancelled doesn't affect others ─────────

#[tokio::test]
async fn concurrent_cancel_isolation_through_registry() {
    let echo_handler_fn = |_ctx: &ToolContext, args: serde_json::Value| async move {
        Ok::<serde_json::Value, wacp_tools::ToolError>(args)
    };
    let slow_handler_fn = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(json!({"slow": true}))
    };

    // Register two tools: one will be cancelled, one will run normally
    let pkg_slow = PackageBuilder::new(test_desc(
        "slow_iso",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", slow_handler_fn)
    .build()
    .unwrap();

    let pkg_fast = PackageBuilder::new(test_desc(
        "fast_iso",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", echo_handler_fn)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig::default());
    registry.register(pkg_slow).await.unwrap();
    registry.register(pkg_fast).await.unwrap();
    let registry = Arc::new(registry);

    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel2 = cancel.clone();

    // Spawn slow tool with cancel
    let reg1 = registry.clone();
    let handle_slow = tokio::spawn(async move {
        reg1.execute(
            "slow_iso",
            "run",
            json!({}),
            ExecutionOptions {
                cancellation_token: Some(cancel),
                ..ExecutionOptions::default()
            },
        )
        .await
    });

    // Spawn fast tool without cancel
    let reg2 = registry.clone();
    let handle_fast = tokio::spawn(async move {
        reg2.execute(
            "fast_iso",
            "run",
            json!({"value": 42}),
            ExecutionOptions::default(),
        )
        .await
    });

    // Cancel the slow tool
    tokio::time::sleep(Duration::from_millis(20)).await;
    cancel2.cancel();

    let result_slow = handle_slow.await.unwrap();
    let result_fast = handle_fast.await.unwrap();

    // Slow tool was cancelled
    assert_eq!(
        result_slow.unwrap_err().code,
        wacp_tools::handler::ToolErrorCode::Cancelled
    );
    // Fast tool completed normally
    assert_eq!(result_fast.unwrap()["value"], 42);
}

// ── Pre-cancelled through registry: immediate return ───────────────────

#[tokio::test]
async fn pre_cancelled_through_registry_returns_immediately() {
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok(json!({"should_not_reach": true}))
    };

    let pkg = PackageBuilder::new(test_desc(
        "pre_cancel",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig::default());
    registry.register(pkg).await.unwrap();

    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel(); // pre-cancel

    let start = std::time::Instant::now();
    let err = registry
        .execute(
            "pre_cancel",
            "run",
            json!({}),
            ExecutionOptions {
                cancellation_token: Some(cancel),
                ..ExecutionOptions::default()
            },
        )
        .await
        .unwrap_err();

    assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Cancelled);
    // Should return near-instantly, definitely under 1 second
    assert!(start.elapsed() < Duration::from_secs(1));
}

// ── Handler panic through registry: caught, InternalError returned ─────

#[tokio::test]
async fn panic_through_registry_returns_internal_error() {
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        panic!("registry-level panic test");
    };

    let pkg = PackageBuilder::new(test_desc(
        "panic_tool",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig::default());
    registry.register(pkg).await.unwrap();

    let err = registry
        .execute("panic_tool", "run", json!({}), ExecutionOptions::default())
        .await
        .unwrap_err();

    assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::InternalError);
    assert!(err.message.contains("registry-level panic test"));
    assert!(!err.retryable);
}

// ── Overloaded does NOT count for circuit breaker ──────────────────────

#[tokio::test]
async fn overloaded_does_not_count_for_circuit_breaker() {
    let handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(json!({"ok": true}))
    };

    let pkg = PackageBuilder::new(test_desc(
        "overload_cb",
        vec![test_cap("run", json!({"type": "object"}))],
    ))
    .handler("run", handler)
    .build()
    .unwrap();

    let mut registry = ToolRegistry::new(RegistryConfig {
        default_concurrency: wacp_tools::ConcurrencyConfig {
            max_concurrent: 1,
            max_queued: 0,
        },
        default_circuit_breaker: CircuitBreakerConfig {
            enabled: true,
            failure_threshold: 2,
            cooldown: Duration::from_secs(60),
            failure_window: Duration::from_secs(60),
        },
        ..RegistryConfig::default()
    });
    registry.register(pkg).await.unwrap();
    let registry = Arc::new(registry);

    // Fill the single permit
    let reg2 = registry.clone();
    let _hold = tokio::spawn(async move {
        reg2.execute("overload_cb", "run", json!({}), ExecutionOptions::default())
            .await
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // These will be Overloaded — should NOT trip the breaker
    for _ in 0..5 {
        let err = registry
            .execute("overload_cb", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.code, wacp_tools::handler::ToolErrorCode::Overloaded);
    }

    // Wait for the held permit to finish
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Breaker should still be closed — next call succeeds
    let result = registry
        .execute("overload_cb", "run", json!({}), ExecutionOptions::default())
        .await;
    assert!(result.is_ok());
}
