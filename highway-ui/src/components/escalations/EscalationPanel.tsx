import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { useStore, type ActiveEscalation } from "@/store";
import { respondToEscalation } from "@/transport/rpcs.js";

export function EscalationPanel() {
  const escalations = useStore((s) => s.escalations);
  const navigate = useNavigate();

  const sortedEscalations = useMemo(
    () => [...escalations.active.values()].sort((a, b) =>
      Number(a.createdAt - b.createdAt),
    ),
    [escalations.active],
  );

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Escalations</h2>
      {sortedEscalations.length === 0 ? (
        <p className="text-zinc-500 text-sm">No active escalations.</p>
      ) : (
        <div className="space-y-3">
          {sortedEscalations.map((esc) => (
            <EscalationCard
              key={esc.escalationId}
              escalation={esc}
              isInFlight={escalations.inFlight.has(esc.escalationId)}
              onNavigate={navigate}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ── Escalation Card ──

interface EscalationCardProps {
  escalation: ActiveEscalation;
  isInFlight: boolean;
  onNavigate: (path: string) => void;
}

function EscalationCard({ escalation: esc, isInFlight, onNavigate }: EscalationCardProps) {
  const [confirmAbort, setConfirmAbort] = useState(false);
  const [ackResult, setAckResult] = useState<boolean | null>(null);

  // Clear ack result after brief flash
  useEffect(() => {
    if (ackResult !== null) {
      const timer = setTimeout(() => setAckResult(null), 1500);
      return () => clearTimeout(timer);
    }
  }, [ackResult]);

  const handleSendFeedback = useCallback(() => {
    onNavigate(`/inject?workspace=${encodeURIComponent(esc.workspaceId)}&type=feedback`);
  }, [esc.workspaceId, onNavigate]);

  const handleAbort = useCallback(async () => {
    try {
      const result = await respondToEscalation(esc.escalationId, { type: "abort" });
      setAckResult(result.applied);
    } catch {
      // RPC failed
    }
    setConfirmAbort(false);
  }, [esc.escalationId]);

  const handleDelegate = useCallback(async () => {
    try {
      const result = await respondToEscalation(esc.escalationId, { type: "delegate" });
      setAckResult(result.applied);
    } catch {
      // RPC failed
    }
  }, [esc.escalationId]);

  const relativeTime = useMemo(() => {
    const ms = Number(esc.createdAt / 1000n);
    const diff = Date.now() - ms;
    if (diff < 1000) return "just now";
    if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
    return `${Math.floor(diff / 3_600_000)}h ago`;
  }, [esc.createdAt]);

  return (
    <div className={`bg-zinc-900 rounded-lg p-4 border ${isInFlight ? "border-blue-700" : "border-red-900"}`}>
      {/* Header */}
      <div className="flex items-center gap-2 mb-2">
        <button
          onClick={() => onNavigate(`/workspaces/${esc.workspaceId}`)}
          className="text-sm font-medium text-blue-400 hover:text-blue-300 font-mono"
        >
          {esc.workspaceId}
        </button>
        <span className="text-xs text-zinc-500">{relativeTime}</span>
        <span className="text-xs text-zinc-600 ml-auto">owner: {esc.owner}</span>
      </div>

      {/* Context */}
      <div className="text-sm text-zinc-300 mb-3 whitespace-pre-wrap bg-zinc-800 rounded p-3 max-h-32 overflow-auto">
        {esc.context || "(no context provided)"}
      </div>

      {/* View workspace link */}
      <button
        onClick={() => onNavigate(`/workspaces/${esc.workspaceId}`)}
        className="text-xs text-zinc-500 hover:text-zinc-300 mb-3 block"
      >
        View workspace details
      </button>

      {/* Ack feedback */}
      {ackResult !== null && (
        <div className={`text-xs mb-2 ${ackResult ? "text-green-400" : "text-amber-400"}`}>
          {ackResult ? "Response applied" : "Escalation already resolved"}
        </div>
      )}

      {/* Abort confirmation dialog */}
      {confirmAbort && (
        <div className="bg-red-950 border border-red-800 rounded p-3 mb-3">
          <p className="text-sm text-red-200 mb-2">
            This will fail the workspace. The agent's work will be lost. Continue?
          </p>
          <div className="flex gap-2">
            <button
              onClick={handleAbort}
              disabled={isInFlight}
              className="text-xs px-3 py-1 rounded bg-red-700 hover:bg-red-600 disabled:bg-zinc-700 disabled:text-zinc-500 text-white"
            >
              Confirm Abort
            </button>
            <button
              onClick={() => setConfirmAbort(false)}
              className="text-xs px-3 py-1 rounded bg-zinc-700 hover:bg-zinc-600 text-white"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Action buttons */}
      <div className="flex gap-2">
        <button
          onClick={handleSendFeedback}
          disabled={isInFlight}
          className="text-xs px-3 py-1 rounded bg-blue-700 hover:bg-blue-600 disabled:bg-zinc-700 disabled:text-zinc-500 text-white"
        >
          Send Feedback
        </button>
        <button
          onClick={() => setConfirmAbort(true)}
          disabled={isInFlight || confirmAbort}
          className="text-xs px-3 py-1 rounded bg-red-700 hover:bg-red-600 disabled:bg-zinc-700 disabled:text-zinc-500 text-white"
        >
          {isInFlight ? "..." : "Abort Workspace"}
        </button>
        <button
          onClick={handleDelegate}
          disabled={isInFlight}
          className="text-xs px-3 py-1 rounded bg-zinc-700 hover:bg-zinc-600 disabled:text-zinc-500 text-white"
        >
          Delegate
        </button>
      </div>
    </div>
  );
}
