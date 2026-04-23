// Focus-trap hook for modal / dialog surfaces.
//
// Contract (per WAI-ARIA APG modal pattern):
//   - While `active`, Tab + Shift+Tab cycle within the ref's subtree; focus
//     never escapes the container.
//   - On activation, focus moves to the first focusable descendant.
//   - On deactivation, focus restores to the element that was active when
//     the hook activated (typically the trigger button that opened the modal).
//
// Minimal hand-rolled implementation; no external focus-lock library per
// P0 finding (package.json clean).

import { useEffect, useRef } from "react";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(", ");

function getFocusable(container: HTMLElement): HTMLElement[] {
  const candidates = container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
  return Array.from(candidates).filter((el) => !el.hasAttribute("aria-hidden") && el.offsetParent !== null);
}

export function useFocusTrap(containerRef: React.RefObject<HTMLElement | null>, active: boolean = true): void {
  const restoreRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!active) return;
    const container = containerRef.current;
    if (!container) return;

    restoreRef.current = document.activeElement as HTMLElement | null;

    const focusables = getFocusable(container);
    const first = focusables[0];
    if (first) first.focus();

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const current = getFocusable(container);
      const firstEl = current[0];
      const lastEl = current[current.length - 1];
      if (!firstEl || !lastEl) {
        e.preventDefault();
        return;
      }
      const activeEl = document.activeElement as HTMLElement | null;

      if (e.shiftKey && activeEl === firstEl) {
        e.preventDefault();
        lastEl.focus();
      } else if (!e.shiftKey && activeEl === lastEl) {
        e.preventDefault();
        firstEl.focus();
      }
    };

    container.addEventListener("keydown", onKeyDown);

    return () => {
      container.removeEventListener("keydown", onKeyDown);
      const restore = restoreRef.current;
      if (restore && document.body.contains(restore)) {
        restore.focus();
      }
    };
  }, [active, containerRef]);
}
