import { useState, useMemo } from "react";
import { useSessionStore } from "../../store/session";
import { ClickCard } from "../../components/ClickCard";

function formatTimestamp(iso: string): string {
  try {
    const d = new Date(iso);
    const h = String(d.getHours()).padStart(2, "0");
    const m = String(d.getMinutes()).padStart(2, "0");
    const s = String(d.getSeconds()).padStart(2, "0");
    const ms = String(d.getMilliseconds()).padStart(3, "0");
    return `${h}:${m}:${s}.${ms}`;
  } catch {
    return iso;
  }
}

export function TrailStream() {
  const trail = useSessionStore((s) => s.trail);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [filterType, setFilterType] = useState("");
  const [filterWorkspace, setFilterWorkspace] = useState("");

  // Collect unique event_types and workspace_labels for filter dropdowns
  const eventTypes = useMemo(() => {
    const set = new Set<string>();
    for (const entry of trail) {
      if (entry.event_type) set.add(entry.event_type);
    }
    return Array.from(set).sort();
  }, [trail]);

  const workspaceLabels = useMemo(() => {
    const set = new Set<string>();
    for (const entry of trail) {
      if (entry.workspace_label) set.add(entry.workspace_label);
    }
    return Array.from(set).sort();
  }, [trail]);

  // Filter and reverse (newest first)
  const filtered = useMemo(() => {
    let items = trail;
    if (filterType) {
      items = items.filter((e) => e.event_type === filterType);
    }
    if (filterWorkspace) {
      items = items.filter((e) => e.workspace_label === filterWorkspace);
    }
    return [...items].reverse();
  }, [trail, filterType, filterWorkspace]);

  return (
    <div>
      {/* Filter bar */}
      <div style={{ display: "flex", gap: 12, marginBottom: 16 }}>
        <select
          value={filterType}
          onChange={(e) => setFilterType(e.target.value)}
          style={{
            padding: "6px 10px",
            fontSize: 13,
            border: "1px solid var(--color-border)",
            borderRadius: 6,
            background: "var(--color-bg)",
            color: "var(--color-text)",
          }}
        >
          <option value="">All event types</option>
          {eventTypes.map((t) => (
            <option key={t} value={t}>{t}</option>
          ))}
        </select>

        <select
          value={filterWorkspace}
          onChange={(e) => setFilterWorkspace(e.target.value)}
          style={{
            padding: "6px 10px",
            fontSize: 13,
            border: "1px solid var(--color-border)",
            borderRadius: 6,
            background: "var(--color-bg)",
            color: "var(--color-text)",
          }}
        >
          <option value="">All workspaces</option>
          {workspaceLabels.map((w) => (
            <option key={w} value={w}>{w}</option>
          ))}
        </select>

        <span style={{ fontSize: 12, color: "var(--color-text-muted)", alignSelf: "center" }}>
          {filtered.length} entries
        </span>
      </div>

      {/* Trail list */}
      {filtered.length === 0 && (
        <p style={{ color: "var(--color-text-muted)", fontSize: 14 }}>
          No trail entries yet.
        </p>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        {filtered.map((entry) => {
          const isRefusal = entry.event_type === "refusal";
          const expanded = expandedId === entry.id;

          return (
            <ClickCard
              key={entry.id}
              aria-label={expanded ? "Collapse trail entry" : "Expand trail entry"}
              style={{
                padding: "8px 12px",
                borderRadius: 6,
                background: "var(--color-bg-secondary)",
                borderLeft: isRefusal ? "3px solid var(--color-danger)" : "3px solid transparent",
                cursor: "pointer",
              }}
              onClick={() => setExpandedId(expanded ? null : entry.id)}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                {/* Timestamp */}
                <span
                  style={{
                    fontSize: 12,
                    fontFamily: "monospace",
                    color: "var(--color-text-muted)",
                    flexShrink: 0,
                  }}
                >
                  {formatTimestamp(entry.timestamp)}
                </span>

                {/* Workspace badge */}
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
                  {entry.workspace_label}
                </span>

                {/* Event type tag */}
                <span
                  style={{
                    fontSize: 11,
                    fontWeight: 600,
                    padding: "1px 6px",
                    borderRadius: 3,
                    background: isRefusal ? "var(--color-danger)" : "var(--color-accent)",
                    color: "#fff",
                    flexShrink: 0,
                  }}
                >
                  {entry.event_type}
                </span>

                {/* Summary */}
                <span
                  style={{
                    fontSize: 13,
                    color: "var(--color-text)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {entry.summary}
                </span>
              </div>

              {/* Expanded JSON */}
              {expanded && (
                <pre
                  style={{
                    marginTop: 8,
                    padding: 12,
                    fontSize: 12,
                    fontFamily: "monospace",
                    background: "var(--color-bg-tertiary)",
                    borderRadius: 4,
                    overflow: "auto",
                    maxHeight: 300,
                    color: "var(--color-text-secondary)",
                  }}
                >
                  {JSON.stringify(entry, null, 2)}
                </pre>
              )}
            </ClickCard>
          );
        })}
      </div>
    </div>
  );
}
