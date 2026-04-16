//! Comprehensive branch-coverage tests for the PII / content filter.
//!
//! Covers every redaction rule, every FilterAction path, every policy toggle,
//! and edge cases (empty input, boundary positions, overlapping patterns,
//! unicode, already-redacted markers, large input, disabled rules, etc.).

use regex::Regex;
use wacp_security::{ContentFilter, FilterAction, FilterPolicy, FilterRule, Redaction};

// ═══════════════════════════════════════════════════════════════════════════
// 1. Empty / trivial inputs — no panics, no false positives
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn empty_string_no_redaction_no_panic() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("");
    assert_eq!(r.output, "");
    assert!(r.redactions.is_empty());
    assert!(!r.blocked);
}

#[test]
fn whitespace_only_input_unchanged() {
    let f = ContentFilter::with_defaults();
    for ws in &["   ", "\t\t", "\n\n\n", " \t \n "] {
        let r = f.filter(ws);
        assert_eq!(r.output, *ws);
        assert!(r.redactions.is_empty());
    }
}

#[test]
fn single_character_inputs_unchanged() {
    let f = ContentFilter::with_defaults();
    for ch in &["a", "1", "@", "-", "\0", "\n"] {
        let r = f.filter(ch);
        assert_eq!(r.output, *ch);
        assert!(r.redactions.is_empty());
    }
}

#[test]
fn plain_text_no_pii_unchanged() {
    let f = ContentFilter::with_defaults();
    let input = "The quick brown fox jumps over the lazy dog.";
    let r = f.filter(input);
    assert_eq!(r.output, input);
    assert!(r.redactions.is_empty());
    assert!(!r.blocked);
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Each default rule exercised individually
// ═══════════════════════════════════════════════════════════════════════════

// ── 2a. api_key rule ─────────────────────────────────────────────────────

#[test]
fn api_key_sk_prefix_exactly_20_chars() {
    let f = ContentFilter::with_defaults();
    // Exactly 20 alphanumeric chars after "sk-"
    let r = f.filter("sk-aaaabbbbccccddddeeee");
    assert!(r.output.contains("[REDACTED_API_KEY]"));
    assert_eq!(r.redactions.len(), 1);
    assert_eq!(r.redactions[0].rule_name, "api_key");
}

#[test]
fn api_key_sk_prefix_longer_than_20_chars() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("sk-aaaabbbbccccddddeeeeFFFFGGGG");
    assert!(r.output.contains("[REDACTED_API_KEY]"));
    assert!(!r.output.contains("sk-"));
}

#[test]
fn api_key_sk_prefix_too_short_no_match() {
    let f = ContentFilter::with_defaults();
    // Only 19 alphanumeric chars after "sk-" -- must NOT match
    let input = "sk-aaaabbbbccccddddeee";
    let r = f.filter(input);
    assert_eq!(r.output, input, "19-char key body should not trigger api_key rule");
    assert!(r.redactions.is_empty());
}

#[test]
fn api_key_key_prefix_matches() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("key-aaaabbbbccccddddeeee");
    assert!(r.output.contains("[REDACTED_API_KEY]"));
    assert_eq!(r.redactions[0].rule_name, "api_key");
}

#[test]
fn api_key_key_prefix_too_short_no_match() {
    let f = ContentFilter::with_defaults();
    let input = "key-short";
    let r = f.filter(input);
    assert_eq!(r.output, input);
}

// ── 2b. bearer_token rule ────────────────────────────────────────────────

#[test]
fn bearer_token_simple_jwt() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.test");
    assert!(r.output.contains("[REDACTED_BEARER]"));
    assert!(!r.output.contains("eyJ"));
    assert_eq!(r.redactions.len(), 1);
    assert_eq!(r.redactions[0].rule_name, "bearer_token");
}

#[test]
fn bearer_token_with_padding_equals() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Bearer dGVzdA==");
    assert!(r.output.contains("[REDACTED_BEARER]"));
}

#[test]
fn bearer_token_with_special_chars() {
    let f = ContentFilter::with_defaults();
    // Token chars: A-Z a-z 0-9 - . _ ~ + / =
    let r = f.filter("Bearer abc-._~+/xyz123===");
    assert!(r.output.contains("[REDACTED_BEARER]"));
}

#[test]
fn bearer_without_space_no_match() {
    let f = ContentFilter::with_defaults();
    let input = "Bearertoken123";
    let r = f.filter(input);
    // "Bearer" must be followed by whitespace
    assert_eq!(r.output, input);
}

// ── 2c. aws_key rule ─────────────────────────────────────────────────────

#[test]
fn aws_key_exact_format() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("AKIAIOSFODNN7EXAMPLE");
    assert!(r.output.contains("[REDACTED_AWS_KEY]"));
    assert_eq!(r.redactions[0].rule_name, "aws_key");
}

#[test]
fn aws_key_lowercase_no_match() {
    let f = ContentFilter::with_defaults();
    let input = "akiaiosfodnn7example";
    let r = f.filter(input);
    assert_eq!(r.output, input, "AWS keys are uppercase only");
}

#[test]
fn aws_key_too_short_no_match() {
    let f = ContentFilter::with_defaults();
    // AKIA + only 15 chars (need 16)
    let input = "AKIAIOSFODNN7EXA";
    let r = f.filter(input);
    assert_eq!(r.output, input);
}

// ── 2d. private_key rule ─────────────────────────────────────────────────

#[test]
fn private_key_rsa_header() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("-----BEGIN RSA PRIVATE KEY-----");
    assert!(r.output.contains("[REDACTED_PRIVATE_KEY]"));
    assert_eq!(r.redactions[0].rule_name, "private_key");
}

#[test]
fn private_key_generic_header() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("-----BEGIN PRIVATE KEY-----");
    assert!(r.output.contains("[REDACTED_PRIVATE_KEY]"));
}

#[test]
fn private_key_with_body() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----");
    assert!(r.output.contains("[REDACTED_PRIVATE_KEY]"));
    // The rule only matches the BEGIN line, body remains
    assert!(r.output.contains("MIIEpAIBAAKCAQEA"));
}

#[test]
fn public_key_header_no_match() {
    let f = ContentFilter::with_defaults();
    let input = "-----BEGIN PUBLIC KEY-----";
    let r = f.filter(input);
    assert_eq!(r.output, input, "PUBLIC KEY should not be redacted");
}

// ── 2e. email rule ───────────────────────────────────────────────────────

#[test]
fn email_simple() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("user@example.com");
    assert_eq!(r.output, "[REDACTED_EMAIL]");
    assert_eq!(r.redactions[0].rule_name, "email");
}

#[test]
fn email_with_dots_and_plus() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("first.last+tag@sub.domain.co.uk");
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(!r.output.contains("@"));
}

#[test]
fn email_with_percent_encoding() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("user%name@example.com");
    assert!(r.output.contains("[REDACTED_EMAIL]"));
}

#[test]
fn email_at_start_of_input() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("admin@server.org is the address");
    assert!(r.output.starts_with("[REDACTED_EMAIL]"));
}

#[test]
fn email_at_end_of_input() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Contact me at admin@server.org");
    assert!(r.output.ends_with("[REDACTED_EMAIL]"));
}

#[test]
fn email_alone_in_input() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("test@test.io");
    assert_eq!(r.output, "[REDACTED_EMAIL]");
}

#[test]
fn email_not_at_sign_alone() {
    let f = ContentFilter::with_defaults();
    let input = "use @ for decorators";
    let r = f.filter(input);
    assert_eq!(r.output, input, "bare @ is not an email");
}

// ── 2f. ssn rule ─────────────────────────────────────────────────────────

#[test]
fn ssn_standard_format() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("SSN: 123-45-6789");
    assert!(r.output.contains("[REDACTED_SSN]"));
    assert!(!r.output.contains("123-45-6789"));
    assert_eq!(r.redactions[0].rule_name, "ssn");
}

#[test]
fn ssn_no_dashes_no_match() {
    let f = ContentFilter::with_defaults();
    let input = "SSN: 123456789";
    let r = f.filter(input);
    assert_eq!(r.output, input, "SSN without dashes should not match");
}

#[test]
fn ssn_wrong_grouping_no_match() {
    let f = ContentFilter::with_defaults();
    let input = "12-345-6789";
    let r = f.filter(input);
    assert_eq!(r.output, input, "Wrong SSN digit grouping should not match");
}

#[test]
fn ssn_embedded_in_larger_number_no_match() {
    let f = ContentFilter::with_defaults();
    // \b boundaries prevent matching inside longer numeric strings
    let input = "9999123-45-67899999";
    let r = f.filter(input);
    assert_eq!(r.output, input, "SSN-like pattern inside larger string should not match due to word boundaries");
}

// ── 2g. credit_card rule ─────────────────────────────────────────────────

#[test]
fn credit_card_with_spaces() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Card: 4111 1111 1111 1111");
    assert!(r.output.contains("[REDACTED_CC]"));
    assert_eq!(r.redactions[0].rule_name, "credit_card");
}

#[test]
fn credit_card_with_dashes() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Card: 4111-1111-1111-1111");
    assert!(r.output.contains("[REDACTED_CC]"));
}

#[test]
fn credit_card_no_separators() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Card: 4111111111111111");
    assert!(r.output.contains("[REDACTED_CC]"));
}

#[test]
fn credit_card_mixed_separators() {
    let f = ContentFilter::with_defaults();
    // The regex allows optional space-or-dash between each group
    let r = f.filter("4111-1111 1111-1111");
    assert!(r.output.contains("[REDACTED_CC]"));
}

#[test]
fn credit_card_too_few_digits_no_match() {
    let f = ContentFilter::with_defaults();
    let input = "4111 1111 1111";
    let r = f.filter(input);
    // Only 12 digits -- should not match the 16-digit pattern
    assert!(!r.output.contains("[REDACTED_CC]"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Multiple PII types in one input
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn multiple_pii_types_all_redacted() {
    let f = ContentFilter::with_defaults();
    let input = "Email: user@example.com, SSN: 123-45-6789, Key: sk-aaaabbbbccccddddeeee";
    let r = f.filter(input);
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(r.output.contains("[REDACTED_SSN]"));
    assert!(r.output.contains("[REDACTED_API_KEY]"));
    assert!(r.redactions.len() >= 3);
    // Verify no raw PII remains
    assert!(!r.output.contains("user@example.com"));
    assert!(!r.output.contains("123-45-6789"));
    assert!(!r.output.contains("sk-aaaa"));
}

#[test]
fn all_seven_rules_in_one_input() {
    let f = ContentFilter::with_defaults();
    let input = concat!(
        "sk-aaaabbbbccccddddeeeeFFFF ",
        "Bearer eyJhbGciOiJIUzI1NiJ9.test ",
        "AKIAIOSFODNN7EXAMPLE ",
        "-----BEGIN PRIVATE KEY----- ",
        "user@example.com ",
        "123-45-6789 ",
        "4111 1111 1111 1111",
    );
    let r = f.filter(input);
    assert!(r.output.contains("[REDACTED_API_KEY]"));
    assert!(r.output.contains("[REDACTED_BEARER]"));
    assert!(r.output.contains("[REDACTED_AWS_KEY]"));
    assert!(r.output.contains("[REDACTED_PRIVATE_KEY]"));
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(r.output.contains("[REDACTED_SSN]"));
    assert!(r.output.contains("[REDACTED_CC]"));
    assert!(r.redactions.len() >= 7);
}

#[test]
fn duplicate_emails_both_redacted() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("a@b.com and a@b.com");
    assert_eq!(r.output, "[REDACTED_EMAIL] and [REDACTED_EMAIL]");
    assert!(r.redactions.len() >= 2);
}

#[test]
fn two_different_emails_in_one_input() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("from alice@example.com to bob@example.com");
    assert!(!r.output.contains("alice@"));
    assert!(!r.output.contains("bob@"));
    // Both replaced
    let count = r.output.matches("[REDACTED_EMAIL]").count();
    assert_eq!(count, 2);
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Overlapping / ambiguous patterns
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn api_key_that_contains_email_like_substring() {
    // The api_key rule uses alternation (sk-|key-) + alphanum. An email won't
    // appear inside those, but we can have both side by side.
    let f = ContentFilter::with_defaults();
    let r = f.filter("sk-aaaabbbbccccddddeeee@example.com");
    // api_key rule should catch the sk-... portion, email rule catches the @... portion
    assert!(!r.redactions.is_empty());
}

#[test]
fn bearer_token_followed_by_email() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Bearer tokenABC123 user@company.org");
    assert!(r.output.contains("[REDACTED_BEARER]"));
    assert!(r.output.contains("[REDACTED_EMAIL]"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. PII at start / end / middle of input
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pii_at_very_start() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("123-45-6789 is my SSN");
    assert!(r.output.starts_with("[REDACTED_SSN]"));
}

#[test]
fn pii_at_very_end() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("My SSN is 123-45-6789");
    assert!(r.output.ends_with("[REDACTED_SSN]"));
}

#[test]
fn pii_in_middle() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("before 123-45-6789 after");
    assert!(r.output.contains("[REDACTED_SSN]"));
    assert!(r.output.starts_with("before"));
    assert!(r.output.ends_with("after"));
}

#[test]
fn pii_is_entire_input() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("123-45-6789");
    assert_eq!(r.output, "[REDACTED_SSN]");
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Very large input — performance sanity + PII near boundaries
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn large_input_no_pii_completes_fast() {
    let f = ContentFilter::with_defaults();
    let chunk = "The quick brown fox jumps over the lazy dog. ";
    let input: String = chunk.repeat(10_000); // ~450KB
    let r = f.filter(&input);
    assert_eq!(r.output.len(), input.len());
    assert!(r.redactions.is_empty());
}

#[test]
fn large_input_with_pii_at_end() {
    let f = ContentFilter::with_defaults();
    let padding: String = "x".repeat(100_000);
    let input = format!("{} user@example.com", padding);
    let r = f.filter(&input);
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(!r.output.contains("user@example.com"));
    assert_eq!(r.redactions.len(), 1);
}

#[test]
fn large_input_with_pii_at_start() {
    let f = ContentFilter::with_defaults();
    let padding: String = "x".repeat(100_000);
    let input = format!("user@example.com {}", padding);
    let r = f.filter(&input);
    assert!(r.output.starts_with("[REDACTED_EMAIL]"));
    assert_eq!(r.redactions.len(), 1);
}

#[test]
fn large_input_with_pii_scattered() {
    let f = ContentFilter::with_defaults();
    // Use spaces as separators so word-boundary rules (\b in SSN) still match
    let segment = " ".repeat(10_000);
    let input = format!(
        "{}user@a.com{}123-45-6789{}sk-aaaabbbbccccddddeeee{}",
        segment, segment, segment, segment
    );
    let r = f.filter(&input);
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(r.output.contains("[REDACTED_SSN]"));
    assert!(r.output.contains("[REDACTED_API_KEY]"));
    assert!(r.redactions.len() >= 3);
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Unicode content with embedded PII
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn unicode_emoji_surrounding_email() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Hello 🌍 user@example.com 🎉");
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(r.output.contains("🌍"));
    assert!(r.output.contains("🎉"));
}

#[test]
fn unicode_cjk_with_ssn() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("社会保障番号: 123-45-6789 です");
    assert!(r.output.contains("[REDACTED_SSN]"));
    assert!(r.output.contains("社会保障番号"));
    assert!(r.output.contains("です"));
}

#[test]
fn unicode_accented_chars_with_pii() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Résumé de jean@example.fr contient des données");
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(r.output.contains("Résumé"));
    assert!(r.output.contains("contient"));
}

#[test]
fn unicode_rtl_text_with_email() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("مرحبا user@domain.com عالم");
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(r.output.contains("مرحبا"));
}

#[test]
fn zero_width_chars_adjacent_to_pii() {
    let f = ContentFilter::with_defaults();
    // Zero-width space (U+200B) before and after email
    let input = "text\u{200B}user@example.com\u{200B}more";
    let r = f.filter(input);
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(!r.output.contains("user@example.com"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Already-redacted markers — no double-redaction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn already_redacted_email_marker_unchanged() {
    let f = ContentFilter::with_defaults();
    let input = "Contact [REDACTED_EMAIL] for info";
    let r = f.filter(input);
    assert_eq!(r.output, input, "Already-redacted markers should pass through");
    assert!(r.redactions.is_empty());
}

#[test]
fn already_redacted_api_key_marker_unchanged() {
    let f = ContentFilter::with_defaults();
    let input = "Key was [REDACTED_API_KEY]";
    let r = f.filter(input);
    assert_eq!(r.output, input);
    assert!(r.redactions.is_empty());
}

#[test]
fn already_redacted_ssn_marker_unchanged() {
    let f = ContentFilter::with_defaults();
    let input = "SSN: [REDACTED_SSN]";
    let r = f.filter(input);
    assert_eq!(r.output, input);
    assert!(r.redactions.is_empty());
}

#[test]
fn already_redacted_cc_marker_unchanged() {
    let f = ContentFilter::with_defaults();
    let input = "Card: [REDACTED_CC]";
    let r = f.filter(input);
    assert_eq!(r.output, input);
}

#[test]
fn already_redacted_bearer_marker_unchanged() {
    let f = ContentFilter::with_defaults();
    let input = "Auth: [REDACTED_BEARER]";
    let r = f.filter(input);
    assert_eq!(r.output, input);
}

#[test]
fn already_redacted_aws_marker_unchanged() {
    let f = ContentFilter::with_defaults();
    let input = "AWS: [REDACTED_AWS_KEY]";
    let r = f.filter(input);
    assert_eq!(r.output, input);
}

#[test]
fn already_redacted_private_key_marker_unchanged() {
    let f = ContentFilter::with_defaults();
    let input = "Key: [REDACTED_PRIVATE_KEY]";
    let r = f.filter(input);
    assert_eq!(r.output, input);
}

#[test]
fn mix_of_redacted_and_live_pii() {
    let f = ContentFilter::with_defaults();
    let input = "[REDACTED_EMAIL] sent to real@example.com";
    let r = f.filter(input);
    assert!(r.output.contains("[REDACTED_EMAIL] sent to [REDACTED_EMAIL]"));
    // Only the live email should be in redactions
    assert_eq!(r.redactions.len(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Policy / action mode coverage
// ═══════════════════════════════════════════════════════════════════════════

// ── 9a. Disabled policy ──────────────────────────────────────────────────

#[test]
fn disabled_policy_passes_all_pii_through() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Redact,
        enabled: false,
    });
    let input = "sk-aaaabbbbccccddddeeee user@e.com 123-45-6789";
    let r = f.filter(input);
    assert_eq!(r.output, input);
    assert!(r.redactions.is_empty());
    assert!(!r.blocked);
}

#[test]
fn disabled_filter_constructor_passes_everything() {
    let f = ContentFilter::disabled();
    let input = "sk-aaaabbbbccccddddeeee user@e.com 123-45-6789";
    let r = f.filter(input);
    assert_eq!(r.output, input);
    assert!(r.redactions.is_empty());
}

// ── 9b. Block action ─────────────────────────────────────────────────────

#[test]
fn block_action_returns_empty_output_and_blocked_flag() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("sk-aaaabbbbccccddddeeee");
    assert!(r.blocked);
    assert!(r.output.is_empty());
    assert_eq!(r.redactions.len(), 1);
}

#[test]
fn block_action_no_match_passes_through() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let input = "Hello world, no secrets here.";
    let r = f.filter(input);
    assert!(!r.blocked);
    assert_eq!(r.output, input);
    assert!(r.redactions.is_empty());
}

#[test]
fn block_action_stops_at_first_matching_rule() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    // The input has an api_key (rule 0) and email (rule 4).
    // Block should return on first match.
    let r = f.filter("sk-aaaabbbbccccddddeeee user@e.com");
    assert!(r.blocked);
    assert!(r.output.is_empty());
    // Only one redaction entry (the rule that triggered the block)
    assert_eq!(r.redactions.len(), 1);
    assert_eq!(r.redactions[0].rule_name, "api_key");
}

#[test]
fn block_action_email_triggers_block() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("Send to user@example.com please");
    assert!(r.blocked);
    assert!(r.output.is_empty());
}

// ── 9c. Warn action ──────────────────────────────────────────────────────

#[test]
fn warn_action_preserves_output_but_records_redactions() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Warn,
        enabled: true,
    });
    let input = "Key is sk-aaaabbbbccccddddeeee";
    let r = f.filter(input);
    assert_eq!(r.output, input, "Warn should not modify output");
    assert!(!r.redactions.is_empty(), "Warn should still record matches");
    assert!(!r.blocked);
}

#[test]
fn warn_action_multiple_rules_all_recorded() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Warn,
        enabled: true,
    });
    let input = "sk-aaaabbbbccccddddeeee user@e.com 123-45-6789";
    let r = f.filter(input);
    assert_eq!(r.output, input);
    assert!(r.redactions.len() >= 3);
}

#[test]
fn warn_action_empty_input_no_warnings() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Warn,
        enabled: true,
    });
    let r = f.filter("");
    assert_eq!(r.output, "");
    assert!(r.redactions.is_empty());
}

// ── 9d. Redact action (default) ──────────────────────────────────────────

#[test]
fn redact_action_replaces_and_records() {
    let f = ContentFilter::with_defaults(); // default is Redact
    let r = f.filter("SSN: 123-45-6789");
    assert!(r.output.contains("[REDACTED_SSN]"));
    assert!(!r.output.contains("123-45-6789"));
    assert!(!r.blocked);
    assert_eq!(r.redactions.len(), 1);
    assert_eq!(r.redactions[0].rule_name, "ssn");
    assert_eq!(r.redactions[0].original_length, "123-45-6789".len());
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. FilterRule enabled/disabled toggle
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn disabled_rule_skipped() {
    let mut f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Redact,
        enabled: true,
    });
    f.add_rule(FilterRule {
        name: "test_rule".into(),
        pattern: Regex::new(r"SECRET").unwrap(),
        replacement: "[REDACTED]".into(),
        enabled: false,
    });
    // "SECRET" does not match any default rule, and our custom rule is disabled
    let input = "The word SECRET is here";
    let r = f.filter(input);
    assert_eq!(r.output, input, "Disabled rule should be skipped");
}

#[test]
fn enabled_rule_applied() {
    let mut f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Redact,
        enabled: true,
    });
    f.add_rule(FilterRule {
        name: "test_rule".into(),
        pattern: Regex::new(r"SECRET").unwrap(),
        replacement: "[REDACTED]".into(),
        enabled: true,
    });
    let r = f.filter("The word SECRET is here");
    assert!(r.output.contains("[REDACTED]"));
}

#[test]
fn mix_of_enabled_and_disabled_rules() {
    let mut f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Redact,
        enabled: true,
    });
    f.add_rule(FilterRule {
        name: "rule_a".into(),
        pattern: Regex::new(r"XAAX").unwrap(),
        replacement: "[A]".into(),
        enabled: true,
    });
    f.add_rule(FilterRule {
        name: "rule_b".into(),
        pattern: Regex::new(r"XBBX").unwrap(),
        replacement: "[B]".into(),
        enabled: false,
    });
    // Use tokens that do not match any default rules
    let r = f.filter("XAAX and XBBX");
    assert!(r.output.contains("[A]"));
    assert!(r.output.contains("XBBX"), "Disabled rule_b should not redact XBBX");
}

// ═══════════════════════════════════════════════════════════════════════════
// 11. Custom rules
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn custom_rule_added_to_existing_defaults() {
    let mut f = ContentFilter::with_defaults();
    f.add_rule(FilterRule {
        name: "ip_address".into(),
        pattern: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap(),
        replacement: "[REDACTED_IP]".into(),
        enabled: true,
    });
    let r = f.filter("Server at 192.168.1.1 has email admin@server.com");
    assert!(r.output.contains("[REDACTED_IP]"));
    assert!(r.output.contains("[REDACTED_EMAIL]"));
}

#[test]
fn custom_rule_with_capture_group_replacement() {
    let mut f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Redact,
        enabled: true,
    });
    f.add_rule(FilterRule {
        name: "partial_mask".into(),
        pattern: Regex::new(r"xsecret-(\w+)").unwrap(),
        replacement: "xsecret-***".into(),
        enabled: true,
    });
    let r = f.filter("The value is xsecret-alpha123");
    assert_eq!(r.output, "The value is xsecret-***");
}

// ═══════════════════════════════════════════════════════════════════════════
// 12. Redaction metadata correctness
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn redaction_position_is_correct() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Hi 123-45-6789 there");
    assert_eq!(r.redactions.len(), 1);
    assert_eq!(r.redactions[0].position, 3); // "Hi " is 3 bytes
    assert_eq!(r.redactions[0].original_length, 11); // "123-45-6789"
}

#[test]
fn redaction_position_at_start() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("123-45-6789 end");
    assert_eq!(r.redactions[0].position, 0);
}

#[test]
fn multiple_redactions_have_correct_positions() {
    let f = ContentFilter::with_defaults();
    // Two emails in the input. The redactions record positions on the
    // *original* input because find_iter runs on `input`.
    let r = f.filter("a@b.com and c@d.com");
    assert_eq!(r.redactions.len(), 2);
    assert_eq!(r.redactions[0].position, 0); // "a@b.com" starts at 0
    assert_eq!(r.redactions[1].position, 12); // "c@d.com" starts at 12
}

// ═══════════════════════════════════════════════════════════════════════════
// 13. FilterResult fields
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn filter_result_blocked_false_on_redact() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("sk-aaaabbbbccccddddeeee");
    assert!(!r.blocked);
}

#[test]
fn filter_result_blocked_false_on_warn() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Warn,
        enabled: true,
    });
    let r = f.filter("sk-aaaabbbbccccddddeeee");
    assert!(!r.blocked);
}

#[test]
fn filter_result_blocked_true_only_on_block() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("sk-aaaabbbbccccddddeeee");
    assert!(r.blocked);
}

// ═══════════════════════════════════════════════════════════════════════════
// 14. Newlines, tabs, special whitespace
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pii_on_different_lines() {
    let f = ContentFilter::with_defaults();
    let input = "line1 user@a.com\nline2 123-45-6789\nline3";
    let r = f.filter(input);
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(r.output.contains("[REDACTED_SSN]"));
    assert!(r.output.contains("line1"));
    assert!(r.output.contains("line3"));
}

#[test]
fn pii_split_across_lines_private_key_header_still_matches() {
    // The private_key regex uses \s+ between words which matches newlines,
    // so a newline between PRIVATE and KEY still triggers redaction.
    let f = ContentFilter::with_defaults();
    let input = "-----BEGIN RSA PRIVATE\nKEY-----";
    let r = f.filter(input);
    assert!(r.output.contains("[REDACTED_PRIVATE_KEY]"),
        "\\s+ in regex matches newlines, so split header is still caught");
}

#[test]
fn pii_split_private_key_words_does_not_match() {
    // Breaking the actual keyword structure prevents matching
    let f = ContentFilter::with_defaults();
    let input = "-----BEGIN RSA PRIV\nATE KEY-----";
    let r = f.filter(input);
    assert!(!r.output.contains("[REDACTED_PRIVATE_KEY]"),
        "Broken keyword should not match");
}

#[test]
fn tabs_around_pii() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("\tuser@example.com\t");
    assert_eq!(r.output, "\t[REDACTED_EMAIL]\t");
}

// ═══════════════════════════════════════════════════════════════════════════
// 15. Repeated filtering (idempotency)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn double_filtering_is_idempotent() {
    let f = ContentFilter::with_defaults();
    let input = "Contact user@example.com for key sk-aaaabbbbccccddddeeee";
    let first = f.filter(input);
    let second = f.filter(&first.output);
    assert_eq!(first.output, second.output, "Filtering twice should produce same output");
    assert!(second.redactions.is_empty(), "Second pass should find nothing new");
}

// ═══════════════════════════════════════════════════════════════════════════
// 16. Constructors
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn with_defaults_has_redact_action_and_enabled() {
    let f = ContentFilter::with_defaults();
    // Verify defaults work by confirming it redacts
    let r = f.filter("user@a.com");
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(!r.blocked);
}

#[test]
fn with_policy_uses_provided_policy() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("user@a.com");
    assert!(r.blocked);
}

#[test]
fn disabled_constructor_has_no_rules_and_disabled_policy() {
    let f = ContentFilter::disabled();
    // Even an obvious secret passes through
    let r = f.filter("-----BEGIN PRIVATE KEY-----");
    assert_eq!(r.output, "-----BEGIN PRIVATE KEY-----");
    assert!(r.redactions.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// 17. FilterPolicy Default trait
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn filter_policy_default_is_redact_and_enabled() {
    let policy = FilterPolicy::default();
    assert_eq!(policy.default_action, FilterAction::Redact);
    assert!(policy.enabled);
}

// ═══════════════════════════════════════════════════════════════════════════
// 18. FilterAction PartialEq, Eq, Clone, Copy, Debug
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn filter_action_equality() {
    assert_eq!(FilterAction::Redact, FilterAction::Redact);
    assert_eq!(FilterAction::Block, FilterAction::Block);
    assert_eq!(FilterAction::Warn, FilterAction::Warn);
    assert_ne!(FilterAction::Redact, FilterAction::Block);
    assert_ne!(FilterAction::Block, FilterAction::Warn);
}

#[test]
fn filter_action_clone_copy() {
    let a = FilterAction::Redact;
    let b = a; // Copy
    #[allow(clippy::clone_on_copy)]
    let c = a.clone(); // Clone — intentionally exercising Clone impl
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn filter_action_debug_format() {
    let dbg = format!("{:?}", FilterAction::Redact);
    assert!(dbg.contains("Redact"));
    let dbg = format!("{:?}", FilterAction::Block);
    assert!(dbg.contains("Block"));
    let dbg = format!("{:?}", FilterAction::Warn);
    assert!(dbg.contains("Warn"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 19. Redaction struct derives
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn redaction_debug_and_clone() {
    let r = Redaction {
        rule_name: "test".into(),
        original_length: 5,
        position: 10,
    };
    let cloned = r.clone();
    assert_eq!(cloned.rule_name, "test");
    assert_eq!(cloned.original_length, 5);
    assert_eq!(cloned.position, 10);
    let dbg = format!("{:?}", cloned);
    assert!(dbg.contains("test"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 20. Regex-specific edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn regex_special_chars_in_surrounding_text() {
    let f = ContentFilter::with_defaults();
    // Surrounding text with regex-special chars that should not affect matching
    let r = f.filter("(user@example.com) [123-45-6789] {test}");
    assert!(r.output.contains("[REDACTED_EMAIL]"));
    assert!(r.output.contains("[REDACTED_SSN]"));
}

#[test]
fn backslash_in_surrounding_text() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("path\\to\\user@example.com");
    assert!(r.output.contains("[REDACTED_EMAIL]"));
}

#[test]
fn consecutive_pii_no_separator() {
    let f = ContentFilter::with_defaults();
    // Two SSNs back to back with only a space
    let r = f.filter("123-45-6789 987-65-4321");
    let count = r.output.matches("[REDACTED_SSN]").count();
    assert_eq!(count, 2);
}

// ═══════════════════════════════════════════════════════════════════════════
// 21. Credit card format variations
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn credit_card_all_zeros() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("0000 0000 0000 0000");
    assert!(r.output.contains("[REDACTED_CC]"));
}

#[test]
fn credit_card_no_space_continuous() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("5500000000000004");
    assert!(r.output.contains("[REDACTED_CC]"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 22. Bearer token edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn bearer_with_multiple_spaces() {
    let f = ContentFilter::with_defaults();
    // The regex is Bearer\s+ so multiple spaces should work
    let r = f.filter("Bearer   tokenvalue123");
    assert!(r.output.contains("[REDACTED_BEARER]"));
}

#[test]
fn bearer_with_tab() {
    let f = ContentFilter::with_defaults();
    let r = f.filter("Bearer\ttokenvalue123");
    assert!(r.output.contains("[REDACTED_BEARER]"));
}

#[test]
fn bearer_lowercase_no_match() {
    let f = ContentFilter::with_defaults();
    let input = "bearer tokenvalue123";
    let r = f.filter(input);
    assert_eq!(r.output, input, "Lowercase 'bearer' should not match");
}

// ═══════════════════════════════════════════════════════════════════════════
// 23. Block action with each specific rule
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn block_on_email() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("No secrets except email@test.com");
    assert!(r.blocked);
}

#[test]
fn block_on_ssn() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("SSN is 999-88-7777");
    assert!(r.blocked);
}

#[test]
fn block_on_credit_card() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("Card: 4111 1111 1111 1111");
    assert!(r.blocked);
}

#[test]
fn block_on_aws_key() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("AKIAIOSFODNN7EXAMPLE");
    assert!(r.blocked);
}

#[test]
fn block_on_private_key() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("-----BEGIN PRIVATE KEY-----");
    assert!(r.blocked);
}

#[test]
fn block_on_bearer_token() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Block,
        enabled: true,
    });
    let r = f.filter("Bearer abc123xyz");
    assert!(r.blocked);
}

// ═══════════════════════════════════════════════════════════════════════════
// 24. Warn action with each specific rule
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn warn_on_email_preserves_and_records() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Warn,
        enabled: true,
    });
    let input = "user@example.com";
    let r = f.filter(input);
    assert_eq!(r.output, input);
    assert_eq!(r.redactions[0].rule_name, "email");
}

#[test]
fn warn_on_ssn_preserves_and_records() {
    let f = ContentFilter::with_policy(FilterPolicy {
        default_action: FilterAction::Warn,
        enabled: true,
    });
    let input = "123-45-6789";
    let r = f.filter(input);
    assert_eq!(r.output, input);
    assert!(r.redactions.iter().any(|rd| rd.rule_name == "ssn"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 25. FilterPolicy Clone derive
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn filter_policy_clone() {
    let p = FilterPolicy {
        default_action: FilterAction::Block,
        enabled: false,
    };
    let p2 = p.clone();
    assert_eq!(p2.default_action, FilterAction::Block);
    assert!(!p2.enabled);
}

#[test]
fn filter_policy_debug() {
    let p = FilterPolicy::default();
    let dbg = format!("{:?}", p);
    assert!(dbg.contains("Redact"));
    assert!(dbg.contains("true"));
}
