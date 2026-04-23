// Global `aria-live` region + announce() helper for programmatic screen-
// reader announcements (ErrorBanner mount, async operation status, toast-
// like notifications once B.1 lands).
//
// Pattern: one <LiveRegion/> mounted at the root layout; anywhere in the
// app can call `announce("...")` to push a message into it. Messages are
// appended to the polite region; after 3s, cleared.
//
// Two politeness levels: "polite" (default; waits for user to finish
// reading current content) and "assertive" (interrupts). `aria-live`
// regions are not focusable and don't disrupt tab order.

import { useEffect, useState } from "react";

type Politeness = "polite" | "assertive";

type Listener = (msg: string, politeness: Politeness) => void;
const listeners = new Set<Listener>();

export function announce(msg: string, politeness: Politeness = "polite"): void {
  listeners.forEach((l) => l(msg, politeness));
}

export function LiveRegion() {
  const [polite, setPolite] = useState("");
  const [assertive, setAssertive] = useState("");

  useEffect(() => {
    const l: Listener = (msg, politeness) => {
      if (politeness === "assertive") {
        setAssertive("");
        requestAnimationFrame(() => setAssertive(msg));
      } else {
        setPolite("");
        requestAnimationFrame(() => setPolite(msg));
      }
    };
    listeners.add(l);
    return () => void listeners.delete(l);
  }, []);

  useEffect(() => {
    if (!polite) return;
    const t = setTimeout(() => setPolite(""), 3000);
    return () => clearTimeout(t);
  }, [polite]);

  useEffect(() => {
    if (!assertive) return;
    const t = setTimeout(() => setAssertive(""), 3000);
    return () => clearTimeout(t);
  }, [assertive]);

  return (
    <>
      <div
        role="status"
        aria-live="polite"
        aria-atomic="true"
        style={{
          position: "absolute",
          width: 1,
          height: 1,
          padding: 0,
          margin: -1,
          overflow: "hidden",
          clip: "rect(0, 0, 0, 0)",
          whiteSpace: "nowrap",
          border: 0,
        }}
      >
        {polite}
      </div>
      <div
        role="alert"
        aria-live="assertive"
        aria-atomic="true"
        style={{
          position: "absolute",
          width: 1,
          height: 1,
          padding: 0,
          margin: -1,
          overflow: "hidden",
          clip: "rect(0, 0, 0, 0)",
          whiteSpace: "nowrap",
          border: 0,
        }}
      >
        {assertive}
      </div>
    </>
  );
}
