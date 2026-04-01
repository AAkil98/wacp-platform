//! Cross-module integration tests for the security crate.

use wacp_security::*;

// ── ContentFilter + SecretStore integration ─────────────────────────────────

#[test]
fn content_filter_catches_secret_from_store() {
    let mut store = SecretStore::new();
    store.set("api_key", "sk-testkey1234567890abcdef");

    let filter = ContentFilter::with_defaults();
    let input = "Use this key: sk-testkey1234567890abcdef to authenticate";

    // Filter catches the API key pattern
    let result = filter.filter(input);
    assert!(!result.redactions.is_empty());
    assert!(result.output.contains("[REDACTED_API_KEY]"));

    // SecretStore also detects the leak
    let leaks = store.scan_for_leaks(input);
    assert!(leaks.contains(&"api_key".to_string()));

    // After filtering, the leak is gone
    let leaks_after = store.scan_for_leaks(&result.output);
    assert!(leaks_after.is_empty());
}

// ── AuditEvent + sha256 for tool invocation ─────────────────────────────────

#[test]
fn audit_event_tool_invocation_with_hashes() {
    let input = b"read /src/main.rs";
    let output = b"fn main() { println!(\"hello\"); }";

    let event = AuditEvent::ToolInvocation {
        tool_name: "read_file".into(),
        capability: "read".into(),
        input_hash: sha256_hex(input),
        output_hash: Some(sha256_hex(output)),
        duration_ms: 42,
        success: true,
        error_code: None,
    };

    let payload = event.to_trail_payload();
    let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();

    assert_eq!(parsed["audit_type"], "tool_invocation");
    assert_eq!(parsed["input_hash"].as_str().unwrap().len(), 64);
    assert_eq!(parsed["output_hash"].as_str().unwrap().len(), 64);
    assert_eq!(parsed["duration_ms"], 42);

    // Verify determinism — same input → same hash
    assert_eq!(
        parsed["input_hash"],
        sha256_hex(input),
    );
}

// ── ContentFilter with multiple secrets in one message ──────────────────────

#[test]
fn filter_redacts_multiple_secrets_in_conversation() {
    let filter = ContentFilter::with_defaults();

    // Simulate an LLM conversation with leaked secrets
    let messages = vec![
        "My API key is sk-anthropic1234567890abcde and email is user@secret.com",
        "Also my AWS key AKIAIOSFODNN7EXAMPLE is here",
        "SSN: 123-45-6789 and card 4111-1111-1111-1111",
    ];

    let mut total_redactions = 0;
    for msg in &messages {
        let result = filter.filter(msg);
        total_redactions += result.redactions.len();
        // Verify no raw secrets remain
        assert!(!result.output.contains("sk-anthropic"));
        assert!(!result.output.contains("@secret.com"));
        assert!(!result.output.contains("AKIA"));
        assert!(!result.output.contains("123-45-6789"));
    }
    assert!(total_redactions >= 5); // at least one per secret
}

// ── SecretStore + ContentFilter full pipeline ───────────────────────────────

#[test]
fn full_security_pipeline_filter_then_verify() {
    let mut store = SecretStore::new();
    store.set("anthropic_key", "sk-proj1234567890abcdefghijklmno");
    store.set("db_password", "p@ssw0rd!secret");

    let filter = ContentFilter::with_defaults();

    let input = "Connect with sk-proj1234567890abcdefghijklmno to api.anthropic.com";

    // Step 1: Filter catches the key
    let filtered = filter.filter(input);
    assert!(!filtered.redactions.is_empty());

    // Step 2: Verify filtered output has no leaks
    let leaks = store.scan_for_leaks(&filtered.output);
    assert!(leaks.is_empty(), "filtered output should contain no secrets");

    // Step 3: Create audit event for the filtering
    let audit = AuditEvent::ContentFiltered {
        rule_name: filtered.redactions[0].rule_name.clone(),
        action: "redacted".into(),
        context: "llm_request".into(),
    };
    let payload = audit.to_trail_payload();
    assert!(!payload.is_empty());

    // Step 4: Audit payload itself has no secrets
    let payload_str = String::from_utf8(payload).unwrap();
    let payload_leaks = store.scan_for_leaks(&payload_str);
    assert!(payload_leaks.is_empty(), "audit payload should contain no secrets");
}
