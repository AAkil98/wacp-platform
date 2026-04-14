import { create } from "zustand";
import { api, ApiError } from "../api/client";

interface User {
  user_id: string;
  username: string;
  console_role: string;
}

interface AuthState {
  user: User | null;
  loading: boolean;
  error: string | null;
  mustChangePassword: boolean;

  login: (username: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  checkSession: () => Promise<void>;
  changePassword: (current: string, newPassword: string) => Promise<void>;
  clearError: () => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  user: null,
  loading: true,
  error: null,
  mustChangePassword: false,

  login: async (username, password) => {
    set({ error: null, loading: true });
    try {
      const result = await api.post<{
        user_id: string;
        username: string;
        console_role: string;
        must_change_password: boolean;
      }>("/api/auth/login", { username, password });
      if (result.must_change_password) {
        set({
          user: { user_id: result.user_id, username: result.username, console_role: result.console_role },
          mustChangePassword: true,
          loading: false,
        });
      } else {
        set({
          user: { user_id: result.user_id, username: result.username, console_role: result.console_role },
          mustChangePassword: false,
          loading: false,
        });
      }
    } catch (e) {
      const msg = e instanceof ApiError
        ? (e.detail as { message?: string })?.message ?? e.code
        : "Login failed";
      set({ error: msg, loading: false });
      throw e;
    }
  },

  logout: async () => {
    try {
      await api.post("/api/auth/logout");
    } finally {
      set({ user: null, loading: false, mustChangePassword: false });
    }
  },

  checkSession: async () => {
    try {
      const user = await api.get<User>("/api/auth/whoami");
      set({ user, loading: false });
    } catch {
      set({ user: null, loading: false });
    }
  },

  changePassword: async (current, newPassword) => {
    set({ error: null });
    try {
      await api.post("/api/auth/change-password", {
        current_password: current,
        new_password: newPassword,
      });
      set({ mustChangePassword: false });
    } catch (e) {
      const msg = e instanceof ApiError
        ? (e.detail as { message?: string })?.message ?? e.code
        : "Password change failed";
      set({ error: msg });
      throw e;
    }
  },

  clearError: () => set({ error: null }),
}));
