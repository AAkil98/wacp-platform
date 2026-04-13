use regex::Regex;

/// Content filter applied at the LLM boundary.
pub struct ContentFilter {
    rules: Vec<FilterRule>,
    policy: FilterPolicy,
}

/// A single redaction rule.
pub struct FilterRule {
    pub name: String,
    pub pattern: Regex,
    pub replacement: String,
    pub enabled: bool,
}

/// What to do when content matches a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterAction {
    /// Replace matched content with the replacement string.
    Redact,
    /// Block the entire request and return an error.
    Block,
    /// Log a warning but allow the content through.
    Warn,
}

/// Per-workspace filter configuration.
#[derive(Debug, Clone)]
pub struct FilterPolicy {
    pub default_action: FilterAction,
    pub enabled: bool,
}

impl Default for FilterPolicy {
    fn default() -> Self {
        Self {
            default_action: FilterAction::Redact,
            enabled: true,
        }
    }
}

/// Result of filtering a string.
pub struct FilterResult {
    pub output: String,
    pub redactions: Vec<Redaction>,
    pub blocked: bool,
}

/// A single redaction that occurred.
#[derive(Debug, Clone)]
pub struct Redaction {
    pub rule_name: String,
    pub original_length: usize,
    pub position: usize,
}

impl ContentFilter {
    /// Create with default built-in rules (all enabled, Redact action).
    pub fn with_defaults() -> Self {
        Self {
            rules: default_rules(),
            policy: FilterPolicy::default(),
        }
    }

    /// Create a disabled filter (pass-through).
    pub fn disabled() -> Self {
        Self {
            rules: Vec::new(),
            policy: FilterPolicy {
                enabled: false,
                ..Default::default()
            },
        }
    }

    /// Create with custom policy.
    pub fn with_policy(policy: FilterPolicy) -> Self {
        Self {
            rules: default_rules(),
            policy,
        }
    }

    /// Add a custom rule.
    pub fn add_rule(&mut self, rule: FilterRule) {
        self.rules.push(rule);
    }

    /// Filter a string. Returns the filtered output and any redactions.
    pub fn filter(&self, input: &str) -> FilterResult {
        if !self.policy.enabled {
            return FilterResult {
                output: input.to_string(),
                redactions: Vec::new(),
                blocked: false,
            };
        }

        let mut output = input.to_string();
        let mut redactions = Vec::new();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            match self.policy.default_action {
                FilterAction::Block => {
                    if rule.pattern.is_match(&output) {
                        return FilterResult {
                            output: String::new(),
                            redactions: vec![Redaction {
                                rule_name: rule.name.clone(),
                                original_length: 0,
                                position: 0,
                            }],
                            blocked: true,
                        };
                    }
                }
                FilterAction::Redact => {
                    let mut offset_adj: i64 = 0;
                    for m in rule.pattern.find_iter(input) {
                        redactions.push(Redaction {
                            rule_name: rule.name.clone(),
                            original_length: m.len(),
                            position: m.start(),
                        });
                    }
                    output = rule
                        .pattern
                        .replace_all(&output, rule.replacement.as_str())
                        .to_string();
                }
                FilterAction::Warn => {
                    for m in rule.pattern.find_iter(&output) {
                        redactions.push(Redaction {
                            rule_name: rule.name.clone(),
                            original_length: m.len(),
                            position: m.start(),
                        });
                    }
                    // Don't modify output — just record
                }
            }
        }

        FilterResult {
            output,
            redactions,
            blocked: false,
        }
    }
}

fn default_rules() -> Vec<FilterRule> {
    vec![
        FilterRule {
            name: "api_key".into(),
            pattern: Regex::new(r"(sk-[a-zA-Z0-9]{20,})|(key-[a-zA-Z0-9]{20,})").unwrap(),
            replacement: "[REDACTED_API_KEY]".into(),
            enabled: true,
        },
        FilterRule {
            name: "bearer_token".into(),
            pattern: Regex::new(r"Bearer\s+[A-Za-z0-9\-._~+/]+=*").unwrap(),
            replacement: "[REDACTED_BEARER]".into(),
            enabled: true,
        },
        FilterRule {
            name: "aws_key".into(),
            pattern: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
            replacement: "[REDACTED_AWS_KEY]".into(),
            enabled: true,
        },
        FilterRule {
            name: "private_key".into(),
            pattern: Regex::new(r"-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----").unwrap(),
            replacement: "[REDACTED_PRIVATE_KEY]".into(),
            enabled: true,
        },
        FilterRule {
            name: "email".into(),
            pattern: Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
            replacement: "[REDACTED_EMAIL]".into(),
            enabled: true,
        },
        FilterRule {
            name: "ssn".into(),
            pattern: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
            replacement: "[REDACTED_SSN]".into(),
            enabled: true,
        },
        FilterRule {
            name: "credit_card".into(),
            pattern: Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b").unwrap(),
            replacement: "[REDACTED_CC]".into(),
            enabled: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_api_key() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("My key is sk-abc123def456ghi789jklmno");
        assert!(result.output.contains("[REDACTED_API_KEY]"));
        assert!(!result.output.contains("sk-abc123"));
        assert_eq!(result.redactions.len(), 1);
        assert_eq!(result.redactions[0].rule_name, "api_key");
    }

    #[test]
    fn detect_bearer_token() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("Auth: Bearer eyJhbGciOiJIUzI1NiJ9.eyJ0ZXN0IjoidmFsdWUifQ==");
        assert!(result.output.contains("[REDACTED_BEARER]"));
    }

    #[test]
    fn detect_aws_key() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("AWS key: AKIAIOSFODNN7EXAMPLE");
        assert!(result.output.contains("[REDACTED_AWS_KEY]"));
    }

    #[test]
    fn detect_private_key() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("-----BEGIN RSA PRIVATE KEY-----\nMIIE...");
        assert!(result.output.contains("[REDACTED_PRIVATE_KEY]"));
    }

    #[test]
    fn detect_email() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("Contact user@example.com for info");
        assert!(result.output.contains("[REDACTED_EMAIL]"));
        assert!(!result.output.contains("user@example.com"));
    }

    #[test]
    fn detect_ssn() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("SSN is 123-45-6789");
        assert!(result.output.contains("[REDACTED_SSN]"));
    }

    #[test]
    fn detect_credit_card() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("Card: 4111 1111 1111 1111");
        assert!(result.output.contains("[REDACTED_CC]"));
    }

    #[test]
    fn no_false_positives_on_normal_text() {
        let filter = ContentFilter::with_defaults();
        let input = "Hello, please implement the authentication module for our web app.";
        let result = filter.filter(input);
        assert_eq!(result.output, input);
        assert!(result.redactions.is_empty());
    }

    #[test]
    fn multiple_matches_in_one_string() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("Key: sk-abc123def456ghi789jklmno and email: a@b.com");
        assert!(result.output.contains("[REDACTED_API_KEY]"));
        assert!(result.output.contains("[REDACTED_EMAIL]"));
        assert_eq!(result.redactions.len(), 2);
    }

    #[test]
    fn disabled_filter_passes_everything() {
        let filter = ContentFilter::disabled();
        let input = "sk-abc123def456ghi789jklmno secret@email.com";
        let result = filter.filter(input);
        assert_eq!(result.output, input);
        assert!(result.redactions.is_empty());
    }

    #[test]
    fn block_action_returns_empty() {
        let filter = ContentFilter::with_policy(FilterPolicy {
            default_action: FilterAction::Block,
            enabled: true,
        });
        let result = filter.filter("Here is sk-abc123def456ghi789jklmno");
        assert!(result.blocked);
        assert!(result.output.is_empty());
    }

    #[test]
    fn warn_action_preserves_content() {
        let filter = ContentFilter::with_policy(FilterPolicy {
            default_action: FilterAction::Warn,
            enabled: true,
        });
        let input = "Key: sk-abc123def456ghi789jklmno";
        let result = filter.filter(input);
        assert_eq!(result.output, input); // preserved
        assert!(!result.redactions.is_empty()); // but recorded
    }

    #[test]
    fn custom_rule() {
        let mut filter = ContentFilter::disabled();
        filter.policy.enabled = true;
        filter.policy.default_action = FilterAction::Redact;
        filter.add_rule(FilterRule {
            name: "project_id".into(),
            pattern: Regex::new(r"proj-[a-z0-9]{8}").unwrap(),
            replacement: "[REDACTED_PROJECT]".into(),
            enabled: true,
        });
        let result = filter.filter("Project: proj-abc12345");
        assert!(result.output.contains("[REDACTED_PROJECT]"));
    }

    // --- Edge cases ---

    #[test]
    fn filter_with_unicode_content() {
        let filter = ContentFilter::with_defaults();
        let input = "Hello 🌍! User admin@example.com sent a résumé";
        let result = filter.filter(input);
        assert!(result.output.contains("[REDACTED_EMAIL]"));
        assert!(result.output.contains("Hello 🌍"));
        assert!(result.output.contains("résumé"));
    }

    #[test]
    fn filter_with_overlapping_patterns() {
        let filter = ContentFilter::with_defaults();
        // Email inside an API key-like string — both should be caught
        let input = "key-abcdefghijklmnopqrstuvwx user@test.com";
        let result = filter.filter(input);
        assert!(result.redactions.len() >= 2);
    }

    #[test]
    fn filter_empty_string() {
        let filter = ContentFilter::with_defaults();
        let result = filter.filter("");
        assert_eq!(result.output, "");
        assert!(result.redactions.is_empty());
    }

    #[test]
    fn filter_preserves_newlines_and_whitespace() {
        let filter = ContentFilter::with_defaults();
        let input = "line 1\n\n  line 3\n\tindented";
        let result = filter.filter(input);
        assert_eq!(result.output, input);
    }
}
