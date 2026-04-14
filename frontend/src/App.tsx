import { useEffect } from "react";
import { Routes, Route, Navigate } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Layout } from "./components/Layout";
import { LoginPage } from "./surfaces/auth/LoginPage";
import { ChangePasswordPage } from "./surfaces/auth/ChangePasswordPage";
import { DiscoveryPage } from "./surfaces/discovery/DiscoveryPage";
import { ProfilesPage } from "./surfaces/profiles/ProfilesPage";
import { SessionsPage } from "./surfaces/sessions/SessionsPage";
import { OversightPage } from "./surfaces/oversight/OversightPage";
import { SettingsPage } from "./surfaces/settings/SettingsPage";
import { useAuthStore } from "./store/auth";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 30_000,
    },
  },
});

function AuthInit({ children }: { children: React.ReactNode }) {
  const { checkSession } = useAuthStore();
  useEffect(() => { void checkSession(); }, [checkSession]);
  return <>{children}</>;
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthInit>
        <Routes>
          {/* Public routes */}
          <Route path="/login" element={<LoginPage />} />
          <Route path="/change-password" element={<ChangePasswordPage />} />

          {/* Authenticated routes */}
          <Route element={<Layout />}>
            <Route path="/discovery" element={<DiscoveryPage />} />
            <Route path="/profiles" element={<ProfilesPage />} />
            <Route path="/sessions" element={<SessionsPage />} />
            <Route path="/sessions/:id/oversight" element={<OversightPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            {/* Admin routes use same Layout — permission checked in component */}
            <Route path="/admin/users" element={<div>User Management (Phase 5)</div>} />
            <Route path="/admin/audit" element={<div>Audit Log (Phase 5)</div>} />
          </Route>

          <Route path="/" element={<Navigate to="/discovery" replace />} />
        </Routes>
      </AuthInit>
    </QueryClientProvider>
  );
}
