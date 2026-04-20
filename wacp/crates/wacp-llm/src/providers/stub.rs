//! Deterministic stub `LlmAdapter` for integration and E2E tests.
//!
//! Serves canned responses from a YAML fixture file, keyed by message-prefix,
//! by SHA-256 hash, or by substring-contains. Safe to run in CI — no network,
//! no secrets. See `wcon-llm-stub` for the full design.

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use futures::Stream;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::adapter::{CompletionOptions, LlmAdapter};
use crate::error::LlmError;
use crate::result::{CompletionResult, ModelInfo, ProviderHealth, TokenUsage, ToolCall};
use crate::stream::{StreamEvent, StreamHandle};
use crate::types::{Content, ContentBlock, Message, Role, ToolDefinition};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single fixture entry: how to recognize a request and what to return.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StubEntry {
    #[serde(rename = "match")]
    pub matcher: StubMatcher,
    pub response: StubResponse,
}

/// How to match a request against this entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StubMatcher {
    /// Serialized message stream starts with `value`.
    Prefix { value: String },
    /// SHA-256 of the serialized message stream (lowercase hex) equals `value`.
    Hash { value: String },
    /// Serialized message stream contains `value` as a substring.
    Contains { value: String },
}

/// A canned response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StubResponse {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub tool_calls: Vec<StubToolCall>,
}

/// A canned tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StubToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// Loaded fixture set. Cheap to clone (fields are already owned).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StubFixtures {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub default: Option<StubResponse>,
    #[serde(default)]
    pub entries: Vec<StubEntry>,
}

fn default_version() -> u32 {
    1
}

impl StubFixtures {
    /// Load fixtures from a YAML file on disk.
    pub fn load(path: &Path) -> Result<Self, LlmError> {
        let yaml = std::fs::read_to_string(path).map_err(|e| {
            LlmError::structural(format!("stub fixture read {}: {e}", path.display()))
        })?;
        Self::from_yaml(&yaml)
    }

    /// Parse fixtures from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, LlmError> {
        let parsed: StubFixtures = serde_yaml_ng::from_str(yaml)
            .map_err(|e| LlmError::structural(format!("stub fixture parse: {e}")))?;
        if parsed.version != 1 {
            return Err(LlmError::structural(format!(
                "stub fixture version {}: only version 1 is supported",
                parsed.version
            )));
        }
        Ok(parsed)
    }

    /// Find the first entry whose matcher matches `serialized`, or fall back
    /// to the default entry. Returns `None` if nothing matches and no default
    /// is set.
    pub fn matches(&self, serialized: &str) -> Option<&StubResponse> {
        for entry in &self.entries {
            if entry.matcher.matches(serialized) {
                return Some(&entry.response);
            }
        }
        self.default.as_ref()
    }
}

impl StubMatcher {
    pub fn matches(&self, serialized: &str) -> bool {
        match self {
            StubMatcher::Prefix { value } => serialized.starts_with(value),
            StubMatcher::Hash { value } => {
                let digest = hash_hex(serialized);
                digest == value.to_lowercase()
            }
            StubMatcher::Contains { value } => serialized.contains(value),
        }
    }
}

// ---------------------------------------------------------------------------
// The adapter
// ---------------------------------------------------------------------------

/// `LlmAdapter` backed by a `StubFixtures` set.
#[derive(Clone)]
pub struct StubAdapter {
    fixtures: Arc<StubFixtures>,
    default_model: String,
    token_delay_ms: u64,
}

impl StubAdapter {
    /// Create a new stub adapter. `default_model` is used whenever the caller
    /// does not pass `CompletionOptions.model`. `token_delay_ms` adds a delay
    /// between streamed events — useful for tests that exercise streaming
    /// backpressure. Pass `0` for the fastest possible stream.
    pub fn new(
        fixtures: StubFixtures,
        default_model: impl Into<String>,
        token_delay_ms: u64,
    ) -> Self {
        Self {
            fixtures: Arc::new(fixtures),
            default_model: default_model.into(),
            token_delay_ms,
        }
    }

    fn resolve_response(
        &self,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> Result<(StubResponse, usize), LlmError> {
        // C5 (backend-perf-baseline-plan): serialize once per complete(),
        // return `(response, serialized_len)` so the caller can compute
        // `input_tokens = len / 4` without a second `serialize_for_match`
        // allocation. Previously 2× alloc per call; now 1×.
        let serialized = serialize_for_match(messages, &options.tools);
        let serialized_len = serialized.len();
        match self.fixtures.matches(&serialized) {
            Some(resp) => Ok((resp.clone(), serialized_len)),
            None => Err(LlmError::structural(format!(
                "stub: no fixture match for {}-char input (hash={})",
                serialized_len,
                hash_hex(&serialized),
            ))),
        }
    }

    fn resolved_model(&self, options: &CompletionOptions) -> String {
        options
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone())
    }
}

impl LlmAdapter for StubAdapter {
    fn complete(
        &self,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<CompletionResult, LlmError>> + Send + '_>>
    {
        let messages = messages.to_vec();
        let options = clone_options(options);
        Box::pin(async move {
            let started = std::time::Instant::now();
            let (response, serialized_len) = self.resolve_response(&messages, &options)?;
            let model = self.resolved_model(&options);
            let input_tokens = (serialized_len / 4) as u32;
            let tool_calls = response
                .tool_calls
                .iter()
                .map(|tc| ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                })
                .collect();
            Ok(CompletionResult {
                content: response.content,
                tool_calls,
                usage: TokenUsage {
                    input_tokens,
                    output_tokens: response.output_tokens,
                },
                cost: None,
                model,
                latency_ms: started.elapsed().as_millis() as u64,
                truncated: false,
            })
        })
    }

    fn complete_stream(
        &self,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<StreamHandle, LlmError>> + Send + '_>>
    {
        let messages = messages.to_vec();
        let options = clone_options(options);
        Box::pin(async move {
            let (response, serialized_len) = self.resolve_response(&messages, &options)?;
            let model = self.resolved_model(&options);
            let input_tokens = (serialized_len / 4) as u32;
            let usage = TokenUsage {
                input_tokens,
                output_tokens: response.output_tokens,
            };
            let delay_ms = self.token_delay_ms;
            // C6 (backend-perf-baseline-plan): yield stream events lazily via
            // `async_stream!` instead of materializing the full `Vec<StreamEvent>`
            // upfront. Peak memory now O(1) in event count — the 1000-token
            // fixture doesn't preallocate 1000 `StreamEvent`s.
            let stream: Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>> =
                Box::pin(async_stream::stream! {
                    for ch in response.content.chars() {
                        if delay_ms > 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        }
                        yield Ok(StreamEvent::ContentDelta { delta: ch.to_string() });
                    }
                    for (i, tc) in response.tool_calls.iter().enumerate() {
                        if delay_ms > 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        }
                        yield Ok(StreamEvent::ToolCallDelta {
                            index: i as u32,
                            id: Some(tc.id.clone()),
                            name: Some(tc.name.clone()),
                            arguments_delta: None,
                        });
                        if delay_ms > 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        }
                        yield Ok(StreamEvent::ToolCallDelta {
                            index: i as u32,
                            id: None,
                            name: None,
                            arguments_delta: Some(tc.arguments.to_string()),
                        });
                    }
                    if delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    yield Ok(StreamEvent::Usage { usage });
                    if delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    yield Ok(StreamEvent::Done);
                });
            Ok(StreamHandle::new(
                stream,
                model,
                Some(format!("stub-req-{}", uuid::Uuid::new_v4())),
            ))
        })
    }

    fn models(
        &self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<ModelInfo>, LlmError>> + Send + '_>>
    {
        let model = self.default_model.clone();
        Box::pin(async move {
            Ok(vec![ModelInfo {
                id: model,
                name: Some("Stub (deterministic)".into()),
                max_context: Some(1_000_000),
                max_output: Some(1_000_000),
                supports_tools: true,
                supports_streaming: true,
            }])
        })
    }

    fn health(&self) -> Pin<Box<dyn std::future::Future<Output = ProviderHealth> + Send + '_>> {
        Box::pin(async move {
            ProviderHealth {
                healthy: true,
                latency_ms: Some(0),
                error: None,
                models_available: 1,
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Serialize a message stream into a stable matcher-friendly form.
pub fn serialize_for_match(messages: &[Message], tools: &[ToolDefinition]) -> String {
    let mut out = String::new();
    for msg in messages {
        out.push_str(role_name(msg.role));
        out.push_str(":\n");
        serialize_content(&msg.content, &mut out);
        out.push_str("\n---\n");
    }
    if !tools.is_empty() {
        out.push_str("tools:\n");
        for t in tools {
            out.push_str(&t.name);
            out.push_str(": ");
            out.push_str(&t.description);
            out.push('\n');
        }
    }
    out
}

fn serialize_content(content: &Content, out: &mut String) {
    match content {
        Content::Text(s) => out.push_str(s),
        Content::Blocks(blocks) => {
            for b in blocks {
                match b {
                    ContentBlock::Text { text } => out.push_str(text),
                    ContentBlock::ToolUse { id: _, name, input } => {
                        out.push_str("tool_use(");
                        out.push_str(name);
                        out.push_str(", ");
                        out.push_str(&input.to_string());
                        out.push(')');
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        out.push_str("tool_result(");
                        out.push_str(tool_use_id);
                        out.push_str(", ");
                        out.push_str(content);
                        if *is_error {
                            out.push_str(", error");
                        }
                        out.push(')');
                    }
                }
                out.push('\n');
            }
        }
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn hash_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let out = hasher.finalize();
    let mut hex = String::with_capacity(out.len() * 2);
    for byte in out {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn clone_options(o: &CompletionOptions) -> CompletionOptions {
    CompletionOptions {
        model: o.model.clone(),
        max_tokens: o.max_tokens,
        temperature: o.temperature,
        stop: o.stop.clone(),
        tools: o.tools.clone(),
        timeout_ms: o.timeout_ms,
        extra: o.extra.clone(),
    }
}

/// Base64-decode a fixture payload field. Exposed so integration tests can
/// decode payloads coming out of `StubToolCall.arguments.payload`.
pub fn decode_b64_payload(encoded: &str) -> Result<Vec<u8>, LlmError> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| LlmError::structural(format!("stub: base64 decode: {e}")))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use serde_json::json;

    fn fx() -> StubFixtures {
        StubFixtures {
            version: 1,
            default: Some(StubResponse {
                content: "default-response".into(),
                output_tokens: 2,
                tool_calls: vec![],
            }),
            entries: vec![
                StubEntry {
                    matcher: StubMatcher::Prefix {
                        value: "system:\nYou are".into(),
                    },
                    response: StubResponse {
                        content: "prefix-hit".into(),
                        output_tokens: 1,
                        tool_calls: vec![],
                    },
                },
                StubEntry {
                    matcher: StubMatcher::Contains {
                        value: "emit complete".into(),
                    },
                    response: StubResponse {
                        content: "complete".into(),
                        output_tokens: 1,
                        tool_calls: vec![StubToolCall {
                            id: "t1".into(),
                            name: "emit_signal".into(),
                            arguments: json!({"type": "complete"}),
                        }],
                    },
                },
            ],
        }
    }

    fn opts() -> CompletionOptions {
        CompletionOptions::default()
    }

    #[test]
    fn matcher_prefix() {
        let m = StubMatcher::Prefix {
            value: "hello".into(),
        };
        assert!(m.matches("hello world"));
        assert!(!m.matches("world hello"));
    }

    #[test]
    fn matcher_contains() {
        let m = StubMatcher::Contains {
            value: "middle".into(),
        };
        assert!(m.matches("start middle end"));
        assert!(!m.matches("start end"));
    }

    #[test]
    fn matcher_hash_matches_hex() {
        let digest = hash_hex("canary");
        let m = StubMatcher::Hash {
            value: digest.clone(),
        };
        assert!(m.matches("canary"));
        assert!(!m.matches("other"));
    }

    #[test]
    fn matcher_hash_is_case_insensitive_on_value() {
        let upper = hash_hex("canary").to_uppercase();
        let m = StubMatcher::Hash { value: upper };
        assert!(m.matches("canary"));
    }

    #[test]
    fn fixtures_from_yaml_roundtrip() {
        let yaml = r#"
version: 1
default:
  content: "hi"
  output_tokens: 3
entries:
  - match: {kind: prefix, value: "system"}
    response: {content: "p", output_tokens: 1}
  - match: {kind: contains, value: "dispatch"}
    response: {content: "c", output_tokens: 1}
"#;
        let fx = StubFixtures::from_yaml(yaml).unwrap();
        assert_eq!(fx.version, 1);
        assert_eq!(fx.default.as_ref().unwrap().content, "hi");
        assert_eq!(fx.entries.len(), 2);
    }

    #[test]
    fn fixtures_unsupported_version_rejected() {
        let yaml = "version: 2\nentries: []\n";
        let err = StubFixtures::from_yaml(yaml).unwrap_err();
        assert!(err.message.contains("version 2"));
    }

    #[test]
    fn fixtures_default_optional() {
        let yaml = "version: 1\nentries: []\n";
        let fx = StubFixtures::from_yaml(yaml).unwrap();
        assert!(fx.default.is_none());
        assert!(fx.entries.is_empty());
    }

    #[test]
    fn fixtures_load_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fx.yaml");
        std::fs::write(&path, "version: 1\nentries: []\n").unwrap();
        let fx = StubFixtures::load(&path).unwrap();
        assert!(fx.entries.is_empty());
    }

    #[test]
    fn fixtures_load_missing_file_errors() {
        let err = StubFixtures::load(Path::new("/nonexistent/path/to/fx.yaml")).unwrap_err();
        assert!(err.message.contains("stub fixture read"));
    }

    #[test]
    fn serialize_for_match_stable() {
        let msgs = vec![Message::system("You are helpful"), Message::user("Hi")];
        let s1 = serialize_for_match(&msgs, &[]);
        let s2 = serialize_for_match(&msgs, &[]);
        assert_eq!(s1, s2);
        assert!(s1.starts_with("system:\nYou are helpful"));
        assert!(s1.contains("user:\nHi"));
    }

    #[test]
    fn serialize_for_match_includes_tools() {
        let msgs = vec![Message::user("x")];
        let tools = vec![ToolDefinition {
            name: "read_file".into(),
            description: "read a file".into(),
            input_schema: json!({}),
        }];
        let s = serialize_for_match(&msgs, &tools);
        assert!(s.contains("tools:"));
        assert!(s.contains("read_file: read a file"));
    }

    #[test]
    fn serialize_content_blocks_variants() {
        let msgs = vec![Message {
            role: Role::Assistant,
            content: Content::Blocks(vec![
                ContentBlock::Text {
                    text: "alpha".into(),
                },
                ContentBlock::ToolUse {
                    id: "id1".into(),
                    name: "do_it".into(),
                    input: json!({"k": 1}),
                },
                ContentBlock::ToolResult {
                    tool_use_id: "id1".into(),
                    content: "ok".into(),
                    is_error: false,
                },
            ]),
        }];
        let s = serialize_for_match(&msgs, &[]);
        assert!(s.contains("alpha"));
        assert!(s.contains("tool_use(do_it"));
        assert!(s.contains("tool_result(id1, ok"));
    }

    #[test]
    fn serialize_content_tool_result_error_flag() {
        let msg = Message::tool_result("tid", "boom", true);
        let s = serialize_for_match(&[msg], &[]);
        assert!(s.contains(", error"));
    }

    #[tokio::test]
    async fn complete_returns_prefix_match() {
        let adapter = StubAdapter::new(fx(), "stub-model", 0);
        let msgs = vec![Message::system("You are helpful")];
        let result = adapter.complete(&msgs, &opts()).await.unwrap();
        assert_eq!(result.content, "prefix-hit");
        assert_eq!(result.usage.output_tokens, 1);
        assert_eq!(result.model, "stub-model");
        assert!(result.cost.is_none());
        assert!(!result.truncated);
    }

    #[tokio::test]
    async fn complete_falls_back_to_default() {
        let adapter = StubAdapter::new(fx(), "stub-model", 0);
        let msgs = vec![Message::user("totally unrelated input")];
        let result = adapter.complete(&msgs, &opts()).await.unwrap();
        assert_eq!(result.content, "default-response");
    }

    #[tokio::test]
    async fn complete_tool_call_roundtrips() {
        let adapter = StubAdapter::new(fx(), "stub-model", 0);
        let msgs = vec![Message::user("please emit complete now")];
        let result = adapter.complete(&msgs, &opts()).await.unwrap();
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "emit_signal");
        assert_eq!(result.tool_calls[0].arguments["type"], "complete");
    }

    #[tokio::test]
    async fn complete_errors_when_no_match_and_no_default() {
        let fixtures = StubFixtures {
            version: 1,
            default: None,
            entries: vec![StubEntry {
                matcher: StubMatcher::Prefix {
                    value: "nope".into(),
                },
                response: StubResponse::default(),
            }],
        };
        let adapter = StubAdapter::new(fixtures, "stub-model", 0);
        let err = adapter
            .complete(&[Message::user("anything")], &opts())
            .await
            .unwrap_err();
        assert!(err.message.contains("no fixture match"));
    }

    #[tokio::test]
    async fn complete_uses_caller_model_override() {
        let adapter = StubAdapter::new(fx(), "stub-default", 0);
        let mut o = opts();
        o.model = Some("custom-model".into());
        let result = adapter.complete(&[Message::user("x")], &o).await.unwrap();
        assert_eq!(result.model, "custom-model");
    }

    #[tokio::test]
    async fn stream_emits_content_then_usage_then_done() {
        let adapter = StubAdapter::new(fx(), "stub-model", 0);
        let handle = adapter
            .complete_stream(&[Message::system("You are helpful")], &opts())
            .await
            .unwrap();
        assert_eq!(handle.model, "stub-model");
        assert!(handle.request_id.is_some());
        let events: Vec<_> = handle
            .into_stream()
            .collect::<Vec<Result<StreamEvent, LlmError>>>()
            .await;
        // "prefix-hit" is 10 chars → 10 ContentDelta + Usage + Done = 12 events
        assert_eq!(events.len(), 12);
        let content: String = events
            .iter()
            .take(10)
            .filter_map(|ev| match ev.as_ref().unwrap() {
                StreamEvent::ContentDelta { delta } => Some(delta.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(content, "prefix-hit");
        assert!(matches!(
            events[10].as_ref().unwrap(),
            StreamEvent::Usage { .. }
        ));
        assert!(matches!(events[11].as_ref().unwrap(), StreamEvent::Done));
    }

    #[tokio::test]
    async fn stream_emits_tool_calls() {
        let adapter = StubAdapter::new(fx(), "stub-model", 0);
        let handle = adapter
            .complete_stream(&[Message::user("please emit complete now")], &opts())
            .await
            .unwrap();
        let events: Vec<_> = handle
            .into_stream()
            .collect::<Vec<Result<StreamEvent, LlmError>>>()
            .await;
        let tool_events: Vec<_> = events
            .iter()
            .filter_map(|ev| match ev.as_ref().unwrap() {
                StreamEvent::ToolCallDelta { .. } => Some(ev.as_ref().unwrap().clone()),
                _ => None,
            })
            .collect();
        // 1 content char + 2 tool events + Usage + Done
        assert_eq!(tool_events.len(), 2);
        match &tool_events[0] {
            StreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments_delta,
            } => {
                assert_eq!(*index, 0);
                assert_eq!(id.as_deref(), Some("t1"));
                assert_eq!(name.as_deref(), Some("emit_signal"));
                assert!(arguments_delta.is_none());
            }
            _ => unreachable!(),
        }
        match &tool_events[1] {
            StreamEvent::ToolCallDelta {
                index,
                arguments_delta,
                ..
            } => {
                assert_eq!(*index, 0);
                let args = arguments_delta.as_ref().unwrap();
                assert!(args.contains("complete"));
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn stream_delay_observed() {
        // Use a tiny delay so the test doesn't pay too much wall-time but we
        // can still assert the pacing code path ran.
        let adapter = StubAdapter::new(fx(), "stub-model", 2);
        let started = std::time::Instant::now();
        let handle = adapter
            .complete_stream(&[Message::system("You are helpful")], &opts())
            .await
            .unwrap();
        let events: Vec<_> = handle
            .into_stream()
            .collect::<Vec<Result<StreamEvent, LlmError>>>()
            .await;
        let elapsed = started.elapsed();
        assert_eq!(events.len(), 12);
        // 12 events × 2 ms ≥ 20 ms floor (allow a generous CI margin).
        assert!(
            elapsed >= std::time::Duration::from_millis(15),
            "expected ≥ 15 ms, got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn models_returns_single_entry() {
        let adapter = StubAdapter::new(fx(), "stub-model-1", 0);
        let models = adapter.models().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "stub-model-1");
        assert!(models[0].supports_tools);
        assert!(models[0].supports_streaming);
    }

    #[tokio::test]
    async fn health_always_ok() {
        let adapter = StubAdapter::new(fx(), "stub-model", 0);
        let h = adapter.health().await;
        assert!(h.healthy);
        assert_eq!(h.models_available, 1);
        assert!(h.error.is_none());
    }

    #[test]
    fn decode_b64_payload_ok_and_err() {
        let bytes = decode_b64_payload("aGVsbG8=").unwrap();
        assert_eq!(bytes, b"hello");
        let err = decode_b64_payload("!!!not_base64!!!").unwrap_err();
        assert!(err.message.contains("base64 decode"));
    }

    #[test]
    fn serialize_is_order_sensitive() {
        let a = vec![Message::system("A"), Message::user("B")];
        let b = vec![Message::user("B"), Message::system("A")];
        assert_ne!(serialize_for_match(&a, &[]), serialize_for_match(&b, &[]));
    }

    #[test]
    fn matches_first_entry_wins() {
        let fixtures = StubFixtures {
            version: 1,
            default: None,
            entries: vec![
                StubEntry {
                    matcher: StubMatcher::Contains {
                        value: "keyword".into(),
                    },
                    response: StubResponse {
                        content: "first".into(),
                        output_tokens: 0,
                        tool_calls: vec![],
                    },
                },
                StubEntry {
                    matcher: StubMatcher::Contains {
                        value: "keyword".into(),
                    },
                    response: StubResponse {
                        content: "second".into(),
                        output_tokens: 0,
                        tool_calls: vec![],
                    },
                },
            ],
        };
        let adapter = StubAdapter::new(fixtures, "m", 0);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt
            .block_on(adapter.complete(&[Message::user("has keyword inside")], &opts()))
            .unwrap();
        assert_eq!(result.content, "first");
    }

    #[test]
    fn fixtures_clone_is_cheap() {
        let a = fx();
        let b = a.clone();
        assert_eq!(a.entries.len(), b.entries.len());
    }
}
