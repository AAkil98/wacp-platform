// §13.7.7 D2 — session cancellation flows (stubbed).
//
// Audit scope: cancel from wizard (returns to sessions list with state
// preserved), cancel from dashboard (workspace aborts, session marked
// CANCELLED). Stubbed for the same reason golden-path's tail is stubbed —
// the mock runtime cannot yet drive the session-launcher far enough for a
// session to become active + cancellable.

import { test } from "./fixtures";

test.skip("cancel from the launch wizard returns to sessions list with draft state preserved", () => {
  // Unskip requirements:
  //   1. The wizard renders but the user can step through without a
  //      functioning Dispatch path (since we abort before launch). That is
  //      already true today — the wizard's Cancel button is UI-local and
  //      doesn't hit the runtime.
  //   2. Need a pre-populated profile fixture so the assign-profiles step
  //      isn't blocked. Either seed a profile inline via the console API or
  //      accept that this scenario runs after `golden-path.spec.ts` seeds
  //      one implicitly.
  //   3. Verify via assertion that the wizard draft state is persisted
  //      across a Cancel → Re-enter cycle; this depends on whether the
  //      draft state lives in zustand (cleared on unmount) or in localStorage
  //      (preserved). Check `src/store/session.ts` before writing the
  //      assertion.
  //
  // Anchor: `wcon-sessions` §2.3 (wizard state machine), §3 (cancel).
});

test.skip("cancel from the oversight dashboard aborts the workspace and marks session CANCELLED", () => {
  // Unskip requirements:
  //   1. Same mock-runtime driving dependencies as the deferred golden-path
  //      tail: need SubmitGoal / Decompose / Dispatch and a way for the
  //      session to reach ACTIVE.
  //   2. The console's Cancel action calls `AbortWorkspace` on the
  //      CoordinatorService; the mock needs to acknowledge this cleanly and
  //      emit a Workspace state-change frame so the monitor's completion
  //      detector transitions the session to CANCELLED.
  //   3. Scripting the mock to send exactly the post-abort Workspace=Closed
  //      frame (and not also a Completed frame, which would mask the
  //      CANCELLED→COMPLETED drift that T7.3 closed).
  //
  // Anchor: `wcon-sessions` §5 (cancel from dashboard), `wcon-highway` §6
  // (AbortWorkspace frame ordering).
});
