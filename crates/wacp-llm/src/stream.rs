use std::pin::Pin;

use futures::Stream;
use serde::Serialize;

use crate::error::LlmError;
use crate::result::TokenUsage;

/// Handle to an in-progress streaming completion.
pub struct StreamHandle {
    pub(crate) inner: Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>,
    /// Model used for this request.
    pub model: String,
    /// Provider-assigned request ID.
    pub request_id: Option<String>,
}

impl StreamHandle {
    /// Create a new stream handle.
    pub fn new(
        stream: Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>,
        model: String,
        request_id: Option<String>,
    ) -> Self {
        Self {
            inner: stream,
            model,
            request_id,
        }
    }

    /// Consume the handle and return the underlying stream.
    pub fn into_stream(self) -> Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>> {
        self.inner
    }
}

/// Events emitted during streaming.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// A text token.
    ContentDelta { delta: String },
    /// An incremental tool call fragment.
    ToolCallDelta {
        index: u32,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: Option<String>,
    },
    /// Token usage report (emitted before Done).
    Usage { usage: TokenUsage },
    /// Stream complete. MUST be the last event.
    Done,
}

/// Parse SSE lines from a byte stream.
/// SSE format: lines prefixed with "event:", "data:", or blank lines as separators.
pub fn parse_sse_line(line: &str) -> Option<SseLine> {
    let line = line.trim_end_matches('\r');
    if line.is_empty() {
        return Some(SseLine::Empty);
    }
    if let Some(event) = line.strip_prefix("event:") {
        return Some(SseLine::Event(event.trim().to_string()));
    }
    if let Some(data) = line.strip_prefix("data:") {
        let data = data.trim();
        if data == "[DONE]" {
            return Some(SseLine::Done);
        }
        return Some(SseLine::Data(data.to_string()));
    }
    if line.starts_with(':') {
        return Some(SseLine::Comment);
    }
    // Non-standard line — treat as data
    Some(SseLine::Data(line.to_string()))
}

/// Parsed SSE line.
#[derive(Debug, Clone, PartialEq)]
pub enum SseLine {
    Event(String),
    Data(String),
    Done,
    Empty,
    Comment,
}

// ---------------------------------------------------------------------------
// Provider-specific event parsers
// ---------------------------------------------------------------------------

/// Parse an Anthropic SSE data payload into a StreamEvent.
pub fn parse_anthropic_event(event_type: &str, data: &str) -> Option<StreamEvent> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;

    match event_type {
        "content_block_delta" => {
            let delta = json.get("delta")?;
            let delta_type = delta.get("type")?.as_str()?;
            match delta_type {
                "text_delta" => {
                    let text = delta.get("text")?.as_str()?;
                    Some(StreamEvent::ContentDelta {
                        delta: text.to_string(),
                    })
                }
                "input_json_delta" => {
                    let partial = delta.get("partial_json")?.as_str()?;
                    Some(StreamEvent::ToolCallDelta {
                        index: json.get("index")?.as_u64()? as u32,
                        id: None,
                        name: None,
                        arguments_delta: Some(partial.to_string()),
                    })
                }
                _ => None,
            }
        }
        "content_block_start" => {
            let content_block = json.get("content_block")?;
            let block_type = content_block.get("type")?.as_str()?;
            if block_type == "tool_use" {
                Some(StreamEvent::ToolCallDelta {
                    index: json.get("index")?.as_u64()? as u32,
                    id: content_block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    name: content_block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    arguments_delta: None,
                })
            } else {
                None
            }
        }
        "message_delta" => {
            if let Some(usage) = json.get("usage") {
                let output = usage
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                Some(StreamEvent::Usage {
                    usage: TokenUsage {
                        input_tokens: 0,
                        output_tokens: output,
                    },
                })
            } else {
                None
            }
        }
        "message_start" => {
            if let Some(usage) = json.get("message").and_then(|m| m.get("usage")) {
                let input = usage
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                Some(StreamEvent::Usage {
                    usage: TokenUsage {
                        input_tokens: input,
                        output_tokens: 0,
                    },
                })
            } else {
                None
            }
        }
        "message_stop" => Some(StreamEvent::Done),
        _ => None,
    }
}

/// Parse an OpenAI SSE data payload into a StreamEvent.
pub fn parse_openai_event(data: &str) -> Option<StreamEvent> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;

    // Check for usage in the response (sent in the final chunk)
    if let Some(usage) = json.get("usage") {
        let input = usage
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let output = usage
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        if input > 0 || output > 0 {
            return Some(StreamEvent::Usage {
                usage: TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                },
            });
        }
    }

    let choices = json.get("choices")?.as_array()?;
    let choice = choices.first()?;
    let delta = choice.get("delta")?;

    // Text content
    if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
        if !content.is_empty() {
            return Some(StreamEvent::ContentDelta {
                delta: content.to_string(),
            });
        }
    }

    // Tool calls
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        if let Some(tc) = tool_calls.first() {
            return Some(StreamEvent::ToolCallDelta {
                index: tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                id: tc.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                name: tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                arguments_delta: tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }

    None
}

/// Parse an NDJSON line (Ollama/local providers) into a StreamEvent.
pub fn parse_ndjson_event(data: &str) -> Option<StreamEvent> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;

    // Check if done
    if json.get("done").and_then(|v| v.as_bool()) == Some(true) {
        // Extract usage if available
        if let (Some(prompt), Some(eval)) = (
            json.get("prompt_eval_count").and_then(|v| v.as_u64()),
            json.get("eval_count").and_then(|v| v.as_u64()),
        ) {
            return Some(StreamEvent::Usage {
                usage: TokenUsage {
                    input_tokens: prompt as u32,
                    output_tokens: eval as u32,
                },
            });
            // Note: Done will be emitted by the caller after this Usage event
        }
        return Some(StreamEvent::Done);
    }

    // Content delta
    if let Some(response) = json.get("response").and_then(|v| v.as_str()) {
        if !response.is_empty() {
            return Some(StreamEvent::ContentDelta {
                delta: response.to_string(),
            });
        }
    }

    // Also handle "message" format (Ollama chat API)
    if let Some(content) = json
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
    {
        if !content.is_empty() {
            return Some(StreamEvent::ContentDelta {
                delta: content.to_string(),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SSE line parsing ---

    #[test]
    fn parse_event_line() {
        assert_eq!(
            parse_sse_line("event: content_block_delta"),
            Some(SseLine::Event("content_block_delta".into()))
        );
    }

    #[test]
    fn parse_data_line() {
        assert_eq!(
            parse_sse_line("data: {\"text\": \"hello\"}"),
            Some(SseLine::Data("{\"text\": \"hello\"}".into()))
        );
    }

    #[test]
    fn parse_done_line() {
        assert_eq!(parse_sse_line("data: [DONE]"), Some(SseLine::Done));
    }

    #[test]
    fn parse_empty_line() {
        assert_eq!(parse_sse_line(""), Some(SseLine::Empty));
    }

    #[test]
    fn parse_comment_line() {
        assert_eq!(parse_sse_line(": keep-alive"), Some(SseLine::Comment));
    }

    #[test]
    fn parse_data_with_carriage_return() {
        assert_eq!(
            parse_sse_line("data: hello\r"),
            Some(SseLine::Data("hello".into()))
        );
    }

    #[test]
    fn parse_ndjson_line() {
        // Non-prefixed JSON line (NDJSON format) → treated as data
        assert_eq!(
            parse_sse_line("{\"response\": \"hi\"}"),
            Some(SseLine::Data("{\"response\": \"hi\"}".into()))
        );
    }

    // --- StreamEvent serde ---

    #[test]
    fn stream_event_content_delta_serde() {
        let event = StreamEvent::ContentDelta {
            delta: "hello".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content_delta");
        assert_eq!(json["delta"], "hello");
    }

    #[test]
    fn stream_event_usage_serde() {
        let event = StreamEvent::Usage {
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "usage");
        assert_eq!(json["usage"]["input_tokens"], 100);
    }

    #[test]
    fn stream_event_done_serde() {
        let event = StreamEvent::Done;
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "done");
    }

    // --- Anthropic event parsing ---

    #[test]
    fn anthropic_text_delta() {
        let event = parse_anthropic_event(
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
        );
        assert_eq!(
            event,
            Some(StreamEvent::ContentDelta {
                delta: "Hello".into()
            })
        );
    }

    #[test]
    fn anthropic_tool_call_start() {
        let event = parse_anthropic_event(
            "content_block_start",
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call_1","name":"read_file","input":{}}}"#,
        );
        assert_eq!(
            event,
            Some(StreamEvent::ToolCallDelta {
                index: 1,
                id: Some("call_1".into()),
                name: Some("read_file".into()),
                arguments_delta: None,
            })
        );
    }

    #[test]
    fn anthropic_tool_call_delta() {
        let event = parse_anthropic_event(
            "content_block_delta",
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#,
        );
        assert!(matches!(
            event,
            Some(StreamEvent::ToolCallDelta {
                arguments_delta: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn anthropic_message_delta_usage() {
        let event = parse_anthropic_event(
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#,
        );
        assert_eq!(
            event,
            Some(StreamEvent::Usage {
                usage: TokenUsage {
                    input_tokens: 0,
                    output_tokens: 42
                },
            })
        );
    }

    #[test]
    fn anthropic_message_start_usage() {
        let event = parse_anthropic_event(
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":100,"output_tokens":0}}}"#,
        );
        assert_eq!(
            event,
            Some(StreamEvent::Usage {
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 0
                },
            })
        );
    }

    #[test]
    fn anthropic_message_stop() {
        let event = parse_anthropic_event("message_stop", r#"{"type":"message_stop"}"#);
        assert_eq!(event, Some(StreamEvent::Done));
    }

    #[test]
    fn anthropic_unknown_event_returns_none() {
        let event = parse_anthropic_event("ping", r#"{"type":"ping"}"#);
        assert_eq!(event, None);
    }

    // --- OpenAI event parsing ---

    #[test]
    fn openai_content_delta() {
        let event = parse_openai_event(r#"{"choices":[{"index":0,"delta":{"content":"Hello"}}]}"#);
        assert_eq!(
            event,
            Some(StreamEvent::ContentDelta {
                delta: "Hello".into()
            })
        );
    }

    #[test]
    fn openai_tool_call_delta() {
        let event = parse_openai_event(
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"path\":"}}]}}]}"#,
        );
        assert_eq!(
            event,
            Some(StreamEvent::ToolCallDelta {
                index: 0,
                id: Some("call_1".into()),
                name: Some("read_file".into()),
                arguments_delta: Some("{\"path\":".into()),
            })
        );
    }

    #[test]
    fn openai_usage_in_final_chunk() {
        let event = parse_openai_event(
            r#"{"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":50}}"#,
        );
        assert_eq!(
            event,
            Some(StreamEvent::Usage {
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50
                },
            })
        );
    }

    #[test]
    fn openai_empty_content_ignored() {
        let event = parse_openai_event(
            r#"{"choices":[{"index":0,"delta":{"content":"","role":"assistant"}}]}"#,
        );
        assert_eq!(event, None); // empty content string ignored
    }

    // --- NDJSON event parsing ---

    #[test]
    fn ndjson_content_delta() {
        let event = parse_ndjson_event(r#"{"response":"Hello","done":false}"#);
        assert_eq!(
            event,
            Some(StreamEvent::ContentDelta {
                delta: "Hello".into()
            })
        );
    }

    #[test]
    fn ndjson_done_with_usage() {
        let event = parse_ndjson_event(r#"{"done":true,"prompt_eval_count":50,"eval_count":30}"#);
        assert_eq!(
            event,
            Some(StreamEvent::Usage {
                usage: TokenUsage {
                    input_tokens: 50,
                    output_tokens: 30
                },
            })
        );
    }

    #[test]
    fn ndjson_done_without_usage() {
        let event = parse_ndjson_event(r#"{"done":true}"#);
        assert_eq!(event, Some(StreamEvent::Done));
    }

    #[test]
    fn ndjson_chat_format() {
        let event =
            parse_ndjson_event(r#"{"message":{"role":"assistant","content":"Hi"},"done":false}"#);
        assert_eq!(
            event,
            Some(StreamEvent::ContentDelta { delta: "Hi".into() })
        );
    }

    #[test]
    fn ndjson_invalid_json_returns_none() {
        let event = parse_ndjson_event("not json");
        assert_eq!(event, None);
    }

    // --- SSE edge cases ---

    #[test]
    fn parse_sse_line_data_with_embedded_colon() {
        assert_eq!(
            parse_sse_line("data: {\"key\": \"value:with:colons\"}"),
            Some(SseLine::Data("{\"key\": \"value:with:colons\"}".into()))
        );
    }

    #[test]
    fn parse_sse_line_unicode() {
        assert_eq!(
            parse_sse_line("data: {\"text\": \"Hello 🌍\"}"),
            Some(SseLine::Data("{\"text\": \"Hello 🌍\"}".into()))
        );
    }

    #[test]
    fn anthropic_text_block_start_ignored() {
        let event = parse_anthropic_event(
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        );
        assert_eq!(event, None);
    }

    #[test]
    fn openai_empty_choices_returns_none() {
        assert_eq!(parse_openai_event(r#"{"choices":[]}"#), None);
    }

    #[test]
    fn openai_invalid_json_returns_none() {
        assert_eq!(parse_openai_event("not json"), None);
    }

    // --- StreamEvent serde ---

    #[test]
    fn stream_event_tool_call_delta_serde() {
        let event = StreamEvent::ToolCallDelta {
            index: 0,
            id: Some("call_1".into()),
            name: Some("read_file".into()),
            arguments_delta: Some("{\"pa".into()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "tool_call_delta");
        assert_eq!(json["index"], 0);
        assert_eq!(json["id"], "call_1");
    }
}
