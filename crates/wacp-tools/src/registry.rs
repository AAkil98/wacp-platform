use std::collections::HashMap;

use crate::descriptor::ToolDescriptor;
use crate::execution::{self, ExecutionConfig, ExecutionOptions};
use crate::handler::{ToolError, ToolHandler};
use crate::package::{PackageError, ToolPackage};
use crate::resilience::{
    CircuitBreaker, CircuitBreakerConfig, ConcurrencyConfig, ConcurrencyLimiter,
};

/// Central data structure: holds loaded tools, resolves invocations, manages lifecycle.
pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
    config: RegistryConfig,
}

/// Framework-level configuration for the registry.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Per-tool deployment configuration. Tool name → config value.
    pub tool_configs: HashMap<String, serde_json::Value>,
    /// Execution defaults.
    pub execution: ExecutionConfig,
    /// Default concurrency config for tools that don't override.
    pub default_concurrency: ConcurrencyConfig,
    /// Default circuit breaker config for tools that don't override.
    pub default_circuit_breaker: CircuitBreakerConfig,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            tool_configs: HashMap::new(),
            execution: ExecutionConfig::default(),
            default_concurrency: ConcurrencyConfig::default(),
            default_circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

/// A tool loaded into the registry with its runtime state.
struct RegisteredTool {
    descriptor: ToolDescriptor,
    handlers: HashMap<String, Box<dyn ToolHandler>>,
    config: serde_json::Value,
    circuit_breaker: CircuitBreaker,
    concurrency: ConcurrencyLimiter,
}

/// Errors during registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("package error: {0}")]
    Package(#[from] PackageError),

    #[error("tool '{name}' is already registered")]
    DuplicateName { name: String },

    #[error("tool '{name}' not found")]
    NotFound { name: String },

    #[error("initialize failed for '{name}': {error}")]
    InitializeFailed { name: String, error: ToolError },
}

impl ToolRegistry {
    /// Create a new empty registry.
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            tools: HashMap::new(),
            config,
        }
    }

    /// Register a tool package. Validates, injects config, calls initialize.
    pub async fn register(&mut self, package: ToolPackage) -> Result<(), RegistryError> {
        // 1. Validate package (descriptor + handler alignment)
        package.validate()?;

        let name = package.descriptor.name.clone();

        // 2. Check for duplicates
        if self.tools.contains_key(&name) {
            return Err(RegistryError::DuplicateName { name });
        }

        // 3. Resolve and validate config
        let tool_config = self
            .config
            .tool_configs
            .get(&name)
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        if let Some(schema) = &package.descriptor.config_schema {
            jsonschema::validate(schema, &tool_config).map_err(|e| {
                PackageError::InvalidConfig {
                    reason: e.to_string(),
                }
            })?;
        }

        // 4. Call initialize if present
        if let Some(init) = package.initialize {
            init(tool_config.clone()).await.map_err(|e| {
                RegistryError::InitializeFailed {
                    name: name.clone(),
                    error: e,
                }
            })?;
        }

        // 5. Register
        self.tools.insert(
            name,
            RegisteredTool {
                descriptor: package.descriptor,
                handlers: package.handlers,
                config: tool_config,
                circuit_breaker: CircuitBreaker::new(self.config.default_circuit_breaker.clone()),
                concurrency: ConcurrencyLimiter::new(self.config.default_concurrency.clone()),
            },
        );

        Ok(())
    }

    /// Unregister a tool. Calls shutdown if present.
    pub async fn unregister(&mut self, name: &str) -> Result<(), RegistryError> {
        self.tools
            .remove(name)
            .ok_or_else(|| RegistryError::NotFound {
                name: name.to_string(),
            })?;
        // Note: shutdown hook was consumed during register (FnOnce).
        // In the full implementation, shutdown would be stored separately.
        Ok(())
    }

    /// Execute a tool capability.
    pub async fn execute(
        &self,
        tool_name: &str,
        capability_name: &str,
        args: serde_json::Value,
        opts: ExecutionOptions,
    ) -> Result<serde_json::Value, ToolError> {
        // 1. Lookup
        let tool = self.tools.get(tool_name).ok_or_else(|| {
            ToolError::validation(format!("tool '{tool_name}' not found"))
        })?;

        let capability = tool.descriptor.capability(capability_name).ok_or_else(|| {
            ToolError::validation(format!(
                "capability '{capability_name}' not found in tool '{tool_name}'"
            ))
        })?;

        let handler = tool.handlers.get(capability_name).ok_or_else(|| {
            ToolError::internal(format!(
                "handler missing for capability '{capability_name}' — registry inconsistency"
            ))
        })?;

        // 2. Circuit breaker check
        tool.circuit_breaker.check()?;

        // 3. Concurrency permit
        let _permit = tool.concurrency.acquire().await?;

        // 4. Execute through engine
        let result = execution::execute(
            handler.as_ref(),
            capability,
            tool_name,
            &tool.config,
            &self.config.execution,
            args,
            opts,
        )
        .await;

        // 5. Update circuit breaker
        match &result {
            Ok(_) => tool.circuit_breaker.record_success(),
            Err(e) => match e.code {
                crate::handler::ToolErrorCode::Timeout
                | crate::handler::ToolErrorCode::ExecutionFailed
                | crate::handler::ToolErrorCode::InternalError => {
                    tool.circuit_breaker.record_failure();
                }
                _ => {} // validation, cancelled, overloaded don't count
            },
        }

        result
    }

    /// List all registered tool descriptors.
    pub fn list_tools(&self) -> Vec<&ToolDescriptor> {
        self.tools.values().map(|t| &t.descriptor).collect()
    }

    /// Get a tool descriptor by name.
    pub fn get_tool(&self, name: &str) -> Option<&ToolDescriptor> {
        self.tools.get(name).map(|t| &t.descriptor)
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::Capability;
    use crate::handler::{ToolContext, ToolErrorCode};
    use crate::package::PackageBuilder;
    use serde_json::json;

    fn test_capability(name: &str) -> Capability {
        Capability {
            name: name.into(),
            description: format!("{name} cap"),
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            timeout_ms: None,
            idempotent: false,
            side_effects: false,
        }
    }

    fn test_descriptor(name: &str, caps: Vec<&str>) -> ToolDescriptor {
        ToolDescriptor {
            name: name.into(),
            version: "1.0.0".into(),
            description: format!("{name} tool"),
            capabilities: caps.into_iter().map(test_capability).collect(),
            config_schema: None,
            tags: vec![],
        }
    }

    fn echo_handler() -> impl ToolHandler {
        |_ctx: &ToolContext, args: serde_json::Value| async move { Ok(args) }
    }

    fn echo_package(name: &str) -> ToolPackage {
        PackageBuilder::new(test_descriptor(name, vec!["run"]))
            .handler("run", echo_handler())
            .build()
            .unwrap()
    }

    // --- Registration ---

    #[tokio::test]
    async fn register_valid_package() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        assert!(reg.register(echo_package("test_tool")).await.is_ok());
        assert_eq!(reg.len(), 1);
    }

    #[tokio::test]
    async fn register_duplicate_name_rejected() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        reg.register(echo_package("test_tool")).await.unwrap();
        let err = reg.register(echo_package("test_tool")).await.unwrap_err();
        assert!(matches!(err, RegistryError::DuplicateName { .. }));
    }

    #[tokio::test]
    async fn register_multiple_tools() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        reg.register(echo_package("tool_a")).await.unwrap();
        reg.register(echo_package("tool_b")).await.unwrap();
        assert_eq!(reg.len(), 2);
    }

    // --- Unregister ---

    #[tokio::test]
    async fn unregister_existing() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        reg.register(echo_package("test_tool")).await.unwrap();
        assert!(reg.unregister("test_tool").await.is_ok());
        assert!(reg.is_empty());
    }

    #[tokio::test]
    async fn unregister_nonexistent_fails() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        assert!(matches!(
            reg.unregister("nonexistent").await,
            Err(RegistryError::NotFound { .. })
        ));
    }

    // --- Execution ---

    #[tokio::test]
    async fn execute_valid() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        reg.register(echo_package("test_tool")).await.unwrap();

        let result = reg
            .execute("test_tool", "run", json!({"x": 1}), ExecutionOptions::default())
            .await
            .unwrap();
        assert_eq!(result, json!({"x": 1}));
    }

    #[tokio::test]
    async fn execute_unknown_tool() {
        let reg = ToolRegistry::new(RegistryConfig::default());
        let err = reg
            .execute("nonexistent", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.code, ToolErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn execute_unknown_capability() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        reg.register(echo_package("test_tool")).await.unwrap();

        let err = reg
            .execute("test_tool", "nonexistent", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.code, ToolErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn execute_after_unregister_fails() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        reg.register(echo_package("test_tool")).await.unwrap();
        reg.unregister("test_tool").await.unwrap();

        let err = reg
            .execute("test_tool", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.code, ToolErrorCode::ValidationFailed);
    }

    // --- Lookup ---

    #[tokio::test]
    async fn list_tools_returns_all() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        reg.register(echo_package("tool_a")).await.unwrap();
        reg.register(echo_package("tool_b")).await.unwrap();

        let list = reg.list_tools();
        assert_eq!(list.len(), 2);
        let names: Vec<&str> = list.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"tool_a"));
        assert!(names.contains(&"tool_b"));
    }

    #[tokio::test]
    async fn get_tool_found() {
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        reg.register(echo_package("test_tool")).await.unwrap();
        assert!(reg.get_tool("test_tool").is_some());
    }

    #[tokio::test]
    async fn get_tool_not_found() {
        let reg = ToolRegistry::new(RegistryConfig::default());
        assert!(reg.get_tool("nonexistent").is_none());
    }

    // --- Config injection ---

    #[tokio::test]
    async fn config_injected_into_handler() {
        let config_reader = |ctx: &ToolContext, _args: serde_json::Value| {
            let config = ctx.config.clone();
            async move { Ok(config) }
        };

        let pkg = PackageBuilder::new(test_descriptor("configured_tool", vec!["run"]))
            .handler("run", config_reader)
            .build()
            .unwrap();

        let mut config = RegistryConfig::default();
        config
            .tool_configs
            .insert("configured_tool".into(), json!({"api_key": "secret123"}));

        let mut reg = ToolRegistry::new(config);
        reg.register(pkg).await.unwrap();

        let result = reg
            .execute("configured_tool", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap();
        assert_eq!(result["api_key"], "secret123");
    }

    // --- Initialize failure ---

    #[tokio::test]
    async fn initialize_failure_prevents_registration() {
        let desc = test_descriptor("failing_tool", vec!["run"]);
        let pkg = ToolPackage {
            descriptor: desc,
            handlers: {
                let mut h: HashMap<String, Box<dyn ToolHandler>> = HashMap::new();
                h.insert("run".into(), Box::new(echo_handler()));
                h
            },
            initialize: Some(Box::new(|_config| {
                Box::pin(async { Err(ToolError::internal("init crashed")) })
            })),
            shutdown: None,
        };

        let mut reg = ToolRegistry::new(RegistryConfig::default());
        let err = reg.register(pkg).await.unwrap_err();
        assert!(matches!(err, RegistryError::InitializeFailed { .. }));
        assert!(reg.is_empty()); // tool was not registered
    }

    // --- Circuit breaker integration ---

    #[tokio::test]
    async fn circuit_breaker_trips_through_registry() {
        let failing_handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            Err(ToolError::execution("always fails", false))
        };

        let pkg = PackageBuilder::new(test_descriptor("flaky_tool", vec!["run"]))
            .handler("run", failing_handler)
            .build()
            .unwrap();

        let mut reg = ToolRegistry::new(RegistryConfig {
            default_circuit_breaker: CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 2,
                cooldown: std::time::Duration::from_secs(60),
                failure_window: std::time::Duration::from_secs(60),
            },
            ..RegistryConfig::default()
        });
        reg.register(pkg).await.unwrap();

        // First two calls fail (ExecutionFailed) → CB records failures
        let err1 = reg
            .execute("flaky_tool", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err1.code, ToolErrorCode::ExecutionFailed);

        let err2 = reg
            .execute("flaky_tool", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err2.code, ToolErrorCode::ExecutionFailed);

        // Third call → CB is open → Unavailable (handler never called)
        let err3 = reg
            .execute("flaky_tool", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err3.code, ToolErrorCode::Unavailable);
        assert!(err3.retryable);
    }

    #[tokio::test]
    async fn circuit_breaker_resets_on_success() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count2 = call_count.clone();

        // Fails first 2 calls, then succeeds
        let sometimes_handler = move |_ctx: &ToolContext, _args: serde_json::Value| {
            let count = call_count2.fetch_add(1, Ordering::Relaxed);
            async move {
                if count < 2 {
                    Err(ToolError::execution("transient", true))
                } else {
                    Ok(json!({"ok": true}))
                }
            }
        };

        let pkg = PackageBuilder::new(test_descriptor("recovering_tool", vec!["run"]))
            .handler("run", sometimes_handler)
            .build()
            .unwrap();

        let mut reg = ToolRegistry::new(RegistryConfig {
            default_circuit_breaker: CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 2,
                cooldown: std::time::Duration::from_millis(0), // instant cooldown
                failure_window: std::time::Duration::from_secs(60),
            },
            ..RegistryConfig::default()
        });
        reg.register(pkg).await.unwrap();

        // 2 failures → trips
        reg.execute("recovering_tool", "run", json!({}), ExecutionOptions::default()).await.unwrap_err();
        reg.execute("recovering_tool", "run", json!({}), ExecutionOptions::default()).await.unwrap_err();

        // Cooldown is 0ms → half-open → probe succeeds → closed
        let result = reg
            .execute("recovering_tool", "run", json!({}), ExecutionOptions::default())
            .await;
        assert!(result.is_ok());

        // Should be closed again → next call succeeds
        let result2 = reg
            .execute("recovering_tool", "run", json!({}), ExecutionOptions::default())
            .await;
        assert!(result2.is_ok());
    }

    // --- Concurrency integration ---

    #[tokio::test]
    async fn concurrency_limit_through_registry() {
        let slow_handler = |_ctx: &ToolContext, _args: serde_json::Value| async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            Ok(json!({"ok": true}))
        };

        let pkg = PackageBuilder::new(test_descriptor("slow_tool", vec!["run"]))
            .handler("run", slow_handler)
            .build()
            .unwrap();

        let mut reg = ToolRegistry::new(RegistryConfig {
            default_concurrency: ConcurrencyConfig {
                max_concurrent: 1,
                max_queued: 0, // no queue → reject immediately
            },
            ..RegistryConfig::default()
        });
        reg.register(pkg).await.unwrap();

        // Use Arc to share registry across tasks
        let reg = std::sync::Arc::new(reg);
        let reg2 = reg.clone();

        // First call takes the permit
        let handle = tokio::spawn(async move {
            reg2.execute("slow_tool", "run", json!({}), ExecutionOptions::default()).await
        });

        // Give first task time to acquire permit
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Second call → no permit, no queue → Overloaded
        let err = reg
            .execute("slow_tool", "run", json!({}), ExecutionOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.code, ToolErrorCode::Overloaded);

        handle.await.unwrap().unwrap(); // first call completes normally
    }

    // --- Concurrent execution ---

    #[tokio::test]
    async fn concurrent_execute_all_succeed() {
        let pkg = PackageBuilder::new(test_descriptor("fast_tool", vec!["run"]))
            .handler("run", echo_handler())
            .build()
            .unwrap();

        let mut reg = ToolRegistry::new(RegistryConfig {
            default_concurrency: ConcurrencyConfig {
                max_concurrent: 10,
                max_queued: 50,
            },
            ..RegistryConfig::default()
        });
        reg.register(pkg).await.unwrap();
        let reg = std::sync::Arc::new(reg);

        let mut handles = vec![];
        for i in 0..10 {
            let reg = reg.clone();
            handles.push(tokio::spawn(async move {
                reg.execute("fast_tool", "run", json!({"i": i}), ExecutionOptions::default()).await
            }));
        }

        for handle in handles {
            assert!(handle.await.unwrap().is_ok());
        }
    }

    #[tokio::test]
    async fn validation_errors_dont_count_for_circuit_breaker() {
        let pkg = PackageBuilder::new(test_descriptor("validated_tool", vec!["run"]))
            .handler("run", echo_handler())
            .build()
            .unwrap();

        let mut reg = ToolRegistry::new(RegistryConfig {
            default_circuit_breaker: CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 2,
                cooldown: std::time::Duration::from_secs(60),
                failure_window: std::time::Duration::from_secs(60),
            },
            ..RegistryConfig::default()
        });
        reg.register(pkg).await.unwrap();

        // Unknown capability → ValidationFailed (should NOT count for CB)
        for _ in 0..5 {
            let _ = reg.execute("validated_tool", "nonexistent", json!({}), ExecutionOptions::default()).await;
        }

        // CB should still be closed → valid call succeeds
        let result = reg
            .execute("validated_tool", "run", json!({}), ExecutionOptions::default())
            .await;
        assert!(result.is_ok());
    }

    // --- Config validation ---

    #[tokio::test]
    async fn invalid_config_rejects_registration() {
        let mut desc = test_descriptor("strict_tool", vec!["run"]);
        desc.config_schema = Some(json!({
            "type": "object",
            "properties": {"port": {"type": "integer"}},
            "required": ["port"]
        }));

        let pkg = PackageBuilder::new(desc)
            .handler("run", echo_handler())
            .build()
            .unwrap();

        // No config provided → missing required "port"
        let mut reg = ToolRegistry::new(RegistryConfig::default());
        let err = reg.register(pkg).await.unwrap_err();
        assert!(matches!(err, RegistryError::Package(PackageError::InvalidConfig { .. })));
    }
}
