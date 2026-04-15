//! Enrich raw highway stream events with taxonomy-sourced metadata before
//! they reach the frontend. Keeps all taxonomy lookups in one place so the
//! session monitor (W3) can stay shape-focused.
//!
//! Spec: `wcon-highway` §3.1, §4.1, §5, §7.

use std::sync::Arc;

use arc_swap::ArcSwap;
use console_runtime::proto as proto;
use serde::Serialize;

use crate::taxonomy::TaxonomyIndex;

/// Shared enricher — cheap to clone. Used by the session monitor to annotate
/// raw trail / gate / escalation / workspace events before broadcasting them
/// to WebSocket subscribers.
///
/// The monitor maintains a workspace_id → label map (seeded from
/// `WorkspaceView.role` responses); the enricher is a pure formatter over
/// raw events + optional label. Taxonomy is held for future lookups (e.g.,
/// checkpoint schema resolution) but currently unused — keeping the handle
/// around so the type doesn't churn when W3 gains checkpoint enrichment.
#[derive(Clone)]
pub struct EventEnricher {
    #[allow(dead_code)]
    taxonomy: Arc<ArcSwap<TaxonomyIndex>>,
}

impl EventEnricher {
    pub fn new(taxonomy: Arc<ArcSwap<TaxonomyIndex>>) -> Self {
        Self { taxonomy }
    }

    /// Enrich a raw trail entry. `workspace_label` is produced by the
    /// monitor from `WorkspaceView.role` (via `GetWorkspace` at session
    /// start); a missing label falls back to the raw workspace id.
    pub fn enrich_trail(
        &self,
        raw: &proto::TrailEntry,
        workspace_label: Option<&str>,
    ) -> EnrichedTrailEntry {
        let event_type = raw.event_type.clone();
        let summary = summarize_trail(&event_type, &raw.body);
        EnrichedTrailEntry {
            id: raw.id.clone(),
            timestamp: timestamp_rfc3339(&raw.timestamp),
            sequence_number: raw.sequence_number,
            workspace_id: raw.workspace_id.clone(),
            workspace_label: workspace_label.unwrap_or(&raw.workspace_id).to_string(),
            actor: raw.actor.clone(),
            event_type,
            summary,
            body_len: raw.body.len() as u64,
        }
    }

    pub fn enrich_gate(
        &self,
        raw: &proto::GateEvent,
        workspace_label: Option<&str>,
    ) -> EnrichedGate {
        EnrichedGate {
            gate_id: raw.gate_id.clone(),
            type_: gate_type_string(raw.r#type()),
            workspace_id: raw.workspace_id.clone(),
            workspace_label: workspace_label.unwrap_or(&raw.workspace_id).to_string(),
            task_id: raw.task_id.clone(),
            timeout_ms: raw.timeout_ms,
            fallback_action: raw.fallback_action.clone(),
            created_at: timestamp_rfc3339(&raw.created_at),
            subject_len: raw.subject.len() as u64,
        }
    }

    pub fn enrich_escalation(
        &self,
        raw: &proto::EscalationEvent,
        workspace_label: Option<&str>,
    ) -> EnrichedEscalation {
        EnrichedEscalation {
            escalation_id: raw.escalation_id.clone(),
            workspace_id: raw.workspace_id.clone(),
            workspace_label: workspace_label.unwrap_or(&raw.workspace_id).to_string(),
            owner: raw.owner.clone(),
            created_at: timestamp_rfc3339(&raw.created_at),
            context_len: raw.context.len() as u64,
        }
    }

}

#[derive(Debug, Clone, Serialize)]
pub struct EnrichedTrailEntry {
    pub id: String,
    pub timestamp: String,
    pub sequence_number: u64,
    pub workspace_id: String,
    pub workspace_label: String,
    pub actor: String,
    pub event_type: String,
    pub summary: String,
    /// `body` itself is often large; we ship only its length to the frontend
    /// by default. Frontends request the full body via `GET
    /// /api/sessions/:id/trail/:entry` when the user clicks to expand.
    pub body_len: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnrichedGate {
    pub gate_id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub workspace_id: String,
    pub workspace_label: String,
    pub task_id: String,
    pub timeout_ms: u64,
    pub fallback_action: String,
    pub created_at: String,
    pub subject_len: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnrichedEscalation {
    pub escalation_id: String,
    pub workspace_id: String,
    pub workspace_label: String,
    pub owner: String,
    pub created_at: String,
    pub context_len: u64,
}

fn gate_type_string(t: proto::GateType) -> String {
    match t {
        proto::GateType::Unspecified => "unspecified",
        proto::GateType::TaskApproval => "task_approval",
        proto::GateType::WorkspaceCreate => "workspace_create",
        proto::GateType::EnvelopeDelivery => "envelope_delivery",
        proto::GateType::Integration => "integration",
        proto::GateType::ConflictResolution => "conflict_resolution",
        proto::GateType::WorkspaceAbort => "workspace_abort",
    }
    .to_string()
}

/// Produce a one-line summary from a trail entry's `event_type` + body. Keeps
/// the frontend's default row short — full body is on-demand via REST.
fn summarize_trail(event_type: &str, body: &[u8]) -> String {
    let body_preview = if body.len() <= 64 {
        String::from_utf8_lossy(body).to_string()
    } else {
        format!("<{} bytes>", body.len())
    };
    match event_type {
        "envelope_delivered" => format!("envelope delivered ({body_preview})"),
        "signal" => format!("signal ({body_preview})"),
        "checkpoint" => format!("checkpoint ({body_preview})"),
        "gate_created" => "gate created".into(),
        "gate_resolved" => "gate resolved".into(),
        "escalation_created" => "escalation created".into(),
        "state_change" => format!("state change ({body_preview})"),
        other => format!("{other} ({body_preview})"),
    }
}

/// Proto `Timestamp { physical_us, logical }` → RFC 3339. `physical_us` is
/// microseconds since Unix epoch; `logical` is a 16-bit logical counter
/// which we surface as a numeric suffix if non-zero.
fn timestamp_rfc3339(ts: &Option<proto::Timestamp>) -> String {
    let Some(ts) = ts else {
        return String::new();
    };
    let secs = (ts.physical_us / 1_000_000) as i64;
    let nanos = ((ts.physical_us % 1_000_000) * 1_000) as u32;
    match chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos) {
        Some(dt) if ts.logical == 0 => dt.to_rfc3339(),
        Some(dt) => format!("{}#{}", dt.to_rfc3339(), ts.logical),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy_builder;

    fn empty_taxonomy() -> Arc<ArcSwap<TaxonomyIndex>> {
        Arc::new(ArcSwap::from_pointee(
            taxonomy_builder::build_index(None, &[], &[]).index,
        ))
    }

    fn enricher() -> EventEnricher {
        EventEnricher::new(empty_taxonomy())
    }

    fn ts(physical_us: u64) -> Option<proto::Timestamp> {
        Some(proto::Timestamp {
            physical_us,
            logical: 0,
        })
    }

    #[test]
    fn enrich_trail_uses_fallback_label_when_none_provided() {
        let raw = proto::TrailEntry {
            id: "t1".into(),
            timestamp: ts(1_700_000_000_000_000),
            workspace_id: "ws-7".into(),
            actor: "agent".into(),
            event_type: "envelope_delivered".into(),
            body: b"hello".to_vec(),
            sequence_number: 42,
            chain_hash: vec![],
        };
        let out = enricher().enrich_trail(&raw, None);
        assert_eq!(out.workspace_label, "ws-7");
        assert!(out.summary.contains("envelope delivered"));
        assert_eq!(out.sequence_number, 42);
        assert_eq!(out.body_len, 5);
    }

    #[test]
    fn enrich_trail_applies_provided_label() {
        let raw = proto::TrailEntry {
            id: "t2".into(),
            timestamp: ts(1_700_000_000_000_000),
            workspace_id: "ws-9".into(),
            actor: "agent".into(),
            event_type: "signal".into(),
            body: vec![0; 200],
            sequence_number: 43,
            chain_hash: vec![],
        };
        let out = enricher().enrich_trail(&raw, Some("swe:implementer (Fast)"));
        assert_eq!(out.workspace_label, "swe:implementer (Fast)");
        assert_eq!(out.body_len, 200);
        // Large body summarized as byte count, not contents.
        assert!(out.summary.contains("200 bytes"));
    }

    #[test]
    fn enrich_gate_maps_gate_type_to_string() {
        let raw = proto::GateEvent {
            gate_id: "g1".into(),
            r#type: proto::GateType::TaskApproval as i32,
            subject: vec![0; 10],
            workspace_id: "ws-1".into(),
            task_id: "t-1".into(),
            timeout_ms: 30_000,
            fallback_action: "reject".into(),
            created_at: ts(1_700_000_000_000_000),
        };
        let out = enricher().enrich_gate(&raw, None);
        assert_eq!(out.type_, "task_approval");
        assert_eq!(out.timeout_ms, 30_000);
        assert_eq!(out.fallback_action, "reject");
        assert_eq!(out.subject_len, 10);
    }

    #[test]
    fn enrich_escalation_copies_owner() {
        let raw = proto::EscalationEvent {
            escalation_id: "e1".into(),
            workspace_id: "ws-1".into(),
            owner: "u-42".into(),
            context: vec![],
            created_at: ts(1_700_000_000_000_000),
        };
        let out = enricher().enrich_escalation(&raw, None);
        assert_eq!(out.owner, "u-42");
        assert_eq!(out.workspace_label, "ws-1");
    }

    #[test]
    fn timestamp_rfc3339_with_logical_counter() {
        let ts = Some(proto::Timestamp {
            physical_us: 1_700_000_000_000_000,
            logical: 7,
        });
        let s = timestamp_rfc3339(&ts);
        assert!(s.ends_with("#7"), "expected logical suffix, got {s}");
    }
}
