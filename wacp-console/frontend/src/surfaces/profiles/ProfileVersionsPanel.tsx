import type { VersionEntry } from "./ProfilesPage.types";

interface ProfileVersionsPanelProps {
  versions: VersionEntry[];
  isLoading: boolean;
}

export function ProfileVersionsPanel({ versions, isLoading }: ProfileVersionsPanelProps) {
  return (
    <div
      style={{
        marginTop: 16,
        padding: 16,
        border: "1px solid var(--color-border)",
        borderRadius: 6,
        background: "var(--color-bg-secondary)",
      }}
    >
      <h3 style={{ margin: "0 0 12px", fontSize: 16 }}>Version History</h3>
      {isLoading && (
        <p style={{ color: "var(--color-text-secondary)" }}>Loading versions...</p>
      )}
      {versions.length === 0 && !isLoading && (
        <p style={{ color: "var(--color-text-secondary)" }}>
          No version history available.
        </p>
      )}
      {versions.map((v) => (
        <div
          key={v.version}
          style={{
            padding: "8px 0",
            borderBottom: "1px solid var(--color-border)",
            fontSize: 13,
          }}
        >
          <span style={{ fontWeight: 600 }}>v{v.version}</span>
          <span style={{ marginLeft: 12, color: "var(--color-text-secondary)" }}>
            {v.created_at}
          </span>
          {v.summary && <span style={{ marginLeft: 12 }}>{v.summary}</span>}
        </div>
      ))}
    </div>
  );
}
