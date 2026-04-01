//! WACP LLM Adapter Framework
//!
//! Provider-agnostic LLM inference with streaming, cost tracking, and resilience.

pub mod adapter;
pub mod error;
pub mod result;
pub mod stream;
pub mod types;

pub use adapter::{CompletionOptions, LlmAdapter};
pub use error::{ErrorOrigin, ErrorPersistence, LlmError};
pub use result::{CompletionResult, Cost, ModelInfo, ProviderHealth, TokenUsage, ToolCall};
pub use stream::{StreamEvent, StreamHandle};
pub use types::{Content, ContentBlock, Message, Role, ToolDefinition};
