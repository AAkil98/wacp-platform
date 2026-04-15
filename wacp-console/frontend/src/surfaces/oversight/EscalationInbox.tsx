import { useState } from "react";
import { useSessionStore } from "../../store/session";
import { api } from "../../api/client";

function timeAgo(timestamp: string | undefined): string {
  if (!timestamp) return "--";
  const diff = Date.now() - new Date(timestamp).getTime();
  if (diff < 0) return "just now";
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  return `${hours}h ago`;
}

interface EscalationInboxProps {
  sessionId: string;
}

export function EscalationInbox({ sessionId }: EscalationInboxProps) {
  const escalations = useSessionStore((s) => s.escalations);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [responses, setResponses] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState<Set<string>>(new Set());

  function setResponse(escId: string, value: string) {
    setResponses((prev) => ({ ...prev, [escId]: value }));
  }

  async function handleSubmit(escId: string) {
    const response = responses[escId] ?? "";
    if (!response.trim()) return;
    setSubmitting((prev) => new Set(prev).add(escId));
    try {
      await api.post(`/api/sessions/${sessionId}/escalations/${escId}`, { response });
      setResponses((prev) => {
        const next = { ...prev };
        delete next[escId];
        return next;
      });
    } finally {
      setSubmitting((prev) => {
        const next = new Set(prev);
        next.delete(escId);
        return next;
      });
    }
  }

  if (escalations.length === 0) {
    return <p style={{ color: "var(--color-text-muted)", fontSize: 14 }}>No escalations.</p>;
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {escalations.map((esc) => {
        const expanded = expandedId === esc.escalation_id;
        const response = responses[esc.escalation_id] ?? "";
        const isSubmitting = submitting.has(esc.escalation_id);
        const timestamp = esc.timestamp as string | undefined;

        return (
          <div
            key={esc.escalation_id}
            style={{
              borderRadius: 8,
              background: "var(--color-bg-secondary)",
              border: "1px solid var(--color-border)",
              overflow: "hidden",
            }}
          >
            {/* Summary row */}
            <div
              style={{
                padding: "12px 14px",
                display: "flex",
                alignItems: "center",
                gap: 10,
                cursor: "pointer",
              }}
              onClick={() => setExpandedId(expanded ? null : esc.escalation_id)}
            >
              <span
                style={{
                  fontSize: 11,
                  padding: "1px 6px",
                  borderRadius: 3,
                  background: "var(--color-bg-tertiary)",
                  color: "var(--color-text-secondary)",
                  flexShrink: 0,
                }}
              >
                {esc.workspace_label}
              </span>

              <span style={{ fontSize: 13, color: "var(--color-text)", flex: 1 }}>
                {esc.reason}
              </span>

              <span style={{ fontSize: 12, color: "var(--color-text-muted)", flexShrink: 0 }}>
                {timeAgo(timestamp)}
              </span>

              <span style={{ fontSize: 12, color: "var(--color-text-muted)" }}>
                {expanded ? "\u25B2" : "\u25BC"}
              </span>
            </div>

            {/* Expanded detail */}
            {expanded && (
              <div
                style={{
                  padding: "0 14px 14px 14px",
                  borderTop: "1px solid var(--color-border)",
                }}
              >
                {/* Full context JSON */}
                <pre
                  style={{
                    marginTop: 12,
                    padding: 12,
                    fontSize: 12,
                    fontFamily: "monospace",
                    background: "var(--color-bg-tertiary)",
                    borderRadius: 4,
                    overflow: "auto",
                    maxHeight: 240,
                    color: "var(--color-text-secondary)",
                  }}
                >
                  {JSON.stringify(esc, null, 2)}
                </pre>

                {/* Response area */}
                <div style={{ marginTop: 12, display: "flex", gap: 8, alignItems: "flex-end" }}>
                  <textarea
                    placeholder="Type your response..."
                    value={response}
                    onChange={(e) => setResponse(esc.escalation_id, e.target.value)}
                    rows={3}
                    style={{
                      flex: 1,
                      padding: "8px 10px",
                      fontSize: 13,
                      border: "1px solid var(--color-border)",
                      borderRadius: 6,
                      background: "var(--color-bg)",
                      color: "var(--color-text)",
                      resize: "vertical",
                      fontFamily: "inherit",
                    }}
                  />
                  <button
                    onClick={() => void handleSubmit(esc.escalation_id)}
                    disabled={isSubmitting || !response.trim()}
                    style={{
                      padding: "8px 18px",
                      fontSize: 13,
                      fontWeight: 600,
                      color: "#fff",
                      background: "var(--color-accent)",
                      border: "none",
                      borderRadius: 6,
                      cursor: isSubmitting || !response.trim() ? "not-allowed" : "pointer",
                      opacity: isSubmitting || !response.trim() ? 0.6 : 1,
                      alignSelf: "flex-end",
                    }}
                  >
                    {isSubmitting ? "Sending..." : "Submit"}
                  </button>
                </div>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
