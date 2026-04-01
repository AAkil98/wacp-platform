use serde_json::json;

use crate::cost::{self, ANTHROPIC_PRICING};
use crate::error::LlmError;
use crate::result::{CompletionResult, ModelInfo, TokenUsage, ToolCall};
use crate::types::{Content, ContentBlock, Message, Role, ToolDefinition};

/// Build an Anthropic Messages API request body from common types.
pub fn build_request(
    messages: &[Message],
    model: &str,
    max_tokens: u32,
    temperature: Option<f64>,
    stop: &[String],
    tools: &[ToolDefinition],
    stream: bool,
) -> serde_json::Value {
    // Extract system message (Anthropic uses a separate field)
    let system_text: Vec<String> = messages
        .iter()
        .filter(|m| m.role == Role::System)
        .filter_map(|m| m.content.as_text().map(|s| s.to_string()))
        .collect();

    let api_messages: Vec<serde_json::Value> = messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(message_to_anthropic)
        .collect();

    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": api_messages,
        "stream": stream,
    });

    if !system_text.is_empty() {
        body["system"] = json!(system_text.join("\n\n"));
    }
    if let Some(temp) = temperature {
        body["temperature"] = json!(temp);
    }
    if !stop.is_empty() {
        body["stop_sequences"] = json!(stop);
    }
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(tool_to_anthropic).collect::<Vec<_>>());
    }

    body
}

/// Parse an Anthropic Messages API response into common types.
pub fn parse_response(body: &serde_json::Value) -> Result<CompletionResult, LlmError> {
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let content_blocks = body
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| LlmError::structural("missing 'content' array in response"))?;

    let mut text = String::new();
    let mut tool_calls = Vec::new();

    for block in content_blocks {
        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match block_type {
            "text" => {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    text.push_str(t);
                }
            }
            "tool_use" => {
                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let input = block.get("input").cloned().unwrap_or(json!({}));
                tool_calls.push(ToolCall { id, name, arguments: input });
            }
            _ => {}
        }
    }

    let usage = parse_usage(body);
    let pricing = cost::lookup_pricing(&model, ANTHROPIC_PRICING);
    let cost = pricing.map(|p| cost::calculate_cost(&usage, &p));

    let truncated = body
        .get("stop_reason")
        .and_then(|v| v.as_str())
        == Some("max_tokens");

    Ok(CompletionResult {
        content: text,
        tool_calls,
        usage,
        cost,
        model,
        latency_ms: 0, // set by caller
        truncated,
    })
}

/// Parse usage from the response body.
fn parse_usage(body: &serde_json::Value) -> TokenUsage {
    let usage = body.get("usage");
    TokenUsage {
        input_tokens: usage
            .and_then(|u| u.get("input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        output_tokens: usage
            .and_then(|u| u.get("output_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    }
}

/// Parse an Anthropic error response.
pub fn parse_error(status: u16, body: &serde_json::Value) -> LlmError {
    let error_type = body
        .get("error")
        .and_then(|e| e.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown_error");
    let message = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown error");

    let mut err = LlmError::from_status(status, message);
    err.code = Some(error_type.to_string());
    err
}

fn message_to_anthropic(msg: &Message) -> serde_json::Value {
    let role = match msg.role {
        Role::User | Role::Tool => "user",
        Role::Assistant => "assistant",
        Role::System => unreachable!("system messages filtered before this"),
    };

    let content = match &msg.content {
        Content::Text(s) => json!(s),
        Content::Blocks(blocks) => {
            json!(blocks.iter().map(block_to_anthropic).collect::<Vec<_>>())
        }
    };

    json!({ "role": role, "content": content })
}

fn block_to_anthropic(block: &ContentBlock) -> serde_json::Value {
    match block {
        ContentBlock::Text { text } => json!({"type": "text", "text": text}),
        ContentBlock::ToolUse { id, name, input } => {
            json!({"type": "tool_use", "id": id, "name": name, "input": input})
        }
        ContentBlock::ToolResult { tool_use_id, content, is_error } => {
            json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content, "is_error": is_error})
        }
    }
}

fn tool_to_anthropic(tool: &ToolDefinition) -> serde_json::Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

/// Known Anthropic models.
pub fn known_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "claude-opus-4-20250514".into(),
            name: Some("Claude Opus 4".into()),
            max_context: Some(200_000),
            max_output: Some(32_000),
            supports_tools: true,
            supports_streaming: true,
        },
        ModelInfo {
            id: "claude-sonnet-4-20250514".into(),
            name: Some("Claude Sonnet 4".into()),
            max_context: Some(200_000),
            max_output: Some(16_000),
            supports_tools: true,
            supports_streaming: true,
        },
        ModelInfo {
            id: "claude-haiku-4-20250514".into(),
            name: Some("Claude Haiku 4".into()),
            max_context: Some(200_000),
            max_output: Some(8_000),
            supports_tools: true,
            supports_streaming: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Request building ---

    #[test]
    fn build_request_basic() {
        let messages = vec![
            Message::system("You are helpful."),
            Message::user("Hello"),
        ];
        let body = build_request(&messages, "claude-sonnet-4-20250514", 1024, None, &[], &[], false);

        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["system"], "You are helpful.");
        assert_eq!(body["stream"], false);

        // Only non-system messages in messages array
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn build_request_with_tools() {
        let tools = vec![ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        }];
        let body = build_request(&[Message::user("read /tmp/x")], "claude-sonnet-4-20250514", 1024, None, &[], &tools, false);

        let api_tools = body["tools"].as_array().unwrap();
        assert_eq!(api_tools.len(), 1);
        assert_eq!(api_tools[0]["name"], "read_file");
        assert!(api_tools[0].get("input_schema").is_some());
    }

    #[test]
    fn build_request_with_temperature_and_stop() {
        let body = build_request(
            &[Message::user("hi")],
            "claude-sonnet-4-20250514",
            100,
            Some(0.7),
            &["STOP".into()],
            &[],
            true,
        );
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["stop_sequences"][0], "STOP");
        assert_eq!(body["stream"], true);
    }

    // --- Response parsing ---

    #[test]
    fn parse_text_response() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "content": [{"type": "text", "text": "Hello, world!"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "stop_reason": "end_turn"
        });
        let result = parse_response(&body).unwrap();
        assert_eq!(result.content, "Hello, world!");
        assert!(result.tool_calls.is_empty());
        assert_eq!(result.usage.input_tokens, 10);
        assert_eq!(result.usage.output_tokens, 5);
        assert!(!result.truncated);
        assert!(result.cost.is_some()); // sonnet pricing known
    }

    #[test]
    fn parse_tool_use_response() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "content": [
                {"type": "text", "text": "I'll read that file."},
                {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "/tmp/x"}}
            ],
            "usage": {"input_tokens": 20, "output_tokens": 15},
            "stop_reason": "tool_use"
        });
        let result = parse_response(&body).unwrap();
        assert_eq!(result.content, "I'll read that file.");
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "read_file");
        assert_eq!(result.tool_calls[0].arguments["path"], "/tmp/x");
    }

    #[test]
    fn parse_truncated_response() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "content": [{"type": "text", "text": "partial..."}],
            "usage": {"input_tokens": 100, "output_tokens": 1024},
            "stop_reason": "max_tokens"
        });
        let result = parse_response(&body).unwrap();
        assert!(result.truncated);
    }

    #[test]
    fn parse_unknown_model_no_cost() {
        let body = json!({
            "model": "claude-unknown-99",
            "content": [{"type": "text", "text": "hi"}],
            "usage": {"input_tokens": 1, "output_tokens": 1},
            "stop_reason": "end_turn"
        });
        let result = parse_response(&body).unwrap();
        assert!(result.cost.is_none());
    }

    // --- Error parsing ---

    #[test]
    fn parse_rate_limit_error() {
        let body = json!({
            "error": {"type": "rate_limit_error", "message": "Rate limit exceeded"}
        });
        let err = parse_error(429, &body);
        assert!(err.retryable);
        assert_eq!(err.code.as_deref(), Some("rate_limit_error"));
        assert!(err.message.contains("Rate limit exceeded"));
    }

    #[test]
    fn parse_auth_error() {
        let body = json!({
            "error": {"type": "authentication_error", "message": "Invalid API key"}
        });
        let err = parse_error(401, &body);
        assert!(!err.retryable);
        assert_eq!(err.code.as_deref(), Some("authentication_error"));
    }

    // --- Known models ---

    #[test]
    fn known_models_not_empty() {
        let models = known_models();
        assert!(models.len() >= 3);
        assert!(models.iter().all(|m| m.supports_tools));
        assert!(models.iter().all(|m| m.supports_streaming));
    }
}
