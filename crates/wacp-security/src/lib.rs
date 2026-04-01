//! WACP Security Middleware
//!
//! Cross-cutting security: content filtering, secret management, audit events.

pub mod audit;
pub mod filter;
pub mod secrets;

pub use audit::{sha256_hex, AuditEvent};
pub use filter::{ContentFilter, FilterAction, FilterPolicy, FilterResult, FilterRule, Redaction};
pub use secrets::{SecretStore, SecretValue};
