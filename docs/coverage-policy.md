# Coverage Policy

This file is the source of truth for what `codecov.yml` enforces on every push and PR.
Edit here first, then mirror the numbers into `codecov.yml`.

Plan reference: `impl/archive/v0.1.0-gate-enforcement-plan.md` (P1).
Originally landed: 2026-04-24.

## Floors

Per-component absolute floors plus a workspace-wide guardrail. Failing either fails the gate.

| Flag | Line floor | Branch floor | Source baseline (line / branch) | Buffer |
|---|---|---|---|---|
| _workspace `default`_ | **70 %** | — | weighted ~74 % across all flags | 4 pt |
| `rust-wacp` | **83 %** | **TBD** (set post-P1.a re-baseline) | 85.3 % / TBD | 2 pt |
| `rust-console` | **60 %** | **TBD** (set post-P1.a re-baseline) | 62.2 % / TBD | 2 pt |
| `frontend` | **65 %** | **50 %** | 69.2 % / 54.6 % | 4 / 4 pt |
| `python` | **78 %** | **38 %** | 80.2 % / 41.5 % | 2 / 3 pt |
| `frontend-e2e` | _no gate_ | _no gate_ | 0 % when `E2E_COVERAGE` unset | — |

Rationale for buffer sizes:

- 2 pt — small, stable components where the baseline reflects a settled state.
- 4 pt — components that union multiple sources (Vitest + Playwright V8 under same flag) or have legitimate fluctuation.
- `frontend-e2e` is not gated as a separate flag because V8 collection requires `E2E_COVERAGE=1` env, which isn't always set on main pushes; the lcov is unioned into the `frontend` flag at Codecov so its data isn't lost.

## Workspace guardrail

The `default: 70 %` workspace target catches the failure mode where one component is allowed to silently slide while others mask the decline at the aggregate. Per-component floors alone don't catch "one large component drops 30 pt while staying within its individual floor." Both must hold.

## Ratchet schedule

A floor ratchets up by 2 pt when the corresponding component has held at floor + 5 pt for **4 consecutive weekly Codecov reports** on `main`. The bump lands as a `codecov.yml` edit + a corresponding row update here.

Manual ratchets (operator-driven, off-schedule) are allowed when a substantial coverage improvement lands — but the same floor + 5 pt sustained-rule applies retroactively. Don't ratchet to the literal current measurement; the buffer prevents a single low-coverage commit from wedging the gate.

End-state targets (per `codecov.yml` header): Rust 95 % line / 95 % branch, Frontend 95 % branch, Python 95 % branch. Distance from current floors to end-state is intentional — closing the gap is post-v0.1.0 work.

## Adding a new component

When a new flag is introduced (new crate, new package):

1. Land the component without coverage upload first; let `coverage.yml` measure it for one push.
2. Pull the lcov; compute line + branch %.
3. Add `<flag>: target: <baseline minus 2pt>%` (and `branches:` if applicable) to `codecov.yml`.
4. Add a row to the table above with the source baseline.
5. Open a PR with all three changes (workflow, codecov.yml, this file) — the PR's own coverage run is the verification.

If the new component starts below 50 %, set the floor at baseline minus 5 pt as a regression-guard only; ratchet schedule still applies.

## Handling a temporary regression

A merged PR with intentionally-removed coverage (e.g., a feature being rolled back) may legitimately drop a component below its floor. Three options:

1. **Restore coverage in the same PR.** Best — the gate doesn't fire.
2. **Lower the floor as part of the PR.** `codecov.yml` edit + table update + commit-body justification. Counts as a deliberate ratchet-down.
3. **Codecov UI override per-PR.** Bypasses the gate for one merge. Use sparingly; document the override in the PR description.

Don't add flags to `codecov.yml` to silence specific files — coverage gates are about workspace health, not per-file curation.

## Verification

P1.d of the original plan verified the gate fires by deleting one high-impact test on a throwaway branch and confirming Codecov status went red. Re-verification triggers:

- After any `codecov.yml` edit that changes target numbers.
- After a CI infrastructure change that touches the coverage pipeline (e.g., switching `cargo-llvm-cov` major version).
- Quarterly spot-check (drop a token test, verify red, revert).

## Cross-refs

- `codecov.yml` — the enforced configuration this document tracks.
- `.github/workflows/coverage.yml` — the measurement pipeline.
- `impl/archive/v0.1.0-gate-enforcement-plan.md` — the plan that introduced these floors.
- `AUDIT-2026-04-15.md` §13.7.10 — the original Codecov-ratchet AUDIT item closed by P1.
