//! WACP Security Middleware
//!
//! Cross-cutting security: content filtering, secret management, audit events.

pub mod audit;
pub mod filter;
pub mod secrets;

pub use audit::{AuditEvent, sha256_hex};
pub use filter::{ContentFilter, FilterAction, FilterPolicy, FilterResult, FilterRule, Redaction};
pub use secrets::{SecretStore, SecretValue};
