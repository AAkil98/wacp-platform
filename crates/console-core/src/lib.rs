pub mod auth;
pub mod config;
pub mod error;

pub use auth::{AuthenticatedUser, ConsoleRole};
pub use config::ConsoleConfig;
pub use error::{ConsoleError, Violation};
