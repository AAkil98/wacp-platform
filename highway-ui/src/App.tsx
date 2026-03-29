import { BrowserRouter, Routes, Route, Navigate } from "react-router";
import { useStore } from "@/store";
import { MainLayout } from "@/components/layout/MainLayout.js";
import { LoginScreen } from "@/components/layout/LoginScreen.js";
import { TrailViewer } from "@/components/trail/TrailViewer.js";
import { GatePanel } from "@/components/gates/GatePanel.js";
import { EscalationPanel } from "@/components/escalations/EscalationPanel.js";
import { WorkspaceTreeView } from "@/components/workspaces/WorkspaceTreeView.js";
import { TaskGraphView } from "@/components/tasks/TaskGraphView.js";
import { InjectionForm } from "@/components/injection/InjectionForm.js";
import { SettingsPanel } from "@/components/settings/SettingsPanel.js";
import { useState } from "react";

function AuthGate({ children }: { children: React.ReactNode }) {
  const sessionState = useStore((s) => s.session.state);
  const setSession = useStore((s) => s.setSession);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleLogin = async (token: string) => {
    setLoading(true);
    setError(null);
    setSession("authenticating");

    try {
      // In production, this calls the Authenticate RPC via the transport layer.
      // For now, accept any non-empty token for development.
      await new Promise((resolve) => setTimeout(resolve, 300));
      setSession("connected", `user-${token.slice(0, 8)}`, [
        "inject",
        "gate_respond",
        "escalation_respond",
      ]);
    } catch {
      setError("Authentication failed");
      setSession("disconnected");
    } finally {
      setLoading(false);
    }
  };

  if (sessionState === "disconnected" || sessionState === "authenticating") {
    return (
      <LoginScreen onLogin={handleLogin} error={error} loading={loading} />
    );
  }

  return <>{children}</>;
}

export function App() {
  return (
    <BrowserRouter>
      <AuthGate>
        <Routes>
          <Route element={<MainLayout />}>
            <Route path="/trail" element={<TrailViewer />} />
            <Route path="/gates" element={<GatePanel />} />
            <Route path="/escalations" element={<EscalationPanel />} />
            <Route path="/workspaces" element={<WorkspaceTreeView />} />
            <Route path="/tasks" element={<TaskGraphView />} />
            <Route path="/inject" element={<InjectionForm />} />
            <Route path="/settings" element={<SettingsPanel />} />
            <Route path="*" element={<Navigate to="/trail" replace />} />
          </Route>
        </Routes>
      </AuthGate>
    </BrowserRouter>
  );
}
