import { lazy, Suspense, useEffect } from "react";
import { Routes, Route, Navigate } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Layout } from "./components/Layout";
import { DiscoveryPage } from "./surfaces/discovery/DiscoveryPage";
import { AdminGuard } from "./components/AdminGuard";
import { useAuthStore } from "./store/auth";

// HEALTH-LOG §4 item 5 / frontend-perf-plan F6 — route-level lazy loading.
// DiscoveryPage stays eager (it's the cold-start destination via "/" redirect);
// every other surface is split into its own chunk so the initial bundle only
// pays for the landing view.
const LoginPage = lazy(() => import("./surfaces/auth/LoginPage").then((m) => ({ default: m.LoginPage })));
const ChangePasswordPage = lazy(() =>
  import("./surfaces/auth/ChangePasswordPage").then((m) => ({ default: m.ChangePasswordPage })),
);
const ProfilesPage = lazy(() =>
  import("./surfaces/profiles/ProfilesPage").then((m) => ({ default: m.ProfilesPage })),
);
const SessionsPage = lazy(() =>
  import("./surfaces/sessions/SessionsPage").then((m) => ({ default: m.SessionsPage })),
);
const OversightPage = lazy(() =>
  import("./surfaces/oversight/OversightPage").then((m) => ({ default: m.OversightPage })),
);
const SettingsPage = lazy(() =>
  import("./surfaces/settings/SettingsPage").then((m) => ({ default: m.SettingsPage })),
);
const UsersPage = lazy(() =>
  import("./surfaces/admin/UsersPage").then((m) => ({ default: m.UsersPage })),
);
const AuditLogPage = lazy(() =>
  import("./surfaces/admin/AuditLogPage").then((m) => ({ default: m.AuditLogPage })),
);

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

function PageLoading() {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        color: "var(--color-text-secondary)",
      }}
    >
      Loading…
    </div>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthInit>
        <Suspense fallback={<PageLoading />}>
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
              {/* Admin routes — 403 for non-admins */}
              <Route path="/admin/users" element={<AdminGuard><UsersPage /></AdminGuard>} />
              <Route path="/admin/audit" element={<AdminGuard><AuditLogPage /></AdminGuard>} />
            </Route>

            <Route path="/" element={<Navigate to="/discovery" replace />} />
          </Routes>
        </Suspense>
      </AuthInit>
    </QueryClientProvider>
  );
}
