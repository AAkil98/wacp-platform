//! WACP Tool Framework
//!
//! Middleware for defining, packaging, discovering, and executing tools.
//! Tools are capabilities that agents use to interact with the outside world.

pub mod descriptor;
pub mod execution;
pub mod handler;
pub mod package;
pub mod registry;
pub mod resilience;
pub mod sandbox;

pub use descriptor::{Capability, DescriptorError, ToolDescriptor};
pub use handler::{ToolContext, ToolError, ToolErrorCode, ToolHandler};
pub use package::{PackageBuilder, PackageError, ToolPackage};
pub use execution::{ExecutionConfig, ExecutionOptions};
pub use registry::{RegistryConfig, RegistryError, ToolRegistry};
pub use sandbox::{SandboxLevel, SandboxPolicy};
pub use resilience::{
    BreakerStatus, CircuitBreaker, CircuitBreakerConfig, ConcurrencyConfig, ConcurrencyLimiter,
};
