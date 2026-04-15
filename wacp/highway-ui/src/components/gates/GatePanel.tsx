import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { useStore, type PendingGate } from "@/store";
import { respondToGate, GateDecision } from "@/transport/rpcs.js";

// ── Gate type display ──

const GATE_TYPE_LABELS: Record<string, string> = {
  task_approval: "Task Approval",
  workspace_create: "Workspace Create",
  envelope_delivery: "Envelope Delivery",
  integration: "Integration",
  conflict_resolution: "Conflict Resolution",
  workspace_abort: "Workspace Abort",
};

const GATE_TYPE_COLORS: Record<string, string> = {
  task_approval: "bg-indigo-700 text-indigo-100",
  workspace_create: "bg-blue-700 text-blue-100",
  envelope_delivery: "bg-cyan-700 text-cyan-100",
  integration: "bg-emerald-700 text-emerald-100",
  conflict_resolution: "bg-orange-700 text-orange-100",
  workspace_abort: "bg-red-700 text-red-100",
};

export function GatePanel() {
  const gates = useStore((s) => s.gates);
  const navigate = useNavigate();

  const sortedGates = useMemo(() => {
    const now = Date.now() * 1000; // microseconds
    return [...gates.pending.values()].sort((a, b) => {
      // No-timeout gates sort to bottom
      if (a.timeoutMs === 0n && b.timeoutMs !== 0n) return 1;
      if (a.timeoutMs !== 0n && b.timeoutMs === 0n) return -1;
      if (a.timeoutMs === 0n && b.timeoutMs === 0n) return 0;

      const remainA = Number(a.createdAt) + Number(a.timeoutMs) * 1000 - now;
      const remainB = Number(b.createdAt) + Number(b.timeoutMs) * 1000 - now;
      return remainA - remainB;
    });
  }, [gates.pending]);

  // Batch approval: count task_approval gates
  const taskApprovalGates = useMemo(
    () => sortedGates.filter((g) => g.gateType === "task_approval"),
    [sortedGates],
  );
  const showBatchBar = taskApprovalGates.length > 1;

  const handleBatchApprove = useCallback(async () => {
    await Promise.allSettled(
      taskApprovalGates.map((g) => respondToGate(g.gateId, GateDecision.APPROVE)),
    );
  }, [taskApprovalGates]);

  const handleBatchReject = useCallback(async () => {
    await Promise.allSettled(
      taskApprovalGates.map((g) => respondToGate(g.gateId, GateDecision.REJECT)),
    );
  }, [taskApprovalGates]);

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Gates</h2>

      {/* Batch approval bar */}
      {showBatchBar && (
        <div className="flex items-center gap-3 mb-4 p-3 bg-zinc-900 rounded-lg border border-zinc-800">
          <span className="text-sm text-zinc-400">
            {taskApprovalGates.length} task approvals pending
          </span>
          <div className="ml-auto flex gap-2">
            <button
              onClick={handleBatchApprove}
              className="text-xs px-3 py-1 rounded bg-green-700 hover:bg-green-600 text-white"
            >
              Approve All
            </button>
            <button
              onClick={handleBatchReject}
              className="text-xs px-3 py-1 rounded bg-red-700 hover:bg-red-600 text-white"
            >
              Reject All
            </button>
          </div>
        </div>
      )}

      {sortedGates.length === 0 ? (
        <p className="text-zinc-500 text-sm">No pending gates.</p>
      ) : (
        <div className="space-y-3">
          {sortedGates.map((gate) => (
            <GateCard
              key={gate.gateId}
              gate={gate}
              isInFlight={gates.inFlight.has(gate.gateId)}
              onNavigate={navigate}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ── Gate Card ──

interface GateCardProps {
  gate: PendingGate;
  isInFlight: boolean;
  onNavigate: (path: string) => void;
}

function GateCard({ gate, isInFlight, onNavigate }: GateCardProps) {
  const [modifyOpen, setModifyOpen] = useState(false);
  const [modifyText, setModifyText] = useState("");
  const [ackResult, setAckResult] = useState<boolean | null>(null);

  // Clear ack result after brief flash
  useEffect(() => {
    if (ackResult !== null) {
      const timer = setTimeout(() => setAckResult(null), 1500);
      return () => clearTimeout(timer);
    }
  }, [ackResult]);

  const handleAction = async (decision: GateDecision, modifications?: Uint8Array) => {
    try {
      const result = await respondToGate(gate.gateId, decision, modifications);
      setAckResult(result.applied);
    } catch {
      // RPC failed — gate remains pending, inFlight flag is stuck.
      // In production, the session manager reconnection would clean this up.
    }
  };

  const handleModifySubmit = () => {
    const encoded = new TextEncoder().encode(modifyText);
    handleAction(GateDecision.MODIFY, encoded);
    setModifyOpen(false);
  };

  // Subject: decode bytes to text
  const subjectText = useMemo(() => {
    if (gate.subject.length === 0) return "";
    try {
      return new TextDecoder("utf-8", { fatal: true }).decode(gate.subject);
    } catch {
      return `(${gate.subject.length} bytes)`;
    }
  }, [gate.subject]);

  const typeLabel = GATE_TYPE_LABELS[gate.gateType] ?? gate.gateType;
  const typeColor = GATE_TYPE_COLORS[gate.gateType] ?? "bg-zinc-700 text-zinc-300";

  return (
    <div className={`bg-zinc-900 rounded-lg p-4 border ${isInFlight ? "border-blue-700" : "border-zinc-800"}`}>
      {/* Header row */}
      <div className="flex items-center gap-2 mb-2">
        <span className={`text-xs font-medium px-2 py-0.5 rounded ${typeColor}`}>
          {typeLabel}
        </span>
        <Countdown createdAt={gate.createdAt} timeoutMs={gate.timeoutMs} />
        <span className="text-zinc-600 text-xs ml-auto">{gate.gateId.slice(0, 12)}</span>
      </div>

      {/* Fallback label */}
      <div className="text-xs text-zinc-500 mb-2">
        Fallback: {gate.fallbackAction}
      </div>

      {/* Subject */}
      {subjectText && (
        <div className="text-sm text-zinc-300 mb-2 font-mono bg-zinc-800 rounded p-2 max-h-24 overflow-auto whitespace-pre-wrap break-all">
          {subjectText}
        </div>
      )}

      {/* Context links */}
      <div className="flex gap-3 text-xs text-zinc-500 mb-3">
        {gate.workspaceId && (
          <button
            onClick={() => onNavigate(`/workspaces/${gate.workspaceId}`)}
            className="text-blue-400 hover:text-blue-300"
          >
            Workspace: {gate.workspaceId.slice(0, 12)}
          </button>
        )}
        {gate.taskId && (
          <button
            onClick={() => onNavigate("/tasks")}
            className="text-blue-400 hover:text-blue-300"
          >
            Task: {gate.taskId.slice(0, 12)}
          </button>
        )}
      </div>

      {/* Ack feedback */}
      {ackResult !== null && (
        <div className={`text-xs mb-2 ${ackResult ? "text-green-400" : "text-amber-400"}`}>
          {ackResult ? "Response applied" : "Gate already resolved"}
        </div>
      )}

      {/* Action buttons */}
      <div className="flex gap-2">
        <button
          onClick={() => handleAction(GateDecision.APPROVE)}
          disabled={isInFlight}
          className="text-xs px-3 py-1 rounded bg-green-700 hover:bg-green-600 disabled:bg-zinc-700 disabled:text-zinc-500 text-white"
        >
          {isInFlight ? "..." : "Approve"}
        </button>
        <button
          onClick={() => handleAction(GateDecision.REJECT)}
          disabled={isInFlight}
          className="text-xs px-3 py-1 rounded bg-red-700 hover:bg-red-600 disabled:bg-zinc-700 disabled:text-zinc-500 text-white"
        >
          Reject
        </button>
        <button
          onClick={() => setModifyOpen(!modifyOpen)}
          disabled={isInFlight}
          className="text-xs px-3 py-1 rounded bg-zinc-700 hover:bg-zinc-600 disabled:text-zinc-500 text-white"
        >
          Modify
        </button>
      </div>

      {/* Modify editor */}
      {modifyOpen && (
        <div className="mt-3 space-y-2">
          <textarea
            value={modifyText}
            onChange={(e) => setModifyText(e.target.value)}
            placeholder="Modifications (text)"
            rows={3}
            className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-xs text-white font-mono"
          />
          <button
            onClick={handleModifySubmit}
            disabled={!modifyText.trim()}
            className="text-xs px-3 py-1 rounded bg-blue-700 hover:bg-blue-600 disabled:bg-zinc-700 disabled:text-zinc-500 text-white"
          >
            Submit Modification
          </button>
        </div>
      )}
    </div>
  );
}

// ── Countdown Timer ──

function Countdown({ createdAt, timeoutMs }: { createdAt: bigint; timeoutMs: bigint }) {
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    if (timeoutMs === 0n) return;
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, [timeoutMs]);

  if (timeoutMs === 0n) {
    return <span className="text-xs text-zinc-500">indefinite</span>;
  }

  const createdMs = Number(createdAt / 1000n); // createdAt is microseconds
  const timeoutTotal = Number(timeoutMs);
  const elapsed = now - createdMs;
  const remaining = Math.max(0, timeoutTotal - elapsed);
  const fraction = remaining / timeoutTotal;

  let colorClass: string;
  if (remaining <= 0) {
    colorClass = "text-red-400";
  } else if (remaining < 30_000) {
    colorClass = "text-red-400 animate-pulse";
  } else if (fraction < 0.1) {
    colorClass = "text-red-400";
  } else if (fraction < 0.5) {
    colorClass = "text-amber-400";
  } else {
    colorClass = "text-green-400";
  }

  const secs = Math.ceil(remaining / 1000);
  const mm = Math.floor(secs / 60);
  const ss = secs % 60;
  const display = remaining <= 0 ? "expired" : `${mm}:${ss.toString().padStart(2, "0")}`;

  return <span className={`text-xs font-mono ${colorClass}`}>{display}</span>;
}
