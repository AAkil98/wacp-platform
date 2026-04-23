import { useState, useMemo } from "react";
import { useSessionStore } from "../../store/session";
import { api } from "../../api/client";
import { EmptyState } from "../../components/EmptyState";

const URGENCY_ORDER: Record<string, number> = {
  critical: 0,
  high: 1,
  medium: 2,
  low: 3,
};

function urgencyRank(u: string): number {
  return URGENCY_ORDER[u] ?? 99;
}

function remainingTime(timeoutAt: string | undefined): string {
  if (!timeoutAt) return "--";
  const diff = new Date(timeoutAt).getTime() - Date.now();
  if (diff <= 0) return "expired";
  const secs = Math.floor(diff / 1000);
  const mins = Math.floor(secs / 60);
  const remSecs = secs % 60;
  return `${mins}m ${String(remSecs).padStart(2, "0")}s`;
}

function urgencyColor(urgency: string): string {
  switch (urgency) {
    case "critical": return "var(--color-danger)";
    case "high": return "var(--color-warning)";
    default: return "var(--color-text-muted)";
  }
}

interface GateQueueProps {
  sessionId: string;
}

export function GateQueue({ sessionId }: GateQueueProps) {
  const gates = useSessionStore((s) => s.gates);
  const [reasons, setReasons] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [submitting, setSubmitting] = useState<Set<string>>(new Set());
  const [batchReason, setBatchReason] = useState("");

  const sorted = useMemo(() => {
    return [...gates].sort((a, b) => {
      const urgDiff = urgencyRank(a.urgency) - urgencyRank(b.urgency);
      if (urgDiff !== 0) return urgDiff;
      // Then by timeout (sooner first)
      const aTime = a.timeout_at ? new Date(a.timeout_at).getTime() : Infinity;
      const bTime = b.timeout_at ? new Date(b.timeout_at).getTime() : Infinity;
      return aTime - bTime;
    });
  }, [gates]);

  async function handleDecision(gateId: string, decision: "approve" | "reject") {
    const reason = reasons[gateId] ?? "";
    setSubmitting((prev) => new Set(prev).add(gateId));
    try {
      await api.post(`/api/sessions/${sessionId}/gates/${gateId}`, { decision, reason });
    } finally {
      setSubmitting((prev) => {
        const next = new Set(prev);
        next.delete(gateId);
        return next;
      });
    }
  }

  async function handleBatchApprove() {
    const ids = Array.from(selected);
    if (ids.length === 0) return;
    setSubmitting(new Set(ids));
    try {
      await Promise.all(
        ids.map((gateId) =>
          api.post(`/api/sessions/${sessionId}/gates/${gateId}`, {
            decision: "approve",
            reason: batchReason,
          })
        )
      );
      setSelected(new Set());
      setBatchReason("");
    } finally {
      setSubmitting(new Set());
    }
  }

  function toggleSelect(gateId: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(gateId)) {
        next.delete(gateId);
      } else {
        next.add(gateId);
      }
      return next;
    });
  }

  function setReason(gateId: string, value: string) {
    setReasons((prev) => ({ ...prev, [gateId]: value }));
  }

  if (sorted.length === 0) {
    return <EmptyState title="No pending gates." size="compact" />;
  }

  return (
    <div>
      {/* Batch controls */}
      {selected.size > 0 && (
        <div
          style={{
            display: "flex",
            gap: 12,
            alignItems: "center",
            marginBottom: 16,
            padding: 12,
            background: "var(--color-bg-secondary)",
            borderRadius: 8,
            border: "1px solid var(--color-border)",
          }}
        >
          <span style={{ fontSize: 13, fontWeight: 600 }}>
            {selected.size} selected
          </span>
          <input
            type="text"
            placeholder="Batch reason (optional)"
            value={batchReason}
            onChange={(e) => setBatchReason(e.target.value)}
            style={{
              flex: 1,
              padding: "6px 10px",
              fontSize: 13,
              border: "1px solid var(--color-border)",
              borderRadius: 6,
              background: "var(--color-bg)",
              color: "var(--color-text)",
            }}
          />
          <button
            onClick={() => void handleBatchApprove()}
            style={{
              padding: "6px 16px",
              fontSize: 13,
              fontWeight: 600,
              color: "#fff",
              background: "var(--color-success)",
              border: "none",
              borderRadius: 6,
              cursor: "pointer",
            }}
          >
            Approve Selected
          </button>
        </div>
      )}

      {/* Gate list */}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {sorted.map((gate) => {
          const isSubmitting = submitting.has(gate.gate_id);
          const reason = reasons[gate.gate_id] ?? "";

          return (
            <div
              key={gate.gate_id}
              style={{
                padding: 14,
                borderRadius: 8,
                background: "var(--color-bg-secondary)",
                border: "1px solid var(--color-border)",
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 8 }}>
                <input
                  type="checkbox"
                  checked={selected.has(gate.gate_id)}
                  onChange={() => toggleSelect(gate.gate_id)}
                  style={{ cursor: "pointer" }}
                />

                {/* Type badge */}
                <span
                  style={{
                    fontSize: 11,
                    fontWeight: 600,
                    padding: "2px 8px",
                    borderRadius: 4,
                    background: "var(--color-accent)",
                    color: "#fff",
                  }}
                >
                  {gate.type}
                </span>

                {/* Workspace label */}
                <span
                  style={{
                    fontSize: 12,
                    padding: "1px 6px",
                    borderRadius: 3,
                    background: "var(--color-bg-tertiary)",
                    color: "var(--color-text-secondary)",
                  }}
                >
                  {gate.workspace_label}
                </span>

                {/* Urgency */}
                <span
                  style={{
                    fontSize: 11,
                    fontWeight: 700,
                    color: urgencyColor(gate.urgency),
                    textTransform: "uppercase",
                  }}
                >
                  {gate.urgency}
                </span>

                {/* Timeout */}
                <span
                  style={{
                    fontSize: 12,
                    fontFamily: "monospace",
                    color: "var(--color-text-muted)",
                    marginLeft: "auto",
                  }}
                >
                  {remainingTime(gate.timeout_at)}
                </span>
              </div>

              {/* Subject */}
              <p style={{ fontSize: 13, color: "var(--color-text)", margin: "0 0 10px 0" }}>
                {typeof gate.subject === "object" && gate.subject !== null
                  ? String((gate.subject as Record<string, unknown>)["description"] ?? JSON.stringify(gate.subject))
                  : String(gate.subject)}
              </p>

              {/* Reason + actions */}
              <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <textarea
                  placeholder="Reason (optional)"
                  value={reason}
                  onChange={(e) => setReason(gate.gate_id, e.target.value)}
                  rows={1}
                  style={{
                    flex: 1,
                    padding: "6px 10px",
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
                  onClick={() => void handleDecision(gate.gate_id, "approve")}
                  disabled={isSubmitting}
                  style={{
                    padding: "6px 14px",
                    fontSize: 13,
                    fontWeight: 600,
                    color: "#fff",
                    background: "var(--color-success)",
                    border: "none",
                    borderRadius: 6,
                    cursor: isSubmitting ? "not-allowed" : "pointer",
                    opacity: isSubmitting ? 0.6 : 1,
                  }}
                >
                  Approve
                </button>
                <button
                  onClick={() => void handleDecision(gate.gate_id, "reject")}
                  disabled={isSubmitting}
                  style={{
                    padding: "6px 14px",
                    fontSize: 13,
                    fontWeight: 600,
                    color: "#fff",
                    background: "var(--color-danger)",
                    border: "none",
                    borderRadius: 6,
                    cursor: isSubmitting ? "not-allowed" : "pointer",
                    opacity: isSubmitting ? 0.6 : 1,
                  }}
                >
                  Reject
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
