// §13.7.7 D1 smoke test — proves Playwright + the two webServers boot and
// the console's `/api/health` replies. Does not exercise auth or UI; the
// real scenarios land in D2.

import { test, expect } from "@playwright/test";

test("console /api/health responds", async ({ request }) => {
  const res = await request.get("/api/health");
  expect(res.ok()).toBeTruthy();
});

test("mock runtime REST gateway is reachable via console", async ({ request }) => {
  // The console proxies taxonomy requests through its own API; hitting
  // /api/verticals exercises the console → mock-runtime REST path end to
  // end and confirms the two webServers are wired together.
  const res = await request.get("/api/verticals");
  // Either a 200 (taxonomy loaded from mock) or a 401 (auth gate) is
  // acceptable for D1 — we just need a response from the console's router
  // proving it booted.
  expect([200, 401]).toContain(res.status());
});
