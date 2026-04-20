/**
 * Notification system — toast display + nav badge.
 * Toasts auto-dismiss after 5s (normal) or require manual dismiss (high-priority).
 *
 * Spec: wcon-highway §9, wcon-ui §2.1
 */

import { useState, useEffect, useCallback } from "react";
import { useSessionStore } from "../store/session";
import { ClickCard } from "./ClickCard";

interface Toast {
  id: string;
  message: string;
  priority: "normal" | "high";
  timestamp: number;
}

export function Notifications() {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  // Auto-dismiss normal toasts after 5s
  useEffect(() => {
    const timer = setInterval(() => {
      const now = Date.now();
      setToasts((prev) =>
        prev.filter((t) => t.priority === "high" || now - t.timestamp < 5000)
      );
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  if (toasts.length === 0) return null;

  return (
    <div style={{
      position: "fixed",
      bottom: 16,
      right: 16,
      zIndex: 1000,
      display: "flex",
      flexDirection: "column",
      gap: 8,
      maxWidth: 360,
    }}>
      {toasts.map((toast) => (
        <ClickCard
          key={toast.id}
          aria-label="Dismiss notification"
          onClick={() => dismiss(toast.id)}
          style={{
            padding: "12px 16px",
            borderRadius: 8,
            backgroundColor: toast.priority === "high" ? "var(--color-danger)" : "var(--color-bg-tertiary)",
            color: toast.priority === "high" ? "#fff" : "var(--color-text)",
            fontSize: 13,
            boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
            cursor: "pointer",
            border: `1px solid ${toast.priority === "high" ? "var(--color-danger)" : "var(--color-border)"}`,
          }}
        >
          {toast.message}
        </ClickCard>
      ))}
    </div>
  );
}

/** Nav badge showing pending gate + escalation + refusal counts. */
export function NavBadge() {
  const { gates, escalations, refusals } = useSessionStore();
  const count = gates.length + escalations.length + refusals.length;

  if (count === 0) return null;

  return (
    <span style={{
      display: "inline-flex",
      alignItems: "center",
      justifyContent: "center",
      minWidth: 18,
      height: 18,
      borderRadius: 9,
      backgroundColor: "var(--color-danger)",
      color: "#fff",
      fontSize: 11,
      fontWeight: 600,
      padding: "0 5px",
    }}>
      {count}
    </span>
  );
}
