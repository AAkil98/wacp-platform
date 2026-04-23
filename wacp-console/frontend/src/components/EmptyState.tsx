import type React from "react";

// Shared empty-state component for list/table views with no data.
//
// Renders a centered block: optional leading icon, title (as `role="status"`
// so screen readers announce when the region becomes populated), optional
// description, optional action slot (typically a CTA button).
//
// API is final once this component ships — consumers in P4 adopt without
// editing the API. If a variant shape is needed (e.g., table-row empty),
// factor a sibling component rather than expanding props.

export interface EmptyStateProps {
  /** Primary message. Required. Announced via role=status on mount. */
  title: string;
  /** Secondary detail text. Muted. Optional. */
  description?: string;
  /** Leading icon / illustration slot. Rendered above the title. Optional. */
  icon?: React.ReactNode;
  /** Action slot — typically a button. Rendered below description. Optional. */
  action?: React.ReactNode;
  /** Size preset. "compact" for sidebar / narrow containers; "default" for full-surface landings. */
  size?: "compact" | "default";
}

const container = (size: "compact" | "default"): React.CSSProperties => ({
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  textAlign: "center",
  padding: size === "compact" ? "16px 12px" : "48px 24px",
  color: "var(--color-text-muted)",
});

const titleStyle = (size: "compact" | "default"): React.CSSProperties => ({
  margin: 0,
  fontSize: size === "compact" ? 14 : 16,
  fontWeight: 500,
  color: "var(--color-text-secondary)",
});

const descriptionStyle = (size: "compact" | "default"): React.CSSProperties => ({
  margin: "6px 0 0",
  fontSize: size === "compact" ? 13 : 14,
  color: "var(--color-text-muted)",
  maxWidth: 360,
});

const iconSlot: React.CSSProperties = {
  marginBottom: 12,
  color: "var(--color-text-muted)",
  display: "flex",
};

const actionSlot: React.CSSProperties = {
  marginTop: 16,
};

export function EmptyState({
  title,
  description,
  icon,
  action,
  size = "default",
}: EmptyStateProps) {
  return (
    <div style={container(size)} role="status">
      {icon && <div style={iconSlot} aria-hidden="true">{icon}</div>}
      <p style={titleStyle(size)}>{title}</p>
      {description && <p style={descriptionStyle(size)}>{description}</p>}
      {action && <div style={actionSlot}>{action}</div>}
    </div>
  );
}
