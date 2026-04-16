import { describe, it, expect, vi, beforeEach } from "vitest";
import { useAuthStore } from "./auth";
import { ApiError } from "../api/client";

// Mock the api module
vi.mock("../api/client", async () => {
  const actual = await vi.importActual<typeof import("../api/client")>("../api/client");
  return {
    ...actual,
    api: {
      get: vi.fn(),
      post: vi.fn(),
      put: vi.fn(),
      patch: vi.fn(),
      delete: vi.fn(),
    },
  };
});

import { api } from "../api/client";

const mockedApi = api as {
  get: ReturnType<typeof vi.fn>;
  post: ReturnType<typeof vi.fn>;
  put: ReturnType<typeof vi.fn>;
  patch: ReturnType<typeof vi.fn>;
  delete: ReturnType<typeof vi.fn>;
};

function resetStore() {
  useAuthStore.setState({
    user: null,
    loading: true,
    error: null,
    mustChangePassword: false,
  });
}

describe("useAuthStore", () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
  });

  // ---- Initial state ----
  describe("initial state", () => {
    it("has null user", () => {
      expect(useAuthStore.getState().user).toBeNull();
    });

    it("has loading true", () => {
      expect(useAuthStore.getState().loading).toBe(true);
    });

    it("has null error", () => {
      expect(useAuthStore.getState().error).toBeNull();
    });

    it("has mustChangePassword false", () => {
      expect(useAuthStore.getState().mustChangePassword).toBe(false);
    });
  });

  // ---- login ----
  describe("login", () => {
    const loginResponse = {
      user_id: "u1",
      username: "admin",
      console_role: "operator",
      must_change_password: false,
    };

    it("sets loading and clears error before the request", async () => {
      useAuthStore.setState({ error: "old error", loading: false });
      mockedApi.post.mockResolvedValueOnce(loginResponse);

      const statesObserved: { loading: boolean; error: string | null }[] = [];
      const unsub = useAuthStore.subscribe((s) => {
        statesObserved.push({ loading: s.loading, error: s.error });
      });

      await useAuthStore.getState().login("admin", "pass");
      unsub();

      // First state change should have cleared error and set loading
      expect(statesObserved[0]).toEqual({ loading: true, error: null });
    });

    it("sets user on successful login without password change", async () => {
      mockedApi.post.mockResolvedValueOnce(loginResponse);

      await useAuthStore.getState().login("admin", "pass");

      const state = useAuthStore.getState();
      expect(state.user).toEqual({
        user_id: "u1",
        username: "admin",
        console_role: "operator",
      });
      expect(state.mustChangePassword).toBe(false);
      expect(state.loading).toBe(false);
      expect(state.error).toBeNull();
    });

    it("calls api.post with correct path and body", async () => {
      mockedApi.post.mockResolvedValueOnce(loginResponse);

      await useAuthStore.getState().login("myuser", "mypass");

      expect(mockedApi.post).toHaveBeenCalledWith("/api/auth/login", {
        username: "myuser",
        password: "mypass",
      });
    });

    it("sets mustChangePassword when response requires it", async () => {
      mockedApi.post.mockResolvedValueOnce({
        ...loginResponse,
        must_change_password: true,
      });

      await useAuthStore.getState().login("admin", "pass");

      const state = useAuthStore.getState();
      expect(state.mustChangePassword).toBe(true);
      expect(state.user).not.toBeNull();
      expect(state.loading).toBe(false);
    });

    it("sets error message from ApiError with detail.message", async () => {
      const apiError = new ApiError(401, "UNAUTHORIZED", { message: "Bad creds" });
      mockedApi.post.mockRejectedValueOnce(apiError);

      await expect(useAuthStore.getState().login("x", "y")).rejects.toThrow();

      const state = useAuthStore.getState();
      expect(state.error).toBe("Bad creds");
      expect(state.loading).toBe(false);
      expect(state.user).toBeNull();
    });

    it("falls back to ApiError.code when detail has no message", async () => {
      const apiError = new ApiError(403, "FORBIDDEN");
      mockedApi.post.mockRejectedValueOnce(apiError);

      await expect(useAuthStore.getState().login("x", "y")).rejects.toThrow();

      expect(useAuthStore.getState().error).toBe("FORBIDDEN");
    });

    it("sets generic message for non-ApiError failures", async () => {
      mockedApi.post.mockRejectedValueOnce(new Error("network down"));

      await expect(useAuthStore.getState().login("x", "y")).rejects.toThrow();

      expect(useAuthStore.getState().error).toBe("Login failed");
    });

    it("re-throws the original error", async () => {
      const apiError = new ApiError(401, "UNAUTHORIZED");
      mockedApi.post.mockRejectedValueOnce(apiError);

      await expect(useAuthStore.getState().login("x", "y")).rejects.toBe(apiError);
    });
  });

  // ---- logout ----
  describe("logout", () => {
    it("calls api.post to /api/auth/logout", async () => {
      mockedApi.post.mockResolvedValueOnce(undefined);

      await useAuthStore.getState().logout();

      expect(mockedApi.post).toHaveBeenCalledWith("/api/auth/logout");
    });

    it("clears user, loading, and mustChangePassword on success", async () => {
      useAuthStore.setState({
        user: { user_id: "u1", username: "admin", console_role: "operator" },
        loading: true,
        mustChangePassword: true,
      });
      mockedApi.post.mockResolvedValueOnce(undefined);

      await useAuthStore.getState().logout();

      const state = useAuthStore.getState();
      expect(state.user).toBeNull();
      expect(state.loading).toBe(false);
      expect(state.mustChangePassword).toBe(false);
    });

    it("clears state even when logout api call fails", async () => {
      useAuthStore.setState({
        user: { user_id: "u1", username: "admin", console_role: "operator" },
      });
      mockedApi.post.mockRejectedValueOnce(new Error("network error"));

      // logout uses try/finally without catch, so the error propagates
      await expect(useAuthStore.getState().logout()).rejects.toThrow("network error");

      const state = useAuthStore.getState();
      expect(state.user).toBeNull();
      expect(state.loading).toBe(false);
      expect(state.mustChangePassword).toBe(false);
    });
  });

  // ---- checkSession ----
  describe("checkSession", () => {
    it("sets user on successful whoami", async () => {
      const user = { user_id: "u2", username: "ops", console_role: "viewer" };
      mockedApi.get.mockResolvedValueOnce(user);

      await useAuthStore.getState().checkSession();

      const state = useAuthStore.getState();
      expect(state.user).toEqual(user);
      expect(state.loading).toBe(false);
    });

    it("calls api.get with /api/auth/whoami", async () => {
      mockedApi.get.mockResolvedValueOnce({ user_id: "u1", username: "a", console_role: "x" });

      await useAuthStore.getState().checkSession();

      expect(mockedApi.get).toHaveBeenCalledWith("/api/auth/whoami");
    });

    it("sets user to null on whoami failure", async () => {
      useAuthStore.setState({
        user: { user_id: "u1", username: "old", console_role: "op" },
        loading: true,
      });
      mockedApi.get.mockRejectedValueOnce(new Error("401"));

      await useAuthStore.getState().checkSession();

      const state = useAuthStore.getState();
      expect(state.user).toBeNull();
      expect(state.loading).toBe(false);
    });
  });

  // ---- changePassword ----
  describe("changePassword", () => {
    it("clears error at the start", async () => {
      useAuthStore.setState({ error: "previous error" });
      mockedApi.post.mockResolvedValueOnce(undefined);

      await useAuthStore.getState().changePassword("old", "new");

      expect(useAuthStore.getState().error).toBeNull();
    });

    it("calls api.post with correct path and body", async () => {
      mockedApi.post.mockResolvedValueOnce(undefined);

      await useAuthStore.getState().changePassword("curr", "next");

      expect(mockedApi.post).toHaveBeenCalledWith("/api/auth/change-password", {
        current_password: "curr",
        new_password: "next",
      });
    });

    it("clears mustChangePassword on success", async () => {
      useAuthStore.setState({ mustChangePassword: true });
      mockedApi.post.mockResolvedValueOnce(undefined);

      await useAuthStore.getState().changePassword("old", "new");

      expect(useAuthStore.getState().mustChangePassword).toBe(false);
    });

    it("sets error from ApiError detail.message on failure", async () => {
      const apiError = new ApiError(400, "WEAK_PASSWORD", { message: "Too short" });
      mockedApi.post.mockRejectedValueOnce(apiError);

      await expect(useAuthStore.getState().changePassword("old", "x")).rejects.toThrow();

      expect(useAuthStore.getState().error).toBe("Too short");
    });

    it("falls back to ApiError.code when detail has no message", async () => {
      const apiError = new ApiError(400, "VALIDATION_ERROR");
      mockedApi.post.mockRejectedValueOnce(apiError);

      await expect(useAuthStore.getState().changePassword("old", "x")).rejects.toThrow();

      expect(useAuthStore.getState().error).toBe("VALIDATION_ERROR");
    });

    it("sets generic message for non-ApiError failures", async () => {
      mockedApi.post.mockRejectedValueOnce(new TypeError("fetch failed"));

      await expect(useAuthStore.getState().changePassword("old", "x")).rejects.toThrow();

      expect(useAuthStore.getState().error).toBe("Password change failed");
    });

    it("re-throws the original error", async () => {
      const err = new ApiError(400, "WEAK_PASSWORD");
      mockedApi.post.mockRejectedValueOnce(err);

      await expect(useAuthStore.getState().changePassword("old", "x")).rejects.toBe(err);
    });

    it("does not clear mustChangePassword on failure", async () => {
      useAuthStore.setState({ mustChangePassword: true });
      mockedApi.post.mockRejectedValueOnce(new ApiError(400, "FAIL"));

      await expect(useAuthStore.getState().changePassword("old", "x")).rejects.toThrow();

      expect(useAuthStore.getState().mustChangePassword).toBe(true);
    });
  });

  // ---- clearError ----
  describe("clearError", () => {
    it("sets error to null", () => {
      useAuthStore.setState({ error: "some error" });

      useAuthStore.getState().clearError();

      expect(useAuthStore.getState().error).toBeNull();
    });

    it("is a no-op when error is already null", () => {
      useAuthStore.setState({ error: null });

      useAuthStore.getState().clearError();

      expect(useAuthStore.getState().error).toBeNull();
    });
  });
});
