import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect, beforeEach } from "vitest";
import { App } from "./App.tsx";
import { useAuthStore } from "./store/auth.ts";

// Surfaces are lazy-loaded per frontend-perf-plan F6 — Suspense shows a
// "Loading…" fallback until the chunk resolves. Tests await the post-load
// content via `waitFor`. DiscoveryPage is the only eager import (cold-start
// destination); its tests can stay sync.

describe("App", () => {
  beforeEach(() => {
    // Reset auth store to unauthenticated
    useAuthStore.setState({ user: null, loading: false, mustChangePassword: false, error: null });
  });

  it("renders login page when unauthenticated", async () => {
    render(
      <MemoryRouter initialEntries={["/discovery"]}>
        <App />
      </MemoryRouter>,
    );
    await waitFor(() => expect(screen.getByText("WACP Console")).toBeInTheDocument());
    expect(screen.getByText("Sign In")).toBeInTheDocument();
  });

  it("renders login page at /login", async () => {
    render(
      <MemoryRouter initialEntries={["/login"]}>
        <App />
      </MemoryRouter>,
    );
    await waitFor(() => expect(screen.getByText("Sign In")).toBeInTheDocument());
  });

  it("renders discovery when authenticated", () => {
    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      loading: false,
    });
    render(
      <MemoryRouter initialEntries={["/discovery"]}>
        <App />
      </MemoryRouter>,
    );
    expect(screen.getByText("Discovery Browser")).toBeInTheDocument();
  });
});
