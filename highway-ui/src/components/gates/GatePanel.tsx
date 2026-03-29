import { useStore } from "@/store";

export function GatePanel() {
  const pending = useStore((s) => s.gates.pending);

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Gates</h2>
      {pending.size === 0 ? (
        <p className="text-zinc-500 text-sm">No pending gates.</p>
      ) : (
        <div className="space-y-3">
          {[...pending.values()].map((gate) => (
            <div
              key={gate.gateId}
              className="bg-zinc-900 rounded-lg p-4 border border-zinc-800"
            >
              <div className="flex items-center gap-2 mb-2">
                <span className="text-xs font-medium px-2 py-0.5 rounded bg-amber-900 text-amber-200">
                  {gate.gateType}
                </span>
                <span className="text-zinc-500 text-xs">{gate.gateId}</span>
              </div>
              <div className="text-sm text-zinc-400">
                Workspace: {gate.workspaceId || "\u2014"} | Fallback:{" "}
                {gate.fallbackAction}
              </div>
              <div className="flex gap-2 mt-3">
                <button className="text-xs px-3 py-1 rounded bg-green-700 hover:bg-green-600 text-white">
                  Approve
                </button>
                <button className="text-xs px-3 py-1 rounded bg-red-700 hover:bg-red-600 text-white">
                  Reject
                </button>
                <button className="text-xs px-3 py-1 rounded bg-zinc-700 hover:bg-zinc-600 text-white">
                  Modify
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
