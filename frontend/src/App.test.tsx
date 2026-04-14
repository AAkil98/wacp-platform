import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect } from "vitest";
import { App } from "./App.tsx";

describe("App", () => {
  it("renders discovery page at /discovery", () => {
    render(
      <MemoryRouter initialEntries={["/discovery"]}>
        <App />
      </MemoryRouter>,
    );
    expect(screen.getByText("Discovery Browser")).toBeInTheDocument();
  });

  it("redirects / to /discovery", () => {
    render(
      <MemoryRouter initialEntries={["/"]}>
        <App />
      </MemoryRouter>,
    );
    expect(screen.getByText("Discovery Browser")).toBeInTheDocument();
  });

  it("renders login page at /login", () => {
    render(
      <MemoryRouter initialEntries={["/login"]}>
        <App />
      </MemoryRouter>,
    );
    expect(screen.getByText("Sign in to continue.")).toBeInTheDocument();
  });
});
