import { useState } from "react";
import { useParams } from "react-router";
import { useSession } from "../../api/hooks";
import { useSessionStream } from "../../realtime/useSessionStream";
import { useSessionStore } from "../../store/session";
import { api } from "../../api/client";
import { TrailStream } from "./TrailStream";
import { GateQueue } from "./GateQueue";
import { EscalationInbox } from "./EscalationInbox";
import { RefusalPanel } from "./RefusalPanel";
import { WorkspaceTree } from "./WorkspaceTree";
import { InjectionBar } from "./InjectionBar";
import { QualityReport } from "./QualityReport";

const TABS = ["Trail", "Gates", "Escalations", "Refusals", "Workspaces", "Inject"] as const;
type Tab = (typeof TABS)[number];

const TERMINAL_STATES = ["session_completed", "session_cancelled", "session_failed"];

function isTerminal(status: string | null): boolean {
  return status != null && TERMINAL_STATES.includes(status);
}

function StateBadge({ state }: { state: string | null }) {
  const label = state ?? "connecting";
  const color = isTerminal(state)
    ? "var(--color-text-muted)"
    : state === "session_active"
      ? "var(--color-success)"
      : "var(--color-warning)";

  return (
    <span
      style={{
        display: "inline-block",
        padding: "2px 10px",
        fontSize: 12,
        fontWeight: 600,
        borderRadius: 12,
        border: `1px solid ${color}`,
        color,
      }}
    >
      {label.replace("session_", "")}
    </span>
  );
}

export function OversightPage() {
  const { id } = useParams<{ id: string }>();
  const sessionId = id ?? "";

  const [activeTab, setActiveTab] = useState<Tab>("Trail");
  const [cancelling, setCancelling] = useState(false);

  // Connect WebSocket stream
  useSessionStream({ sessionId, enabled: !!sessionId });

  // Static session detail
  const { data: sessionDetail } = useSession(sessionId);
  const detail = sessionDetail as {
    session_id?: string;
    name?: string;
    vertical?: string;
    workflow?: string;
    created_at?: string;
    ended_at?: string;
    state?: string;
  } | undefined;

  const sessionStatus = useSessionStore((s) => s.sessionStatus);
  const gates = useSessionStore((s) => s.gates);
  const escalations = useSessionStore((s) => s.escalations);

  const sessionName = detail?.name
    ?? (detail?.vertical && detail?.workflow
      ? `${detail.vertical}/${detail.workflow}`
      : sessionId);

  const terminal = isTerminal(sessionStatus);

  async function handleCancel() {
    if (cancelling) return;
    setCancelling(true);
    try {
      await api.post(`/api/sessions/${sessionId}/cancel`);
    } finally {
      setCancelling(false);
    }
  }

  return (
    <div style={{ padding: 24, color: "var(--color-text)" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 4 }}>
        <h1 style={{ fontSize: 24, fontWeight: 700, margin: 0 }}>{sessionName}</h1>
        <StateBadge state={sessionStatus} />
        {detail?.vertical && (
          <span
            style={{
              fontSize: 11,
              padding: "2px 8px",
              borderRadius: 4,
              background: "var(--color-bg-tertiary)",
              color: "var(--color-text-secondary)",
            }}
          >
            {detail.vertical}
          </span>
        )}
        {!terminal && (
          <button
            onClick={() => void handleCancel()}
            disabled={cancelling}
            style={{
              marginLeft: "auto",
              padding: "6px 16px",
              fontSize: 13,
              fontWeight: 600,
              color: "var(--color-danger)",
              background: "transparent",
              border: `1px solid var(--color-danger)`,
              borderRadius: 6,
              cursor: cancelling ? "not-allowed" : "pointer",
              opacity: cancelling ? 0.6 : 1,
            }}
          >
            {cancelling ? "Cancelling..." : "Cancel Session"}
          </button>
        )}
      </div>

      <p style={{ color: "var(--color-text-muted)", fontSize: 13, marginBottom: 20 }}>
        Session {sessionId}
      </p>

      {/* Terminal banner */}
      {terminal && (
        <div
          style={{
            padding: 16,
            marginBottom: 20,
            borderRadius: 8,
            background: "var(--color-bg-secondary)",
            border: "1px solid var(--color-border)",
          }}
        >
          <QualityReport sessionDetail={detail} sessionStatus={sessionStatus} />
        </div>
      )}

      {/* Tab bar */}
      <div
        style={{
          display: "flex",
          gap: 0,
          borderBottom: "2px solid var(--color-border)",
          marginBottom: 20,
        }}
      >
        {TABS.map((tab) => {
          let badge: number | null = null;
          if (tab === "Gates") badge = gates.length;
          if (tab === "Escalations") badge = escalations.length;

          return (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              style={{
                padding: "8px 20px",
                fontSize: 14,
                fontWeight: activeTab === tab ? 600 : 400,
                color: activeTab === tab ? "var(--color-accent)" : "var(--color-text-secondary)",
                background: "transparent",
                border: "none",
                borderBottom: activeTab === tab ? "2px solid var(--color-accent)" : "2px solid transparent",
                marginBottom: -2,
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                gap: 6,
              }}
            >
              {tab}
              {badge != null && badge > 0 && (
                <span
                  style={{
                    fontSize: 11,
                    fontWeight: 700,
                    padding: "1px 6px",
                    borderRadius: 10,
                    background: "var(--color-danger)",
                    color: "#fff",
                  }}
                >
                  {badge}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Tab content */}
      {activeTab === "Trail" && <TrailStream />}
      {activeTab === "Gates" && <GateQueue sessionId={sessionId} />}
      {activeTab === "Escalations" && <EscalationInbox sessionId={sessionId} />}
      {activeTab === "Refusals" && <RefusalPanel />}
      {activeTab === "Workspaces" && <WorkspaceTree />}
      {activeTab === "Inject" && <InjectionBar sessionId={sessionId} />}
    </div>
  );
}
