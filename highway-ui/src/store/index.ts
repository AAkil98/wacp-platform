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

export interface WorkspaceView {
  id: string;
  state: string;
  role: string;
  parent: string;
  owner: string;
  originator: string;
  taskId: string;
  checkpointCount: number;
}

export interface TaskView {
  id: string;
  name: string;
  status: string;
  workspaceRef: string;
  dependsOn: string[];
  parentTask: string;
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
  };
  appendTrailEntry: (entry: TrailEntry) => void;
  setTrailPaused: (paused: boolean) => void;

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
  };
  upsertWorkspace: (view: WorkspaceView) => void;
  addWorkspaceChange: (change: { workspaceId: string; previous: string; current: string; timestamp: bigint }) => void;

  // Task graph
  taskGraph: {
    tasks: TaskView[];
    lastFetched: number | null;
  };
  setTaskGraph: (tasks: TaskView[]) => void;
}

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
  trail: { entries: [], paused: false },
  appendTrailEntry: (entry) =>
    set((s) => {
      const entries = [...s.trail.entries, entry];
      if (entries.length > TRAIL_CAP) entries.splice(0, entries.length - TRAIL_CAP);
      return { trail: { ...s.trail, entries } };
    }),
  setTrailPaused: (paused) =>
    set((s) => ({ trail: { ...s.trail, paused } })),

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
  workspaces: { views: new Map(), changes: [] },
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

  // Task graph
  taskGraph: { tasks: [], lastFetched: null },
  setTaskGraph: (tasks) =>
    set(() => ({ taskGraph: { tasks, lastFetched: Date.now() } })),
}));
