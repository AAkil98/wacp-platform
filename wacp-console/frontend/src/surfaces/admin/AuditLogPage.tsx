import type React from "react";
import { useState } from "react";
import { useAuditLog } from "../../api/hooks/index";
import { EmptyState } from "../../components/EmptyState";

// --- Styles ---

const page: React.CSSProperties = {
  padding: 24,
  color: "var(--color-text)",
  background: "var(--color-bg)",
};

const filterRow: React.CSSProperties = {
  display: "flex",
  gap: 12,
  marginBottom: 20,
  alignItems: "flex-end",
  flexWrap: "wrap",
};

const filterGroup: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
};

const filterLabel: React.CSSProperties = {
  fontSize: 12,
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

const inputStyle: React.CSSProperties = {
  padding: "8px 10px",
  border: "1px solid var(--color-border)",
  borderRadius: 4,
  background: "var(--color-bg)",
  color: "var(--color-text)",
  fontSize: 14,
};

const table: React.CSSProperties = {
  width: "100%",
  borderCollapse: "collapse",
  fontSize: 14,
};

const th: React.CSSProperties = {
  textAlign: "left",
  padding: "10px 12px",
  borderBottom: "2px solid var(--color-border)",
  fontWeight: 700,
  fontSize: 13,
  color: "var(--color-text-secondary)",
};

const td: React.CSSProperties = {
  padding: "10px 12px",
  borderBottom: "1px solid var(--color-border)",
  verticalAlign: "top",
};

const expandBtn: React.CSSProperties = {
  background: "none",
  border: "none",
  cursor: "pointer",
  color: "var(--color-accent)",
  fontSize: 13,
  padding: 0,
  textDecoration: "underline",
};

const detailBox: React.CSSProperties = {
  marginTop: 8,
  padding: 12,
  background: "var(--color-bg-secondary)",
  border: "1px solid var(--color-border)",
  borderRadius: 4,
  fontFamily: "monospace",
  fontSize: 12,
  whiteSpace: "pre-wrap",
  wordBreak: "break-all",
};

// --- Types ---

interface AuditEntry {
  id: string;
  timestamp: string;
  user_id: string;
  action: string;
  target_kind: string;
  target_id: string;
  detail: unknown;
}

const ACTIONS = [
  "",
  "login",
  "logout",
  "create_profile",
  "update_profile",
  "delete_profile",
  "clone_profile",
  "import_profile",
  "create_user",
  "update_user",
  "change_setting",
  "create_session",
  "approve_checkpoint",
  "reject_checkpoint",
] as const;

export function AuditLogPage() {
  const [userId, setUserId] = useState("");
  const [action, setAction] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const params = {
    user_id: userId || undefined,
    action: action || undefined,
    limit: 100,
  };

  const auditQuery = useAuditLog(params);
  const entries = (auditQuery.data ?? []) as AuditEntry[];

  function toggleExpand(id: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  return (
    <div style={page}>
      <h1 style={{ margin: "0 0 20px", fontSize: 24, fontWeight: 700 }}>Audit Log</h1>

      {/* Filters */}
      <div style={filterRow}>
        <div style={filterGroup}>
          <span style={filterLabel}>User ID</span>
          <input
            style={{ ...inputStyle, width: 200 }}
            placeholder="Filter by user..."
            value={userId}
            onChange={(e) => setUserId(e.target.value)}
          />
        </div>
        <div style={filterGroup}>
          <span style={filterLabel}>Action</span>
          <select
            style={{ ...inputStyle, width: 200 }}
            value={action}
            onChange={(e) => setAction(e.target.value)}
          >
            <option value="">All actions</option>
            {ACTIONS.filter(Boolean).map((a) => (
              <option key={a} value={a}>{a}</option>
            ))}
          </select>
        </div>
      </div>

      {auditQuery.isLoading ? (
        <p style={{ color: "var(--color-text-secondary)" }}>Loading audit log...</p>
      ) : entries.length === 0 ? (
        <EmptyState title="No audit log entries found." />
      ) : (
        <table style={table}>
          <thead>
            <tr>
              <th style={th}>Timestamp</th>
              <th style={th}>User</th>
              <th style={th}>Action</th>
              <th style={th}>Target Kind</th>
              <th style={th}>Target ID</th>
              <th style={th}>Detail</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((entry) => (
              <tr key={entry.id}>
                <td style={td}>{entry.timestamp}</td>
                <td style={td}>{entry.user_id}</td>
                <td style={td}>{entry.action}</td>
                <td style={td}>{entry.target_kind}</td>
                <td style={td}>{entry.target_id}</td>
                <td style={td}>
                  {entry.detail != null ? (
                    <>
                      <button style={expandBtn} onClick={() => toggleExpand(entry.id)}>
                        {expanded.has(entry.id) ? "Hide" : "Show"}
                      </button>
                      {expanded.has(entry.id) && (
                        <div style={detailBox}>
                          {JSON.stringify(entry.detail, null, 2)}
                        </div>
                      )}
                    </>
                  ) : (
                    <span style={{ color: "var(--color-text-secondary)" }}>--</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
