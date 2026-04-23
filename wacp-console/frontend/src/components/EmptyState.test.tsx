import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders title as a status region so SR announces on reveal", () => {
    render(<EmptyState title="No sessions yet." />);
    const region = screen.getByRole("status");
    expect(region).toHaveTextContent("No sessions yet.");
  });

  it("renders description when provided", () => {
    render(<EmptyState title="No sessions yet." description="Launch one to get started." />);
    expect(screen.getByText("Launch one to get started.")).toBeInTheDocument();
  });

  it("omits description when absent", () => {
    render(<EmptyState title="No sessions yet." />);
    expect(screen.queryByText(/get started/i)).not.toBeInTheDocument();
  });

  it("renders action slot when provided", () => {
    render(
      <EmptyState
        title="No profiles"
        action={<button>Create profile</button>}
      />,
    );
    expect(screen.getByRole("button", { name: /create profile/i })).toBeInTheDocument();
  });

  it("renders icon slot when provided, marked aria-hidden", () => {
    render(
      <EmptyState
        title="No data"
        icon={<svg data-testid="icon" />}
      />,
    );
    const iconWrapper = screen.getByTestId("icon").parentElement;
    expect(iconWrapper).toHaveAttribute("aria-hidden", "true");
  });

  it("accepts 'compact' size without error", () => {
    render(<EmptyState title="No tools" size="compact" />);
    expect(screen.getByRole("status")).toHaveTextContent("No tools");
  });
});
