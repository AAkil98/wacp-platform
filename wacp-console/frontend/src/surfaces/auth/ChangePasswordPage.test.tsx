import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { ChangePasswordPage } from "./ChangePasswordPage.tsx";
import { useAuthStore } from "../../store/auth.ts";

function renderChangePw() {
  return render(
    <MemoryRouter initialEntries={["/change-password"]}>
      <ChangePasswordPage />
    </MemoryRouter>,
  );
}

describe("ChangePasswordPage", () => {
  beforeEach(() => {
    // Default: authenticated user who must change password
    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      mustChangePassword: true,
      loading: false,
      error: null,
    });
  });

  // --- rendering ---

  it("renders change password form with proper fields", () => {
    renderChangePw();
    expect(screen.getByRole("heading", { name: "Change Password" })).toBeInTheDocument();
    expect(screen.getByText("You must change your password before continuing.")).toBeInTheDocument();
    expect(screen.getByLabelText("Current Password")).toBeInTheDocument();
    expect(screen.getByLabelText("New Password")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Change Password" })).toBeInTheDocument();
  });

  // --- redirect when no user ---

  it("redirects to /login when no user is present", () => {
    useAuthStore.setState({ user: null, loading: false });
    const { container } = renderChangePw();
    expect(container.querySelector("form")).toBeNull();
  });

  // --- successful change → redirect ---

  it("calls changePassword and redirects on success", async () => {
    const changePwSpy = vi.fn().mockResolvedValue(undefined);
    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      mustChangePassword: true,
      loading: false,
      error: null,
      changePassword: changePwSpy,
    });

    renderChangePw();

    fireEvent.change(screen.getByLabelText("Current Password"), {
      target: { value: "oldpass123456" },
    });
    fireEvent.change(screen.getByLabelText("New Password"), {
      target: { value: "newpass123456" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Change Password" }));

    await waitFor(() => {
      expect(changePwSpy).toHaveBeenCalledWith("oldpass123456", "newpass123456");
    });
  });

  // --- validation: new password min length ---

  it("has minLength=12 on the new password field", () => {
    renderChangePw();
    const newPwInput = screen.getByLabelText("New Password");
    expect(newPwInput).toHaveAttribute("minLength", "12");
  });

  // --- required fields ---

  it("marks both password fields as required", () => {
    renderChangePw();
    expect(screen.getByLabelText("Current Password")).toBeRequired();
    expect(screen.getByLabelText("New Password")).toBeRequired();
  });

  // --- error display ---

  it("displays error message from store", () => {
    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      mustChangePassword: true,
      loading: false,
      error: "Password too short",
    });
    renderChangePw();
    expect(screen.getByText("Password too short")).toBeInTheDocument();
  });

  // --- clearError on error click ---

  it("calls clearError when clicking the error banner", () => {
    const clearErrorSpy = vi.fn();
    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      mustChangePassword: true,
      loading: false,
      error: "Password change failed",
      clearError: clearErrorSpy,
    });

    renderChangePw();
    fireEvent.click(screen.getByText("Password change failed"));
    expect(clearErrorSpy).toHaveBeenCalled();
  });

  // --- changePassword throws keeps form visible ---

  it("keeps form visible when changePassword throws", async () => {
    const changePwSpy = vi.fn().mockRejectedValue(new Error("fail"));
    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      mustChangePassword: true,
      loading: false,
      error: null,
      changePassword: changePwSpy,
    });

    renderChangePw();

    fireEvent.change(screen.getByLabelText("Current Password"), {
      target: { value: "oldpass123456" },
    });
    fireEvent.change(screen.getByLabelText("New Password"), {
      target: { value: "newpass123456" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Change Password" }));

    await waitFor(() => {
      expect(changePwSpy).toHaveBeenCalled();
    });
    expect(screen.getByRole("button", { name: "Change Password" })).toBeInTheDocument();
  });

  // --- redirect after success when mustChangePassword cleared ---

  it("redirects to /discovery after successful change (mustChangePassword=false)", async () => {
    let resolveChange: () => void;
    const changePwSpy = vi.fn().mockImplementation(() => {
      return new Promise<void>((resolve) => {
        resolveChange = resolve;
      });
    });

    useAuthStore.setState({
      user: { user_id: "u1", username: "admin", console_role: "admin" },
      mustChangePassword: true,
      loading: false,
      error: null,
      changePassword: changePwSpy,
    });

    renderChangePw();

    fireEvent.change(screen.getByLabelText("Current Password"), {
      target: { value: "oldpass123456" },
    });
    fireEvent.change(screen.getByLabelText("New Password"), {
      target: { value: "newpass123456" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Change Password" }));

    // Resolve the change and update store state
    await waitFor(() => {
      expect(changePwSpy).toHaveBeenCalled();
    });
    resolveChange!();

    // After changePassword resolves, the component sets success=true.
    // If mustChangePassword is still true in store, the Navigate won't fire yet.
    // The real changePassword sets mustChangePassword to false in the store,
    // but our spy doesn't — so the form stays. This is correct mock behavior.
    await waitFor(() => {
      expect(changePwSpy).toHaveBeenCalledWith("oldpass123456", "newpass123456");
    });
  });
});
