# Roadmap

Where the project is headed. Everything here is open to discussion — file an issue to pick up any item, redirect one, or add a new one. Dates are aspirational; capacity is the throttle.

The current version is **v0.1** (pre-release). The roadmap is organized by release gate: what's needed before `v0.1.0` lands, what `v0.1.0` looks like, and where things go after that.

---

## Pre-`v0.1.0`

Items that block a first tagged release.

### Repo maturity

- **OCI image publication to GHCR.** `release-console.yml` + `release-runtime.yml` are wired (multi-platform amd64 + arm64, SBOM, Trivy scan) but haven't fired — no `wacp-console-v*` tag exists yet. First release tag ships `docker pull ghcr.io/aakil98/wacp-console:vX.Y.Z` as the one-line quick-start.
- **`release-console.yml` `:latest` gate.** The current `type=raw,value=latest,enable=…` condition evaluates false for every semver tag scheme; fixing the regex lands alongside the first tag push.
- **CI clippy scope.** Current `cargo clippy -p <crate> -- -D warnings` matches per-crate only. Widening to `--all-targets` surfaces a handful of test-only drifts; cleanup + gate-widening is a small follow-up.

### Quality gates — landed

All three quality-gate bullets landed via `impl/archive/v0.1.0-gate-enforcement-plan.md` (April 2026):

- **Coverage floors enforced per component.** `codecov.yml` gates merges on per-component absolute targets (rust-wacp 83% line / 72% branch, rust-console 60% line, frontend 65% line / 50% branch, python 78% line / 38% branch) plus a workspace `default: 70%` guardrail. Source of truth: `docs/coverage-policy.md`. Closes AUDIT §13.7.10 (Codecov monthly ratchet — this IS the first ratchet).
- **Mutation gate on every PR.** `ci-mutation.yml` now triggers on `pull_request: [main]` in addition to the Monday cron. Threshold tightened 85 → 90 (3 of 4 targets at 100%, one at 98.2% — 90% fast-fails small targets on 1 new survivor). Parallel wall ~11 min per PR.
- **Playwright E2E `--coverage`.** Landed — V8 coverage unioned with Vitest lcov under the `frontend` Codecov flag.

One branch-coverage item deferred-on-tool-failure: `rust-console` branch coverage blocked by an upstream LLVM `llvm-cov export` SIGSEGV under nightly+--branch on the wacp-console object set. Re-enable when the bug is fixed upstream; tracked in `docs/coverage-policy.md` "Toolchain notes".

### UX polish — landed

All three UX-polish bullets landed via `impl/archive/ux-polish-pre-v0.1.0-plan.md` (April 2026):

- **A11y audit + focus management.** Full `eslint-plugin-jsx-a11y` strict ruleset enforced; axe-core sweep across 7 surfaces clean (button-name + select-name labels, `--color-text-muted` token bumped for AA contrast). Modal focus-trap + route-change focus + skip-link + ARIA live-region infrastructure shipped.
- **Empty + error states.** Shared `<EmptyState>` (`role=status`) + `<ErrorBanner>` (`role=alert`) components with locked APIs; 26 empty-state sites + 4 error-render sites adopted across 14 surfaces.
- **Onboarding.** First-run `/setup` page surfaces the bootstrap credential inline; `GET /api/auth/bootstrap-state` endpoint with security-gated token exposure; new `00-first-run.spec.ts` Playwright spec covers the flow end-to-end.

---

## `v0.1.0` — first tagged release

What changes publicly when the tag lands:

- Pinned images at `ghcr.io/aakil98/wacp-console:v0.1.0` and `…/wacp-runtime:v0.1.0`, both with CycloneDX SBOMs and Trivy HIGH/CRITICAL scans.
- README quick-start swaps `docker compose up --build` for `docker pull`-based one-liner.
- Versioned `CHANGELOG.md` covers everything since the protocol and console entered public view.
- Release notes name the first external contributors (if any by then) and outline the v0.1.x patch-release policy.

---

## Post-`v0.1.0`

Items that don't block a first release but shape where the project goes. Order is rough and open to reshuffling.

### Console capabilities

- **Session history replay.** Scrubbable trail timeline — replay what an agent did, when, and what the operator chose to intervene on.
- **Gate / escalation batch actions.** Approve / reject / defer multiple pending gates in one action, scoped by session / role / author.
- **Filterable trail views.** Filter by actor, severity, artifact type, or workspace. Search by artifact content.
- **Runtime multi-connect.** Today the console assumes one runtime endpoint; post-v0.1 it should support connecting to multiple (e.g., dev + staging + prod) and switching per-session.

### Authentication

- **OIDC / SSO.** Today the console ships local-only multi-user auth (Argon2id + CSRF + rate limiting). Adding OIDC as a pluggable auth provider enables org / team deployments without standing up a separate identity layer.
- **API token UX.** The REST API accepts Bearer tokens today; the console UI surfaces token creation / rotation / revocation after v0.1.

### Ecosystem

- **New verticals.** Seven verticals ship today (SWE, DevOps, MLOps, finance, healthcare, analytics, datasci) — adding a new vertical is manifest-driven, no code change required. Good contributor on-ramp.
- **External runtime implementations.** The protocol is spec-complete and licensed CC BY-SA 4.0 — a runtime implementation in a different language (Go, Python, TypeScript) would be a meaningful validation of the spec and is something maintainers would actively support reviewing.

### Infrastructure

- **Cross-runtime integration-test harness hardening.** The test harness uses batch-ephemeral-port selection to avoid intra-test collisions; a full holder-listener pattern (keep the `TcpListener` open, inherit via FD to the child) would eliminate cross-harness residuals as well.
- **Monitoring / metrics.** Console side has basic observability; runtime exposes Prometheus on port 9095. Post-v0.1: canonical Grafana dashboards + alerting wiring.

---

## Out of scope

Things explicitly *not* on the roadmap, so expectations are clear:

- **Hosted / SaaS version.** Not planned. The OSS binaries are the product; if operators want managed hosting, that's theirs to run or delegate.
- **Re-implementing orchestration frameworks.** WACP is protocol + workbench, not an orchestrator. Integration with existing frameworks (LangChain, CrewAI, AutoGen, etc.) via the Agent gRPC service is in-scope; re-implementing their surface area is not.
- **Closed-source extensions.** Everything ships Apache-2.0; no "enterprise edition" fork.

---

## Proposing changes

The roadmap isn't immutable. To move an item:

- **Pick up an existing item** → open an issue referencing the roadmap section, or comment on an existing issue for that item. Assignment is first-come if you're ready to work on it.
- **Add a new item** → open a `feat:` issue (or prefix with `design:` if it's architectural — see [`CONTRIBUTING.md` §Larger changes](CONTRIBUTING.md#larger-changes)). Scope gets agreed before code lands.
- **Challenge an item** → file an issue with your reasoning. If something doesn't belong, better to argue it out now than after someone's implemented it.

---

*Updated at release-tag cadence and whenever items ship. Commit history is the diff.*
