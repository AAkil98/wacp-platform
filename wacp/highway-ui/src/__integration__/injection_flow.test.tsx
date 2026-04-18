/**
 * Integration: InjectionForm + rpcs + store — 2 tests
 * Verifies the inject envelope flow from form to RPC to store.
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { useStore } from "../store/index.js";

const mockInject = vi.fn();

vi.mock("../transport/client.js", () => ({
  getClient: () => ({
    injectEnvelope: mockInject,
  }),
}));

import { InjectionForm } from "../components/injection/InjectionForm.js";

beforeEach(() => {
  mockInject.mockReset();
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: [] },
    workspaces: { views: new Map(), changes: [], expandedNodes: new Set() },
  });
});

describe("injection flow integration", () => {
  it("submits form and calls injectEnvelope RPC", async () => {
    mockInject.mockResolvedValueOnce({ envelopeId: "env-injected" });

    render(
      <MemoryRouter>
        <InjectionForm />
      </MemoryRouter>,
    );

    // Fill workspace field (placeholder is "ws-...")
    const wsInput = screen.getByPlaceholderText("ws-...");
    fireEvent.change(wsInput, { target: { value: "ws-target" } });

    // Fill payload (placeholder is "Envelope payload (UTF-8 text)")
    const payloadInput = screen.getByPlaceholderText(/payload/i);
    fireEvent.change(payloadInput, { target: { value: "test payload" } });

    // Submit
    const submitBtn = screen.getByRole("button", { name: /send/i });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(mockInject).toHaveBeenCalledOnce();
    });
    const arg = mockInject.mock.calls[0]![0];
    expect(arg.toWorkspace).toBe("ws-target");
  });

  it("shows success message after injection", async () => {
    mockInject.mockResolvedValueOnce({ envelopeId: "env-success" });

    render(
      <MemoryRouter>
        <InjectionForm />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByPlaceholderText("ws-..."), { target: { value: "ws-1" } });
    fireEvent.change(screen.getByPlaceholderText(/payload/i), { target: { value: "data" } });
    fireEvent.click(screen.getByRole("button", { name: /send/i }));

    await waitFor(() => {
      expect(screen.getByText(/env-success|success/i)).toBeInTheDocument();
    });
  });
});
