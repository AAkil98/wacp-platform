use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::descriptor::ToolDescriptor;
use crate::handler::{ToolError, ToolHandler};

/// A distributable tool unit: descriptor + handlers + lifecycle hooks.
pub struct ToolPackage {
    /// The tool's full descriptor.
    pub descriptor: ToolDescriptor,

    /// One handler per capability. Key = capability name.
    pub handlers: HashMap<String, Box<dyn ToolHandler>>,

    /// Called once at load time after config validation. Optional.
    pub initialize: Option<InitializeFn>,

    /// Called once at shutdown. Optional.
    pub shutdown: Option<ShutdownFn>,
}

pub type InitializeFn = Box<
    dyn FnOnce(serde_json::Value) -> Pin<Box<dyn Future<Output = Result<(), ToolError>> + Send>>
        + Send,
>;

pub type ShutdownFn =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), ToolError>> + Send>> + Send>;

/// Errors when validating a package before registration.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("descriptor validation failed: {0}")]
    InvalidDescriptor(#[from] crate::descriptor::DescriptorError),

    #[error("capability '{capability}' has no handler")]
    MissingHandler { capability: String },

    #[error("handler '{handler}' has no matching capability")]
    ExtraHandler { handler: String },

    #[error("config validation failed: {reason}")]
    InvalidConfig { reason: String },

    #[error("initialize failed: {0}")]
    InitializeFailed(ToolError),
}

impl ToolPackage {
    /// Validate descriptor and handler-descriptor alignment.
    /// Does NOT call initialize — that happens at registration.
    pub fn validate(&self) -> Result<(), PackageError> {
        self.descriptor.validate()?;

        // Every capability must have a handler
        for cap in &self.descriptor.capabilities {
            if !self.handlers.contains_key(&cap.name) {
                return Err(PackageError::MissingHandler {
                    capability: cap.name.clone(),
                });
            }
        }

        // Every handler must match a capability
        for key in self.handlers.keys() {
            if self.descriptor.capability(key).is_none() {
                return Err(PackageError::ExtraHandler {
                    handler: key.clone(),
                });
            }
        }

        Ok(())
    }
}

/// Builder for constructing ToolPackage ergonomically.
pub struct PackageBuilder {
    descriptor: ToolDescriptor,
    handlers: HashMap<String, Box<dyn ToolHandler>>,
    initialize: Option<InitializeFn>,
    shutdown: Option<ShutdownFn>,
}

impl PackageBuilder {
    /// Create a builder from a descriptor.
    pub fn new(descriptor: ToolDescriptor) -> Self {
        Self {
            descriptor,
            handlers: HashMap::new(),
            initialize: None,
            shutdown: None,
        }
    }

    /// Register a handler for a capability.
    pub fn handler(
        mut self,
        capability_name: impl Into<String>,
        handler: impl ToolHandler,
    ) -> Self {
        self.handlers
            .insert(capability_name.into(), Box::new(handler));
        self
    }

    /// Set the initialize hook.
    pub fn initialize(mut self, f: InitializeFn) -> Self {
        self.initialize = Some(f);
        self
    }

    /// Set the shutdown hook.
    pub fn shutdown(mut self, f: ShutdownFn) -> Self {
        self.shutdown = Some(f);
        self
    }

    /// Build and validate the package.
    pub fn build(self) -> Result<ToolPackage, PackageError> {
        let package = ToolPackage {
            descriptor: self.descriptor,
            handlers: self.handlers,
            initialize: self.initialize,
            shutdown: self.shutdown,
        };
        package.validate()?;
        Ok(package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::Capability;
    use crate::handler::ToolContext;
    use serde_json::json;

    fn test_capability(name: &str) -> Capability {
        Capability {
            name: name.into(),
            description: format!("{name} capability"),
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            timeout_ms: None,
            idempotent: false,
            side_effects: false,
        }
    }

    fn test_descriptor(caps: Vec<&str>) -> ToolDescriptor {
        ToolDescriptor {
            name: "test_tool".into(),
            version: "1.0.0".into(),
            description: "A test tool".into(),
            capabilities: caps.into_iter().map(test_capability).collect(),
            config_schema: None,
            tags: vec![],
        }
    }

    fn noop_handler() -> impl ToolHandler {
        |_ctx: &ToolContext, _args: serde_json::Value| async move { Ok(json!({})) }
    }

    // --- Alignment validation ---

    #[test]
    fn valid_package_accepted() {
        let pkg = PackageBuilder::new(test_descriptor(vec!["read"]))
            .handler("read", noop_handler())
            .build();
        assert!(pkg.is_ok());
    }

    #[test]
    fn multi_capability_package_accepted() {
        let pkg = PackageBuilder::new(test_descriptor(vec!["read", "write"]))
            .handler("read", noop_handler())
            .handler("write", noop_handler())
            .build();
        assert!(pkg.is_ok());
    }

    #[test]
    fn missing_handler_rejected() {
        let pkg = PackageBuilder::new(test_descriptor(vec!["read", "write"]))
            .handler("read", noop_handler())
            // "write" handler missing
            .build();
        assert!(
            matches!(pkg, Err(PackageError::MissingHandler { capability }) if capability == "write")
        );
    }

    #[test]
    fn extra_handler_rejected() {
        let pkg = PackageBuilder::new(test_descriptor(vec!["read"]))
            .handler("read", noop_handler())
            .handler("write", noop_handler()) // no "write" capability
            .build();
        assert!(matches!(pkg, Err(PackageError::ExtraHandler { handler }) if handler == "write"));
    }

    #[test]
    fn invalid_descriptor_rejected() {
        let mut desc = test_descriptor(vec!["read"]);
        desc.name = "INVALID".into(); // uppercase
        let pkg = PackageBuilder::new(desc)
            .handler("read", noop_handler())
            .build();
        assert!(matches!(pkg, Err(PackageError::InvalidDescriptor(_))));
    }

    #[test]
    fn empty_capabilities_rejected() {
        let pkg = PackageBuilder::new(test_descriptor(vec![])).build();
        assert!(matches!(pkg, Err(PackageError::InvalidDescriptor(_))));
    }

    // --- Package fields ---

    #[test]
    fn package_has_no_hooks_by_default() {
        let pkg = PackageBuilder::new(test_descriptor(vec!["read"]))
            .handler("read", noop_handler())
            .build()
            .unwrap();
        assert!(pkg.initialize.is_none());
        assert!(pkg.shutdown.is_none());
    }

    #[test]
    fn package_with_hooks() {
        let pkg = PackageBuilder::new(test_descriptor(vec!["read"]))
            .handler("read", noop_handler())
            .initialize(Box::new(|_config| Box::pin(async { Ok(()) })))
            .shutdown(Box::new(|| Box::pin(async { Ok(()) })))
            .build()
            .unwrap();
        assert!(pkg.initialize.is_some());
        assert!(pkg.shutdown.is_some());
    }
}
