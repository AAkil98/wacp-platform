import { useState, type FormEvent } from "react";
import { Navigate } from "react-router";
import { useAuthStore } from "../../store/auth";

export function LoginPage() {
  const { user, error, loading, mustChangePassword, login, clearError } = useAuthStore();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  if (user && !mustChangePassword) {
    return <Navigate to="/discovery" replace />;
  }

  if (user && mustChangePassword) {
    return <Navigate to="/change-password" replace />;
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    try {
      await login(username, password);
    } catch {
      // error is set in store
    }
  }

  return (
    <div className="flex items-center justify-center min-h-screen" style={{ backgroundColor: "var(--color-bg-secondary)" }}>
      <div className="w-full max-w-sm p-6 rounded-lg shadow-md" style={{ backgroundColor: "var(--color-bg)" }}>
        <h1 className="text-xl font-semibold mb-4 text-center">WACP Console</h1>

        {error && (
          <div
            className="mb-4 p-3 rounded text-sm"
            style={{ backgroundColor: "var(--color-danger)", color: "#fff" }}
            onClick={clearError}
          >
            {error}
          </div>
        )}

        <form onSubmit={(e) => void handleSubmit(e)} className="space-y-4">
          <div>
            <label htmlFor="login-username" className="block text-sm mb-1" style={{ color: "var(--color-text-secondary)" }}>Username</label>
            <input id="login-username" type="text" value={username} onChange={(e) => setUsername(e.target.value)}
              className="w-full px-3 py-2 border rounded text-sm"
              style={{ borderColor: "var(--color-border)", backgroundColor: "var(--color-bg)", color: "var(--color-text)" }}
              autoFocus required />
          </div>
          <div>
            <label htmlFor="login-password" className="block text-sm mb-1" style={{ color: "var(--color-text-secondary)" }}>Password</label>
            <input id="login-password" type="password" value={password} onChange={(e) => setPassword(e.target.value)}
              className="w-full px-3 py-2 border rounded text-sm"
              style={{ borderColor: "var(--color-border)", backgroundColor: "var(--color-bg)", color: "var(--color-text)" }}
              required />
          </div>
          <button type="submit" disabled={loading}
            className="w-full py-2 rounded text-sm font-medium text-white"
            style={{ backgroundColor: "var(--color-accent)" }}>
            {loading ? "Signing in..." : "Sign In"}
          </button>
        </form>
      </div>
    </div>
  );
}
