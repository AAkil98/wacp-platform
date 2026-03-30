import { create } from "zustand";

// ── Application types (domain types, not protobuf) ──

export type SessionState =
  | "disconnected"
  | "authenticating"
  | "connected"
  | "reconnecting";

export interface TrailEntry {
  timestamp: bigint;
  eventType: string;
  actor: string;
  workspaceId: string;
  body: string;
  sequenceNumber: bigint;
}

export interface PendingGate {
  gateId: string;
  gateType: string;
  subject: Uint8Array;
  workspaceId: string;
  taskId: string;
  timeoutMs: bigint;
  fallbackAction: string;
  createdAt: bigint;
}

export interface ActiveEscalation {
  escalationId: string;
  workspaceId: string;
  owner: string;
  context: string;
  createdAt: bigint;
}

export interface ResourceUsage {
  tokens: bigint;
  wallTimeMs: bigint;
  storageBytes: bigint;
  networkBytes: bigint;
  costMicros: bigint;
}

export interface ResourceBudget {
  maxTokens: bigint;
  maxWallTimeMs: bigint;
  maxStorageBytes: bigint;
  maxNetworkBytes: bigint;
  maxCostMicros: bigint;
  warningThreshold: number;
}

export interface WorkspaceView {
  id: string;
  state: string;
  role: string;
  parent: string;
  owner: string;
  originator: string;
  taskId: string;
  checkpointCount: number;
  currentUsage?: ResourceUsage;
  budget?: ResourceBudget;
  createdAt?: bigint;
  lastActivity?: bigint;
}

export interface TaskView {
  id: string;
  name: string;
  status: string;
  workspaceRef: string;
  dependsOn: string[];
  parentTask: string;
}

export interface TrailFilter {
  eventTypes: Set<string>;
  workspaceId: string;
  actor: string;
}

export interface WorkspaceTreeNode {
  workspace: WorkspaceView;
  children: WorkspaceTreeNode[];
  expanded: boolean;
}

// ── Selectors ──

export function selectFilteredTrail(trail: HighwayStore["trail"]): TrailEntry[] {
  const { entries } = trail;
  const { eventTypes, workspaceId, actor } = trail.filters;

  const hasEventFilter = eventTypes.size > 0;
  const hasWorkspaceFilter = workspaceId !== "";
  const hasActorFilter = actor !== "";

  if (!hasEventFilter && !hasWorkspaceFilter && !hasActorFilter) {
    return entries;
  }

  return entries.filter((e) => {
    if (hasEventFilter && !eventTypes.has(e.eventType)) return false;
    if (hasWorkspaceFilter && e.workspaceId !== workspaceId) return false;
    if (hasActorFilter && e.actor !== actor) return false;
    return true;
  });
}

export function selectWorkspaceTree(workspaces: HighwayStore["workspaces"]): WorkspaceTreeNode[] {
  const { views, expandedNodes } = workspaces;

  if (views.size === 0) return [];

  const childrenMap = new Map<string, WorkspaceView[]>();
  const roots: WorkspaceView[] = [];

  for (const ws of views.values()) {
    if (ws.parent === "" || !views.has(ws.parent)) {
      roots.push(ws);
    } else {
      let siblings = childrenMap.get(ws.parent);
      if (!siblings) {
        siblings = [];
        childrenMap.set(ws.parent, siblings);
      }
      siblings.push(ws);
    }
  }

  function buildNode(ws: WorkspaceView): WorkspaceTreeNode {
    const children = (childrenMap.get(ws.id) ?? []).map(buildNode);
    return {
      workspace: ws,
      children,
      expanded: expandedNodes.has(ws.id),
    };
  }

  return roots.map(buildNode);
}

// ── Store shape ──

const TRAIL_CAP = 10_000;
const CHANGES_CAP = 1_000;

export interface HighwayStore {
  // Session
  session: {
    state: SessionState;
    userId: string | null;
    capabilities: string[];
  };
  setSession: (state: SessionState, userId?: string, capabilities?: string[]) => void;

  // Trail
  trail: {
    entries: TrailEntry[];
    paused: boolean;
    filters: TrailFilter;
    expandedEntryIndex: number | null;
  };
  appendTrailEntry: (entry: TrailEntry) => void;
  setTrailPaused: (paused: boolean) => void;
  setTrailFilter: (filter: Partial<TrailFilter>) => void;
  clearTrailFilters: () => void;
  setExpandedEntry: (index: number | null) => void;

  // Gates
  gates: {
    pending: Map<string, PendingGate>;
    resolved: Map<string, { applied: boolean }>;
    inFlight: Set<string>;
  };
  addPendingGate: (gate: PendingGate) => void;
  markGateInFlight: (gateId: string) => void;
  resolveGate: (gateId: string, applied: boolean) => void;

  // Escalations
  escalations: {
    active: Map<string, ActiveEscalation>;
    resolved: Map<string, { applied: boolean }>;
    inFlight: Set<string>;
  };
  addEscalation: (esc: ActiveEscalation) => void;
  markEscalationInFlight: (id: string) => void;
  resolveEscalation: (id: string, applied: boolean) => void;

  // Workspaces
  workspaces: {
    views: Map<string, WorkspaceView>;
    changes: { workspaceId: string; previous: string; current: string; timestamp: bigint }[];
    expandedNodes: Set<string>;
  };
  upsertWorkspace: (view: WorkspaceView) => void;
  addWorkspaceChange: (change: { workspaceId: string; previous: string; current: string; timestamp: bigint }) => void;
  toggleNodeExpanded: (workspaceId: string) => void;

  // Task graph
  taskGraph: {
    tasks: TaskView[];
    lastFetched: number | null;
  };
  setTaskGraph: (tasks: TaskView[]) => void;
}

const emptyFilters: TrailFilter = {
  eventTypes: new Set(),
  workspaceId: "",
  actor: "",
};

export const useStore = create<HighwayStore>((set) => ({
  // Session
  session: { state: "disconnected", userId: null, capabilities: [] },
  setSession: (state, userId, capabilities) =>
    set((s) => ({
      session: {
        state,
        userId: userId ?? s.session.userId,
        capabilities: capabilities ?? s.session.capabilities,
      },
    })),

  // Trail
  trail: { entries: [], paused: false, filters: { ...emptyFilters, eventTypes: new Set() }, expandedEntryIndex: null },
  appendTrailEntry: (entry) =>
    set((s) => {
      const entries = [...s.trail.entries, entry];
      if (entries.length > TRAIL_CAP) entries.splice(0, entries.length - TRAIL_CAP);
      return { trail: { ...s.trail, entries } };
    }),
  setTrailPaused: (paused) =>
    set((s) => ({ trail: { ...s.trail, paused } })),
  setTrailFilter: (filter) =>
    set((s) => ({
      trail: {
        ...s.trail,
        filters: { ...s.trail.filters, ...filter },
      },
    })),
  clearTrailFilters: () =>
    set((s) => ({
      trail: {
        ...s.trail,
        filters: { eventTypes: new Set(), workspaceId: "", actor: "" },
      },
    })),
  setExpandedEntry: (index) =>
    set((s) => ({ trail: { ...s.trail, expandedEntryIndex: index } })),

  // Gates
  gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
  addPendingGate: (gate) =>
    set((s) => {
      const pending = new Map(s.gates.pending);
      pending.set(gate.gateId, gate);
      return { gates: { ...s.gates, pending } };
    }),
  markGateInFlight: (gateId) =>
    set((s) => {
      const inFlight = new Set(s.gates.inFlight);
      inFlight.add(gateId);
      return { gates: { ...s.gates, inFlight } };
    }),
  resolveGate: (gateId, applied) =>
    set((s) => {
      const pending = new Map(s.gates.pending);
      pending.delete(gateId);
      const inFlight = new Set(s.gates.inFlight);
      inFlight.delete(gateId);
      const resolved = new Map(s.gates.resolved);
      resolved.set(gateId, { applied });
      return { gates: { pending, inFlight, resolved } };
    }),

  // Escalations
  escalations: { active: new Map(), resolved: new Map(), inFlight: new Set() },
  addEscalation: (esc) =>
    set((s) => {
      const active = new Map(s.escalations.active);
      active.set(esc.escalationId, esc);
      return { escalations: { ...s.escalations, active } };
    }),
  markEscalationInFlight: (id) =>
    set((s) => {
      const inFlight = new Set(s.escalations.inFlight);
      inFlight.add(id);
      return { escalations: { ...s.escalations, inFlight } };
    }),
  resolveEscalation: (id, applied) =>
    set((s) => {
      const active = new Map(s.escalations.active);
      active.delete(id);
      const inFlight = new Set(s.escalations.inFlight);
      inFlight.delete(id);
      const resolved = new Map(s.escalations.resolved);
      resolved.set(id, { applied });
      return { escalations: { active, inFlight, resolved } };
    }),

  // Workspaces
  workspaces: { views: new Map(), changes: [], expandedNodes: new Set() },
  upsertWorkspace: (view) =>
    set((s) => {
      const views = new Map(s.workspaces.views);
      views.set(view.id, view);
      return { workspaces: { ...s.workspaces, views } };
    }),
  addWorkspaceChange: (change) =>
    set((s) => {
      const changes = [...s.workspaces.changes, change];
      if (changes.length > CHANGES_CAP) changes.splice(0, changes.length - CHANGES_CAP);
      return { workspaces: { ...s.workspaces, changes } };
    }),
  toggleNodeExpanded: (workspaceId) =>
    set((s) => {
      const expandedNodes = new Set(s.workspaces.expandedNodes);
      if (expandedNodes.has(workspaceId)) {
        expandedNodes.delete(workspaceId);
      } else {
        expandedNodes.add(workspaceId);
      }
      return { workspaces: { ...s.workspaces, expandedNodes } };
    }),

  // Task graph
  taskGraph: { tasks: [], lastFetched: null },
  setTaskGraph: (tasks) =>
    set(() => ({ taskGraph: { tasks, lastFetched: Date.now() } })),
}));
