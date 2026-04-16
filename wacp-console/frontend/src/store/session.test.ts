import { describe, it, expect, beforeEach } from "vitest";
import { useSessionStore } from "./session";

const initialState = {
  sessionId: null,
  trail: [],
  gates: [],
  escalations: [],
  refusals: [],
  workspaces: new Map<string, string>(),
  sessionStatus: null,
  trailBufferSize: 1000,
};

function resetStore() {
  useSessionStore.setState({
    ...initialState,
    workspaces: new Map(),
  });
}

function makeTrailEntry(overrides: Record<string, unknown> = {}) {
  return {
    id: "t1",
    timestamp: "2026-04-16T00:00:00Z",
    workspace_id: "ws1",
    workspace_label: "workspace-1",
    event_type: "tool_call",
    summary: "Called tool X",
    ...overrides,
  };
}

function makeGate(overrides: Record<string, unknown> = {}) {
  return {
    gate_id: "g1",
    type: "confirmation",
    workspace_id: "ws1",
    workspace_label: "workspace-1",
    subject: { action: "delete-file" },
    urgency: "high",
    ...overrides,
  };
}

function makeEscalation(overrides: Record<string, unknown> = {}) {
  return {
    escalation_id: "e1",
    workspace_id: "ws1",
    workspace_label: "workspace-1",
    reason: "Exceeded budget",
    ...overrides,
  };
}

function makeRefusal(overrides: Record<string, unknown> = {}) {
  return {
    refusal_id: "r1",
    workspace_id: "ws1",
    workspace_label: "workspace-1",
    tool_name: "shell",
    error_code: "POLICY_DENIED",
    policy_kind: "deny-list",
    reason: "Command not allowed",
    unblock_hint: "Use a different command",
    ...overrides,
  };
}

describe("useSessionStore", () => {
  beforeEach(() => {
    resetStore();
  });

  // ---- Initial state ----
  describe("initial state", () => {
    it("has null sessionId", () => {
      expect(useSessionStore.getState().sessionId).toBeNull();
    });

    it("has empty trail", () => {
      expect(useSessionStore.getState().trail).toEqual([]);
    });

    it("has empty gates", () => {
      expect(useSessionStore.getState().gates).toEqual([]);
    });

    it("has empty escalations", () => {
      expect(useSessionStore.getState().escalations).toEqual([]);
    });

    it("has empty refusals", () => {
      expect(useSessionStore.getState().refusals).toEqual([]);
    });

    it("has empty workspaces map", () => {
      expect(useSessionStore.getState().workspaces).toEqual(new Map());
      expect(useSessionStore.getState().workspaces.size).toBe(0);
    });

    it("has null sessionStatus", () => {
      expect(useSessionStore.getState().sessionStatus).toBeNull();
    });

    it("has trailBufferSize of 1000", () => {
      expect(useSessionStore.getState().trailBufferSize).toBe(1000);
    });
  });

  // ---- setSessionId ----
  describe("setSessionId", () => {
    it("sets the session ID", () => {
      useSessionStore.getState().setSessionId("sess-abc");

      expect(useSessionStore.getState().sessionId).toBe("sess-abc");
    });

    it("resets trail, gates, escalations, refusals, workspaces, and sessionStatus", () => {
      // Populate some state first
      useSessionStore.setState({
        trail: [makeTrailEntry()],
        gates: [makeGate()],
        escalations: [makeEscalation()],
        refusals: [makeRefusal()],
        workspaces: new Map([["ws1", "running"]]),
        sessionStatus: "session_active",
      });

      useSessionStore.getState().setSessionId("sess-new");

      const state = useSessionStore.getState();
      expect(state.sessionId).toBe("sess-new");
      expect(state.trail).toEqual([]);
      expect(state.gates).toEqual([]);
      expect(state.escalations).toEqual([]);
      expect(state.refusals).toEqual([]);
      expect(state.workspaces.size).toBe(0);
      expect(state.sessionStatus).toBeNull();
    });

    it("can be set to null", () => {
      useSessionStore.getState().setSessionId("sess-1");
      useSessionStore.getState().setSessionId(null);

      expect(useSessionStore.getState().sessionId).toBeNull();
    });
  });

  // ---- addTrailEntry ----
  describe("addTrailEntry", () => {
    it("appends an entry to the trail", () => {
      const entry = makeTrailEntry();
      useSessionStore.getState().addTrailEntry(entry);

      expect(useSessionStore.getState().trail).toHaveLength(1);
      expect(useSessionStore.getState().trail[0]).toMatchObject({ id: "t1" });
    });

    it("appends multiple entries in order", () => {
      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "t1" }));
      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "t2" }));
      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "t3" }));

      const trail = useSessionStore.getState().trail;
      expect(trail).toHaveLength(3);
      expect(trail.map((e) => e.id)).toEqual(["t1", "t2", "t3"]);
    });

    it("trims trail to trailBufferSize when exceeded", () => {
      // Set a small buffer for testing
      useSessionStore.setState({ trailBufferSize: 3 });

      for (let i = 0; i < 5; i++) {
        useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: `t${i}` }));
      }

      const trail = useSessionStore.getState().trail;
      expect(trail).toHaveLength(3);
      // Should keep the last 3 entries (t2, t3, t4)
      expect(trail.map((e) => e.id)).toEqual(["t2", "t3", "t4"]);
    });

    it("handles entries at exactly the buffer limit", () => {
      useSessionStore.setState({ trailBufferSize: 2 });

      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "t0" }));
      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "t1" }));

      const trail = useSessionStore.getState().trail;
      expect(trail).toHaveLength(2);
      expect(trail.map((e) => e.id)).toEqual(["t0", "t1"]);
    });

    it("handles buffer size of 1", () => {
      useSessionStore.setState({ trailBufferSize: 1 });

      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "t0" }));
      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "t1" }));

      const trail = useSessionStore.getState().trail;
      expect(trail).toHaveLength(1);
      expect(trail[0].id).toBe("t1");
    });

    it("handles entry with extra unknown fields", () => {
      const entry = makeTrailEntry({ custom_field: "custom_value", nested: { a: 1 } });
      useSessionStore.getState().addTrailEntry(entry);

      const stored = useSessionStore.getState().trail[0];
      expect((stored as Record<string, unknown>).custom_field).toBe("custom_value");
    });
  });

  // ---- addGate ----
  describe("addGate", () => {
    it("appends a gate to the list", () => {
      useSessionStore.getState().addGate(makeGate());

      expect(useSessionStore.getState().gates).toHaveLength(1);
      expect(useSessionStore.getState().gates[0].gate_id).toBe("g1");
    });

    it("appends multiple gates in order", () => {
      useSessionStore.getState().addGate(makeGate({ gate_id: "g1" }));
      useSessionStore.getState().addGate(makeGate({ gate_id: "g2" }));

      const gates = useSessionStore.getState().gates;
      expect(gates).toHaveLength(2);
      expect(gates.map((g) => g.gate_id)).toEqual(["g1", "g2"]);
    });

    it("preserves optional fields like timeout_at", () => {
      useSessionStore.getState().addGate(makeGate({ timeout_at: "2026-04-17T00:00:00Z" }));

      expect(useSessionStore.getState().gates[0].timeout_at).toBe("2026-04-17T00:00:00Z");
    });
  });

  // ---- addEscalation ----
  describe("addEscalation", () => {
    it("appends an escalation to the list", () => {
      useSessionStore.getState().addEscalation(makeEscalation());

      expect(useSessionStore.getState().escalations).toHaveLength(1);
      expect(useSessionStore.getState().escalations[0].escalation_id).toBe("e1");
    });

    it("appends multiple escalations in order", () => {
      useSessionStore.getState().addEscalation(makeEscalation({ escalation_id: "e1" }));
      useSessionStore.getState().addEscalation(makeEscalation({ escalation_id: "e2" }));
      useSessionStore.getState().addEscalation(makeEscalation({ escalation_id: "e3" }));

      const escs = useSessionStore.getState().escalations;
      expect(escs).toHaveLength(3);
      expect(escs.map((e) => e.escalation_id)).toEqual(["e1", "e2", "e3"]);
    });
  });

  // ---- addRefusal ----
  describe("addRefusal", () => {
    it("appends a refusal to the list", () => {
      useSessionStore.getState().addRefusal(makeRefusal());

      expect(useSessionStore.getState().refusals).toHaveLength(1);
      expect(useSessionStore.getState().refusals[0].refusal_id).toBe("r1");
    });

    it("appends multiple refusals in order", () => {
      useSessionStore.getState().addRefusal(makeRefusal({ refusal_id: "r1" }));
      useSessionStore.getState().addRefusal(makeRefusal({ refusal_id: "r2" }));

      const refs = useSessionStore.getState().refusals;
      expect(refs).toHaveLength(2);
      expect(refs.map((r) => r.refusal_id)).toEqual(["r1", "r2"]);
    });

    it("preserves all refusal fields", () => {
      useSessionStore.getState().addRefusal(makeRefusal());

      const ref = useSessionStore.getState().refusals[0];
      expect(ref.tool_name).toBe("shell");
      expect(ref.error_code).toBe("POLICY_DENIED");
      expect(ref.policy_kind).toBe("deny-list");
      expect(ref.reason).toBe("Command not allowed");
      expect(ref.unblock_hint).toBe("Use a different command");
    });
  });

  // ---- setWorkspaceState ----
  describe("setWorkspaceState", () => {
    it("adds a workspace entry to the map", () => {
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws1", state: "running" });

      expect(useSessionStore.getState().workspaces.get("ws1")).toBe("running");
    });

    it("updates an existing workspace state", () => {
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws1", state: "running" });
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws1", state: "completed" });

      expect(useSessionStore.getState().workspaces.get("ws1")).toBe("completed");
      expect(useSessionStore.getState().workspaces.size).toBe(1);
    });

    it("handles multiple workspaces independently", () => {
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws1", state: "running" });
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws2", state: "idle" });
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws3", state: "error" });

      const wm = useSessionStore.getState().workspaces;
      expect(wm.size).toBe(3);
      expect(wm.get("ws1")).toBe("running");
      expect(wm.get("ws2")).toBe("idle");
      expect(wm.get("ws3")).toBe("error");
    });

    it("does not affect other workspaces when updating one", () => {
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws1", state: "running" });
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws2", state: "idle" });
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws1", state: "completed" });

      const wm = useSessionStore.getState().workspaces;
      expect(wm.get("ws1")).toBe("completed");
      expect(wm.get("ws2")).toBe("idle");
    });
  });

  // ---- setSessionEvent ----
  describe("setSessionEvent", () => {
    it("sets sessionStatus to the event type", () => {
      useSessionStore.getState().setSessionEvent({ type: "session_active" });

      expect(useSessionStore.getState().sessionStatus).toBe("session_active");
    });

    it("updates sessionStatus on subsequent events", () => {
      useSessionStore.getState().setSessionEvent({ type: "session_active" });
      useSessionStore.getState().setSessionEvent({ type: "session_completed" });

      expect(useSessionStore.getState().sessionStatus).toBe("session_completed");
    });

    it("sets sessionStatus to null when event has no type", () => {
      useSessionStore.getState().setSessionEvent({ type: "session_active" });
      useSessionStore.getState().setSessionEvent({});

      expect(useSessionStore.getState().sessionStatus).toBeNull();
    });

    it("sets sessionStatus to null for event with undefined type", () => {
      useSessionStore.getState().setSessionEvent({ type: undefined });

      expect(useSessionStore.getState().sessionStatus).toBeNull();
    });
  });

  // ---- clearSession ----
  describe("clearSession", () => {
    it("resets all state to initial values", () => {
      // Populate the store with data
      useSessionStore.setState({
        sessionId: "sess-99",
        trail: [makeTrailEntry()],
        gates: [makeGate()],
        escalations: [makeEscalation()],
        refusals: [makeRefusal()],
        workspaces: new Map([["ws1", "running"], ["ws2", "idle"]]),
        sessionStatus: "session_active",
      });

      useSessionStore.getState().clearSession();

      const state = useSessionStore.getState();
      expect(state.sessionId).toBeNull();
      expect(state.trail).toEqual([]);
      expect(state.gates).toEqual([]);
      expect(state.escalations).toEqual([]);
      expect(state.refusals).toEqual([]);
      expect(state.workspaces.size).toBe(0);
      expect(state.sessionStatus).toBeNull();
    });

    it("preserves trailBufferSize", () => {
      useSessionStore.setState({ trailBufferSize: 500 });

      useSessionStore.getState().clearSession();

      expect(useSessionStore.getState().trailBufferSize).toBe(500);
    });

    it("is idempotent on already-clear state", () => {
      useSessionStore.getState().clearSession();
      useSessionStore.getState().clearSession();

      const state = useSessionStore.getState();
      expect(state.sessionId).toBeNull();
      expect(state.trail).toEqual([]);
    });
  });

  // ---- Edge cases ----
  describe("edge cases", () => {
    it("adding entries after clearSession works correctly", () => {
      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "old" }));
      useSessionStore.getState().clearSession();
      useSessionStore.getState().addTrailEntry(makeTrailEntry({ id: "new" }));

      const trail = useSessionStore.getState().trail;
      expect(trail).toHaveLength(1);
      expect(trail[0].id).toBe("new");
    });

    it("setSessionId followed by data accumulation, then setSessionId again resets", () => {
      useSessionStore.getState().setSessionId("sess-1");
      useSessionStore.getState().addTrailEntry(makeTrailEntry());
      useSessionStore.getState().addGate(makeGate());
      useSessionStore.getState().addEscalation(makeEscalation());
      useSessionStore.getState().addRefusal(makeRefusal());
      useSessionStore.getState().setWorkspaceState({ workspace_id: "ws1", state: "running" });
      useSessionStore.getState().setSessionEvent({ type: "session_active" });

      // Switch session
      useSessionStore.getState().setSessionId("sess-2");

      const state = useSessionStore.getState();
      expect(state.sessionId).toBe("sess-2");
      expect(state.trail).toEqual([]);
      expect(state.gates).toEqual([]);
      expect(state.escalations).toEqual([]);
      expect(state.refusals).toEqual([]);
      expect(state.workspaces.size).toBe(0);
      expect(state.sessionStatus).toBeNull();
    });

    it("workspace state works with empty workspace_id", () => {
      useSessionStore.getState().setWorkspaceState({ workspace_id: "", state: "unknown" });

      expect(useSessionStore.getState().workspaces.get("")).toBe("unknown");
    });
  });
});
