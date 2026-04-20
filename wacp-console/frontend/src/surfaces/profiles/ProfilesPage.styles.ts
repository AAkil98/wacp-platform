import type React from "react";

// Shared styles for the ProfilesPage subcomponents (F4 decomposition).
// Single source of truth so the sidebar + editor + modals stay visually aligned.

export const sidebar: React.CSSProperties = {
  width: 340,
  borderRight: "1px solid var(--color-border)",
  display: "flex",
  flexDirection: "column",
  background: "var(--color-bg-secondary)",
  overflow: "hidden",
};

export const sidebarHeader: React.CSSProperties = {
  padding: "16px",
  borderBottom: "1px solid var(--color-border)",
  display: "flex",
  flexDirection: "column",
  gap: 8,
};

export const listContainer: React.CSSProperties = {
  flex: 1,
  overflowY: "auto",
};

const LIST_ITEM_BASE: React.CSSProperties = {
  padding: "12px 16px",
  cursor: "pointer",
  borderBottom: "1px solid var(--color-border)",
};

export const LIST_ITEM_STYLE: Record<"selected" | "unselected", React.CSSProperties> = {
  selected: { ...LIST_ITEM_BASE, background: "var(--color-accent)", color: "#fff" },
  unselected: { ...LIST_ITEM_BASE, background: "transparent", color: "var(--color-text)" },
};

export const badge: React.CSSProperties = {
  display: "inline-block",
  padding: "2px 8px",
  borderRadius: 4,
  fontSize: 11,
  fontWeight: 600,
  background: "var(--color-border)",
  color: "var(--color-text-secondary)",
  marginLeft: 6,
};

export const editorPanel: React.CSSProperties = {
  flex: 1,
  overflowY: "auto",
  padding: 24,
};

export const formGrid: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "1fr 1fr",
  gap: 16,
};

export const fullSpan: React.CSSProperties = { gridColumn: "1 / -1" };

export const fieldLabel: React.CSSProperties = {
  display: "block",
  marginBottom: 4,
  fontSize: 13,
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

export const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  border: "1px solid var(--color-border)",
  borderRadius: 4,
  background: "var(--color-bg)",
  color: "var(--color-text)",
  fontSize: 14,
};

export const btnPrimary: React.CSSProperties = {
  padding: "8px 16px",
  background: "var(--color-accent)",
  color: "#fff",
  border: "none",
  borderRadius: 4,
  cursor: "pointer",
  fontWeight: 600,
};

export const btnDanger: React.CSSProperties = {
  ...btnPrimary,
  background: "var(--color-danger)",
};

export const btnSecondary: React.CSSProperties = {
  ...btnPrimary,
  background: "transparent",
  border: "1px solid var(--color-border)",
  color: "var(--color-text)",
};

export const actionsRow: React.CSSProperties = {
  display: "flex",
  gap: 8,
  marginTop: 24,
  paddingTop: 16,
  borderTop: "1px solid var(--color-border)",
};
