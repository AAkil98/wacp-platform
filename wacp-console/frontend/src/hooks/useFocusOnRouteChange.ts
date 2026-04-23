// Moves focus to the `<main id="main">` landmark on every route change.
//
// Without this, SPA route transitions leave focus on the link that was
// clicked. Screen-reader users have no signal that the page has changed
// and must re-explore to find new content. Moving focus into the main
// landmark gives them a predictable reset point.
//
// Skips the initial render — first paint shouldn't move focus away from
// whatever the natural tab order puts first.

import { useEffect, useRef } from "react";
import { useLocation } from "react-router";

export function useFocusOnRouteChange(): void {
  const location = useLocation();
  const first = useRef(true);

  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const main = document.getElementById("main");
    if (main) main.focus();
  }, [location.pathname]);
}
