//! Cross-module integration tests for the tool framework.
//!
//! These test the full pipeline: register → execute → verify result + resilience.

use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use wacp_tools::descriptor::Capability;
use wacp_tools::handler::{ToolContext, ToolHandler};
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
