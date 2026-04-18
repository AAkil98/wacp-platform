import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { Notifications, NavBadge } from "./Notifications";
import { useSessionStore } from "../store/session";

// ----------------------------------------------------------------------------
// Deliverable drift for AUDIT-2026-04-15 §13.7.4 (see HEALTH-LOG
// §2.5).  The deliverable described:
//
//  - Toast variants: success / warning / error / info
//  - Auto-dismiss after TTL; explicit dismiss button
//  - Stacking under ≥5 concurrent toasts with ordering rule
//  - Browser-notification API permission flow (default/granted/denied)
//  - Nav badge overflow as "99+"
//
// Actual `Notifications.tsx` ships:
//
//  - Two priorities: "normal" (auto-dismiss 5 s) / "high" (manual only).
//  - No exported API to add toasts — internal `useState` with no setter
//    plumbed out; the component is not imported anywhere (grep confirms).
//    Effectively a stub waiting to be wired.
//  - No browser-notification (`window.Notification`) integration at all.
//  - `NavBadge` renders the raw count — no "99+" overflow.
//
// This suite therefore tests actual behavior of what's wired:
//
//  - NavBadge (functional) — 11 tests.
//  - Notifications lifecycle (interval + null render) — 5 tests.
//
// The drift itself is captured in `HEALTH-LOG.md` §2.5 as a
// latent-bug signal: operators currently have no way to receive toasts.
// ----------------------------------------------------------------------------

function resetStore() {
  useSessionStore.setState({
    sessionId: null,
    trail: [],
    gates: [],
    escalations: [],
    refusals: [],
    workspaces: new Map(),
    sessionStatus: null,
    trailBufferSize: 1000,
  });
}

function makeGate(id: string) {
  return {
    gate_id: id,
    type: "quality_gate",
    workspace_id: "ws1",
    workspace_label: "w",
    subject: { detail: "x" } as Record<string, unknown>,
    urgency: "medium",
  };
}
function makeEsc(id: string) {
  return {
    escalation_id: id,
    workspace_id: "ws1",
    workspace_label: "w",
    reason: "r",
  };
}
function makeRef(id: string) {
  return {
    refusal_id: id,
    workspace_id: "ws1",
    workspace_label: "w",
    tool_name: "t",
    error_code: "POLICY_X",
    policy_kind: "requires_checkpoint",
    reason: "r",
    unblock_hint: "",
  };
}

// ---- NavBadge ----

describe("NavBadge", () => {
  beforeEach(() => {
    resetStore();
  });
  afterEach(() => {
    resetStore();
  });

  it("renders null when all three pending queues are empty", () => {
    const { container } = render(<NavBadge />);
    expect(container.firstChild).toBeNull();
  });

  it("shows gate count when only gates are pending", () => {
    useSessionStore.setState({
      gates: [makeGate("g1"), makeGate("g2"), makeGate("g3")],
    });
    render(<NavBadge />);
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  it("shows escalation count when only escalations are pending", () => {
    useSessionStore.setState({
      escalations: [makeEsc("e1"), makeEsc("e2")],
    });
    render(<NavBadge />);
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("shows refusal count when only refusals are pending", () => {
    useSessionStore.setState({
      refusals: [makeRef("r1"), makeRef("r2"), makeRef("r3"), makeRef("r4")],
    });
    render(<NavBadge />);
    expect(screen.getByText("4")).toBeInTheDocument();
  });

  it("sums gates + escalations + refusals", () => {
    useSessionStore.setState({
      gates: [makeGate("g1"), makeGate("g2")],
      escalations: [makeEsc("e1"), makeEsc("e2"), makeEsc("e3")],
      refusals: [makeRef("r1")],
    });
    render(<NavBadge />);
    expect(screen.getByText("6")).toBeInTheDocument();
  });

  it("updates from 0 → 1 when a gate is added", () => {
    const { container } = render(<NavBadge />);
    expect(container.firstChild).toBeNull();
    act(() => {
      useSessionStore.setState({ gates: [makeGate("g1")] });
    });
    expect(screen.getByText("1")).toBeInTheDocument();
  });

  it("hides the badge again when the last pending item is cleared", () => {
    useSessionStore.setState({ gates: [makeGate("g1")] });
    const { container } = render(<NavBadge />);
    expect(screen.getByText("1")).toBeInTheDocument();
    act(() => {
      useSessionStore.setState({ gates: [] });
    });
    expect(container.firstChild).toBeNull();
  });

  it("renders a single-digit count as the bare number", () => {
    useSessionStore.setState({ gates: [makeGate("g1")] });
    render(<NavBadge />);
    expect(screen.getByText("1").textContent).toBe("1");
  });

  it("renders a two-digit count as the bare number", () => {
    useSessionStore.setState({
      gates: Array.from({ length: 42 }).map((_, i) => makeGate(`g${i}`)),
    });
    render(<NavBadge />);
    expect(screen.getByText("42")).toBeInTheDocument();
  });

  it("renders counts ≥ 100 as the raw number (no '99+' overflow — drift)", () => {
    useSessionStore.setState({
      gates: Array.from({ length: 150 }).map((_, i) => makeGate(`g${i}`)),
    });
    render(<NavBadge />);
    expect(screen.getByText("150")).toBeInTheDocument();
    expect(screen.queryByText("99+")).not.toBeInTheDocument();
  });

  it("updates immediately when gates change after mount (zustand subscription)", () => {
    render(<NavBadge />);
    act(() => {
      useSessionStore.setState({ gates: [makeGate("g1")] });
    });
    expect(screen.getByText("1")).toBeInTheDocument();
    act(() => {
      useSessionStore.setState({
        gates: [makeGate("g1"), makeGate("g2"), makeGate("g3")],
      });
    });
    expect(screen.getByText("3")).toBeInTheDocument();
  });
});

// ---- Notifications ----

describe("Notifications", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    // Ensure no timers leak between tests even if a test forgot to clean up.
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it("renders null when the toasts state is empty (initial)", () => {
    const { container } = render(<Notifications />);
    expect(container.firstChild).toBeNull();
  });

  it("remains empty across the auto-dismiss interval (no spontaneous toasts)", () => {
    const { container } = render(<Notifications />);
    expect(container.firstChild).toBeNull();
    // Advance past the 1 s auto-dismiss tick several times.
    vi.advanceTimersByTime(6000);
    expect(container.firstChild).toBeNull();
  });

  it("installs a repeating interval on mount", () => {
    const before = vi.getTimerCount();
    render(<Notifications />);
    expect(vi.getTimerCount()).toBeGreaterThan(before);
  });

  it("clears the auto-dismiss interval on unmount (no timer leak)", () => {
    const { unmount } = render(<Notifications />);
    const afterMount = vi.getTimerCount();
    expect(afterMount).toBeGreaterThan(0);
    unmount();
    expect(vi.getTimerCount()).toBe(afterMount - 1);
  });

  it("does not throw when the interval fires with no toasts", () => {
    render(<Notifications />);
    expect(() => vi.advanceTimersByTime(2000)).not.toThrow();
  });
});
