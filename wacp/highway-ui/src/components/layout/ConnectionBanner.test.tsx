import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { useStore } from "@/store";
import { ConnectionBanner } from "./ConnectionBanner.js";

function renderBanner() {
  return render(
    <MemoryRouter>
      <ConnectionBanner />
    </MemoryRouter>,
  );
}

function setSession(state: string) {
  useStore.setState({
    session: { state: state as any, userId: "", capabilities: [] },
  });
}

beforeEach(() => {
  useStore.setState({
    session: { state: "connected", userId: "user-1", capabilities: [] },
    notifications: { escalationBanner: null },
  });
});

describe("ConnectionBanner", () => {
  it("renders nothing when connected and no escalation", () => {
    renderBanner();
    // No visible banner text
    expect(screen.queryByText(/reconnecting|disconnected|authenticating/i)).toBeNull();
  });

  it("shows reconnecting banner", () => {
    setSession("reconnecting");
    renderBanner();
    expect(screen.getByText(/reconnecting/i)).toBeInTheDocument();
  });

  it("shows disconnected banner", () => {
    setSession("disconnected");
    renderBanner();
    expect(screen.getByText(/disconnected/i)).toBeInTheDocument();
  });

  it("shows authenticating banner", () => {
    setSession("authenticating");
    renderBanner();
    expect(screen.getByText(/authenticating/i)).toBeInTheDocument();
  });

  it("shows escalation banner with dismiss button", () => {
    useStore.setState({
      notifications: {
        escalationBanner: { workspaceId: "ws-urgent", escalationId: "esc-1" },
      },
    });
    renderBanner();
    expect(screen.getByText(/escalated/i)).toBeInTheDocument();
    // Dismiss button (×) should exist
    const closeBtn = screen.getByRole("button", { name: /×|close|dismiss/i }).closest("button")
      ?? screen.getByText("×");
    expect(closeBtn).toBeInTheDocument();
  });
});
