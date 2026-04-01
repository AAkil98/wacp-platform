use serde::Serialize;

/// Result of a completion request.
#[derive(Debug, Clone, Serialize)]
pub struct CompletionResult {
    /// The model's text response.
    pub content: String,
    /// Tool calls the model wants to make.
    pub tool_calls: Vec<ToolCall>,
    /// Token usage: input + output.
    pub usage: TokenUsage,
    /// Estimated cost (None if model pricing unknown).
    pub cost: Option<Cost>,
    /// Actual model used (may differ from requested if aliased).
    pub model: String,
    /// Request latency in milliseconds.
    pub latency_ms: u64,
    /// Whether the response was truncated (hit max_tokens).
    pub truncated: bool,
}

/// A tool call from the model.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolCall {
    /// Provider-assigned ID for this tool call.
    pub id: String,
    /// Tool name (maps to ToolDescriptor.name).
    pub name: String,
    /// Arguments as JSON.
    pub arguments: serde_json::Value,
}

/// Token usage: input + output.
#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl TokenUsage {
    pub fn total(&self) -> u32 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// Cost in microdollars (integer arithmetic, no float rounding).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Cost {
    /// Cost in microdollars (1 USD = 1_000_000 microdollars).
    pub amount_micros: u64,
    /// Currency code.
    pub currency: &'static str,
}

impl Cost {
    pub fn usd(amount_micros: u64) -> Self {
        Self { amount_micros, currency: "USD" }
    }

    /// Format as a human-readable dollar string (e.g., "$0.001234").
    pub fn display_usd(&self) -> String {
        let dollars = self.amount_micros / 1_000_000;
        let micros = self.amount_micros % 1_000_000;
        format!("${dollars}.{micros:06}")
    }
}

/// Information about an available model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    /// Model ID as known by the provider.
    pub id: String,
    /// Human-readable name.
    pub name: Option<String>,
    /// Maximum context length in tokens.
    pub max_context: Option<u32>,
    /// Maximum output tokens.
    pub max_output: Option<u32>,
    /// Whether the model supports tool use / function calling.
    pub supports_tools: bool,
    /// Whether the model supports streaming.
    pub supports_streaming: bool,
}

/// Provider health status.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderHealth {
    pub healthy: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub models_available: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_total() {
        let usage = TokenUsage { input_tokens: 100, output_tokens: 50 };
        assert_eq!(usage.total(), 150);
    }

    #[test]
    fn token_usage_total_overflow_saturates() {
        let usage = TokenUsage { input_tokens: u32::MAX, output_tokens: 1 };
        assert_eq!(usage.total(), u32::MAX);
    }

    #[test]
    fn token_usage_default_is_zero() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total(), 0);
    }

    #[test]
    fn cost_usd_constructor() {
        let cost = Cost::usd(1_500_000);
        assert_eq!(cost.amount_micros, 1_500_000);
        assert_eq!(cost.currency, "USD");
    }

    #[test]
    fn cost_display_usd() {
        assert_eq!(Cost::usd(0).display_usd(), "$0.000000");
        assert_eq!(Cost::usd(1_000_000).display_usd(), "$1.000000");
        assert_eq!(Cost::usd(1_234).display_usd(), "$0.001234");
        assert_eq!(Cost::usd(15_000_000).display_usd(), "$15.000000");
    }

    #[test]
    fn cost_microdollar_precision() {
        // Claude Sonnet: 100 input tokens at $3/M = 300 microdollars
        let input_price_micros_per_m: u64 = 3_000_000; // $3 per million
        let input_tokens: u64 = 100;
        let cost = input_tokens * input_price_micros_per_m / 1_000_000;
        assert_eq!(cost, 300); // $0.000300
    }

    #[test]
    fn tool_call_serde() {
        let tc = ToolCall {
            id: "call_1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "/src"}),
        };
        let json = serde_json::to_string(&tc).unwrap();
        assert!(json.contains("call_1"));
        assert!(json.contains("read_file"));
    }

    #[test]
    fn model_info_optional_fields() {
        let info = ModelInfo {
            id: "gpt-4o".into(),
            name: None,
            max_context: Some(128_000),
            max_output: None,
            supports_tools: true,
            supports_streaming: true,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["id"], "gpt-4o");
        assert!(json["name"].is_null());
    }

    #[test]
    fn provider_health_serde() {
        let health = ProviderHealth {
            healthy: true,
            latency_ms: Some(150),
            error: None,
            models_available: 3,
        };
        let json = serde_json::to_value(&health).unwrap();
        assert_eq!(json["healthy"], true);
        assert_eq!(json["models_available"], 3);
    }
}
