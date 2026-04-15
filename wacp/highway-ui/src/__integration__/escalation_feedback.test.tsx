/**
 * Integration: EscalationPanel + InjectionForm + routing — 2 tests
 * Verifies the escalation feedback flow navigates to injection form.
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router";
import { useStore } from "../store/index.js";

vi.mock("../transport/client.js", () => ({
  getClient: () => ({
    respondToEscalation: vi.fn().mockResolvedValue({ applied: true }),
    injectEnvelope: vi.fn().mockResolvedValue({ envelopeId: "env-1" }),
  }),
}));

import { EscalationPanel } from "../components/escalations/EscalationPanel.js";
import { InjectionForm } from "../components/injection/InjectionForm.js";

function setupEscalation() {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: [] },
    escalations: {
      active: new Map([
        ["esc-1", {
          escalationId: "esc-1",
          workspaceId: "ws-help",
          owner: "user-1",
          context: "Agent needs guidance",
          createdAt: 100n,
        }],
      ]),
      resolved: new Map(),
      inFlight: new Set(),
    },
    workspaces: { views: new Map(), changes: [], expandedNodes: new Set() },
  });
}

describe("escalation → feedback flow integration", () => {
  it("escalation panel shows active escalation with workspace", () => {
    setupEscalation();
    render(
      <MemoryRouter>
        <EscalationPanel />
      </MemoryRouter>,
    );
    expect(screen.getByText("ws-help")).toBeInTheDocument();
    expect(screen.getByText("Agent needs guidance")).toBeInTheDocument();
  });

  it("send feedback navigates to injection form", async () => {
    setupEscalation();
    render(
      <MemoryRouter initialEntries={["/escalations"]}>
        <Routes>
          <Route path="/escalations" element={<EscalationPanel />} />
          <Route path="/inject" element={<InjectionForm />} />
        </Routes>
      </MemoryRouter>,
    );

    const feedbackBtn = screen.getByRole("button", { name: /send feedback/i });
    fireEvent.click(feedbackBtn);

    // Should navigate to injection form (has "ws-..." placeholder)
    await waitFor(() => {
      expect(screen.getByPlaceholderText("ws-...")).toBeInTheDocument();
    });
  });
});
