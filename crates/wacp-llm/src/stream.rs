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
        Self { inner: stream, model, request_id }
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
        let event = StreamEvent::ContentDelta { delta: "hello".into() };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content_delta");
        assert_eq!(json["delta"], "hello");
    }

    #[test]
    fn stream_event_usage_serde() {
        let event = StreamEvent::Usage {
            usage: TokenUsage { input_tokens: 100, output_tokens: 50 },
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
