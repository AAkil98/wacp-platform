import type React from "react";
import { AlertCircle, AlertTriangle, Info, X } from "lucide-react";

// Shared error / warning / info banner for surface-level status messages.
//
// API is final once this component ships — consumers in P4 adopt without
// editing the API.
//
// Rendered with `role="alert"` so screen readers announce the title on
// mount (native ARIA live-region semantics — no manual `announce()` call
// needed). Dismiss button only renders when `onDismiss` is provided.

export type ErrorBannerVariant = "error" | "warning" | "info";

export interface ErrorBannerProps {
  variant: ErrorBannerVariant;
  /** Primary message. Required. Announced via role=alert on mount. */
  title: string;
  /** Secondary detail. Optional. */
  description?: string;
  /** Dismiss handler. If provided, an X button renders on the right. */
  onDismiss?: () => void;
}

const variantStyles: Record<ErrorBannerVariant, { bg: string; fg: string; accent: string; Icon: React.ComponentType<{ size?: number }> }> = {
  error: { bg: "var(--color-danger)", fg: "#fff", accent: "#fee2e2", Icon: AlertCircle },
  warning: { bg: "var(--color-warning)", fg: "#422006", accent: "#fef3c7", Icon: AlertTriangle },
  info: { bg: "var(--color-accent)", fg: "#fff", accent: "#dbeafe", Icon: Info },
};

const container: React.CSSProperties = {
  display: "flex",
  alignItems: "flex-start",
  gap: 12,
  padding: "12px 16px",
  borderRadius: 6,
  fontSize: 14,
};

const body: React.CSSProperties = {
  flex: 1,
  display: "flex",
  flexDirection: "column",
  gap: 4,
};

const titleStyle: React.CSSProperties = {
  margin: 0,
  fontWeight: 600,
};

const descriptionStyle: React.CSSProperties = {
  margin: 0,
  opacity: 0.9,
};

const dismissStyle: React.CSSProperties = {
  flexShrink: 0,
  background: "transparent",
  border: "none",
  color: "currentColor",
  cursor: "pointer",
  padding: 4,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  borderRadius: 4,
  opacity: 0.8,
};

export function ErrorBanner({
  variant,
  title,
  description,
  onDismiss,
}: ErrorBannerProps) {
  const v = variantStyles[variant];
  return (
    <div
      role="alert"
      style={{ ...container, background: v.bg, color: v.fg }}
    >
      <v.Icon size={18} aria-hidden="true" />
      <div style={body}>
        <p style={titleStyle}>{title}</p>
        {description && <p style={descriptionStyle}>{description}</p>}
      </div>
      {onDismiss && (
        <button
          type="button"
          onClick={onDismiss}
          style={dismissStyle}
          aria-label="Dismiss"
        >
          <X size={16} />
        </button>
      )}
    </div>
  );
}
