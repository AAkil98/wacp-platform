//! WACP Coordinator SDK
//!
//! Client-facing interface for driving WACP coordination.
//! Provides typed methods for task graph management, workspace lifecycle,
//! signal consumption, and integration.

pub mod context;
pub mod error;
pub mod types;

pub use context::CoordinatorContext;
pub use error::Error;
pub use types::*;
