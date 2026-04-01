use crate::error::LlmError;
use crate::result::{CompletionResult, ModelInfo, ProviderHealth};
use crate::stream::StreamHandle;
use crate::types::{Message, ToolDefinition};

/// Provider-agnostic LLM adapter trait.
pub trait LlmAdapter: Send + Sync + 'static {
    /// Complete a conversation (non-streaming).
    fn complete(
        &self,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CompletionResult, LlmError>> + Send + '_>>;

    /// Stream a completion token-by-token.
    fn complete_stream(
        &self,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<StreamHandle, LlmError>> + Send + '_>>;

    /// List available models.
    fn models(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<ModelInfo>, LlmError>> + Send + '_>>;

    /// Check provider health.
    fn health(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ProviderHealth> + Send + '_>>;
}

/// Options for a completion request.
#[derive(Debug, Clone, Default)]
pub struct CompletionOptions {
    /// Model to use. None = provider default.
    pub model: Option<String>,
    /// Maximum tokens to generate. None = model default.
    pub max_tokens: Option<u32>,
    /// Temperature (0.0–2.0). None = model default.
    pub temperature: Option<f64>,
    /// Stop sequences.
    pub stop: Vec<String>,
    /// Available tools for function-calling.
    pub tools: Vec<ToolDefinition>,
    /// Per-request timeout in milliseconds. None = provider default.
    pub timeout_ms: Option<u64>,
    /// Provider-specific extra options.
    pub extra: serde_json::Value,
}

impl CompletionOptions {
    /// Create minimal options with just a model.
    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: Some(model.into()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_options_default() {
        let opts = CompletionOptions::default();
        assert!(opts.model.is_none());
        assert!(opts.max_tokens.is_none());
        assert!(opts.temperature.is_none());
        assert!(opts.stop.is_empty());
        assert!(opts.tools.is_empty());
        assert!(opts.timeout_ms.is_none());
    }

    #[test]
    fn completion_options_with_model() {
        let opts = CompletionOptions::with_model("claude-sonnet-4-20250514");
        assert_eq!(opts.model.as_deref(), Some("claude-sonnet-4-20250514"));
    }
}
