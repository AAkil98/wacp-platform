import { useStore } from "@/store";

export function TrailViewer() {
  const entries = useStore((s) => s.trail.entries);
  const paused = useStore((s) => s.trail.paused);
  const setPaused = useStore((s) => s.setTrailPaused);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold">Trail</h2>
        <button
          onClick={() => setPaused(!paused)}
          className="text-sm px-3 py-1 rounded bg-zinc-800 hover:bg-zinc-700"
        >
          {paused ? "Resume" : "Pause"}
        </button>
      </div>

      {entries.length === 0 ? (
        <div className="text-zinc-500 text-sm">No trail entries yet.</div>
      ) : (
        <div className="flex-1 overflow-auto font-mono text-xs">
          <table className="w-full">
            <thead className="sticky top-0 bg-zinc-950">
              <tr className="text-left text-zinc-500 border-b border-zinc-800">
                <th className="py-1 pr-3">Time</th>
                <th className="py-1 pr-3">Event</th>
                <th className="py-1 pr-3">Actor</th>
                <th className="py-1 pr-3">Workspace</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry, i) => (
                <tr
                  key={i}
                  className="border-b border-zinc-900 hover:bg-zinc-900"
                >
                  <td className="py-1 pr-3 text-zinc-500">
                    {entry.timestamp.toString()}
                  </td>
                  <td className="py-1 pr-3">{entry.eventType}</td>
                  <td className="py-1 pr-3 text-zinc-400">{entry.actor}</td>
                  <td className="py-1 pr-3 text-zinc-400">
                    {entry.workspaceId || "\u2014"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
