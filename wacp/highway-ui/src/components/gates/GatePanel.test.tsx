import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { GatePanel } from "./GatePanel.js";
import { useStore, type PendingGate } from "@/store";

function resetStore() {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: ["gate_respond"] },
    trail: {
      entries: [],
      paused: false,
      filters: { eventTypes: new Set(), workspaceId: "", actor: "" },
      expandedEntryIndex: null,
    },
    gates: { pending: new Map(), resolved: new Map(), inFlight: new Set() },
    escalations: { active: new Map(), resolved: new Map(), inFlight: new Set() },
    workspaces: { views: new Map(), changes: [], expandedNodes: new Set() },
    taskGraph: { tasks: [], lastFetched: null },
    notifications: { escalationBanner: null },
  });
}

function makeGate(id: string, overrides?: Partial<PendingGate>): PendingGate {
  return {
    gateId: id,
    gateType: "task_approval",
    subject: new Uint8Array(),
    workspaceId: "ws-1",
    taskId: "t-1",
    timeoutMs: 60000n,
    fallbackAction: "approve",
    createdAt: BigInt(Date.now() * 1000),
    ...overrides,
  };
}

function renderPanel() {
  return render(
    <MemoryRouter>
      <GatePanel />
    </MemoryRouter>,
  );
}

describe("GatePanel", () => {
  beforeEach(resetStore);

  it("shows empty state when no pending gates", () => {
    renderPanel();
    expect(screen.getByText("No pending gates.")).toBeDefined();
  });

  it("renders gate cards with type badge", () => {
    useStore.getState().addPendingGate(makeGate("g-1", { gateType: "task_approval" }));
    useStore.getState().addPendingGate(makeGate("g-2", { gateType: "integration" }));
    renderPanel();

    expect(screen.getByText("Task Approval")).toBeDefined();
    expect(screen.getByText("Integration")).toBeDefined();
  });

  it("shows countdown timer for gates with timeout", () => {
    useStore.getState().addPendingGate(makeGate("g-1", { timeoutMs: 120000n }));
    renderPanel();

    // Should show a time like "2:00" or "1:59"
    const timeElements = screen.getAllByText(/\d+:\d{2}/);
    expect(timeElements.length).toBeGreaterThan(0);
  });

  it("shows indefinite for zero timeout", () => {
    useStore.getState().addPendingGate(makeGate("g-1", { timeoutMs: 0n }));
    renderPanel();

    expect(screen.getByText("indefinite")).toBeDefined();
  });

  it("renders approve, reject, modify buttons", () => {
    useStore.getState().addPendingGate(makeGate("g-1"));
    renderPanel();

    expect(screen.getByText("Approve")).toBeDefined();
    expect(screen.getByText("Reject")).toBeDefined();
    expect(screen.getByText("Modify")).toBeDefined();
  });

  it("shows fallback label", () => {
    useStore.getState().addPendingGate(makeGate("g-1", { fallbackAction: "auto-reject" }));
    renderPanel();

    expect(screen.getByText("Fallback: auto-reject")).toBeDefined();
  });

  it("shows batch approval bar for multiple task_approval gates", () => {
    useStore.getState().addPendingGate(makeGate("g-1", { gateType: "task_approval" }));
    useStore.getState().addPendingGate(makeGate("g-2", { gateType: "task_approval" }));
    renderPanel();

    expect(screen.getByText("2 task approvals pending")).toBeDefined();
    expect(screen.getByText("Approve All")).toBeDefined();
    expect(screen.getByText("Reject All")).toBeDefined();
  });

  it("does not show batch bar for single gate", () => {
    useStore.getState().addPendingGate(makeGate("g-1", { gateType: "task_approval" }));
    renderPanel();

    expect(screen.queryByText("Approve All")).toBeNull();
  });

  it("does not show batch bar for non-task_approval gates", () => {
    useStore.getState().addPendingGate(makeGate("g-1", { gateType: "integration" }));
    useStore.getState().addPendingGate(makeGate("g-2", { gateType: "workspace_create" }));
    renderPanel();

    expect(screen.queryByText("Approve All")).toBeNull();
  });

  it("sorts gates by urgency — shortest remaining time first", () => {
    const now = Date.now() * 1000;
    // Gate with 10s remaining (created 50s ago, 60s timeout)
    useStore.getState().addPendingGate(makeGate("g-urgent", {
      createdAt: BigInt(now - 50_000_000),
      timeoutMs: 60000n,
    }));
    // Gate with 55s remaining (created 5s ago, 60s timeout)
    useStore.getState().addPendingGate(makeGate("g-relaxed", {
      createdAt: BigInt(now - 5_000_000),
      timeoutMs: 60000n,
    }));

    renderPanel();

    // The urgent gate should appear before the relaxed one
    const cards = screen.getAllByText(/g-(urgent|relaxed)/);
    expect(cards[0]!.textContent).toContain("g-urgent");
    expect(cards[1]!.textContent).toContain("g-relaxed");
  });

  it("shows workspace and task links", () => {
    useStore.getState().addPendingGate(makeGate("g-1", {
      workspaceId: "ws-linked",
      taskId: "t-linked",
    }));
    renderPanel();

    expect(screen.getByText(/ws-linked/)).toBeDefined();
    expect(screen.getByText(/t-linked/)).toBeDefined();
  });
});
