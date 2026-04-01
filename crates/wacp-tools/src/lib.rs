//! WACP Tool Framework
//!
//! Middleware for defining, packaging, discovering, and executing tools.
//! Tools are capabilities that agents use to interact with the outside world.

pub mod descriptor;
pub mod handler;
pub mod package;

pub use descriptor::{Capability, DescriptorError, ToolDescriptor};
pub use handler::{ToolContext, ToolError, ToolErrorCode, ToolHandler};
pub use package::{PackageBuilder, PackageError, ToolPackage};
