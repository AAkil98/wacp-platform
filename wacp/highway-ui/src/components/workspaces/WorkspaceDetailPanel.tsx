import { useParams, useNavigate } from "react-router";
import { useStore, type ResourceUsage, type ResourceBudget } from "@/store";
import { TrailViewer } from "@/components/trail/TrailViewer.js";

const stateColors: Record<string, string> = {
  WORKSPACE_STATE_ACTIVE: "bg-green-700 text-green-100",
  WORKSPACE_STATE_BLOCKED: "bg-amber-700 text-amber-100",
  WORKSPACE_STATE_SUSPENDED: "bg-amber-700 text-amber-100",
  WORKSPACE_STATE_INTEGRATING: "bg-blue-700 text-blue-100",
  WORKSPACE_STATE_MIGRATING: "bg-blue-700 text-blue-100",
  WORKSPACE_STATE_CONFLICTED: "bg-orange-700 text-orange-100",
  WORKSPACE_STATE_FAILED: "bg-red-700 text-red-100",
  WORKSPACE_STATE_CLOSED: "bg-zinc-600 text-zinc-200",
  WORKSPACE_STATE_IDLE: "bg-zinc-700 text-zinc-300",
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

export function WorkspaceDetailPanel() {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const navigate = useNavigate();
  const workspace = useStore((s) =>
    workspaceId ? s.workspaces.views.get(workspaceId) : undefined,
  );

  if (!workspaceId || !workspace) {
    return (
      <div className="p-4">
        <p className="text-zinc-500">Workspace not found: {workspaceId}</p>
        <button
          onClick={() => navigate("/workspaces")}
          className="mt-2 text-sm text-blue-400 hover:text-blue-300"
        >
          Back to workspaces
        </button>
      </div>
    );
  }

  const stateColor = stateColors[workspace.state] ?? "bg-zinc-700 text-zinc-300";
  const stateLabel = stateLabels[workspace.state] ?? workspace.state;

  return (
    <div className="flex flex-col h-full space-y-6">
      {/* Navigation */}
      <div>
        <button
          onClick={() => navigate("/workspaces")}
          className="text-sm text-zinc-500 hover:text-zinc-300"
        >
          Workspaces /
        </button>
      </div>

      {/* Header */}
      <div className="bg-zinc-900 rounded-lg p-4 border border-zinc-800">
        <div className="flex items-center gap-3 mb-3">
          <h2 className="text-lg font-semibold font-mono">{workspace.id}</h2>
          <span className={`text-xs px-2 py-0.5 rounded ${stateColor}`}>
            {stateLabel}
          </span>
        </div>
        <div className="grid grid-cols-2 gap-x-8 gap-y-1 text-sm">
          <Field label="Role" value={workspace.role} />
          <Field label="Owner" value={workspace.owner} />
          <Field label="Originator" value={workspace.originator} />
          <Field
            label="Task"
            value={workspace.taskId || "\u2014"}
            link={workspace.taskId ? `/tasks` : undefined}
          />
          <Field
            label="Created"
            value={workspace.createdAt ? formatTimestamp(workspace.createdAt) : "\u2014"}
          />
          <Field
            label="Last activity"
            value={workspace.lastActivity ? relativeTime(workspace.lastActivity) : "\u2014"}
          />
        </div>
      </div>

      {/* Resource meter */}
      <ResourceMeter usage={workspace.currentUsage} budget={workspace.budget} />

      {/* Checkpoints */}
      <div className="bg-zinc-900 rounded-lg p-4 border border-zinc-800">
        <h3 className="text-sm font-semibold mb-2">Checkpoints</h3>
        <p className="text-sm text-zinc-400">
          {workspace.checkpointCount} checkpoint{workspace.checkpointCount !== 1 ? "s" : ""} recorded
        </p>
      </div>

      {/* Actions */}
      <div className="flex gap-2">
        <button
          onClick={() => navigate(`/inject?workspace=${workspaceId}`)}
          className="text-sm px-3 py-1.5 rounded bg-blue-700 hover:bg-blue-600"
        >
          Inject Envelope
        </button>
      </div>

      {/* Scoped trail */}
      <div className="flex-1 min-h-0 bg-zinc-900 rounded-lg p-4 border border-zinc-800">
        <TrailViewer workspaceId={workspaceId} showFilters={false} />
      </div>
    </div>
  );
}

// ── Sub-components ──

function Field({
  label,
  value,
  link,
}: {
  label: string;
  value: string;
  link?: string;
}) {
  const navigate = useNavigate();
  return (
    <div className="flex">
      <span className="text-zinc-500 w-24 shrink-0">{label}</span>
      {link ? (
        <button
          onClick={() => navigate(link)}
          className="text-blue-400 hover:text-blue-300 font-mono text-sm truncate"
        >
          {value}
        </button>
      ) : (
        <span className="text-zinc-300 font-mono text-sm truncate">{value}</span>
      )}
    </div>
  );
}

interface ResourceMeterProps {
  usage?: ResourceUsage;
  budget?: ResourceBudget;
}

const RESOURCE_DIMENSIONS: ReadonlyArray<{
  label: string;
  usageKey: keyof ResourceUsage;
  budgetKey: keyof ResourceBudget;
  format?: (n: bigint) => string;
}> = [
  { label: "Tokens", usageKey: "tokens", budgetKey: "maxTokens" },
  { label: "Wall time", usageKey: "wallTimeMs", budgetKey: "maxWallTimeMs", format: formatMs },
  { label: "Storage", usageKey: "storageBytes", budgetKey: "maxStorageBytes", format: formatBytes },
  { label: "Network", usageKey: "networkBytes", budgetKey: "maxNetworkBytes", format: formatBytes },
  { label: "Cost", usageKey: "costMicros", budgetKey: "maxCostMicros", format: formatCost },
];

function ResourceMeter({ usage, budget }: ResourceMeterProps) {
  if (!budget) {
    return (
      <div className="bg-zinc-900 rounded-lg p-4 border border-zinc-800">
        <h3 className="text-sm font-semibold mb-2">Resources</h3>
        <p className="text-sm text-zinc-500">No budget assigned</p>
      </div>
    );
  }

  const threshold = budget.warningThreshold || 0.8;

  return (
    <div className="bg-zinc-900 rounded-lg p-4 border border-zinc-800">
      <h3 className="text-sm font-semibold mb-3">Resources</h3>
      <div className="space-y-2">
        {RESOURCE_DIMENSIONS.map(({ label, usageKey, budgetKey, format }) => {
          const used = usage ? (usage[usageKey] as bigint) : 0n;
          const max = budget[budgetKey] as bigint;
          const percent = max > 0n ? Number((used * 100n) / max) : 0;
          const warning = percent >= threshold * 100;
          const formatFn = format ?? formatBigint;

          return (
            <div key={label}>
              <div className="flex justify-between text-xs mb-0.5">
                <span className="text-zinc-400">{label}</span>
                <span className={warning ? "text-amber-400" : "text-zinc-500"}>
                  {formatFn(used)} / {formatFn(max)}
                </span>
              </div>
              <div className="w-full h-1.5 bg-zinc-800 rounded-full overflow-hidden">
                <div
                  className={`h-full rounded-full transition-all ${
                    percent >= 100
                      ? "bg-red-500"
                      : warning
                        ? "bg-amber-500"
                        : "bg-blue-500"
                  }`}
                  style={{ width: `${Math.min(100, percent)}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Formatters ──

function formatBigint(n: bigint): string {
  if (n >= 1_000_000n) return `${(Number(n) / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000n) return `${(Number(n) / 1_000).toFixed(1)}K`;
  return n.toString();
}

function formatMs(n: bigint): string {
  const s = Number(n) / 1000;
  if (s >= 3600) return `${(s / 3600).toFixed(1)}h`;
  if (s >= 60) return `${(s / 60).toFixed(1)}m`;
  return `${s.toFixed(1)}s`;
}

function formatBytes(n: bigint): string {
  const num = Number(n);
  if (num >= 1_073_741_824) return `${(num / 1_073_741_824).toFixed(1)} GB`;
  if (num >= 1_048_576) return `${(num / 1_048_576).toFixed(1)} MB`;
  if (num >= 1_024) return `${(num / 1_024).toFixed(1)} KB`;
  return `${num} B`;
}

function formatCost(n: bigint): string {
  const dollars = Number(n) / 1_000_000;
  return `$${dollars.toFixed(4)}`;
}

function formatTimestamp(physicalUs: bigint): string {
  const ms = Number(physicalUs / 1000n);
  return new Date(ms).toISOString().replace("T", " ").slice(0, 19);
}

function relativeTime(physicalUs: bigint): string {
  const ms = Number(physicalUs / 1000n);
  const diff = Date.now() - ms;
  if (diff < 1000) return "just now";
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}
