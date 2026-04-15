use serde::Serialize;
use sha2::{Digest, Sha256};

/// An audit event — recorded in the trail for security observability.
///
/// Audit events record *that something happened* and *a hash of what*,
/// never the actual content. This enables verification without storing
/// sensitive data in the trail.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "audit_type", rename_all = "snake_case")]
pub enum AuditEvent {
    /// Authentication attempt (success or failure).
    AuthAttempt {
        identity: String,
        method: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    /// Rate limit triggered.
    RateLimited {
        identity: String,
        limit_type: String,
    },

    /// Secret was accessed (exposed for use).
    SecretAccess { key_name: String, purpose: String },

    /// Content filter triggered.
    ContentFiltered {
        rule_name: String,
        action: String,
        context: String,
    },

    /// Tool invocation (hashes only, no content).
    ToolInvocation {
        tool_name: String,
        capability: String,
        input_hash: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_hash: Option<String>,
        duration_ms: u64,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_code: Option<String>,
    },
}

impl AuditEvent {
    /// Serialize to JSON bytes for trail storage.
    pub fn to_trail_payload(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
}

/// Compute SHA-256 hash of data, return as hex string.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_attempt_success_serde() {
        let event = AuditEvent::AuthAttempt {
            identity: "user-1".into(),
            method: "api_key".into(),
            success: true,
            reason: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["audit_type"], "auth_attempt");
        assert_eq!(json["identity"], "user-1");
        assert_eq!(json["success"], true);
        assert!(json.get("reason").is_none()); // skipped when None
    }

    #[test]
    fn auth_attempt_failure_serde() {
        let event = AuditEvent::AuthAttempt {
            identity: "unknown".into(),
            method: "psk".into(),
            success: false,
            reason: Some("invalid token".into()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["reason"], "invalid token");
    }

    #[test]
    fn rate_limited_serde() {
        let event = AuditEvent::RateLimited {
            identity: "192.168.1.1".into(),
            limit_type: "auth_failure".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["audit_type"], "rate_limited");
        assert_eq!(json["limit_type"], "auth_failure");
    }

    #[test]
    fn secret_access_serde() {
        let event = AuditEvent::SecretAccess {
            key_name: "anthropic_api_key".into(),
            purpose: "llm_auth".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["audit_type"], "secret_access");
        assert_eq!(json["purpose"], "llm_auth");
    }

    #[test]
    fn content_filtered_serde() {
        let event = AuditEvent::ContentFiltered {
            rule_name: "api_key".into(),
            action: "redacted".into(),
            context: "llm_request".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["audit_type"], "content_filtered");
        assert_eq!(json["action"], "redacted");
    }

    #[test]
    fn tool_invocation_success_serde() {
        let event = AuditEvent::ToolInvocation {
            tool_name: "read_file".into(),
            capability: "read".into(),
            input_hash: sha256_hex(b"test input"),
            output_hash: Some(sha256_hex(b"test output")),
            duration_ms: 42,
            success: true,
            error_code: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["audit_type"], "tool_invocation");
        assert_eq!(json["success"], true);
        assert!(json["input_hash"].as_str().unwrap().len() == 64); // SHA-256 hex
        assert!(json.get("error_code").is_none()); // skipped
    }

    #[test]
    fn tool_invocation_failure_serde() {
        let event = AuditEvent::ToolInvocation {
            tool_name: "shell_exec".into(),
            capability: "shell_exec".into(),
            input_hash: sha256_hex(b"rm -rf /"),
            output_hash: None,
            duration_ms: 1,
            success: false,
            error_code: Some("timeout".into()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["error_code"], "timeout");
        assert!(json.get("output_hash").is_none());
    }

    #[test]
    fn to_trail_payload_produces_json() {
        let event = AuditEvent::AuthAttempt {
            identity: "test".into(),
            method: "psk".into(),
            success: true,
            reason: None,
        };
        let payload = event.to_trail_payload();
        assert!(!payload.is_empty());
        let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(parsed["audit_type"], "auth_attempt");
    }

    #[test]
    fn sha256_hex_deterministic() {
        let hash1 = sha256_hex(b"hello");
        let hash2 = sha256_hex(b"hello");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // 256 bits = 64 hex chars
    }

    #[test]
    fn sha256_hex_different_inputs() {
        let hash1 = sha256_hex(b"hello");
        let hash2 = sha256_hex(b"world");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn all_audit_event_variants_serialize() {
        // Ensure every variant can be serialized without panic
        let events: Vec<AuditEvent> = vec![
            AuditEvent::AuthAttempt {
                identity: "x".into(),
                method: "y".into(),
                success: true,
                reason: None,
            },
            AuditEvent::RateLimited {
                identity: "x".into(),
                limit_type: "y".into(),
            },
            AuditEvent::SecretAccess {
                key_name: "x".into(),
                purpose: "y".into(),
            },
            AuditEvent::ContentFiltered {
                rule_name: "x".into(),
                action: "y".into(),
                context: "z".into(),
            },
            AuditEvent::ToolInvocation {
                tool_name: "x".into(),
                capability: "y".into(),
                input_hash: "h".into(),
                output_hash: None,
                duration_ms: 0,
                success: true,
                error_code: None,
            },
        ];
        for event in &events {
            let json = serde_json::to_value(event).unwrap();
            assert!(json.get("audit_type").is_some());
        }
    }
}
