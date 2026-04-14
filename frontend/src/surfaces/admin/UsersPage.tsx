import type React from "react";
import { useState } from "react";
import { useUsers, useCreateUser } from "../../api/hooks/index";
import { api } from "../../api/client";
import { useQueryClient } from "@tanstack/react-query";

// --- Styles ---

const page: React.CSSProperties = {
  padding: 24,
  color: "var(--color-text)",
  background: "var(--color-bg)",
};

const table: React.CSSProperties = {
  width: "100%",
  borderCollapse: "collapse",
  fontSize: 14,
};

const th: React.CSSProperties = {
  textAlign: "left",
  padding: "10px 12px",
  borderBottom: "2px solid var(--color-border)",
  fontWeight: 700,
  fontSize: 13,
  color: "var(--color-text-secondary)",
};

const td: React.CSSProperties = {
  padding: "10px 12px",
  borderBottom: "1px solid var(--color-border)",
};

const inputStyle: React.CSSProperties = {
  padding: "8px 10px",
  border: "1px solid var(--color-border)",
  borderRadius: 4,
  background: "var(--color-bg)",
  color: "var(--color-text)",
  fontSize: 14,
  width: "100%",
};

const btnPrimary: React.CSSProperties = {
  padding: "8px 16px",
  background: "var(--color-accent)",
  color: "#fff",
  border: "none",
  borderRadius: 4,
  cursor: "pointer",
  fontWeight: 600,
};

const btnSecondary: React.CSSProperties = {
  padding: "6px 12px",
  background: "transparent",
  border: "1px solid var(--color-border)",
  borderRadius: 4,
  cursor: "pointer",
  color: "var(--color-text)",
  fontSize: 13,
};

const btnDanger: React.CSSProperties = {
  ...btnSecondary,
  borderColor: "var(--color-danger)",
  color: "var(--color-danger)",
};

const dialogOverlay: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "rgba(0,0,0,0.4)",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  zIndex: 100,
};

const dialogBox: React.CSSProperties = {
  background: "var(--color-bg)",
  border: "1px solid var(--color-border)",
  borderRadius: 8,
  padding: 24,
  width: 420,
  maxWidth: "90vw",
};

const fieldLabel: React.CSSProperties = {
  display: "block",
  marginBottom: 4,
  fontSize: 13,
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

const fieldGroup: React.CSSProperties = {
  marginBottom: 12,
};

const badge = (disabled: boolean): React.CSSProperties => ({
  display: "inline-block",
  padding: "2px 8px",
  borderRadius: 4,
  fontSize: 11,
  fontWeight: 600,
  background: disabled ? "var(--color-danger)" : "var(--color-accent)",
  color: "#fff",
});

// --- Types ---

interface UserEntry {
  user_id: string;
  username: string;
  display_name: string;
  console_role: string;
  disabled: boolean;
  created_at: string;
}

const ROLES = ["admin", "operator", "viewer"] as const;

export function UsersPage() {
  const usersQuery = useUsers();
  const createUser = useCreateUser();
  const qc = useQueryClient();

  const [showCreate, setShowCreate] = useState(false);
  const [createForm, setCreateForm] = useState({ username: "", display_name: "", password: "", console_role: "operator" });
  const [actionError, setActionError] = useState<string | null>(null);
  const [changingRole, setChangingRole] = useState<string | null>(null);
  const [newRole, setNewRole] = useState("");

  const users = (usersQuery.data ?? []) as UserEntry[];

  function handleCreate() {
    if (!createForm.username || !createForm.password) return;
    createUser.mutate(createForm as unknown as Record<string, unknown>, {
      onSuccess: () => {
        setShowCreate(false);
        setCreateForm({ username: "", display_name: "", password: "", console_role: "operator" });
      },
    });
  }

  async function toggleDisable(user: UserEntry) {
    setActionError(null);
    try {
      await api.patch(`/api/users/${user.user_id}`, { disabled: !user.disabled });
      qc.invalidateQueries({ queryKey: ["users"] });
    } catch {
      setActionError(`Failed to ${user.disabled ? "enable" : "disable"} user.`);
    }
  }

  async function changeRole(userId: string) {
    if (!newRole) return;
    setActionError(null);
    try {
      await api.patch(`/api/users/${userId}`, { console_role: newRole });
      qc.invalidateQueries({ queryKey: ["users"] });
      setChangingRole(null);
      setNewRole("");
    } catch {
      setActionError("Failed to change role.");
    }
  }

  async function resetPassword(userId: string) {
    setActionError(null);
    try {
      await api.post(`/api/users/${userId}/reset-password`);
      setActionError(null);
      alert("Password has been reset. The user must change it on next login.");
    } catch {
      setActionError("Failed to reset password.");
    }
  }

  async function unlockUser(userId: string) {
    setActionError(null);
    try {
      await api.post(`/api/users/${userId}/unlock`);
      qc.invalidateQueries({ queryKey: ["users"] });
    } catch {
      setActionError("Failed to unlock user.");
    }
  }

  return (
    <div style={page}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 20 }}>
        <h1 style={{ margin: 0, fontSize: 24, fontWeight: 700 }}>User Management</h1>
        <button style={btnPrimary} onClick={() => setShowCreate(true)}>Create User</button>
      </div>

      {actionError && (
        <div style={{ marginBottom: 12, padding: "8px 12px", background: "var(--color-danger)", color: "#fff", borderRadius: 4, fontSize: 13 }}>
          {actionError}
        </div>
      )}

      {usersQuery.isLoading ? (
        <p style={{ color: "var(--color-text-secondary)" }}>Loading users...</p>
      ) : (
        <table style={table}>
          <thead>
            <tr>
              <th style={th}>Username</th>
              <th style={th}>Display Name</th>
              <th style={th}>Role</th>
              <th style={th}>Status</th>
              <th style={th}>Created</th>
              <th style={th}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {users.map((u) => (
              <tr key={u.user_id}>
                <td style={td}>{u.username}</td>
                <td style={td}>{u.display_name || "--"}</td>
                <td style={td}>
                  {changingRole === u.user_id ? (
                    <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
                      <select
                        style={{ ...inputStyle, width: "auto" }}
                        value={newRole}
                        onChange={(e) => setNewRole(e.target.value)}
                      >
                        <option value="">--</option>
                        {ROLES.map((r) => (
                          <option key={r} value={r}>{r}</option>
                        ))}
                      </select>
                      <button style={btnSecondary} onClick={() => void changeRole(u.user_id)}>OK</button>
                      <button style={btnSecondary} onClick={() => { setChangingRole(null); setNewRole(""); }}>X</button>
                    </div>
                  ) : (
                    u.console_role
                  )}
                </td>
                <td style={td}>
                  <span style={badge(u.disabled)}>{u.disabled ? "Disabled" : "Active"}</span>
                </td>
                <td style={td}>{u.created_at}</td>
                <td style={td}>
                  <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
                    <button style={u.disabled ? btnSecondary : btnDanger} onClick={() => void toggleDisable(u)}>
                      {u.disabled ? "Enable" : "Disable"}
                    </button>
                    <button style={btnSecondary} onClick={() => { setChangingRole(u.user_id); setNewRole(u.console_role); }}>
                      Change Role
                    </button>
                    <button style={btnSecondary} onClick={() => void resetPassword(u.user_id)}>Reset Password</button>
                    <button style={btnSecondary} onClick={() => void unlockUser(u.user_id)}>Unlock</button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {/* Create user dialog */}
      {showCreate && (
        <div style={dialogOverlay} onClick={() => setShowCreate(false)}>
          <div style={dialogBox} onClick={(e) => e.stopPropagation()}>
            <h2 style={{ margin: "0 0 16px", fontSize: 18, fontWeight: 700 }}>Create User</h2>
            <div style={fieldGroup}>
              <label style={fieldLabel}>Username</label>
              <input style={inputStyle} value={createForm.username} onChange={(e) => setCreateForm((f) => ({ ...f, username: e.target.value }))} />
            </div>
            <div style={fieldGroup}>
              <label style={fieldLabel}>Display Name</label>
              <input style={inputStyle} value={createForm.display_name} onChange={(e) => setCreateForm((f) => ({ ...f, display_name: e.target.value }))} />
            </div>
            <div style={fieldGroup}>
              <label style={fieldLabel}>Password</label>
              <input style={inputStyle} type="password" value={createForm.password} onChange={(e) => setCreateForm((f) => ({ ...f, password: e.target.value }))} />
            </div>
            <div style={fieldGroup}>
              <label style={fieldLabel}>Role</label>
              <select style={inputStyle} value={createForm.console_role} onChange={(e) => setCreateForm((f) => ({ ...f, console_role: e.target.value }))}>
                {ROLES.map((r) => (
                  <option key={r} value={r}>{r}</option>
                ))}
              </select>
            </div>
            <div style={{ display: "flex", gap: 8, marginTop: 16 }}>
              <button style={btnPrimary} onClick={handleCreate} disabled={createUser.isPending}>
                {createUser.isPending ? "Creating..." : "Create"}
              </button>
              <button style={{ ...btnSecondary }} onClick={() => setShowCreate(false)}>Cancel</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
