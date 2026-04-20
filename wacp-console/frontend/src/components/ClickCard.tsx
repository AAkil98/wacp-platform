import type React from "react";
import { useCallback, type KeyboardEvent } from "react";

// A11y wrapper for `<div onClick>` patterns per HEALTH-LOG §4 P4 residual
// (frontend-perf-plan F8 follow-up). Renders `role="button"` + `tabIndex={0}`
// with Enter/Space keyboard activation. Use this for cards, list rows, and
// other non-anchor non-button interactive surfaces. For modal-overlay
// background clicks, use a different pattern (Escape key listener + focus
// trap) — this wrapper assumes the caller wants button semantics.

interface ClickCardProps
  extends Omit<React.HTMLAttributes<HTMLDivElement>, "onClick"> {
  onClick: () => void;
}

export function ClickCard({ onClick, children, ...rest }: ClickCardProps) {
  const onKeyDown = useCallback(
    (e: KeyboardEvent<HTMLDivElement>) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        onClick();
      }
    },
    [onClick],
  );
  return (
    <div role="button" tabIndex={0} onClick={onClick} onKeyDown={onKeyDown} {...rest}>
      {children}
    </div>
  );
}
