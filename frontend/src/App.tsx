import { Routes, Route, Navigate } from "react-router";
import { DiscoveryPage } from "./surfaces/discovery/DiscoveryPage.tsx";
import { ProfilesPage } from "./surfaces/profiles/ProfilesPage.tsx";
import { SessionsPage } from "./surfaces/sessions/SessionsPage.tsx";
import { OversightPage } from "./surfaces/oversight/OversightPage.tsx";
import { LoginPage } from "./surfaces/auth/LoginPage.tsx";
import { SettingsPage } from "./surfaces/settings/SettingsPage.tsx";

export function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route path="/discovery" element={<DiscoveryPage />} />
      <Route path="/profiles" element={<ProfilesPage />} />
      <Route path="/sessions" element={<SessionsPage />} />
      <Route path="/sessions/:id/oversight" element={<OversightPage />} />
      <Route path="/settings" element={<SettingsPage />} />
      <Route path="/" element={<Navigate to="/discovery" replace />} />
    </Routes>
  );
}
