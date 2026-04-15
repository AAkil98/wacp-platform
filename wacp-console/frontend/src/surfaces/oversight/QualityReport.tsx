interface QualityReportProps {
  sessionDetail: {
    created_at?: string;
    ended_at?: string;
  } | undefined;
  sessionStatus: string | null;
}

function formatElapsed(createdAt: string | undefined, endedAt: string | undefined): string {
  if (!createdAt) return "unknown";
  const start = new Date(createdAt).getTime();
  const end = endedAt ? new Date(endedAt).getTime() : Date.now();
  const diff = Math.max(0, end - start);

  const secs = Math.floor(diff / 1000);
  const mins = Math.floor(secs / 60);
  const hours = Math.floor(mins / 60);
  const remMins = mins % 60;
  const remSecs = secs % 60;

  if (hours > 0) return `${hours}h ${remMins}m ${remSecs}s`;
  if (mins > 0) return `${remMins}m ${remSecs}s`;
  return `${remSecs}s`;
}

function statusLabel(status: string | null): string {
  switch (status) {
    case "session_completed": return "Completed";
    case "session_cancelled": return "Cancelled";
    case "session_failed": return "Failed";
    default: return status ?? "Unknown";
  }
}

function statusColor(status: string | null): string {
  switch (status) {
    case "session_completed": return "var(--color-success)";
    case "session_cancelled": return "var(--color-warning)";
    case "session_failed": return "var(--color-danger)";
    default: return "var(--color-text-muted)";
  }
}

export function QualityReport({ sessionDetail, sessionStatus }: QualityReportProps) {
  const elapsed = formatElapsed(sessionDetail?.created_at, sessionDetail?.ended_at);

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
      <span
        style={{
          fontSize: 16,
          fontWeight: 700,
          color: statusColor(sessionStatus),
        }}
      >
        Session {statusLabel(sessionStatus)}
      </span>
      <span style={{ fontSize: 13, color: "var(--color-text-secondary)" }}>
        Elapsed time: {elapsed}
      </span>
    </div>
  );
}
