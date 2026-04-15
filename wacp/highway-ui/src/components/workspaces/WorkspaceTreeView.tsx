import { useMemo } from "react";
import { useNavigate } from "react-router";
import { useStore, selectWorkspaceTree, type WorkspaceTreeNode } from "@/store";

const stateColors: Record<string, string> = {
  WORKSPACE_STATE_ACTIVE: "bg-green-500",
  WORKSPACE_STATE_BLOCKED: "bg-amber-500",
  WORKSPACE_STATE_SUSPENDED: "bg-amber-500",
  WORKSPACE_STATE_INTEGRATING: "bg-blue-500",
  WORKSPACE_STATE_MIGRATING: "bg-blue-500",
  WORKSPACE_STATE_CONFLICTED: "bg-orange-500",
  WORKSPACE_STATE_FAILED: "bg-red-500",
  WORKSPACE_STATE_CLOSED: "bg-zinc-500",
  WORKSPACE_STATE_IDLE: "bg-zinc-600",
};

const stateLabels: Record<string, string> = {
  WORKSPACE_STATE_ACTIVE: "active",
  WORKSPACE_STATE_BLOCKED: "blocked",
  WORKSPACE_STATE_SUSPENDED: "suspended",
  WORKSPACE_STATE_INTEGRATING: "integrating",
  WORKSPACE_STATE_MIGRATING: "migrating",
  WORKSPACE_STATE_CONFLICTED: "conflicted",
  WORKSPACE_STATE_FAILED: "failed",
  WORKSPACE_STATE_CLOSED: "closed",
  WORKSPACE_STATE_IDLE: "idle",
};

export function WorkspaceTreeView() {
  const workspaces = useStore((s) => s.workspaces);
  const gates = useStore((s) => s.gates);
  const escalations = useStore((s) => s.escalations);
  const toggleNodeExpanded = useStore((s) => s.toggleNodeExpanded);

  const tree = useMemo(() => selectWorkspaceTree(workspaces), [workspaces]);

  const attentionIds = useMemo(() => {
    const ids = new Set<string>();
    for (const gate of gates.pending.values()) {
      if (gate.workspaceId) ids.add(gate.workspaceId);
    }
    for (const esc of escalations.active.values()) {
      if (esc.workspaceId) ids.add(esc.workspaceId);
    }
    return ids;
  }, [gates.pending, escalations.active]);

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">Workspaces</h2>
      {tree.length === 0 ? (
        <p className="text-zinc-500 text-sm">No workspaces.</p>
      ) : (
        <div className="space-y-0.5">
          {tree.map((node) => (
            <TreeNode
              key={node.workspace.id}
              node={node}
              depth={0}
              attentionIds={attentionIds}
              onToggle={toggleNodeExpanded}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface TreeNodeProps {
  node: WorkspaceTreeNode;
  depth: number;
  attentionIds: Set<string>;
  onToggle: (id: string) => void;
}

function TreeNode({ node, depth, attentionIds, onToggle }: TreeNodeProps) {
  const navigate = useNavigate();
  const { workspace: ws, children, expanded } = node;

  const hasChildren = children.length > 0;
  const needsAttention = attentionIds.has(ws.id);

  // Auto-expand: if any descendant needs attention, show as expanded
  const hasAttentionDescendant = hasChildren && hasDescendantInSet(node, attentionIds);
  const isExpanded = expanded || hasAttentionDescendant;

  const dotColor = stateColors[ws.state] ?? "bg-zinc-500";
  const stateLabel = stateLabels[ws.state] ?? ws.state;

  // Resource bar (tokens)
  let usagePercent: number | null = null;
  let usageWarning = false;
  if (ws.currentUsage && ws.budget && ws.budget.maxTokens > 0n) {
    usagePercent = Number((ws.currentUsage.tokens * 100n) / ws.budget.maxTokens);
    usageWarning = usagePercent >= ws.budget.warningThreshold * 100;
  }

  return (
    <>
      <div
        className={`flex items-center gap-2 py-1.5 px-2 rounded hover:bg-zinc-800 cursor-pointer group ${
          needsAttention ? "ring-1 ring-amber-500/50" : ""
        }`}
        style={{ paddingLeft: `${depth * 20 + 8}px` }}
        onClick={() => navigate(`/workspaces/${ws.id}`)}
      >
        {/* Expand/collapse chevron */}
        <button
          onClick={(e) => {
            e.stopPropagation();
            if (hasChildren) onToggle(ws.id);
          }}
          className={`w-4 h-4 flex items-center justify-center text-zinc-500 ${
            hasChildren ? "hover:text-zinc-300" : "invisible"
          }`}
        >
          {hasChildren && (
            <svg
              className={`w-3 h-3 transition-transform ${isExpanded ? "rotate-90" : ""}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
            </svg>
          )}
        </button>

        {/* State dot */}
        <span className={`w-2 h-2 rounded-full shrink-0 ${dotColor}`} />

        {/* Workspace ID */}
        <span className="font-mono text-sm truncate" title={ws.id}>
          {ws.id.length > 12 ? ws.id.slice(0, 12) + "\u2026" : ws.id}
        </span>

        {/* State label */}
        <span className="text-[10px] text-zinc-500">{stateLabel}</span>

        {/* Role badge */}
        {ws.role && (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-zinc-800 text-zinc-400">
            {ws.role}
          </span>
        )}

        {/* Right side: owner, task, resource bar */}
        <div className="ml-auto flex items-center gap-3 text-xs text-zinc-500">
          {/* Task binding */}
          {ws.taskId && (
            <span className="text-zinc-500" title={`Task: ${ws.taskId}`}>
              {ws.taskId.slice(0, 8)}
            </span>
          )}

          {/* Resource bar */}
          {usagePercent !== null && (
            <div className="w-16 h-1.5 bg-zinc-800 rounded-full overflow-hidden" title={`Tokens: ${usagePercent.toFixed(0)}%`}>
              <div
                className={`h-full rounded-full ${usageWarning ? "bg-amber-500" : "bg-blue-500"}`}
                style={{ width: `${Math.min(100, usagePercent)}%` }}
              />
            </div>
          )}

          {/* Owner */}
          <span className="hidden group-hover:inline">{ws.owner}</span>
        </div>
      </div>

      {/* Children */}
      {isExpanded &&
        children.map((child) => (
          <TreeNode
            key={child.workspace.id}
            node={child}
            depth={depth + 1}
            attentionIds={attentionIds}
            onToggle={onToggle}
          />
        ))}
    </>
  );
}

/** Check if any descendant workspace ID is in the given set. */
function hasDescendantInSet(node: WorkspaceTreeNode, ids: Set<string>): boolean {
  for (const child of node.children) {
    if (ids.has(child.workspace.id)) return true;
    if (hasDescendantInSet(child, ids)) return true;
  }
  return false;
}
