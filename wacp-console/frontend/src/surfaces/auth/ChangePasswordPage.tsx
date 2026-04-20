import { useState, type FormEvent } from "react";
import { Navigate } from "react-router";
import { useAuthStore } from "../../store/auth";

export function ChangePasswordPage() {
  const { user, mustChangePassword, error, changePassword, clearError } = useAuthStore();
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [success, setSuccess] = useState(false);

  if (!user) return <Navigate to="/login" replace />;
  if (!mustChangePassword && success) return <Navigate to="/discovery" replace />;

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    try {
      await changePassword(currentPassword, newPassword);
      setSuccess(true);
    } catch {
      // error set in store
    }
  }

  return (
    <div className="flex items-center justify-center min-h-screen" style={{ backgroundColor: "var(--color-bg-secondary)" }}>
      <div className="w-full max-w-sm p-6 rounded-lg shadow-md" style={{ backgroundColor: "var(--color-bg)" }}>
        <h1 className="text-xl font-semibold mb-2 text-center">Change Password</h1>
        <p className="text-sm mb-4 text-center" style={{ color: "var(--color-text-secondary)" }}>
          You must change your password before continuing.
        </p>

        {error && (
          <div className="mb-4 p-3 rounded text-sm" style={{ backgroundColor: "var(--color-danger)", color: "#fff" }} onClick={clearError}>
            {error}
          </div>
        )}

        <form onSubmit={(e) => void handleSubmit(e)} className="space-y-4">
          <div>
            <label htmlFor="chpw-current" className="block text-sm mb-1" style={{ color: "var(--color-text-secondary)" }}>Current Password</label>
            <input id="chpw-current" type="password" value={currentPassword} onChange={(e) => setCurrentPassword(e.target.value)}
              className="w-full px-3 py-2 border rounded text-sm"
              style={{ borderColor: "var(--color-border)", backgroundColor: "var(--color-bg)", color: "var(--color-text)" }}
              required />
          </div>
          <div>
            <label htmlFor="chpw-new" className="block text-sm mb-1" style={{ color: "var(--color-text-secondary)" }}>New Password</label>
            <input id="chpw-new" type="password" value={newPassword} onChange={(e) => setNewPassword(e.target.value)}
              className="w-full px-3 py-2 border rounded text-sm"
              style={{ borderColor: "var(--color-border)", backgroundColor: "var(--color-bg)", color: "var(--color-text)" }}
              required minLength={12} />
          </div>
          <button type="submit" className="w-full py-2 rounded text-sm font-medium text-white"
            style={{ backgroundColor: "var(--color-accent)" }}>
            Change Password
          </button>
        </form>
      </div>
    </div>
  );
}
