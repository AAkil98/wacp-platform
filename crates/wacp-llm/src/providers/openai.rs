use serde_json::json;

use crate::cost::{self, OPENAI_PRICING};
use crate::error::LlmError;
use crate::result::{CompletionResult, TokenUsage, ToolCall};
use crate::types::{Content, ContentBlock, Message, Role, ToolDefinition};

/// Build an OpenAI Chat Completions API request body.
pub fn build_request(
    messages: &[Message],
    model: &str,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    stop: &[String],
    tools: &[ToolDefinition],
    stream: bool,
) -> serde_json::Value {
    let api_messages: Vec<serde_json::Value> = messages.iter().map(message_to_openai).collect();

    let mut body = json!({
        "model": model,
        "messages": api_messages,
        "stream": stream,
    });

    if let Some(max) = max_tokens {
        body["max_completion_tokens"] = json!(max);
    }
    if let Some(temp) = temperature {
        body["temperature"] = json!(temp);
    }
    if !stop.is_empty() {
        body["stop"] = json!(stop);
    }
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(tool_to_openai).collect::<Vec<_>>());
    }
    if stream {
        // Request usage in streaming mode
        body["stream_options"] = json!({"include_usage": true});
    }

    body
}

/// Parse an OpenAI Chat Completions response.
pub fn parse_response(body: &serde_json::Value) -> Result<CompletionResult, LlmError> {
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let choice = body
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| LlmError::structural("missing 'choices' in response"))?;

    let message = choice
        .get("message")
        .ok_or_else(|| LlmError::structural("missing 'message' in choice"))?;

    let content = message
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut tool_calls = Vec::new();
    if let Some(tcs) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tcs {
            let id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let empty = json!({});
            let function = tc.get("function").unwrap_or(&empty);
            let name = function
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args_str = function
                .get("arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let arguments: serde_json::Value = serde_json::from_str(args_str).unwrap_or(json!({}));
            tool_calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        }
    }

    let usage = parse_usage(body);
    let pricing = cost::lookup_pricing(&model, OPENAI_PRICING);
    let cost = pricing.map(|p| cost::calculate_cost(&usage, &p));

    let truncated = choice.get("finish_reason").and_then(|v| v.as_str()) == Some("length");

    Ok(CompletionResult {
        content,
        tool_calls,
        usage,
        cost,
        model,
        latency_ms: 0,
        truncated,
    })
}

fn parse_usage(body: &serde_json::Value) -> TokenUsage {
    let usage = body.get("usage");
    TokenUsage {
        input_tokens: usage
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        output_tokens: usage
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    }
}

/// Parse an OpenAI error response.
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

fn message_to_openai(msg: &Message) -> serde_json::Value {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };

    match &msg.content {
        Content::Text(s) => {
            json!({"role": role, "content": s})
        }
        Content::Blocks(blocks) => {
            // For tool results, OpenAI uses role=tool + tool_call_id
            if msg.role == Role::Tool {
                if let Some(ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                }) = blocks.first()
                {
                    return json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": content,
                    });
                }
            }
            // For assistant with tool_use blocks, reconstruct tool_calls
            if msg.role == Role::Assistant {
                let text: String = blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                let tool_calls: Vec<serde_json::Value> = blocks.iter().filter_map(|b| match b {
                    ContentBlock::ToolUse { id, name, input } => Some(json!({
                        "id": id,
                        "type": "function",
                        "function": {"name": name, "arguments": serde_json::to_string(input).unwrap_or_default()},
                    })),
                    _ => None,
                }).collect();

                let mut obj = json!({"role": "assistant"});
                if !text.is_empty() {
                    obj["content"] = json!(text);
                }
                if !tool_calls.is_empty() {
                    obj["tool_calls"] = json!(tool_calls);
                }
                return obj;
            }
            // Fallback: join text blocks
            let text: String = blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            json!({"role": role, "content": text})
        }
    }
}

fn tool_to_openai(tool: &ToolDefinition) -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Request building ---

    #[test]
    fn build_request_basic() {
        let messages = vec![Message::system("You are helpful."), Message::user("Hello")];
        let body = build_request(&messages, "gpt-4o", Some(1024), None, &[], &[], false);

        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["max_completion_tokens"], 1024);
        // System message stays in messages array (OpenAI style)
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["role"], "user");
    }

    #[test]
    fn build_request_with_tools() {
        let tools = vec![ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: json!({"type": "object"}),
        }];
        let body = build_request(
            &[Message::user("hi")],
            "gpt-4o",
            None,
            None,
            &[],
            &tools,
            false,
        );

        let api_tools = body["tools"].as_array().unwrap();
        assert_eq!(api_tools[0]["type"], "function");
        assert_eq!(api_tools[0]["function"]["name"], "read_file");
    }

    #[test]
    fn build_request_streaming_includes_usage() {
        let body = build_request(&[Message::user("hi")], "gpt-4o", None, None, &[], &[], true);
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    // --- Response parsing ---

    #[test]
    fn parse_text_response() {
        let body = json!({
            "model": "gpt-4o-2024-08-06",
            "choices": [{"message": {"role": "assistant", "content": "Hello!"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        });
        let result = parse_response(&body).unwrap();
        assert_eq!(result.content, "Hello!");
        assert!(result.tool_calls.is_empty());
        assert_eq!(result.usage.input_tokens, 10);
        assert_eq!(result.usage.output_tokens, 5);
        assert!(!result.truncated);
    }

    #[test]
    fn parse_tool_call_response() {
        let body = json!({
            "model": "gpt-4o",
            "choices": [{"message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "read_file", "arguments": "{\"path\":\"/tmp/x\"}"}}]
            }, "finish_reason": "tool_calls"}],
            "usage": {"prompt_tokens": 20, "completion_tokens": 10}
        });
        let result = parse_response(&body).unwrap();
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "read_file");
        assert_eq!(result.tool_calls[0].arguments["path"], "/tmp/x");
    }

    #[test]
    fn parse_truncated_response() {
        let body = json!({
            "model": "gpt-4o",
            "choices": [{"message": {"content": "partial..."}, "finish_reason": "length"}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 4096}
        });
        let result = parse_response(&body).unwrap();
        assert!(result.truncated);
    }

    // --- Error parsing ---

    #[test]
    fn parse_openai_error() {
        let body = json!({
            "error": {"type": "invalid_request_error", "message": "model not found"}
        });
        let err = parse_error(404, &body);
        assert!(!err.retryable);
        assert_eq!(err.code.as_deref(), Some("invalid_request_error"));
    }

    // --- Tool result message mapping ---

    #[test]
    fn tool_result_maps_to_openai_format() {
        let msg = Message::tool_result("call_1", "file contents", false);
        let openai_msg = message_to_openai(&msg);
        assert_eq!(openai_msg["role"], "tool");
        assert_eq!(openai_msg["tool_call_id"], "call_1");
        assert_eq!(openai_msg["content"], "file contents");
    }
}
