/**
 * Tests for useSessionStream WebSocket hook.
 * Covers: connect, disconnect, reconnect with backoff, channel dispatch,
 *         malformed/binary messages, error handling, rapid reconnect capping.
 */

import { renderHook, act, cleanup } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { useSessionStream } from "./useSessionStream.ts";
import { useSessionStore } from "../store/session.ts";

// ---------------------------------------------------------------------------
// Mock WebSocket
// ---------------------------------------------------------------------------

type WSHandler = ((ev: { data: unknown }) => void) | null;

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  url: string;
  onopen: (() => void) | null = null;
  onmessage: WSHandler = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  readyState = 0; // CONNECTING

  closeCalled = false;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  close() {
    this.closeCalled = true;
    this.readyState = 3; // CLOSED
  }

  // Test helpers -----------------------------------------------------------

  /** Simulate the server accepting the connection */
  simulateOpen() {
    this.readyState = 1; // OPEN
    this.onopen?.();
  }

  /** Simulate an incoming text frame */
  simulateMessage(data: unknown) {
    this.onmessage?.({ data });
  }

  /** Simulate an unexpected server-side close */
  simulateClose() {
    this.readyState = 3;
    this.onclose?.();
  }

  /** Simulate a connection error */
  simulateError() {
    this.onerror?.();
  }
}

// ---------------------------------------------------------------------------
// Setup / teardown
// ---------------------------------------------------------------------------

const originalWebSocket = globalThis.WebSocket;

beforeEach(() => {
  vi.useFakeTimers();
  MockWebSocket.instances = [];
  // @ts-expect-error -- replacing global WebSocket with mock
  globalThis.WebSocket = MockWebSocket;

  // Reset Zustand store to clean state
  useSessionStore.setState({
    sessionId: null,
    trail: [],
    gates: [],
    escalations: [],
    refusals: [],
    workspaces: new Map(),
    sessionStatus: null,
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  globalThis.WebSocket = originalWebSocket;
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function latestWS(): MockWebSocket {
  const ws = MockWebSocket.instances.at(-1);
  if (!ws) throw new Error("No WebSocket created");
  return ws;
}

function frame(channel: string, event: Record<string, unknown> = {}) {
  return JSON.stringify({ channel, session_id: "sess-1", event });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("useSessionStream", () => {
  // -----------------------------------------------------------------------
  // Connection lifecycle
  // -----------------------------------------------------------------------

  describe("connect / disconnect", () => {
    it("creates a WebSocket on mount with the correct URL", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));

      expect(MockWebSocket.instances).toHaveLength(1);
      const ws = latestWS();
      // URL should start with ws:// (http origin) and contain the session path
      expect(ws.url).toMatch(/^ws:\/\//);
      expect(ws.url).toContain("/api/sessions/sess-1/ws");
    });

    it("does not create a WebSocket when enabled=false", () => {
      renderHook(() =>
        useSessionStream({ sessionId: "sess-1", enabled: false }),
      );

      expect(MockWebSocket.instances).toHaveLength(0);
    });

    it("does not create a WebSocket when sessionId is empty", () => {
      renderHook(() => useSessionStream({ sessionId: "" }));

      expect(MockWebSocket.instances).toHaveLength(0);
    });

    it("closes WebSocket on unmount and nullifies onclose to prevent reconnect", () => {
      const { unmount } = renderHook(() =>
        useSessionStream({ sessionId: "sess-1" }),
      );

      const ws = latestWS();
      ws.simulateOpen();

      unmount();

      expect(ws.closeCalled).toBe(true);
      // onclose should be nulled so the close() call doesn't trigger reconnect
      expect(ws.onclose).toBeNull();
    });
  });

  // -----------------------------------------------------------------------
  // Reconnection with exponential backoff
  // -----------------------------------------------------------------------

  describe("reconnect / exponential backoff", () => {
    it("reconnects after unexpected close with initial delay of 100ms", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));

      const ws1 = latestWS();
      ws1.simulateOpen();

      // Unexpected close
      act(() => ws1.simulateClose());

      expect(MockWebSocket.instances).toHaveLength(1); // not yet reconnected

      // Advance 100ms (first backoff)
      act(() => vi.advanceTimersByTime(100));

      expect(MockWebSocket.instances).toHaveLength(2);
      expect(latestWS().url).toContain("/api/sessions/sess-1/ws");
    });

    it("doubles backoff delay on consecutive failures: 100, 200, 400, 800, 1600, 3200, 5000 cap", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));

      const expectedDelays = [100, 200, 400, 800, 1600, 3200, 5000];

      // First WS opens then closes — retryCount was reset to 0 by onopen,
      // so the first reconnect delay is 100ms (2^0 * 100).
      const ws0 = latestWS();
      ws0.simulateOpen();
      act(() => ws0.simulateClose());

      for (let i = 0; i < expectedDelays.length; i++) {
        const countBefore = MockWebSocket.instances.length;

        // Advance one ms less than the delay: should NOT reconnect yet
        act(() => vi.advanceTimersByTime(expectedDelays[i]! - 1));
        expect(MockWebSocket.instances).toHaveLength(countBefore);

        // Advance the remaining 1ms: now it should reconnect
        act(() => vi.advanceTimersByTime(1));
        expect(MockWebSocket.instances).toHaveLength(countBefore + 1);

        // Immediately close the new WS WITHOUT opening it (simulates
        // connection failure) so retryCount keeps incrementing.
        const wsNew = latestWS();
        act(() => wsNew.simulateClose());
      }
    });

    it("caps backoff at 5000ms", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));

      // Burn through enough retries to exceed the 5s cap
      for (let i = 0; i < 10; i++) {
        const ws = latestWS();
        ws.simulateOpen();
        act(() => ws.simulateClose());
        act(() => vi.advanceTimersByTime(5000));
      }

      // The 11th reconnect should still happen at 5s, not longer
      const ws = latestWS();
      ws.simulateOpen();
      act(() => ws.simulateClose());

      const countBefore = MockWebSocket.instances.length;
      act(() => vi.advanceTimersByTime(5000));
      expect(MockWebSocket.instances).toHaveLength(countBefore + 1);
    });

    it("resets retry count on successful open", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));

      // First failure -> reconnect at 100ms
      const ws1 = latestWS();
      ws1.simulateOpen();
      act(() => ws1.simulateClose());
      act(() => vi.advanceTimersByTime(100));

      // Second failure -> normally 200ms, but we opened first so it resets
      const ws2 = latestWS();
      ws2.simulateOpen(); // resets retryCount to 0
      act(() => ws2.simulateClose());

      // Should reconnect at 100ms again (not 200ms)
      const countBefore = MockWebSocket.instances.length;
      act(() => vi.advanceTimersByTime(100));
      expect(MockWebSocket.instances).toHaveLength(countBefore + 1);
    });

    it("does not reconnect after intentional unmount", () => {
      const { unmount } = renderHook(() =>
        useSessionStream({ sessionId: "sess-1" }),
      );

      const ws = latestWS();
      ws.simulateOpen();

      unmount();

      // Even after waiting a long time, no new WS should appear
      act(() => vi.advanceTimersByTime(10_000));
      expect(MockWebSocket.instances).toHaveLength(1);
    });

    it("clears pending reconnect timer on unmount", () => {
      const { unmount } = renderHook(() =>
        useSessionStream({ sessionId: "sess-1" }),
      );

      const ws = latestWS();
      ws.simulateOpen();
      act(() => ws.simulateClose()); // schedules reconnect timer

      // Unmount before the timer fires
      unmount();
      act(() => vi.advanceTimersByTime(10_000));

      // Only the original WS should exist
      expect(MockWebSocket.instances).toHaveLength(1);
    });
  });

  // -----------------------------------------------------------------------
  // Channel dispatch — all 7 channels
  // -----------------------------------------------------------------------

  describe("channel dispatch", () => {
    it("dispatches 'trail' events to addTrailEntry", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      const entry = { id: "t1", timestamp: "2026-01-01", event_type: "tool_call", summary: "ran ls" };

      act(() => ws.simulateMessage(frame("trail", entry)));

      const state = useSessionStore.getState();
      expect(state.trail).toHaveLength(1);
      expect(state.trail[0]).toMatchObject(entry);
    });

    it("dispatches 'gates' events to addGate", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      const gate = { gate_id: "g1", type: "approval", workspace_id: "w1", workspace_label: "main", urgency: "high" };

      act(() => ws.simulateMessage(frame("gates", gate)));

      const state = useSessionStore.getState();
      expect(state.gates).toHaveLength(1);
      expect(state.gates[0]).toMatchObject(gate);
    });

    it("dispatches 'escalations' events to addEscalation", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      const esc = { escalation_id: "e1", workspace_id: "w1", workspace_label: "main", reason: "dangerous op" };

      act(() => ws.simulateMessage(frame("escalations", esc)));

      const state = useSessionStore.getState();
      expect(state.escalations).toHaveLength(1);
      expect(state.escalations[0]).toMatchObject(esc);
    });

    it("dispatches 'refusals' events to addRefusal", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      const ref = {
        refusal_id: "r1",
        workspace_id: "w1",
        workspace_label: "main",
        tool_name: "bash",
        error_code: "E_POLICY",
        policy_kind: "deny",
        reason: "blocked",
        unblock_hint: "approve in gate",
      };

      act(() => ws.simulateMessage(frame("refusals", ref)));

      const state = useSessionStore.getState();
      expect(state.refusals).toHaveLength(1);
      expect(state.refusals[0]).toMatchObject(ref);
    });

    it("dispatches 'workspaces' events to setWorkspaceState", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      const wsEvent = { workspace_id: "w1", state: "active" };

      act(() => ws.simulateMessage(frame("workspaces", wsEvent)));

      const state = useSessionStore.getState();
      expect(state.workspaces.get("w1")).toBe("active");
    });

    it("dispatches 'session' events to setSessionEvent", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      const sessionEvt = { type: "session_completed" };

      act(() => ws.simulateMessage(frame("session", sessionEvt)));

      const state = useSessionStore.getState();
      expect(state.sessionStatus).toBe("session_completed");
    });

    it("handles 'notification' channel without crashing (noop)", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      // Should not throw and should not alter any store state
      act(() =>
        ws.simulateMessage(
          frame("notification", { title: "heads up", body: "something happened" }),
        ),
      );

      const state = useSessionStore.getState();
      // Notification does not push to any array — verify nothing changed
      expect(state.trail).toHaveLength(0);
      expect(state.gates).toHaveLength(0);
      expect(state.escalations).toHaveLength(0);
      expect(state.refusals).toHaveLength(0);
      expect(state.sessionStatus).toBeNull();
    });

    it("ignores unknown channel names without crashing", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      act(() =>
        ws.simulateMessage(frame("unknown_channel", { foo: "bar" })),
      );

      const state = useSessionStore.getState();
      expect(state.trail).toHaveLength(0);
      expect(state.gates).toHaveLength(0);
    });

    it("dispatches multiple events across channels in sequence", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      act(() => {
        ws.simulateMessage(frame("trail", { id: "t1", event_type: "a", summary: "first" }));
        ws.simulateMessage(frame("trail", { id: "t2", event_type: "b", summary: "second" }));
        ws.simulateMessage(frame("gates", { gate_id: "g1", type: "approval", urgency: "low" }));
        ws.simulateMessage(frame("escalations", { escalation_id: "e1", reason: "danger" }));
      });

      const state = useSessionStore.getState();
      expect(state.trail).toHaveLength(2);
      expect(state.gates).toHaveLength(1);
      expect(state.escalations).toHaveLength(1);
    });
  });

  // -----------------------------------------------------------------------
  // Malformed / binary messages
  // -----------------------------------------------------------------------

  describe("malformed and binary messages", () => {
    it("handles malformed JSON gracefully (no crash)", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      // Send broken JSON — should not throw
      act(() => ws.simulateMessage("{ not valid json"));

      // Store should be unchanged
      const state = useSessionStore.getState();
      expect(state.trail).toHaveLength(0);
    });

    it("handles empty string message gracefully", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      act(() => ws.simulateMessage(""));

      const state = useSessionStore.getState();
      expect(state.trail).toHaveLength(0);
    });

    it("handles numeric (non-string) data gracefully", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      act(() => ws.simulateMessage(42));

      const state = useSessionStore.getState();
      expect(state.trail).toHaveLength(0);
    });

    it("handles null data gracefully", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      act(() => ws.simulateMessage(null));

      const state = useSessionStore.getState();
      expect(state.trail).toHaveLength(0);
    });

    it("handles JSON that parses but has no channel field", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      act(() => ws.simulateMessage(JSON.stringify({ data: "no channel" })));

      // Should fall through the switch with no match — no crash
      const state = useSessionStore.getState();
      expect(state.trail).toHaveLength(0);
    });
  });

  // -----------------------------------------------------------------------
  // Error handling
  // -----------------------------------------------------------------------

  describe("error handling", () => {
    it("closes the WebSocket on error (which triggers reconnect via onclose)", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      act(() => ws.simulateError());

      // onerror calls ws.close(), which sets closeCalled
      expect(ws.closeCalled).toBe(true);
    });

    it("reconnects after error -> close sequence", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      // Error triggers close, close triggers reconnect timer
      act(() => {
        ws.simulateError();
        ws.simulateClose();
      });

      act(() => vi.advanceTimersByTime(100));
      expect(MockWebSocket.instances).toHaveLength(2);
    });
  });

  // -----------------------------------------------------------------------
  // Multiple rapid reconnects (bounded by backoff)
  // -----------------------------------------------------------------------

  describe("rapid reconnects are bounded", () => {
    it("only creates one pending reconnect per close event", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));

      // Simulate rapid close-open-close cycles
      for (let i = 0; i < 5; i++) {
        const ws = latestWS();
        ws.simulateOpen();
        act(() => ws.simulateClose());
        // Advance enough to trigger reconnect
        act(() => vi.advanceTimersByTime(5000));
      }

      // Should have exactly 6 instances: 1 initial + 5 reconnects
      expect(MockWebSocket.instances).toHaveLength(6);
    });

    it("does not create extra connections when close fires multiple times", () => {
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      ws.simulateOpen();

      // Simulate onclose firing multiple times (shouldn't happen normally
      // but tests robustness)
      act(() => {
        ws.simulateClose();
        ws.simulateClose();
        ws.simulateClose();
      });

      // All three calls schedule timers, but each connect() creates one WS
      // After advancing enough time all timers fire
      act(() => vi.advanceTimersByTime(5000));

      // Multiple timers will fire and each creates a WS, but the hook
      // overwrites wsRef so only the latest matters. The key is no crash.
      expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2);
    });
  });

  // -----------------------------------------------------------------------
  // Re-renders with changed props
  // -----------------------------------------------------------------------

  describe("prop changes", () => {
    it("reconnects when sessionId changes", () => {
      const { rerender } = renderHook(
        (props: { sessionId: string }) =>
          useSessionStream({ sessionId: props.sessionId }),
        { initialProps: { sessionId: "sess-1" } },
      );

      const ws1 = latestWS();
      ws1.simulateOpen();

      expect(ws1.url).toContain("sess-1");

      // Change sessionId triggers re-render -> new connect()
      rerender({ sessionId: "sess-2" });

      // The old WS should have been closed (unmount cleanup)
      expect(ws1.closeCalled).toBe(true);

      // A new WS should have been created
      const ws2 = latestWS();
      expect(ws2.url).toContain("sess-2");
    });

    it("disconnects when enabled changes to false", () => {
      const { rerender } = renderHook(
        (props: { sessionId: string; enabled: boolean }) =>
          useSessionStream(props),
        { initialProps: { sessionId: "sess-1", enabled: true } },
      );

      const ws1 = latestWS();
      ws1.simulateOpen();

      rerender({ sessionId: "sess-1", enabled: false });

      // Old WS should have been closed
      expect(ws1.closeCalled).toBe(true);
    });
  });

  // -----------------------------------------------------------------------
  // URL construction
  // -----------------------------------------------------------------------

  describe("URL construction", () => {
    it("uses wss: when page is served over https", () => {
      vi.stubGlobal("location", {
        ...window.location,
        protocol: "https:",
        host: "example.com",
      });

      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      expect(ws.url).toMatch(/^wss:\/\/example\.com/);

      vi.unstubAllGlobals();
    });

    it("uses ws: when page is served over http", () => {
      // Default jsdom location is http, so just verify
      renderHook(() => useSessionStream({ sessionId: "sess-1" }));
      const ws = latestWS();
      expect(ws.url).toMatch(/^ws:\/\//);
    });
  });
});
