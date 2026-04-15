import { useMemo } from "react";
import { useSessionStore } from "../../store/session";

function stateColor(state: string): string {
  switch (state.toUpperCase()) {
    case "ACTIVE": return "var(--color-success)";
    case "BLOCKED": return "var(--color-danger)";
    case "IDLE": return "var(--color-text-muted)";
    case "CLOSED": return "var(--color-border)";
    default: return "var(--color-text-muted)";
  }
}

export function WorkspaceTree() {
  const workspaces = useSessionStore((s) => s.workspaces);

  const entries = useMemo(() => {
    return Array.from(workspaces.entries()).sort(([a], [b]) => a.localeCompare(b));
  }, [workspaces]);

  if (entries.length === 0) {
    return <p style={{ color: "var(--color-text-muted)", fontSize: 14 }}>No workspaces active.</p>;
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      {entries.map(([wsId, state]) => {
        const color = stateColor(state);

        return (
          <div
            key={wsId}
            style={{
              padding: "10px 14px",
              borderRadius: 6,
              background: "var(--color-bg-secondary)",
              display: "flex",
              alignItems: "center",
              gap: 12,
            }}
          >
            {/* State dot */}
            <span
              style={{
                width: 10,
                height: 10,
                borderRadius: "50%",
                background: color,
                flexShrink: 0,
              }}
            />

            {/* Workspace ID */}
            <span
              style={{
                fontSize: 13,
                fontFamily: "monospace",
                color: state.toUpperCase() === "CLOSED" ? "var(--color-text-muted)" : "var(--color-text)",
                flex: 1,
              }}
            >
              {wsId}
            </span>

            {/* State badge */}
            <span
              style={{
                fontSize: 11,
                fontWeight: 700,
                padding: "2px 8px",
                borderRadius: 4,
                border: `1px solid ${color}`,
                color,
                textTransform: "uppercase",
              }}
            >
              {state}
            </span>
          </div>
        );
      })}
    </div>
  );
}
