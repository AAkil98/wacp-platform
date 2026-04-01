//! WACP LLM Adapter Framework
//!
//! Provider-agnostic LLM inference with streaming, cost tracking, and resilience.

pub mod adapter;
pub mod cost;
pub mod error;
pub mod providers;
pub mod rate_limit;
pub mod result;
pub mod retry;
pub mod stream;
pub mod types;

pub use adapter::{CompletionOptions, LlmAdapter};
pub use cost::{calculate_cost, CostTracker, ModelPricing};
pub use error::{ErrorOrigin, ErrorPersistence, LlmError};
pub use rate_limit::{RateLimitConfig, RateLimiter};
pub use result::{CompletionResult, Cost, ModelInfo, ProviderHealth, TokenUsage, ToolCall};
pub use retry::{BackoffStrategy, RetryConfig};
pub use stream::{StreamEvent, StreamHandle};
pub use types::{Content, ContentBlock, Message, Role, ToolDefinition};
