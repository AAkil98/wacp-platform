import { btnDanger, btnSecondary } from "./ProfilesPage.styles";

interface DeleteProfileModalProps {
  profileName: string;
  onConfirm: () => void;
  onCancel: () => void;
  isPending: boolean;
}

export function DeleteProfileModal({
  profileName,
  onConfirm,
  onCancel,
  isPending,
}: DeleteProfileModalProps) {
  return (
    <div
      style={{
        marginTop: 16,
        padding: 16,
        border: "1px solid var(--color-danger)",
        borderRadius: 6,
        background: "var(--color-bg-secondary)",
      }}
    >
      <p style={{ margin: "0 0 8px", fontWeight: 600, color: "var(--color-danger)" }}>
        Delete profile &quot;{profileName}&quot;?
      </p>
      <p style={{ margin: "0 0 12px", fontSize: 13, color: "var(--color-warning)" }}>
        Warning: This action cannot be undone. Any sessions using this profile may be affected.
      </p>
      <div style={{ display: "flex", gap: 8 }}>
        <button style={btnDanger} onClick={onConfirm} disabled={isPending}>
          {isPending ? "Deleting..." : "Confirm Delete"}
        </button>
        <button style={btnSecondary} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}
