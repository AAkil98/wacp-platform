// §13.7.7 D2 — golden-path E2E.
//
// Audit scope: bootstrap → login → discover → create profile → launch →
// gate approve → complete → quality report. D2 unskips the bootstrap →
// login → discover → profiles-page portion because that's what the current
// mock runtime can faithfully serve. The launch → gate → complete tail is
// skipped with a pointer to what's needed to unskip.

import { test, expect } from "@playwright/test";
import { ensureAdminOnDiscovery } from "./helpers/admin";

test.describe.configure({ mode: "serial" });

test.describe("Golden path — observable portion", () => {
  test("admin reaches /discovery and the fixture verticals render", async ({ page }) => {
    await ensureAdminOnDiscovery(page);

    // Verticals tab is the rightmost tab; click through to see the list.
    await page.getByRole("button", { name: /^verticals$/i }).click();

    // Mock's /v1/verticals returns both fixtures; console builds a
    // TaxonomyIndex that surfaces them through /api/verticals.
    await expect(page.getByText(/Fixture Simple \(SWE\)/i)).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/Fixture Complex \(Finance\)/i)).toBeVisible();
  });

  test("profiles page renders with the Profiles sidebar and Create New affordance", async ({
    page,
  }) => {
    await ensureAdminOnDiscovery(page);

    // Sidebar Profiles link has role="link" (react-router). The sidebar uses
    // a `<NavLink>` → `<a>` element.
    await page.getByRole("link", { name: /profiles/i }).click();
    await page.waitForURL(/\/profiles/, { timeout: 5_000 });

    // Landing-state assertions:
    //  - the page sidebar h2 "Profiles" is visible (proves the ProfilesPage
    //    component mounted under the authenticated layout).
    //  - Create New button is present (proves the auth-guarded page renders
    //    fully, not a logged-out fallback).
    //  - Empty profile list shows "No profiles found." (consistent with the
    //    freshly-bootstrapped DB state).
    await expect(page.getByRole("heading", { level: 2, name: "Profiles" })).toBeVisible();
    await expect(page.getByRole("button", { name: /create new/i })).toBeVisible();
    await expect(page.getByText("No profiles found.")).toBeVisible();

    // Create New → form renders. §12.5 root cause fixed in closeout-plan P5:
    // `/api/roles` returns `PaginatedResponse<T>` (`{items, cursor, has_more}`),
    // but the frontend cast `rolesQuery.data as RoleSummary[]` and later called
    // `roles.map` inside the form's `<select>` render. The crash deferred until
    // Create-New flipped `creating` to true and the form mounted for the first
    // time. Fix: unwrap `.items` before mapping.
    await page.getByRole("button", { name: /create new/i }).click();
    await expect(
      page.getByRole("heading", { level: 2, name: /new profile/i }),
    ).toBeVisible();
    await expect(page.getByLabel(/^name$/i)).toBeVisible();
    await expect(page.getByLabel(/^role$/i)).toBeVisible();
  });
});

test.describe("Golden path — launch + gate + complete + quality (deferred)", () => {
  test.skip("launch flow with gate approve → complete → quality report", () => {
    // Un-skipping this needs:
    //   1. Mock runtime's `CoordinatorService::SubmitGoal` + `Decompose` +
    //      `Dispatch` implementations to return a realistic `root_workspace_id`
    //      and task list so the console's `SessionLauncher` can persist the
    //      session as ACTIVE instead of erroring out at Step 2/3.
    //   2. Mock `HighwayService::StreamWorkspaceChanges` + `StreamGates` +
    //      `StreamTrail` + `StreamEscalations` to emit a scripted sequence
    //      that covers: initial State=Active, CheckpointProposed →
    //      GateOpened → (gate approved via console) → GateResolved →
    //      AgentSignal=Complete → State=Integrating → State=Closed, with a
    //      final TrailEntry carrying quality_criteria_evaluation.
    //   3. A per-scenario fixture loader on the mock-runtime binary (probably
    //      a WACP_MOCK_SCENARIO_PATH env var pointing at YAML) so the test
    //      can select the golden-path script without global state.
    //
    // None of this is a §13.7.7 deliverable. Filed here for the next engineer
    // to pick up — probably lands as a D2-follow-up + D5 rescope, or as part
    // of §13.7.8 (Rust integration I1–I5) which already touches this surface.
  });
});
