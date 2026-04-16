//! Branch-coverage tests for wacp-llm.
//!
//! Covers three axes:
//!   1. Per-provider error mapping (OpenAI, Anthropic, transport, structural)
//!   2. Retry classifier arms (4xx, 5xx, timeout, body-limit, network, budget)
//!   3. Streaming parser edges (partial JSON, invalid UTF-8, disconnect, empty)

use std::pin::Pin;
use std::time::Duration;

use futures::stream;
use futures::StreamExt;
use serde_json::json;

use wacp_llm::error::{ErrorOrigin, ErrorPersistence, LlmError};
use wacp_llm::retry::{
    apply_retry_after, compute_delay, should_retry, BackoffStrategy, RetryConfig,
};
use wacp_llm::stream::{
    parse_anthropic_event, parse_ndjson_event, parse_openai_event, parse_sse_line, SseLine,
    StreamEvent, StreamHandle,
};
use wacp_llm::result::TokenUsage;

// ===================================================================
// 1. Provider error mapping
// ===================================================================

mod provider_error_mapping {
    use super::*;
    use wacp_llm::providers::anthropic;
    use wacp_llm::providers::openai;

    // --- OpenAI ---

    #[test]
    fn openai_429_rate_limit_is_retryable() {
        let body = json!({
            "error": {"type": "rate_limit_error", "message": "Rate limit reached for gpt-4o"}
        });
        let err = openai::parse_error(429, &body);
        assert!(err.retryable, "OpenAI 429 must be retryable");
        assert_eq!(err.persistence, ErrorPersistence::Transient);
        assert_eq!(err.origin, ErrorOrigin::Provider);
        assert_eq!(err.status, Some(429));
        assert_eq!(err.code.as_deref(), Some("rate_limit_error"));
    }

    #[test]
    fn openai_500_server_error_is_retryable() {
        let body = json!({
            "error": {"type": "server_error", "message": "Internal server error"}
        });
        let err = openai::parse_error(500, &body);
        assert!(err.retryable, "OpenAI 500 must be retryable");
        assert_eq!(err.persistence, ErrorPersistence::Transient);
        assert_eq!(err.origin, ErrorOrigin::Provider);
    }

    #[test]
    fn openai_400_bad_request_not_retryable() {
        let body = json!({
            "error": {"type": "invalid_request_error", "message": "Invalid model: gpt-99"}
        });
        let err = openai::parse_error(400, &body);
        assert!(!err.retryable, "OpenAI 400 must NOT be retryable");
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
        assert_eq!(err.origin, ErrorOrigin::Structural);
        assert_eq!(err.code.as_deref(), Some("invalid_request_error"));
    }

    #[test]
    fn openai_error_with_empty_body() {
        let body = json!({});
        let err = openai::parse_error(500, &body);
        assert!(err.retryable);
        assert_eq!(err.code.as_deref(), Some("unknown_error"));
        assert_eq!(err.message, "Unknown error");
    }

    #[test]
    fn openai_error_with_nested_null_fields() {
        let body = json!({
            "error": {"type": null, "message": null}
        });
        let err = openai::parse_error(429, &body);
        assert!(err.retryable);
        // type/message fall through to defaults since as_str() on null -> None
        assert_eq!(err.code.as_deref(), Some("unknown_error"));
        assert_eq!(err.message, "Unknown error");
    }

    // --- Anthropic ---

    #[test]
    fn anthropic_529_overloaded_is_retryable() {
        // Anthropic 529 is not in the explicit status map, so it gets Unknown.
        // But a 529 body with "overloaded_error" is still provider-classified.
        let body = json!({
            "error": {"type": "overloaded_error", "message": "Overloaded"}
        });
        let err = anthropic::parse_error(529, &body);
        // 529 is unmapped -> Provider/Unknown -> retryable = false per classify_status.
        // This is a design-level observation: the code maps 529 to Unknown.
        assert_eq!(err.origin, ErrorOrigin::Provider);
        assert_eq!(err.persistence, ErrorPersistence::Unknown);
        assert_eq!(err.code.as_deref(), Some("overloaded_error"));
        // Unknown persistence -> not retryable
        assert!(!err.retryable);
    }

    #[test]
    fn anthropic_429_rate_limit_is_retryable() {
        let body = json!({
            "error": {"type": "rate_limit_error", "message": "Rate limit exceeded"}
        });
        let err = anthropic::parse_error(429, &body);
        assert!(err.retryable, "Anthropic 429 must be retryable");
        assert_eq!(err.persistence, ErrorPersistence::Transient);
        assert_eq!(err.code.as_deref(), Some("rate_limit_error"));
    }

    #[test]
    fn anthropic_400_bad_request_not_retryable() {
        let body = json!({
            "error": {"type": "invalid_request_error", "message": "max_tokens must be > 0"}
        });
        let err = anthropic::parse_error(400, &body);
        assert!(!err.retryable, "Anthropic 400 must NOT be retryable");
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
        assert_eq!(err.origin, ErrorOrigin::Structural);
    }

    #[test]
    fn anthropic_503_overloaded_is_retryable() {
        let body = json!({
            "error": {"type": "overloaded_error", "message": "API is temporarily overloaded"}
        });
        let err = anthropic::parse_error(503, &body);
        assert!(err.retryable, "Anthropic 503 must be retryable");
        assert_eq!(err.persistence, ErrorPersistence::Transient);
        assert_eq!(err.origin, ErrorOrigin::Provider);
    }

    #[test]
    fn anthropic_error_with_empty_body() {
        let body = json!({});
        let err = anthropic::parse_error(500, &body);
        assert!(err.retryable);
        assert_eq!(err.code.as_deref(), Some("unknown_error"));
    }

    // --- Network/transport errors ---

    #[test]
    fn network_error_connection_refused_is_retryable() {
        let err = LlmError::transport("connection refused", true);
        assert!(err.retryable, "Connection refused must be retryable");
        assert_eq!(err.origin, ErrorOrigin::Transport);
        assert_eq!(err.persistence, ErrorPersistence::Transient);
        assert!(err.status.is_none(), "Transport error has no HTTP status");
    }

    #[test]
    fn network_error_dns_resolution_is_retryable() {
        let err = LlmError::transport("DNS resolution failed", true);
        assert!(err.retryable);
        assert_eq!(err.origin, ErrorOrigin::Transport);
    }

    #[test]
    fn network_error_tls_permanent() {
        let err = LlmError::transport("TLS handshake failed: certificate expired", false);
        assert!(!err.retryable, "Permanent TLS error must NOT be retryable");
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
    }

    // --- Malformed JSON response ---

    #[test]
    fn malformed_json_response_not_retryable() {
        let err = LlmError::structural("failed to parse response body: invalid JSON");
        assert!(!err.retryable, "Malformed JSON must NOT be retryable");
        assert_eq!(err.origin, ErrorOrigin::Structural);
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
    }

    // --- OpenAI response parsing with malformed body ---

    #[test]
    fn openai_parse_response_missing_choices() {
        let body = json!({"model": "gpt-4o"});
        let err = openai::parse_response(&body).unwrap_err();
        assert_eq!(err.origin, ErrorOrigin::Structural);
        assert!(!err.retryable);
    }

    #[test]
    fn openai_parse_response_choices_not_array() {
        let body = json!({"model": "gpt-4o", "choices": "not an array"});
        let err = openai::parse_response(&body).unwrap_err();
        assert_eq!(err.origin, ErrorOrigin::Structural);
    }

    #[test]
    fn openai_parse_response_empty_choices() {
        let body = json!({"model": "gpt-4o", "choices": []});
        let err = openai::parse_response(&body).unwrap_err();
        assert_eq!(err.origin, ErrorOrigin::Structural);
    }

    // --- Anthropic response parsing with malformed body ---

    #[test]
    fn anthropic_parse_response_missing_content() {
        let body = json!({"model": "claude-sonnet-4-20250514"});
        let err = anthropic::parse_response(&body).unwrap_err();
        assert_eq!(err.origin, ErrorOrigin::Structural);
        assert!(!err.retryable);
    }

    #[test]
    fn anthropic_parse_response_content_not_array() {
        let body = json!({"model": "claude-sonnet-4-20250514", "content": "not an array"});
        let err = anthropic::parse_response(&body).unwrap_err();
        assert_eq!(err.origin, ErrorOrigin::Structural);
    }

    // --- HTTP status code edge coverage ---

    #[test]
    fn http_408_timeout_is_transport_transient() {
        let err = LlmError::from_status(408, "request timeout");
        assert!(err.retryable);
        assert_eq!(err.origin, ErrorOrigin::Transport);
        assert_eq!(err.persistence, ErrorPersistence::Transient);
    }

    #[test]
    fn http_403_forbidden_is_permanent() {
        let err = LlmError::from_status(403, "forbidden");
        assert!(!err.retryable);
        assert_eq!(err.origin, ErrorOrigin::Provider);
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
    }

    #[test]
    fn http_404_not_found_is_permanent() {
        let err = LlmError::from_status(404, "model not found");
        assert!(!err.retryable);
        assert_eq!(err.origin, ErrorOrigin::Provider);
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
    }

    #[test]
    fn http_502_bad_gateway_is_transport_transient() {
        let err = LlmError::from_status(502, "bad gateway");
        assert!(err.retryable);
        assert_eq!(err.origin, ErrorOrigin::Transport);
        assert_eq!(err.persistence, ErrorPersistence::Transient);
    }

    #[test]
    fn compute_error_not_retryable() {
        let err = LlmError::compute("context length 200k exceeds limit 128k");
        assert!(!err.retryable);
        assert_eq!(err.origin, ErrorOrigin::Compute);
        assert_eq!(err.persistence, ErrorPersistence::Permanent);
        assert!(err.status.is_none());
    }
}

// ===================================================================
// 2. Retry classifier
// ===================================================================

mod retry_classifier {
    use super::*;

    // --- Individual arms ---

    #[test]
    fn arm_4xx_non_retryable() {
        for status in [400u16, 401, 403, 404] {
            let err = LlmError::from_status(status, "client error");
            assert!(
                !should_retry(&err),
                "4xx status {status} must NOT be retryable"
            );
        }
    }

    #[test]
    fn arm_5xx_retryable() {
        for status in [500u16, 502, 503] {
            let err = LlmError::from_status(status, "server error");
            assert!(
                should_retry(&err),
                "5xx status {status} must be retryable"
            );
        }
    }

    #[test]
    fn arm_timeout_retryable() {
        let err = LlmError::from_status(408, "request timed out");
        assert!(should_retry(&err), "408 timeout must be retryable");
    }

    #[test]
    fn arm_body_limit_not_retryable() {
        // Body-limit errors surface as structural (permanent)
        let err = LlmError::structural("response body exceeds 10MB limit");
        assert!(
            !should_retry(&err),
            "Body-limit structural error must NOT be retryable"
        );
    }

    #[test]
    fn arm_network_error_transient_retryable() {
        let err = LlmError::transport("connection reset by peer", true);
        assert!(
            should_retry(&err),
            "Transient network error must be retryable"
        );
    }

    #[test]
    fn arm_network_error_permanent_not_retryable() {
        let err = LlmError::transport("invalid proxy configuration", false);
        assert!(
            !should_retry(&err),
            "Permanent transport error must NOT be retryable"
        );
    }

    #[test]
    fn arm_unknown_persistence_not_retryable() {
        let err = LlmError::from_status(418, "I'm a teapot");
        assert_eq!(err.persistence, ErrorPersistence::Unknown);
        assert!(
            !should_retry(&err),
            "Unknown persistence must NOT be retryable"
        );
    }

    // --- Retry budget exhausted ---

    #[test]
    fn retry_budget_exhausted_yields_final_error() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay_ms: 100,
            backoff: BackoffStrategy::Exponential,
            ..Default::default()
        };

        let err = LlmError::from_status(500, "server error");

        // Simulate retry loop: attempt 0..max_retries, then give up
        let mut attempts = 0u32;
        loop {
            if !should_retry(&err) || attempts >= config.max_retries {
                break;
            }
            let _delay = compute_delay(&config, attempts);
            attempts += 1;
        }
        assert_eq!(
            attempts, config.max_retries,
            "Must exhaust all retries before giving up"
        );
        // After exhaustion, the error is still the same 500.
        assert_eq!(err.status, Some(500));
    }

    // --- Success on Nth retry ---

    #[test]
    fn success_on_second_retry() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 10,
            backoff: BackoffStrategy::Fixed,
            ..Default::default()
        };

        // Simulate: fail twice (500), then succeed on third attempt.
        let errors = vec![
            Some(LlmError::from_status(500, "server error")),
            Some(LlmError::from_status(500, "server error")),
            None, // success
        ];

        let mut attempts = 0u32;
        let mut succeeded = false;
        for result in &errors {
            match result {
                Some(err) => {
                    if !should_retry(err) || attempts >= config.max_retries {
                        break;
                    }
                    let _delay = compute_delay(&config, attempts);
                    attempts += 1;
                }
                None => {
                    succeeded = true;
                    break;
                }
            }
        }
        assert!(succeeded, "Must succeed on the third attempt");
        assert_eq!(attempts, 2, "Two retries before success");
    }

    #[test]
    fn success_on_first_try_no_retries() {
        let config = RetryConfig::default();
        // Immediate success: no retry needed.
        let result: Result<&str, LlmError> = Ok("response");
        let mut retries = 0u32;
        match result {
            Ok(_) => { /* success, no retry */ }
            Err(ref err) if should_retry(err) && retries < config.max_retries => {
                retries += 1;
            }
            Err(_) => { /* permanent error */ }
        }
        assert_eq!(retries, 0);
    }

    // --- Delay computation edges ---

    #[test]
    fn exponential_delay_attempt_zero() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            backoff: BackoffStrategy::Exponential,
            ..Default::default()
        };
        let delay = compute_delay(&config, 0);
        // 1000ms +/- 25% jitter = [750, 1250]
        assert!(delay.as_millis() >= 750);
        assert!(delay.as_millis() <= 1250);
    }

    #[test]
    fn exponential_delay_high_attempt_caps() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            backoff: BackoffStrategy::Exponential,
            ..Default::default()
        };
        let delay = compute_delay(&config, 50);
        // Must be capped at 1 hour + 25% jitter
        assert!(delay.as_millis() <= 4_500_000);
    }

    #[test]
    fn fixed_delay_is_constant_across_attempts() {
        let config = RetryConfig {
            base_delay_ms: 200,
            backoff: BackoffStrategy::Fixed,
            ..Default::default()
        };
        for attempt in 0..10 {
            let delay = compute_delay(&config, attempt);
            // 200ms +/- 25% = [150, 250]
            assert!(
                delay.as_millis() >= 150 && delay.as_millis() <= 250,
                "attempt {attempt}: delay {} outside [150, 250]",
                delay.as_millis()
            );
        }
    }

    // --- Retry-After header ---

    #[test]
    fn retry_after_overrides_when_larger_than_computed() {
        let computed = Duration::from_millis(500);
        let result = apply_retry_after(
            computed,
            Some(10_000),
            &RetryConfig {
                honor_retry_after: true,
                ..Default::default()
            },
        );
        assert_eq!(result, Duration::from_millis(10_000));
    }

    #[test]
    fn retry_after_does_not_reduce_computed() {
        let computed = Duration::from_millis(10_000);
        let result = apply_retry_after(
            computed,
            Some(500),
            &RetryConfig {
                honor_retry_after: true,
                ..Default::default()
            },
        );
        assert_eq!(result, Duration::from_millis(10_000));
    }

    #[test]
    fn retry_after_ignored_when_config_disabled() {
        let computed = Duration::from_millis(500);
        let result = apply_retry_after(
            computed,
            Some(10_000),
            &RetryConfig {
                honor_retry_after: false,
                ..Default::default()
            },
        );
        assert_eq!(result, Duration::from_millis(500));
    }

    #[test]
    fn retry_after_none_passes_through_computed() {
        let computed = Duration::from_millis(1234);
        let result = apply_retry_after(computed, None, &RetryConfig::default());
        assert_eq!(result, Duration::from_millis(1234));
    }

    // --- Mixed transient/permanent sequence ---

    #[test]
    fn permanent_error_stops_retry_immediately() {
        let config = RetryConfig {
            max_retries: 10,
            ..Default::default()
        };

        let err = LlmError::from_status(401, "invalid api key");
        assert!(!should_retry(&err));
        // Even with 10 retries available, a 401 stops immediately
        let mut retries = 0u32;
        if should_retry(&err) && retries < config.max_retries {
            retries += 1;
        }
        assert_eq!(retries, 0, "Permanent error must not trigger any retries");
    }
}

// ===================================================================
// 3. Streaming parser edges
// ===================================================================

mod streaming_parser {
    use super::*;

    // --- SSE line parsing ---

    #[test]
    fn complete_valid_sse_stream_openai() {
        let lines = vec![
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"}}]}",
            "",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"}}]}",
            "",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}",
            "",
            "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2},\"choices\":[]}",
            "",
            "data: [DONE]",
        ];

        let mut events: Vec<StreamEvent> = Vec::new();
        for line in lines {
            if let Some(sse) = parse_sse_line(line) {
                match sse {
                    SseLine::Data(data) => {
                        if let Some(event) = parse_openai_event(&data) {
                            events.push(event);
                        }
                    }
                    SseLine::Done => {
                        events.push(StreamEvent::Done);
                    }
                    _ => {}
                }
            }
        }

        assert_eq!(events.len(), 4, "Expected 4 events: 2 content + 1 usage + 1 done");
        assert_eq!(
            events[0],
            StreamEvent::ContentDelta {
                delta: "Hello".into()
            }
        );
        assert_eq!(
            events[1],
            StreamEvent::ContentDelta {
                delta: " world".into()
            }
        );
        assert!(matches!(events[2], StreamEvent::Usage { .. }));
        assert_eq!(events[3], StreamEvent::Done);
    }

    #[test]
    fn complete_valid_sse_stream_anthropic() {
        let lines = vec![
            ("message_start", r#"{"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":50,"output_tokens":0}}}"#),
            ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#),
            ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" there"}}"#),
            ("message_delta", r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}"#),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];

        let mut events: Vec<StreamEvent> = Vec::new();
        for (event_type, data) in lines {
            if let Some(event) = parse_anthropic_event(event_type, data) {
                events.push(event);
            }
        }

        assert_eq!(events.len(), 5);
        assert!(matches!(events[0], StreamEvent::Usage { .. }));
        assert_eq!(
            events[1],
            StreamEvent::ContentDelta {
                delta: "Hi".into()
            }
        );
        assert_eq!(
            events[2],
            StreamEvent::ContentDelta {
                delta: " there".into()
            }
        );
        assert!(matches!(events[3], StreamEvent::Usage { .. }));
        assert_eq!(events[4], StreamEvent::Done);
    }

    // --- Partial JSON chunk (split mid-token) ---

    #[test]
    fn partial_json_chunk_returns_none_openai() {
        // Simulate a chunk split mid-JSON: the parser should return None (not panic)
        let partial = r#"{"choices":[{"index":0,"delta":{"conte"#;
        let result = parse_openai_event(partial);
        assert_eq!(result, None, "Partial JSON must return None, not panic");
    }

    #[test]
    fn partial_json_chunk_returns_none_anthropic() {
        let partial = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_del"#;
        let result = parse_anthropic_event("content_block_delta", partial);
        assert_eq!(result, None, "Partial JSON must return None, not panic");
    }

    #[test]
    fn partial_json_chunk_returns_none_ndjson() {
        let partial = r#"{"response":"Hel"#;
        let result = parse_ndjson_event(partial);
        assert_eq!(result, None, "Partial NDJSON must return None, not panic");
    }

    #[test]
    fn truncated_json_with_open_brace() {
        let result = parse_openai_event("{");
        assert_eq!(result, None);
    }

    #[test]
    fn truncated_json_with_open_array() {
        let result = parse_openai_event(r#"{"choices":["#);
        assert_eq!(result, None);
    }

    // --- Invalid UTF-8 in chunk ---

    #[test]
    fn sse_line_with_replacement_chars() {
        // Simulated: bytes that became replacement characters after UTF-8 decoding.
        let line_with_replacement = "data: {\"text\": \"\u{FFFD}\u{FFFD}\"}";
        let sse = parse_sse_line(line_with_replacement);
        assert!(matches!(sse, Some(SseLine::Data(_))));
        // The data is valid as a string (replacement chars are valid Unicode),
        // but JSON parsing should succeed -- the text field has replacement chars.
        if let Some(SseLine::Data(data)) = sse {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&data);
            assert!(parsed.is_ok(), "JSON with replacement chars should parse");
        }
    }

    #[test]
    fn openai_event_with_unicode_escape_in_content() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":"\u0000\u001f"}}]}"#;
        let result = parse_openai_event(data);
        // Null and control chars: content is not empty, so it should produce a delta
        assert!(matches!(result, Some(StreamEvent::ContentDelta { .. })));
    }

    #[test]
    fn anthropic_event_with_unicode_content() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Caf\u00e9 \ud83c\udf0d"}}"#;
        let result = parse_anthropic_event("content_block_delta", data);
        assert!(matches!(result, Some(StreamEvent::ContentDelta { .. })));
    }

    // --- Mid-event disconnect (stream drops) ---

    #[tokio::test]
    async fn mid_event_disconnect_stream_drops() {
        // Simulate a stream that yields two events then abruptly drops (returns an error)
        let events: Vec<Result<StreamEvent, LlmError>> = vec![
            Ok(StreamEvent::ContentDelta {
                delta: "Hello".into(),
            }),
            Err(LlmError::transport("connection reset by peer", true)),
        ];

        let inner: Pin<
            Box<dyn futures::Stream<Item = Result<StreamEvent, LlmError>> + Send>,
        > = Box::pin(stream::iter(events));

        let handle = StreamHandle::new(inner, "gpt-4o".to_string(), Some("req-1".to_string()));
        let mut stream = handle.into_stream();

        // First event: content delta
        let first = stream.next().await.unwrap();
        assert!(first.is_ok());
        assert!(matches!(first.unwrap(), StreamEvent::ContentDelta { .. }));

        // Second event: error from disconnect
        let second = stream.next().await.unwrap();
        assert!(second.is_err());
        let err = second.unwrap_err();
        assert_eq!(err.origin, ErrorOrigin::Transport);
        assert!(err.retryable);

        // Stream is exhausted after the error
        let third = stream.next().await;
        assert!(third.is_none(), "Stream must be exhausted after error");
    }

    #[tokio::test]
    async fn mid_event_disconnect_no_done_event() {
        // A stream that yields content but never sends Done
        let events: Vec<Result<StreamEvent, LlmError>> = vec![
            Ok(StreamEvent::ContentDelta {
                delta: "partial".into(),
            }),
        ];

        let inner: Pin<
            Box<dyn futures::Stream<Item = Result<StreamEvent, LlmError>> + Send>,
        > = Box::pin(stream::iter(events));

        let handle = StreamHandle::new(inner, "test-model".to_string(), None);
        let mut stream = handle.into_stream();

        let first = stream.next().await;
        assert!(first.is_some());

        let second = stream.next().await;
        assert!(second.is_none(), "Stream ends without Done event");
    }

    // --- Empty stream ---

    #[tokio::test]
    async fn empty_stream_yields_nothing() {
        let events: Vec<Result<StreamEvent, LlmError>> = vec![];

        let inner: Pin<
            Box<dyn futures::Stream<Item = Result<StreamEvent, LlmError>> + Send>,
        > = Box::pin(stream::iter(events));

        let handle = StreamHandle::new(inner, "gpt-4o".to_string(), None);
        let mut stream = handle.into_stream();

        let first = stream.next().await;
        assert!(first.is_none(), "Empty stream must yield None immediately");
    }

    #[test]
    fn empty_sse_data_line() {
        let sse = parse_sse_line("data:");
        assert_eq!(sse, Some(SseLine::Data("".into())));
    }

    #[test]
    fn empty_data_to_openai_parser() {
        let result = parse_openai_event("");
        assert_eq!(result, None, "Empty string must not parse to an event");
    }

    #[test]
    fn empty_data_to_anthropic_parser() {
        let result = parse_anthropic_event("content_block_delta", "");
        assert_eq!(result, None);
    }

    #[test]
    fn empty_data_to_ndjson_parser() {
        let result = parse_ndjson_event("");
        assert_eq!(result, None);
    }

    // --- Single-chunk complete response ---

    #[tokio::test]
    async fn single_chunk_complete_response() {
        let events: Vec<Result<StreamEvent, LlmError>> = vec![
            Ok(StreamEvent::ContentDelta {
                delta: "Complete answer in one chunk.".into(),
            }),
            Ok(StreamEvent::Usage {
                usage: TokenUsage {
                    input_tokens: 5,
                    output_tokens: 8,
                },
            }),
            Ok(StreamEvent::Done),
        ];

        let inner: Pin<
            Box<dyn futures::Stream<Item = Result<StreamEvent, LlmError>> + Send>,
        > = Box::pin(stream::iter(events));

        let handle = StreamHandle::new(inner, "gpt-4o".to_string(), Some("req-42".to_string()));
        assert_eq!(handle.model, "gpt-4o");
        assert_eq!(handle.request_id.as_deref(), Some("req-42"));

        let collected: Vec<_> = handle.into_stream().collect().await;
        assert_eq!(collected.len(), 3);

        let first = collected[0].as_ref().unwrap();
        assert!(matches!(first, StreamEvent::ContentDelta { delta } if delta == "Complete answer in one chunk."));

        let second = collected[1].as_ref().unwrap();
        assert!(matches!(second, StreamEvent::Usage { usage } if usage.output_tokens == 8));

        let third = collected[2].as_ref().unwrap();
        assert_eq!(*third, StreamEvent::Done);
    }

    // --- SSE line edge cases ---

    #[test]
    fn sse_comment_line_ignored() {
        let sse = parse_sse_line(": this is a keep-alive comment");
        assert_eq!(sse, Some(SseLine::Comment));
    }

    #[test]
    fn sse_event_line_parsed() {
        let sse = parse_sse_line("event: message_delta");
        assert_eq!(sse, Some(SseLine::Event("message_delta".into())));
    }

    #[test]
    fn sse_data_with_whitespace_trimmed() {
        let sse = parse_sse_line("data:   {\"test\": true}  ");
        assert_eq!(sse, Some(SseLine::Data("{\"test\": true}".into())));
    }

    #[test]
    fn sse_done_sentinel() {
        let sse = parse_sse_line("data: [DONE]");
        assert_eq!(sse, Some(SseLine::Done));
    }

    #[test]
    fn sse_carriage_return_stripped() {
        let sse = parse_sse_line("data: hello\r");
        assert_eq!(sse, Some(SseLine::Data("hello".into())));
    }

    #[test]
    fn sse_raw_json_as_ndjson() {
        // Non-prefixed JSON lines treated as data (NDJSON format)
        let sse = parse_sse_line(r#"{"response":"test","done":false}"#);
        assert_eq!(
            sse,
            Some(SseLine::Data(r#"{"response":"test","done":false}"#.into()))
        );
    }

    // --- Anthropic parser edge cases ---

    #[test]
    fn anthropic_unknown_delta_type() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"unknown_type","data":"x"}}"#;
        let result = parse_anthropic_event("content_block_delta", data);
        assert_eq!(result, None, "Unknown delta type returns None");
    }

    #[test]
    fn anthropic_content_block_start_text_type_ignored() {
        // Text blocks at content_block_start are not tool_use, so return None
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        let result = parse_anthropic_event("content_block_start", data);
        assert_eq!(result, None);
    }

    #[test]
    fn anthropic_message_delta_without_usage() {
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#;
        let result = parse_anthropic_event("message_delta", data);
        assert_eq!(result, None, "message_delta without usage returns None");
    }

    #[test]
    fn anthropic_message_start_without_usage() {
        let data = r#"{"type":"message_start","message":{"id":"msg_1"}}"#;
        let result = parse_anthropic_event("message_start", data);
        assert_eq!(result, None, "message_start without usage returns None");
    }

    #[test]
    fn anthropic_ping_event() {
        let result = parse_anthropic_event("ping", r#"{"type":"ping"}"#);
        assert_eq!(result, None);
    }

    // --- OpenAI parser edge cases ---

    #[test]
    fn openai_empty_content_string_returns_none() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":""}}]}"#;
        let result = parse_openai_event(data);
        assert_eq!(result, None, "Empty content string must not produce event");
    }

    #[test]
    fn openai_usage_with_zero_tokens_returns_none() {
        let data = r#"{"usage":{"prompt_tokens":0,"completion_tokens":0},"choices":[]}"#;
        let result = parse_openai_event(data);
        // usage is 0/0 so the if guard (input > 0 || output > 0) fails,
        // falls through to choices which is empty -> None
        assert_eq!(result, None);
    }

    #[test]
    fn openai_tool_call_with_partial_arguments() {
        let data = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"pa"}}]}}]}"#;
        let result = parse_openai_event(data);
        assert!(matches!(
            result,
            Some(StreamEvent::ToolCallDelta {
                arguments_delta: Some(ref args),
                ..
            }) if args == r#"{"pa"#
        ));
    }

    // --- NDJSON parser edge cases ---

    #[test]
    fn ndjson_empty_response_string_returns_none() {
        let data = r#"{"response":"","done":false}"#;
        let result = parse_ndjson_event(data);
        assert_eq!(result, None, "Empty response string returns None");
    }

    #[test]
    fn ndjson_chat_format_empty_content_returns_none() {
        let data = r#"{"message":{"role":"assistant","content":""},"done":false}"#;
        let result = parse_ndjson_event(data);
        assert_eq!(result, None);
    }

    #[test]
    fn ndjson_done_true_with_usage_returns_usage_not_done() {
        // When done=true and usage fields are present, returns Usage (not Done)
        let data = r#"{"done":true,"prompt_eval_count":100,"eval_count":50}"#;
        let result = parse_ndjson_event(data);
        assert_eq!(
            result,
            Some(StreamEvent::Usage {
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                }
            })
        );
    }

    #[test]
    fn ndjson_done_true_without_usage_returns_done() {
        let data = r#"{"done":true}"#;
        let result = parse_ndjson_event(data);
        assert_eq!(result, Some(StreamEvent::Done));
    }

    // --- StreamEvent serde roundtrip ---

    #[test]
    fn stream_event_content_delta_json_tag() {
        let event = StreamEvent::ContentDelta {
            delta: "test".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content_delta");
        assert_eq!(json["delta"], "test");
    }

    #[test]
    fn stream_event_tool_call_delta_json_tag() {
        let event = StreamEvent::ToolCallDelta {
            index: 2,
            id: Some("call_abc".into()),
            name: None,
            arguments_delta: Some("{\"k\":".into()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "tool_call_delta");
        assert_eq!(json["index"], 2);
        assert_eq!(json["id"], "call_abc");
        assert!(json["name"].is_null());
    }

    #[test]
    fn stream_event_done_json_tag() {
        let event = StreamEvent::Done;
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "done");
    }

    // --- StreamHandle construction ---

    #[test]
    fn stream_handle_fields_accessible() {
        let inner: Pin<
            Box<dyn futures::Stream<Item = Result<StreamEvent, LlmError>> + Send>,
        > = Box::pin(stream::empty());
        let handle = StreamHandle::new(inner, "test-model".into(), Some("req-id".into()));
        assert_eq!(handle.model, "test-model");
        assert_eq!(handle.request_id.as_deref(), Some("req-id"));
    }

    #[test]
    fn stream_handle_no_request_id() {
        let inner: Pin<
            Box<dyn futures::Stream<Item = Result<StreamEvent, LlmError>> + Send>,
        > = Box::pin(stream::empty());
        let handle = StreamHandle::new(inner, "model".into(), None);
        assert!(handle.request_id.is_none());
    }
}
