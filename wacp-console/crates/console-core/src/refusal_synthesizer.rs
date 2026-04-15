//! Inspect trail entries for refusal markers; synthesize dedicated `refusals`
//! channel frames. Refusals are first-class in the UI (`wcon-highway` §4A)
//! but the runtime emits them as ordinary trail entries — the synthesizer
//! pulls them out so the frontend doesn't have to re-parse trail bodies.
//!
//! Detection rules follow `wcon-highway` §4A.1:
//!   - `event_type = "tool_call_refused"` → tool-layer refusal. Body is a
//!     JSON object with `{ code, tool, reason, … }`.
//!   - `event_type = "agent_refused"` → agent-layer refusal (worker declined
//!     a directive).
//!   - `event_type = "coordinator_refused"` → coordinator-layer refusal
//!     (integration rejected).

use console_runtime::proto as proto;
use serde::Serialize;

#[derive(Clone, Default)]
pub struct RefusalSynthesizer;

impl RefusalSynthesizer {
    pub fn new() -> Self {
        Self
    }

    /// Inspect a trail entry; if it carries a refusal marker, return a
    /// `Refusal` frame.
    pub fn detect(&self, entry: &proto::TrailEntry) -> Option<Refusal> {
        let layer = match entry.event_type.as_str() {
            "tool_call_refused" => RefusalLayer::ToolLayer,
            "agent_refused" => RefusalLayer::AgentLayer,
            "coordinator_refused" => RefusalLayer::CoordinatorLayer,
            _ => return None,
        };
        let (code, reason) = parse_body(&entry.body);
        Some(Refusal {
            id: entry.id.clone(),
            layer,
            workspace_id: entry.workspace_id.clone(),
            actor: entry.actor.clone(),
            code,
            reason,
            sequence_number: entry.sequence_number,
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefusalLayer {
    ToolLayer,
    AgentLayer,
    CoordinatorLayer,
}

#[derive(Debug, Clone, Serialize)]
pub struct Refusal {
    pub id: String,
    pub layer: RefusalLayer,
    pub workspace_id: String,
    pub actor: String,
    pub code: Option<String>,
    pub reason: Option<String>,
    pub sequence_number: u64,
}

/// Body parsing is best-effort. Workers typically serialize refusal bodies
/// as JSON `{ "code": ..., "reason": ... }` but the runtime treats body as
/// opaque bytes. A non-JSON body falls through to `(None, None)` and the
/// frontend is expected to show the raw body on expansion.
fn parse_body(body: &[u8]) -> (Option<String>, Option<String>) {
    let value: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let obj = value.as_object();
    let code = obj
        .and_then(|o| o.get("code"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let reason = obj
        .and_then(|o| o.get("reason"))
        .and_then(|v| v.as_str())
        .map(String::from);
    (code, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(event_type: &str, body: &[u8]) -> proto::TrailEntry {
        proto::TrailEntry {
            id: "e".into(),
            timestamp: None,
            workspace_id: "ws".into(),
            actor: "agent".into(),
            event_type: event_type.into(),
            body: body.to_vec(),
            sequence_number: 0,
            chain_hash: vec![],
        }
    }

    #[test]
    fn non_refusal_returns_none() {
        let e = entry("envelope_delivered", b"");
        assert!(RefusalSynthesizer::new().detect(&e).is_none());
    }

    #[test]
    fn tool_layer_refusal_with_code_and_reason() {
        let body = br#"{"code":"COMPLIANCE_NOT_APPROVED","reason":"PHI access requires gate"}"#;
        let r = RefusalSynthesizer::new()
            .detect(&entry("tool_call_refused", body))
            .expect("refusal");
        assert_eq!(r.layer, RefusalLayer::ToolLayer);
        assert_eq!(r.code.as_deref(), Some("COMPLIANCE_NOT_APPROVED"));
        assert_eq!(r.reason.as_deref(), Some("PHI access requires gate"));
    }

    #[test]
    fn agent_layer_refusal_parses_regardless_of_body_shape() {
        let r = RefusalSynthesizer::new()
            .detect(&entry("agent_refused", b"raw text body"))
            .expect("refusal");
        assert_eq!(r.layer, RefusalLayer::AgentLayer);
        // Non-JSON body → code + reason are None but the refusal still surfaces.
        assert!(r.code.is_none());
        assert!(r.reason.is_none());
    }

    #[test]
    fn coordinator_layer_refusal() {
        let r = RefusalSynthesizer::new()
            .detect(&entry("coordinator_refused", br#"{"code":"BUDGET_EXHAUSTED"}"#))
            .expect("refusal");
        assert_eq!(r.layer, RefusalLayer::CoordinatorLayer);
        assert_eq!(r.code.as_deref(), Some("BUDGET_EXHAUSTED"));
    }
}
