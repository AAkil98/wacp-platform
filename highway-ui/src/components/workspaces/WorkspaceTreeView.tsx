import { useStore } from "@/store";

const stateColors: Record<string, string> = {
  WORKSPACE_STATE_ACTIVE: "bg-green-700",
  WORKSPACE_STATE_BLOCKED: "bg-amber-700",
  WORKSPACE_STATE_SUSPENDED: "bg-amber-700",
  WORKSPACE_STATE_INTEGRATING: "bg-blue-700",
  WORKSPACE_STATE_MIGRATING: "bg-blue-700",
  WORKSPACE_STATE_CONFLICTED: "bg-orange-700",
  WORKSPACE_STATE_FAILED: "bg-red-700",
  WORKSPACE_STATE_CLOSED: "bg-zinc-600",
  WORKSPACE_STATE_IDLE: "bg-zinc-700",
};

export function WorkspaceTreeView() {
  const views = useStore((s) => s.workspaces.views);

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Workspaces</h2>
      {views.size === 0 ? (
        <p className="text-zinc-500 text-sm">No workspaces.</p>
      ) : (
        <div className="space-y-2">
          {[...views.values()].map((ws) => (
            <div
              key={ws.id}
              className="bg-zinc-900 rounded p-3 border border-zinc-800 flex items-center gap-3"
            >
              <span
                className={`w-2 h-2 rounded-full ${stateColors[ws.state] ?? "bg-zinc-500"}`}
              />
              <span className="font-mono text-sm">{ws.id}</span>
              <span className="text-xs text-zinc-500">{ws.state}</span>
              <span className="text-xs text-zinc-500 ml-auto">
                owner: {ws.owner}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
