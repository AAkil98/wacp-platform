import { Outlet, Navigate } from "react-router";
import { useAuthStore } from "../store/auth";
import { Sidebar } from "./Sidebar";
import { SkipToContent } from "./SkipToContent";
import { LiveRegion } from "./LiveRegion";
import { useFocusOnRouteChange } from "../hooks/useFocusOnRouteChange";

export function Layout() {
  const { user, loading, mustChangePassword } = useAuthStore();
  useFocusOnRouteChange();

  if (loading) {
    return (
      <div className="flex items-center justify-center h-screen">
        <span style={{ color: "var(--color-text-muted)" }}>Loading...</span>
      </div>
    );
  }

  if (!user) {
    return <Navigate to="/login" replace />;
  }

  if (mustChangePassword) {
    return <Navigate to="/change-password" replace />;
  }

  return (
    <div className="flex h-screen">
      <SkipToContent />
      <Sidebar />
      <main
        id="main"
        tabIndex={-1}
        className="flex-1 overflow-auto p-6 outline-none"
      >
        <Outlet />
      </main>
      <LiveRegion />
    </div>
  );
}
