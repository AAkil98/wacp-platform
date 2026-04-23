import { useState, type FormEvent } from "react";
import { Navigate } from "react-router";
import { useBootstrapState } from "../../api/hooks/index";
import { useAuthStore } from "../../store/auth";
import { ErrorBanner } from "../../components/ErrorBanner";
import { EmptyState } from "../../components/EmptyState";

// First-run onboarding — surfaces the bootstrap credential the console
// generates on first launch (per `wcon-auth` §6 + BC6 "no default
// credentials") so a fresh admin doesn't have to grep container logs.
//
// Self-contained: the page owns the full bootstrap login → forced-change
// flow. The login form is inlined rather than redirecting to /login;
// otherwise LoginPage's own bootstrap-state check would bounce the user
// straight back here (the bootstrap admin still has must_change_password=1
// so `has_admin_user` stays false until the rotation completes).
//
// Edge cases:
//   - has_admin_user becomes true (admin completed change-password in
//     another tab) → navigate to /login. Don't leak stale token text.
//   - Token file missing on disk (operator deleted it) → render guidance
//     + the file path so the operator knows where to look.
//   - Endpoint failure (e.g., backend not reachable) → ErrorBanner.

export function SetupPage() {
  const [revealed, setRevealed] = useState(false);
  const [password, setPassword] = useState("");
  const { data, isLoading, error: stateError } = useBootstrapState();
  const { user, mustChangePassword, error: loginError, loading: loggingIn, login, clearError } = useAuthStore();

  // Post-login navigation — once the bootstrap admin signs in, they always
  // hit must_change_password=1, so route them to /change-password.
  if (user && mustChangePassword) {
    return <Navigate to="/change-password" replace />;
  }
  // If they somehow got here with a fully-rotated session, bounce to home.
  if (user && !mustChangePassword) {
    return <Navigate to="/discovery" replace />;
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center min-h-screen" style={{ backgroundColor: "var(--color-bg-secondary)" }}>
        <span style={{ color: "var(--color-text-muted)" }}>Loading...</span>
      </div>
    );
  }

  if (stateError) {
    return (
      <div className="flex items-center justify-center min-h-screen" style={{ backgroundColor: "var(--color-bg-secondary)" }}>
        <div className="w-full max-w-md p-6">
          <ErrorBanner
            variant="error"
            title="Could not load bootstrap state"
            description={stateError instanceof Error ? stateError.message : "Backend unreachable. Check that the console binary is running."}
          />
        </div>
      </div>
    );
  }

  if (data?.has_admin_user) {
    return <Navigate to="/login" replace />;
  }

  const tokenMissing = !data?.bootstrap_token;
  const tokenPath = data?.bootstrap_token_path ?? "";

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    try {
      await login("admin", password);
    } catch {
      // error captured in store
    }
  }

  return (
    <div className="flex items-center justify-center min-h-screen" style={{ backgroundColor: "var(--color-bg-secondary)" }}>
      <div className="w-full max-w-md p-6 rounded-lg shadow-md" style={{ backgroundColor: "var(--color-bg)" }}>
        <h1 tabIndex={-1} className="text-xl font-semibold mb-2 text-center">First-Run Setup</h1>
        <p className="text-sm mb-4 text-center" style={{ color: "var(--color-text-secondary)" }}>
          Welcome. The console generated a one-time bootstrap credential on first launch. Sign in below as the initial admin, then rotate to a permanent password.
        </p>

        {tokenMissing ? (
          <>
            <div className="mb-4">
              <ErrorBanner
                variant="warning"
                title="Bootstrap token file not found"
                description={`Expected at: ${tokenPath}\n\nIf you removed the file, restart the console binary to regenerate it (only when no admin user exists yet).`}
              />
            </div>
            <EmptyState title="Once the file is restored, this page will display the credential here." size="compact" />
          </>
        ) : (
          <>
            <div className="mb-3">
              <div className="block text-sm mb-1" style={{ color: "var(--color-text-secondary)" }}>Username</div>
              <code
                className="block px-3 py-2 rounded text-sm font-mono select-all"
                style={{
                  backgroundColor: "var(--color-bg-secondary)",
                  border: "1px solid var(--color-border)",
                  color: "var(--color-text)",
                }}
              >
                admin
              </code>
            </div>

            <div className="mb-4">
              <div className="block text-sm mb-1" style={{ color: "var(--color-text-secondary)" }}>One-time password</div>
              <div className="flex gap-2">
                <code
                  className="flex-1 px-3 py-2 rounded text-sm font-mono break-all select-all"
                  style={{
                    backgroundColor: "var(--color-bg-secondary)",
                    border: "1px solid var(--color-border)",
                    color: "var(--color-text)",
                    minHeight: "2.5em",
                  }}
                >
                  {revealed ? data?.bootstrap_token : "•".repeat(43)}
                </code>
                <button
                  type="button"
                  className="px-3 py-2 rounded text-sm font-medium"
                  style={{ backgroundColor: "var(--color-bg-tertiary)", color: "var(--color-text)" }}
                  onClick={() => setRevealed((r) => !r)}
                  aria-label={revealed ? "Hide password" : "Reveal password"}
                >
                  {revealed ? "Hide" : "Show"}
                </button>
              </div>
              <p className="text-xs mt-2" style={{ color: "var(--color-text-muted)" }}>
                Source file: {tokenPath}
              </p>
            </div>

            <div className="mb-4">
              <ErrorBanner
                variant="warning"
                title="Save this somewhere safe before continuing."
                description="The console will not display this credential again after you sign in. After your first login you'll be required to rotate to a permanent password."
              />
            </div>

            {loginError && (
              <div className="mb-4">
                <ErrorBanner variant="error" title={loginError} onDismiss={clearError} />
              </div>
            )}

            <form onSubmit={(e) => void handleSubmit(e)} className="space-y-3">
              <div>
                <label htmlFor="setup-password" className="block text-sm mb-1" style={{ color: "var(--color-text-secondary)" }}>
                  Paste the one-time password to sign in
                </label>
                <input
                  id="setup-password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className="w-full px-3 py-2 border rounded text-sm font-mono"
                  style={{ borderColor: "var(--color-border)", backgroundColor: "var(--color-bg)", color: "var(--color-text)" }}
                  required
                  autoComplete="off"
                />
              </div>
              <button
                type="submit"
                disabled={loggingIn || !password}
                className="w-full py-2 rounded text-sm font-medium text-white disabled:opacity-50"
                style={{ backgroundColor: "var(--color-accent)" }}
              >
                {loggingIn ? "Signing in..." : "Sign in & continue"}
              </button>
            </form>
          </>
        )}
      </div>
    </div>
  );
}
