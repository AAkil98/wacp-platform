import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { UsersPage } from "./UsersPage.tsx";

// --- Mock api client ---
vi.mock("../../api/client", () => ({
  api: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    patch: vi.fn(),
    delete: vi.fn(),
  },
  ApiError: class ApiError extends Error {
    status: number;
    code: string;
    detail: unknown;
    constructor(status: number, code: string, detail?: unknown) {
      super(`API error ${status}: ${code}`);
      this.status = status;
      this.code = code;
      this.detail = detail;
    }
  },
}));

import { api } from "../../api/client";

const mockedApi = api as {
  get: ReturnType<typeof vi.fn>;
  post: ReturnType<typeof vi.fn>;
  put: ReturnType<typeof vi.fn>;
  patch: ReturnType<typeof vi.fn>;
  delete: ReturnType<typeof vi.fn>;
};

// --- Helpers ---

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });
}

function renderUsersPage(users: unknown[] = []) {
  mockedApi.get.mockResolvedValue(users);
  const qc = createQueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <UsersPage />
    </QueryClientProvider>,
  );
}

// --- Fixtures ---

const USERS = [
  {
    user_id: "u1",
    username: "admin_one",
    display_name: "Admin One",
    console_role: "admin",
    disabled: false,
    created_at: "2026-01-10T08:00:00Z",
  },
  {
    user_id: "u2",
    username: "operator_two",
    display_name: "Operator Two",
    console_role: "operator",
    disabled: false,
    created_at: "2026-02-15T12:00:00Z",
  },
  {
    user_id: "u3",
    username: "viewer_three",
    display_name: "",
    console_role: "viewer",
    disabled: true,
    created_at: "2026-03-01T09:30:00Z",
  },
];

describe("UsersPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ──────────────────────────────────────────────
  // Rendering
  // ──────────────────────────────────────────────

  describe("rendering", () => {
    it("shows loading state initially", () => {
      // Hang the query so it stays in loading
      mockedApi.get.mockReturnValue(new Promise(() => {}));
      const qc = createQueryClient();
      render(
        <QueryClientProvider client={qc}>
          <UsersPage />
        </QueryClientProvider>,
      );
      expect(screen.getByText("Loading users...")).toBeInTheDocument();
    });

    it("renders page heading and create user button", async () => {
      renderUsersPage(USERS);
      expect(screen.getByText("User Management")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Create User" })).toBeInTheDocument();
    });

    it("renders user table with all users", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });
      expect(screen.getByText("operator_two")).toBeInTheDocument();
      expect(screen.getByText("viewer_three")).toBeInTheDocument();
    });

    it("renders table column headers", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });
      expect(screen.getByText("Username")).toBeInTheDocument();
      expect(screen.getByText("Display Name")).toBeInTheDocument();
      expect(screen.getByText("Role")).toBeInTheDocument();
      expect(screen.getByText("Status")).toBeInTheDocument();
      expect(screen.getByText("Created")).toBeInTheDocument();
      expect(screen.getByText("Actions")).toBeInTheDocument();
    });

    it("shows Active badge for enabled users", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });
      const activeBadges = screen.getAllByText("Active");
      expect(activeBadges.length).toBe(2);
    });

    it("shows Disabled badge for disabled users", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("viewer_three")).toBeInTheDocument();
      });
      expect(screen.getByText("Disabled")).toBeInTheDocument();
    });

    it("shows '--' for empty display name", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("viewer_three")).toBeInTheDocument();
      });
      expect(screen.getByText("--")).toBeInTheDocument();
    });

    it("shows user roles in the table", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });
      expect(screen.getByText("admin")).toBeInTheDocument();
      expect(screen.getByText("operator")).toBeInTheDocument();
      expect(screen.getByText("viewer")).toBeInTheDocument();
    });
  });

  // ──────────────────────────────────────────────
  // User Create
  // ──────────────────────────────────────────────

  describe("user create", () => {
    it("opens create dialog when Create User button clicked", async () => {
      renderUsersPage([]);
      fireEvent.click(screen.getByRole("button", { name: "Create User" }));
      await waitFor(() => {
        expect(screen.getByText("Create User", { selector: "h2" })).toBeInTheDocument();
      });
    });

    it("renders all form fields in the create dialog", async () => {
      renderUsersPage([]);
      fireEvent.click(screen.getByRole("button", { name: "Create User" }));
      await waitFor(() => {
        expect(screen.getByText("Username", { selector: "label" })).toBeInTheDocument();
      });
      expect(screen.getByText("Display Name", { selector: "label" })).toBeInTheDocument();
      expect(screen.getByText("Password", { selector: "label" })).toBeInTheDocument();
      expect(screen.getByText("Role", { selector: "label" })).toBeInTheDocument();
    });

    it("has operator as default role in the create dialog", async () => {
      renderUsersPage([]);
      fireEvent.click(screen.getByRole("button", { name: "Create User" }));
      await waitFor(() => {
        expect(screen.getByText("Username", { selector: "label" })).toBeInTheDocument();
      });
      // The role select (2nd select on page) should default to operator
      const roleSelect = screen.getAllByRole("combobox").find(
        (el) => (el as HTMLSelectElement).value === "operator",
      );
      expect(roleSelect).toBeTruthy();
    });

    it("does not call mutate when username is empty", async () => {
      mockedApi.get.mockResolvedValue([]);
      mockedApi.post.mockResolvedValue({});
      const qc = createQueryClient();
      render(
        <QueryClientProvider client={qc}>
          <UsersPage />
        </QueryClientProvider>,
      );

      await waitFor(() => {
        expect(screen.queryByText("Loading users...")).not.toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: "Create User" }));
      await waitFor(() => {
        expect(screen.getByText("Create User", { selector: "h2" })).toBeInTheDocument();
      });

      // Fill password but leave username empty
      const passwordInput = document.querySelector('input[type="password"]') as HTMLInputElement;
      fireEvent.change(passwordInput, { target: { value: "strongpass123" } });

      fireEvent.click(screen.getByRole("button", { name: "Create" }));

      // mutate should not have been called (guard: !createForm.username)
      await waitFor(() => {
        // Post was only called for the initial data fetch, not for user creation
        const postCalls = mockedApi.post.mock.calls.filter(
          (call: unknown[]) => (call[0] as string) === "/api/users",
        );
        expect(postCalls.length).toBe(0);
      });
    });

    it("does not call mutate when password is empty", async () => {
      mockedApi.get.mockResolvedValue([]);
      mockedApi.post.mockResolvedValue({});
      const qc = createQueryClient();
      render(
        <QueryClientProvider client={qc}>
          <UsersPage />
        </QueryClientProvider>,
      );

      await waitFor(() => {
        expect(screen.queryByText("Loading users...")).not.toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: "Create User" }));
      await waitFor(() => {
        expect(screen.getByText("Create User", { selector: "h2" })).toBeInTheDocument();
      });

      // Fill username but leave password empty
      const inputs = screen.getAllByRole("textbox");
      fireEvent.change(inputs[0], { target: { value: "newuser" } });

      fireEvent.click(screen.getByRole("button", { name: "Create" }));

      await waitFor(() => {
        const postCalls = mockedApi.post.mock.calls.filter(
          (call: unknown[]) => (call[0] as string) === "/api/users",
        );
        expect(postCalls.length).toBe(0);
      });
    });

    it("submits the create form and closes dialog on success", async () => {
      mockedApi.get.mockResolvedValue([]);
      mockedApi.post.mockResolvedValue({ user_id: "u-new", username: "newuser" });
      const qc = createQueryClient();
      render(
        <QueryClientProvider client={qc}>
          <UsersPage />
        </QueryClientProvider>,
      );

      await waitFor(() => {
        expect(screen.queryByText("Loading users...")).not.toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: "Create User" }));
      await waitFor(() => {
        expect(screen.getByText("Create User", { selector: "h2" })).toBeInTheDocument();
      });

      // Fill username
      const inputs = screen.getAllByRole("textbox");
      fireEvent.change(inputs[0], { target: { value: "newuser" } });
      // Fill password
      const passwordInput = document.querySelector('input[type="password"]') as HTMLInputElement;
      fireEvent.change(passwordInput, { target: { value: "securePass1!" } });

      fireEvent.click(screen.getByRole("button", { name: "Create" }));

      await waitFor(() => {
        // Dialog should close after success
        expect(screen.queryByText("Create User", { selector: "h2" })).not.toBeInTheDocument();
      });
    });

    it("closes dialog when Cancel button is clicked", async () => {
      renderUsersPage([]);
      fireEvent.click(screen.getByRole("button", { name: "Create User" }));
      await waitFor(() => {
        expect(screen.getByText("Create User", { selector: "h2" })).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
      expect(screen.queryByText("Create User", { selector: "h2" })).not.toBeInTheDocument();
    });

    it("closes dialog when clicking overlay background", async () => {
      renderUsersPage([]);

      fireEvent.click(screen.getByRole("button", { name: "Create User" }));
      await waitFor(() => {
        expect(screen.getByText("Create User", { selector: "h2" })).toBeInTheDocument();
      });

      // Click the overlay (parent of the dialog box)
      const overlay = screen.getByText("Create User", { selector: "h2" }).closest("div[style]")?.parentElement;
      if (overlay) {
        fireEvent.click(overlay);
      }
      expect(screen.queryByText("Create User", { selector: "h2" })).not.toBeInTheDocument();
    });
  });

  // ──────────────────────────────────────────────
  // User Disable / Enable
  // ──────────────────────────────────────────────

  describe("user disable/enable", () => {
    it("renders Disable button for active users", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });
      const disableButtons = screen.getAllByRole("button", { name: "Disable" });
      expect(disableButtons.length).toBe(2); // admin_one and operator_two
    });

    it("renders Enable button for disabled users", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("viewer_three")).toBeInTheDocument();
      });
      expect(screen.getByRole("button", { name: "Enable" })).toBeInTheDocument();
    });

    it("calls PATCH to disable an active user", async () => {
      mockedApi.patch.mockResolvedValue({});
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const disableButtons = screen.getAllByRole("button", { name: "Disable" });
      fireEvent.click(disableButtons[0]); // disable first active user (admin_one)

      await waitFor(() => {
        expect(mockedApi.patch).toHaveBeenCalledWith("/api/users/u1", { disabled: true });
      });
    });

    it("calls PATCH to enable a disabled user", async () => {
      mockedApi.patch.mockResolvedValue({});
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("viewer_three")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: "Enable" }));

      await waitFor(() => {
        expect(mockedApi.patch).toHaveBeenCalledWith("/api/users/u3", { disabled: false });
      });
    });

    it("shows error when disable fails (e.g. LAST_ADMIN guard)", async () => {
      mockedApi.patch.mockRejectedValue(new Error("Cannot disable last admin"));
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const disableButtons = screen.getAllByRole("button", { name: "Disable" });
      fireEvent.click(disableButtons[0]);

      await waitFor(() => {
        expect(screen.getByText("Failed to disable user.")).toBeInTheDocument();
      });
    });

    it("shows error when enable fails", async () => {
      mockedApi.patch.mockRejectedValue(new Error("Server error"));
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("viewer_three")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: "Enable" }));

      await waitFor(() => {
        expect(screen.getByText("Failed to enable user.")).toBeInTheDocument();
      });
    });
  });

  // ──────────────────────────────────────────────
  // Reset Password
  // ──────────────────────────────────────────────

  describe("reset password", () => {
    it("renders Reset Password button for each user", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });
      const resetButtons = screen.getAllByRole("button", { name: "Reset Password" });
      expect(resetButtons.length).toBe(3);
    });

    it("calls POST to reset a user password", async () => {
      mockedApi.post.mockResolvedValue({});
      // Suppress alert
      vi.spyOn(window, "alert").mockImplementation(() => {});
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("operator_two")).toBeInTheDocument();
      });

      const resetButtons = screen.getAllByRole("button", { name: "Reset Password" });
      fireEvent.click(resetButtons[1]); // operator_two

      await waitFor(() => {
        expect(mockedApi.post).toHaveBeenCalledWith("/api/users/u2/reset-password");
      });
    });

    it("shows alert on successful password reset", async () => {
      mockedApi.post.mockResolvedValue({});
      const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const resetButtons = screen.getAllByRole("button", { name: "Reset Password" });
      fireEvent.click(resetButtons[0]);

      await waitFor(() => {
        expect(alertSpy).toHaveBeenCalledWith(
          "Password has been reset. The user must change it on next login.",
        );
      });
    });

    it("shows error when password reset fails", async () => {
      mockedApi.post.mockRejectedValue(new Error("Server error"));
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const resetButtons = screen.getAllByRole("button", { name: "Reset Password" });
      fireEvent.click(resetButtons[0]);

      await waitFor(() => {
        expect(screen.getByText("Failed to reset password.")).toBeInTheDocument();
      });
    });
  });

  // ──────────────────────────────────────────────
  // Unlock User
  // ──────────────────────────────────────────────

  describe("unlock user", () => {
    it("renders Unlock button for each user", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });
      const unlockButtons = screen.getAllByRole("button", { name: "Unlock" });
      expect(unlockButtons.length).toBe(3);
    });

    it("calls POST to unlock a locked user", async () => {
      mockedApi.post.mockResolvedValue({});
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("operator_two")).toBeInTheDocument();
      });

      const unlockButtons = screen.getAllByRole("button", { name: "Unlock" });
      fireEvent.click(unlockButtons[1]); // operator_two

      await waitFor(() => {
        expect(mockedApi.post).toHaveBeenCalledWith("/api/users/u2/unlock");
      });
    });

    it("shows error when unlock fails", async () => {
      mockedApi.post.mockRejectedValue(new Error("Server error"));
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const unlockButtons = screen.getAllByRole("button", { name: "Unlock" });
      fireEvent.click(unlockButtons[0]);

      await waitFor(() => {
        expect(screen.getByText("Failed to unlock user.")).toBeInTheDocument();
      });
    });
  });

  // ──────────────────────────────────────────────
  // Change Role
  // ──────────────────────────────────────────────

  describe("change role", () => {
    it("renders Change Role button for each user", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });
      const changeRoleButtons = screen.getAllByRole("button", { name: "Change Role" });
      expect(changeRoleButtons.length).toBe(3);
    });

    it("shows role select dropdown when Change Role is clicked", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const changeRoleButtons = screen.getAllByRole("button", { name: "Change Role" });
      fireEvent.click(changeRoleButtons[0]);

      // Should show a select with roles + OK and X buttons
      await waitFor(() => {
        expect(screen.getByRole("button", { name: "OK" })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "X" })).toBeInTheDocument();
      });
    });

    it("pre-selects the current role in the dropdown", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const changeRoleButtons = screen.getAllByRole("button", { name: "Change Role" });
      fireEvent.click(changeRoleButtons[0]); // admin_one (role = admin)

      await waitFor(() => {
        const selects = screen.getAllByRole("combobox");
        const roleSelect = selects.find((s) => (s as HTMLSelectElement).value === "admin");
        expect(roleSelect).toBeTruthy();
      });
    });

    it("calls PATCH with new role when OK is clicked", async () => {
      mockedApi.patch.mockResolvedValue({});
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("operator_two")).toBeInTheDocument();
      });

      const changeRoleButtons = screen.getAllByRole("button", { name: "Change Role" });
      fireEvent.click(changeRoleButtons[1]); // operator_two

      await waitFor(() => {
        expect(screen.getByRole("button", { name: "OK" })).toBeInTheDocument();
      });

      // Change dropdown to viewer
      const selects = screen.getAllByRole("combobox");
      const roleSelect = selects.find((s) => (s as HTMLSelectElement).value === "operator");
      fireEvent.change(roleSelect!, { target: { value: "viewer" } });

      fireEvent.click(screen.getByRole("button", { name: "OK" }));

      await waitFor(() => {
        expect(mockedApi.patch).toHaveBeenCalledWith("/api/users/u2", { console_role: "viewer" });
      });
    });

    it("cancels role change when X button is clicked", async () => {
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const changeRoleButtons = screen.getAllByRole("button", { name: "Change Role" });
      fireEvent.click(changeRoleButtons[0]);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: "X" })).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole("button", { name: "X" }));

      // X and OK buttons should disappear
      expect(screen.queryByRole("button", { name: "OK" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "X" })).not.toBeInTheDocument();
    });

    it("shows error when role change fails", async () => {
      mockedApi.patch.mockRejectedValue(new Error("Server error"));
      renderUsersPage(USERS);
      await waitFor(() => {
        expect(screen.getByText("admin_one")).toBeInTheDocument();
      });

      const changeRoleButtons = screen.getAllByRole("button", { name: "Change Role" });
      fireEvent.click(changeRoleButtons[0]);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: "OK" })).toBeInTheDocument();
      });

      // Select a new role and confirm
      const selects = screen.getAllByRole("combobox");
      const roleSelect = selects.find((s) => (s as HTMLSelectElement).value === "admin");
      fireEvent.change(roleSelect!, { target: { value: "viewer" } });
      fireEvent.click(screen.getByRole("button", { name: "OK" }));

      await waitFor(() => {
        expect(screen.getByText("Failed to change role.")).toBeInTheDocument();
      });
    });
  });

  // ──────────────────────────────────────────────
  // Empty state
  // ──────────────────────────────────────────────

  describe("empty state", () => {
    it("renders an empty table when no users exist", async () => {
      renderUsersPage([]);
      await waitFor(() => {
        expect(screen.queryByText("Loading users...")).not.toBeInTheDocument();
      });
      // Table headers should still be visible
      expect(screen.getByText("Username")).toBeInTheDocument();
      // But no user rows
      expect(screen.queryByRole("button", { name: "Disable" })).not.toBeInTheDocument();
    });
  });
});
