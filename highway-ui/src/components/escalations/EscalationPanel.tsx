import { useStore } from "@/store";

export function EscalationPanel() {
  const active = useStore((s) => s.escalations.active);

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Escalations</h2>
      {active.size === 0 ? (
        <p className="text-zinc-500 text-sm">No active escalations.</p>
      ) : (
        <div className="space-y-3">
          {[...active.values()].map((esc) => (
            <div
              key={esc.escalationId}
              className="bg-zinc-900 rounded-lg p-4 border border-red-900"
            >
              <div className="text-sm font-medium mb-1">
                {esc.workspaceId}
              </div>
              <div className="text-zinc-400 text-sm mb-3 whitespace-pre-wrap">
                {esc.context}
              </div>
              <div className="flex gap-2">
                <button className="text-xs px-3 py-1 rounded bg-blue-700 hover:bg-blue-600 text-white">
                  Send Feedback
                </button>
                <button className="text-xs px-3 py-1 rounded bg-red-700 hover:bg-red-600 text-white">
                  Abort Workspace
                </button>
                <button className="text-xs px-3 py-1 rounded bg-zinc-700 hover:bg-zinc-600 text-white">
                  Delegate
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
