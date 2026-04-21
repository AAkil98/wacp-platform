// §13.7.7 D2 — profile YAML export → delete → import round-trip (stubbed).
//
// Audit scope: export a profile's YAML, delete the profile, re-import the
// YAML, confirm the fields match and a new UUID was assigned. Stubbed
// pending a confirmed UI export/import affordance — current `ProfilesPage`
// renders Clone / Delete / Save but no explicit Export-YAML / Import-YAML
// buttons in the shipping surface. See `wcon-profiles` §4 for the spec and
// perf-opt §2.5 for the drift-resolution protocol.

import { test } from "./fixtures";

test.skip("export profile YAML, delete, import — fields match, new UUID", () => {
  // Unskip requirements:
  //   1. Verify the UI surface: `ProfilesPage.tsx` was scanned during D2
  //      reconnaissance and did not show Export-YAML / Import-YAML buttons
  //      at a cursory read. Before writing this test, confirm whether they
  //      exist (and we missed them), are accessed via a sub-menu, or are
  //      genuinely absent. If absent, decide drift-resolution per perf-opt
  //      §2.5 (b): either the feature is latent (ship the UI) or the spec
  //      trims (update `wcon-profiles` §4).
  //   2. If present, the test is straightforward — click Export, intercept
  //      the download stream (`page.waitForEvent("download")`), delete the
  //      profile, click Import, upload the captured YAML blob, verify
  //      field parity with the original + new UUID.
  //   3. The audit-log entries for create + delete + create should all land
  //      and be readable from `/admin/audit` — optional assertion.
  //
  // Anchor: `wcon-profiles` §4 (YAML roundtrip), `wcon-auth` §13 (audit log
  // invariant), `wcon-data-model` §3 (profile versioning semantics).
});
