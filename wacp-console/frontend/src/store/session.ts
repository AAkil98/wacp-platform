/**
 * Zustand store for active session real-time state.
 * Fed by useSessionStream WebSocket events.
 *
 * Spec: wcon-sessions §6.1
 */

import { create } from "zustand";

interface TrailEntry {
  id: string;
  timestamp: string;
  workspace_id: string;
  workspace_label: string;
  event_type: string;
  summary: string;
  [key: string]: unknown;
}

interface GateEvent {
  gate_id: string;
  type: string;
  workspace_id: string;
  workspace_label: string;
  subject: Record<string, unknown>;
  timeout_at?: string;
  urgency: string;
  [key: string]: unknown;
}

interface EscalationEvent {
  escalation_id: string;
  workspace_id: string;
  workspace_label: string;
  reason: string;
  [key: string]: unknown;
}

interface RefusalEvent {
  refusal_id: string;
  workspace_id: string;
  workspace_label: string;
  tool_name: string;
  error_code: string;
  policy_kind: string;
  reason: string;
  unblock_hint: string;
  [key: string]: unknown;
}

interface WorkspaceState {
  workspace_id: string;
  state: string;
  [key: string]: unknown;
}

interface SessionState {
  // Session identity
  sessionId: string | null;

  // Real-time state
  trail: TrailEntry[];
  gates: GateEvent[];
  escalations: EscalationEvent[];
  refusals: RefusalEvent[];
  workspaces: Map<string, string>; // workspace_id → state
  sessionStatus: string | null; // session_active, session_completed, etc.

  // Buffer limit
  trailBufferSize: number;

  // Actions
  setSessionId: (id: string | null) => void;
  addTrailEntry: (entry: Record<string, unknown>) => void;
  addGate: (gate: Record<string, unknown>) => void;
  addEscalation: (esc: Record<string, unknown>) => void;
  addRefusal: (ref: Record<string, unknown>) => void;
  setWorkspaceState: (ws: Record<string, unknown>) => void;
  setSessionEvent: (event: Record<string, unknown>) => void;
  clearSession: () => void;
}

export const useSessionStore = create<SessionState>((set) => ({
  sessionId: null,
  trail: [],
  gates: [],
  escalations: [],
  refusals: [],
  workspaces: new Map(),
  sessionStatus: null,
  trailBufferSize: 1000,

  setSessionId: (id) => set({
    sessionId: id,
    trail: [],
    gates: [],
    escalations: [],
    refusals: [],
    workspaces: new Map(),
    sessionStatus: null,
  }),

  addTrailEntry: (entry) =>
    set((s) => {
      const newTrail = [...s.trail, entry as TrailEntry];
      // Keep buffer bounded
      if (newTrail.length > s.trailBufferSize) {
        newTrail.splice(0, newTrail.length - s.trailBufferSize);
      }
      return { trail: newTrail };
    }),

  addGate: (gate) =>
    set((s) => ({ gates: [...s.gates, gate as GateEvent] })),

  addEscalation: (esc) =>
    set((s) => ({ escalations: [...s.escalations, esc as EscalationEvent] })),

  addRefusal: (ref) =>
    set((s) => ({ refusals: [...s.refusals, ref as RefusalEvent] })),

  setWorkspaceState: (ws) =>
    set((s) => {
      const newMap = new Map(s.workspaces);
      const wsEvent = ws as WorkspaceState;
      newMap.set(wsEvent.workspace_id, wsEvent.state);
      return { workspaces: newMap };
    }),

  setSessionEvent: (event) =>
    set({ sessionStatus: (event as { type?: string }).type ?? null }),

  clearSession: () =>
    set({
      sessionId: null,
      trail: [],
      gates: [],
      escalations: [],
      refusals: [],
      workspaces: new Map(),
      sessionStatus: null,
    }),
}));
