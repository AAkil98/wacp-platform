import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { EscalationPanel } from "./EscalationPanel.js";
import { useStore, type ActiveEscalation } from "@/store";

function resetStore() {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: ["escalation_respond"] },
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

function makeEscalation(id: string, overrides?: Partial<ActiveEscalation>): ActiveEscalation {
  return {
    escalationId: id,
    workspaceId: "ws-stuck",
    owner: "user-1",
    context: "Agent is blocked on external API",
    createdAt: BigInt(Date.now() * 1000),
    ...overrides,
  };
}

function renderPanel() {
  return render(
    <MemoryRouter>
      <EscalationPanel />
    </MemoryRouter>,
  );
}

describe("EscalationPanel", () => {
  beforeEach(resetStore);

  it("shows empty state when no active escalations", () => {
    renderPanel();
    expect(screen.getByText("No active escalations.")).toBeDefined();
  });

  it("renders escalation cards with context", () => {
    useStore.getState().addEscalation(makeEscalation("e-1", {
      context: "Cannot reach database",
    }));
    renderPanel();

    expect(screen.getByText("Cannot reach database")).toBeDefined();
  });

  it("shows workspace link", () => {
    useStore.getState().addEscalation(makeEscalation("e-1", {
      workspaceId: "ws-linked",
    }));
    renderPanel();

    expect(screen.getAllByText("ws-linked").length).toBeGreaterThan(0);
  });

  it("shows all three action buttons", () => {
    useStore.getState().addEscalation(makeEscalation("e-1"));
    renderPanel();

    expect(screen.getByText("Send Feedback")).toBeDefined();
    expect(screen.getByText("Abort Workspace")).toBeDefined();
    expect(screen.getByText("Delegate")).toBeDefined();
  });

  it("shows abort confirmation dialog on click", () => {
    useStore.getState().addEscalation(makeEscalation("e-1"));
    renderPanel();

    fireEvent.click(screen.getByText("Abort Workspace"));
    expect(screen.getByText(/will fail the workspace/)).toBeDefined();
    expect(screen.getByText("Confirm Abort")).toBeDefined();
    expect(screen.getByText("Cancel")).toBeDefined();
  });

  it("cancels abort confirmation", () => {
    useStore.getState().addEscalation(makeEscalation("e-1"));
    renderPanel();

    fireEvent.click(screen.getByText("Abort Workspace"));
    fireEvent.click(screen.getByText("Cancel"));
    expect(screen.queryByText("Confirm Abort")).toBeNull();
  });

  it("shows owner", () => {
    useStore.getState().addEscalation(makeEscalation("e-1", { owner: "admin-user" }));
    renderPanel();

    expect(screen.getByText("owner: admin-user")).toBeDefined();
  });

  it("shows view workspace link", () => {
    useStore.getState().addEscalation(makeEscalation("e-1"));
    renderPanel();

    expect(screen.getByText("View workspace details")).toBeDefined();
  });
});
