import { useStore } from "@/store";

const statusColors: Record<string, string> = {
  TASK_STATUS_DRAFT: "text-zinc-500",
  TASK_STATUS_PENDING: "text-blue-400",
  TASK_STATUS_ASSIGNED: "text-cyan-400",
  TASK_STATUS_IN_PROGRESS: "text-green-400",
  TASK_STATUS_COMPLETED: "text-green-600",
  TASK_STATUS_FAILED: "text-red-400",
  TASK_STATUS_INTEGRATED: "text-purple-400",
  TASK_STATUS_CANCELLED: "text-zinc-600 line-through",
};

export function TaskGraphView() {
  const tasks = useStore((s) => s.taskGraph.tasks);

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Task Graph</h2>
      {tasks.length === 0 ? (
        <p className="text-zinc-500 text-sm">No tasks.</p>
      ) : (
        <div className="space-y-2">
          {tasks.map((task) => (
            <div
              key={task.id}
              className="bg-zinc-900 rounded p-3 border border-zinc-800"
            >
              <div className="flex items-center gap-2">
                <span className={`text-sm font-medium ${statusColors[task.status] ?? ""}`}>
                  {task.name}
                </span>
                <span className="text-xs text-zinc-500">{task.status}</span>
              </div>
              {task.dependsOn.length > 0 && (
                <div className="text-xs text-zinc-500 mt-1">
                  depends on: {task.dependsOn.join(", ")}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
