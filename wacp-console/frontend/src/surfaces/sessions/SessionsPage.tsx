import type React from "react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { TableVirtuoso } from "react-virtuoso";
import { useSessions } from "../../api/hooks/index";
import { Wizard } from "./Wizard";

// HEALTH-LOG §4 item 7 / frontend-perf-plan F7 — threshold-gated virtualization.
const VIRTUOSO_THRESHOLD = 50;

// --- Types ---

interface SessionSummary {
  id: string;
  name: string;
  vertical: string;
  workflow: string;
  state: string;
  created_at: string;
  owner: string;
}

// --- Styles ---

const page: React.CSSProperties = {
  height: "100%",
  display: "flex",
  flexDirection: "column",
  background: "var(--color-bg)",
  color: "var(--color-text)",
};

const header: React.CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "center",
  padding: "20px 24px 16px",
};

const titleStyle: React.CSSProperties = {
  fontSize: 24,
  fontWeight: 700,
  margin: 0,
};

const subtitle: React.CSSProperties = {
  color: "var(--color-text-secondary)",
  fontSize: 14,
  marginTop: 4,
};

const btnPrimary: React.CSSProperties = {
  padding: "8px 16px",
  background: "var(--color-accent)",
  color: "#fff",
  border: "none",
  borderRadius: 4,
  cursor: "pointer",
  fontWeight: 600,
  fontSize: 14,
};

const tableContainer: React.CSSProperties = {
  flex: 1,
  overflowY: "auto",
  padding: "0 24px 24px",
};

const table: React.CSSProperties = {
  width: "100%",
  borderCollapse: "collapse",
};

const th: React.CSSProperties = {
  textAlign: "left",
  padding: "10px 12px",
  fontSize: 12,
  fontWeight: 600,
  textTransform: "uppercase",
  letterSpacing: "0.05em",
  color: "var(--color-text-secondary)",
  borderBottom: "2px solid var(--color-border)",
};

const td: React.CSSProperties = {
  padding: "12px 12px",
  fontSize: 14,
  borderBottom: "1px solid var(--color-border)",
};

const row: React.CSSProperties = {
  cursor: "pointer",
  transition: "background 0.1s",
};

const STATE_COLORS: Record<string, string> = {
  configuring: "#3b82f6",
  active: "#22c55e",
  completed: "#9ca3af",
  failed: "#ef4444",
  cancelled: "#f97316",
};

function stateBadge(state: string): React.CSSProperties {
  const color = STATE_COLORS[state] ?? "var(--color-text-secondary)";
  return {
    display: "inline-block",
    padding: "2px 10px",
    borderRadius: 12,
    fontSize: 12,
    fontWeight: 600,
    color: "#fff",
    background: color,
  };
}

// --- Component ---

export function SessionsPage() {
  const [wizardOpen, setWizardOpen] = useState(false);
  const navigate = useNavigate();
  const sessionsQuery = useSessions();
  const sessions = (sessionsQuery.data ?? []) as SessionSummary[];

  function handleRowClick(id: string) {
    void navigate(`/sessions/${id}/oversight`);
  }

  if (wizardOpen) {
    return (
      <div style={page}>
        <Wizard onClose={() => { setWizardOpen(false); void sessionsQuery.refetch(); }} />
      </div>
    );
  }

  return (
    <div style={page}>
      <div style={header}>
        <div>
          <h1 style={titleStyle}>Sessions</h1>
          <p style={subtitle}>Launch and manage coordination sessions.</p>
        </div>
        <button style={btnPrimary} onClick={() => setWizardOpen(true)}>
          New Session
        </button>
      </div>

      <div style={tableContainer}>
        {sessionsQuery.isLoading && (
          <p style={{ color: "var(--color-text-secondary)", padding: "24px 0" }}>Loading sessions...</p>
        )}

        {!sessionsQuery.isLoading && sessions.length === 0 && (
          <div style={{ textAlign: "center", paddingTop: 64, color: "var(--color-text-secondary)" }}>
            <p style={{ fontSize: 16 }}>No sessions yet.</p>
            <p style={{ fontSize: 14, marginTop: 4 }}>Click "New Session" to launch your first coordination session.</p>
          </div>
        )}

        {sessions.length > 0 &&
          (sessions.length > VIRTUOSO_THRESHOLD ? (
            <TableVirtuoso
              style={{ height: "70vh" }}
              data={sessions}
              fixedHeaderContent={() => (
                <tr>
                  <th style={th}>Name</th>
                  <th style={th}>State</th>
                  <th style={th}>Created</th>
                  <th style={th}>Owner</th>
                </tr>
              )}
              itemContent={(_, s) => (
                <>
                  <td style={td}>{s.name || `${s.vertical}/${s.workflow}`}</td>
                  <td style={td}>
                    <span style={stateBadge(s.state)}>{s.state}</span>
                  </td>
                  <td style={{ ...td, color: "var(--color-text-secondary)" }}>
                    {formatDate(s.created_at)}
                  </td>
                  <td style={{ ...td, color: "var(--color-text-secondary)" }}>{s.owner}</td>
                </>
              )}
            />
          ) : (
            <table style={table}>
              <thead>
                <tr>
                  <th style={th}>Name</th>
                  <th style={th}>State</th>
                  <th style={th}>Created</th>
                  <th style={th}>Owner</th>
                </tr>
              </thead>
              <tbody>
                {sessions.map((s) => (
                  <tr
                    key={s.id}
                    style={row}
                    onClick={() => handleRowClick(s.id)}
                    onMouseEnter={(e) => {
                      (e.currentTarget as HTMLElement).style.background =
                        "var(--color-bg-secondary)";
                    }}
                    onMouseLeave={(e) => {
                      (e.currentTarget as HTMLElement).style.background = "transparent";
                    }}
                  >
                    <td style={td}>{s.name || `${s.vertical}/${s.workflow}`}</td>
                    <td style={td}>
                      <span style={stateBadge(s.state)}>{s.state}</span>
                    </td>
                    <td style={{ ...td, color: "var(--color-text-secondary)" }}>
                      {formatDate(s.created_at)}
                    </td>
                    <td style={{ ...td, color: "var(--color-text-secondary)" }}>{s.owner}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ))}
      </div>
    </div>
  );
}

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}
