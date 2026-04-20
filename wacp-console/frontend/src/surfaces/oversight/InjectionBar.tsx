import { useState, useMemo } from "react";
import { useSessionStore } from "../../store/session";
import { api } from "../../api/client";

interface InjectionBarProps {
  sessionId: string;
}

export function InjectionBar({ sessionId }: InjectionBarProps) {
  const workspaces = useSessionStore((s) => s.workspaces);
  const [payload, setPayload] = useState("");
  const [workspaceId, setWorkspaceId] = useState("");
  const [sending, setSending] = useState(false);
  const [lastResult, setLastResult] = useState<string | null>(null);

  // Only active workspaces
  const activeWorkspaces = useMemo(() => {
    const result: Array<[string, string]> = [];
    for (const [wsId, state] of workspaces.entries()) {
      if (state.toUpperCase() === "ACTIVE") {
        result.push([wsId, state]);
      }
    }
    return result.sort(([a], [b]) => a.localeCompare(b));
  }, [workspaces]);

  async function handleSend() {
    if (!payload.trim() || !workspaceId || sending) return;
    setSending(true);
    setLastResult(null);
    try {
      await api.post(`/api/sessions/${sessionId}/inject`, {
        workspace_id: workspaceId,
        content: { text: payload },
      });
      setLastResult("Directive sent successfully.");
      setPayload("");
    } catch (err) {
      setLastResult(`Failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSending(false);
    }
  }

  return (
    <div style={{ maxWidth: 640 }}>
      <p style={{ fontSize: 13, color: "var(--color-text-secondary)", marginBottom: 16 }}>
        Inject a directive into an active workspace.
      </p>

      {/* Workspace selector */}
      <div style={{ marginBottom: 12 }}>
        <label
          htmlFor="inj-workspace"
          style={{ display: "block", fontSize: 12, fontWeight: 600, marginBottom: 4, color: "var(--color-text-secondary)" }}
        >
          Target workspace
        </label>
        <select
          id="inj-workspace"
          value={workspaceId}
          onChange={(e) => setWorkspaceId(e.target.value)}
          style={{
            width: "100%",
            padding: "8px 10px",
            fontSize: 13,
            border: "1px solid var(--color-border)",
            borderRadius: 6,
            background: "var(--color-bg)",
            color: "var(--color-text)",
          }}
        >
          <option value="">Select workspace...</option>
          {activeWorkspaces.map(([wsId]) => (
            <option key={wsId} value={wsId}>{wsId}</option>
          ))}
        </select>
        {activeWorkspaces.length === 0 && (
          <p style={{ fontSize: 12, color: "var(--color-text-muted)", marginTop: 4 }}>
            No active workspaces available.
          </p>
        )}
      </div>

      {/* Payload */}
      <div style={{ marginBottom: 12 }}>
        <label
          htmlFor="inj-payload"
          style={{ display: "block", fontSize: 12, fontWeight: 600, marginBottom: 4, color: "var(--color-text-secondary)" }}
        >
          Directive payload
        </label>
        <textarea
          id="inj-payload"
          placeholder="Enter directive text..."
          value={payload}
          onChange={(e) => setPayload(e.target.value)}
          rows={5}
          style={{
            width: "100%",
            padding: "8px 10px",
            fontSize: 13,
            border: "1px solid var(--color-border)",
            borderRadius: 6,
            background: "var(--color-bg)",
            color: "var(--color-text)",
            resize: "vertical",
            fontFamily: "inherit",
            boxSizing: "border-box",
          }}
        />
      </div>

      {/* Send button */}
      <button
        onClick={() => void handleSend()}
        disabled={sending || !payload.trim() || !workspaceId}
        style={{
          padding: "8px 24px",
          fontSize: 14,
          fontWeight: 600,
          color: "#fff",
          background: "var(--color-accent)",
          border: "none",
          borderRadius: 6,
          cursor: sending || !payload.trim() || !workspaceId ? "not-allowed" : "pointer",
          opacity: sending || !payload.trim() || !workspaceId ? 0.6 : 1,
        }}
      >
        {sending ? "Sending..." : "Send Directive"}
      </button>

      {/* Result feedback */}
      {lastResult && (
        <p
          style={{
            marginTop: 12,
            fontSize: 13,
            color: lastResult.startsWith("Failed") ? "var(--color-danger)" : "var(--color-success)",
          }}
        >
          {lastResult}
        </p>
      )}
    </div>
  );
}
