import { useSessionStore } from "../../store/session";
import { EmptyState } from "../../components/EmptyState";

function errorCodeColor(code: string): string {
  if (code.startsWith("POLICY_")) return "var(--color-warning)";
  if (code.startsWith("PERM_")) return "var(--color-danger)";
  return "var(--color-text-muted)";
}

export function RefusalPanel() {
  const refusals = useSessionStore((s) => s.refusals);

  if (refusals.length === 0) {
    return <EmptyState title="No refusals recorded." size="compact" />;
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      {refusals.map((ref) => (
        <div
          key={ref.refusal_id}
          style={{
            padding: 14,
            borderRadius: 8,
            background: "var(--color-bg-secondary)",
            border: "1px solid var(--color-border)",
            borderLeft: "3px solid var(--color-danger)",
          }}
        >
          {/* Header row */}
          <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 6 }}>
            <span
              style={{
                fontSize: 11,
                padding: "1px 6px",
                borderRadius: 3,
                background: "var(--color-bg-tertiary)",
                color: "var(--color-text-secondary)",
              }}
            >
              {ref.workspace_label}
            </span>

            <span
              style={{
                fontSize: 13,
                fontWeight: 600,
                fontFamily: "monospace",
                color: "var(--color-text)",
              }}
            >
              {ref.tool_name}
            </span>

            <span
              style={{
                fontSize: 11,
                fontWeight: 700,
                padding: "2px 8px",
                borderRadius: 4,
                background: errorCodeColor(ref.error_code),
                color: "#fff",
              }}
            >
              {ref.error_code}
            </span>

            <span
              style={{
                fontSize: 11,
                padding: "1px 6px",
                borderRadius: 3,
                border: "1px solid var(--color-border)",
                color: "var(--color-text-muted)",
              }}
            >
              {ref.policy_kind}
            </span>
          </div>

          {/* Reason */}
          <p style={{ fontSize: 13, color: "var(--color-text)", margin: "0 0 4px 0" }}>
            {ref.reason}
          </p>

          {/* Unblock hint */}
          {ref.unblock_hint && (
            <p style={{ fontSize: 12, color: "var(--color-text-muted)", fontStyle: "italic", margin: 0 }}>
              {ref.unblock_hint}
            </p>
          )}
        </div>
      ))}
    </div>
  );
}
