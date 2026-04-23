import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ErrorBanner } from "./ErrorBanner";

describe("ErrorBanner", () => {
  it("renders title inside role=alert (SR announces on mount)", () => {
    render(<ErrorBanner variant="error" title="Login failed." />);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Login failed.");
  });

  it("renders description when provided", () => {
    render(
      <ErrorBanner
        variant="error"
        title="Login failed."
        description="Check your credentials and try again."
      />,
    );
    expect(screen.getByText("Check your credentials and try again.")).toBeInTheDocument();
  });

  it("omits dismiss button when onDismiss is absent", () => {
    render(<ErrorBanner variant="warning" title="Pending restart." />);
    expect(screen.queryByRole("button", { name: /dismiss/i })).not.toBeInTheDocument();
  });

  it("renders dismiss button when onDismiss provided; click fires handler", () => {
    const onDismiss = vi.fn();
    render(<ErrorBanner variant="info" title="Saved." onDismiss={onDismiss} />);
    const btn = screen.getByRole("button", { name: /dismiss/i });
    fireEvent.click(btn);
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it.each(["error", "warning", "info"] as const)("renders for variant %s", (variant) => {
    render(<ErrorBanner variant={variant} title={`${variant}-msg`} />);
    expect(screen.getByRole("alert")).toHaveTextContent(`${variant}-msg`);
  });
});
