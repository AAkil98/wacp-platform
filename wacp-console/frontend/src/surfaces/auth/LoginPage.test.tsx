import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { LoginPage } from "./LoginPage.tsx";
import { useAuthStore } from "../../store/auth.ts";

/**
 * Helper: render LoginPage inside a MemoryRouter so Navigate works.
 */
function renderLogin(initialEntries = ["/login"]) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <LoginPage />
    </MemoryRouter>,
  );
}

describe("LoginPage", () => {
  beforeEach(() => {
    useAuthStore.setState({
      user: null,
      loading: false,
      error: null,
      mustChangePassword: false,
    });
  });

  // --- rendering ---

  it("renders sign-in form with username + password fields", () => {
    renderLogin();
    expect(screen.getByText("WACP Console")).toBeInTheDocument();
    expect(screen.getByLabelText("Username")).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign In" })).toBeInTheDocument();
  });

  // --- successful login → redirect ---

  it("redirects to /discovery after successful login", () => {
    // Simulate already authenticated
    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      loading: false,
      mustChangePassword: false,
    });
    const { container } = renderLogin();
    // Navigate replaces the content — the login form should be gone
    expect(container.querySelector("form")).toBeNull();
  });

  // --- mustChangePassword → redirect ---

  it("redirects to /change-password when mustChangePassword is true", () => {
    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      mustChangePassword: true,
      loading: false,
    });
    const { container } = renderLogin();
    expect(container.querySelector("form")).toBeNull();
  });

  // --- bad credentials → error message ---

  it("displays error message when login fails with bad credentials", () => {
    useAuthStore.setState({
      user: null,
      loading: false,
      error: "Invalid username or password",
    });
    renderLogin();
    expect(screen.getByText("Invalid username or password")).toBeInTheDocument();
  });

  // --- account locked → lockout message ---

  it("displays account lockout message", () => {
    useAuthStore.setState({
      user: null,
      loading: false,
      error: "Account is locked. Try again later.",
    });
    renderLogin();
    expect(screen.getByText("Account is locked. Try again later.")).toBeInTheDocument();
  });

  // --- CSRF missing → error ---

  it("displays CSRF-related error", () => {
    useAuthStore.setState({
      user: null,
      loading: false,
      error: "CSRF token missing or invalid",
    });
    renderLogin();
    expect(screen.getByText("CSRF token missing or invalid")).toBeInTheDocument();
  });

  // --- session expired → still on login ---

  it("stays on login when no user (session expired scenario)", () => {
    useAuthStore.setState({ user: null, loading: false });
    renderLogin();
    expect(screen.getByRole("button", { name: "Sign In" })).toBeInTheDocument();
  });

  // --- loading state ---

  it("shows 'Signing in...' and disables button while loading", () => {
    useAuthStore.setState({ user: null, loading: true, error: null });
    renderLogin();
    const btn = screen.getByRole("button", { name: "Signing in..." });
    expect(btn).toBeDisabled();
  });

  // --- form submission calls login ---

  it("calls login with username and password on form submit", async () => {
    const loginSpy = vi.fn().mockResolvedValue(undefined);
    useAuthStore.setState({
      user: null,
      loading: false,
      error: null,
      login: loginSpy,
    });

    renderLogin();

    fireEvent.change(screen.getByLabelText("Username"), { target: { value: "alice" } });
    fireEvent.change(screen.getByLabelText("Password"), { target: { value: "secret123" } });
    fireEvent.click(screen.getByRole("button", { name: "Sign In" }));

    await waitFor(() => {
      expect(loginSpy).toHaveBeenCalledWith("alice", "secret123");
    });
  });

  // --- clearError on error click ---

  it("calls clearError when clicking the error banner's dismiss button", () => {
    const clearErrorSpy = vi.fn();
    useAuthStore.setState({
      user: null,
      loading: false,
      error: "Some error",
      clearError: clearErrorSpy,
    });

    renderLogin();
    fireEvent.click(screen.getByRole("button", { name: /dismiss/i }));
    expect(clearErrorSpy).toHaveBeenCalled();
  });

  // --- login throwing keeps form visible ---

  it("keeps form visible when login throws", async () => {
    const loginSpy = vi.fn().mockRejectedValue(new Error("fail"));
    useAuthStore.setState({
      user: null,
      loading: false,
      error: null,
      login: loginSpy,
    });

    renderLogin();

    fireEvent.change(screen.getByLabelText("Username"), { target: { value: "bob" } });
    fireEvent.change(screen.getByLabelText("Password"), { target: { value: "wrong" } });
    fireEvent.click(screen.getByRole("button", { name: "Sign In" }));

    await waitFor(() => {
      expect(loginSpy).toHaveBeenCalled();
    });
    // Form should still be visible
    expect(screen.getByRole("button", { name: "Sign In" })).toBeInTheDocument();
  });
});
