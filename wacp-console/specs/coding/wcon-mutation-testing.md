---
id: wcon-mutation-testing
type: coding
status: final
created: 2026-04-17T00:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [testing, mutation, ci, quality]
depends_on: [wcon-w1-grpc-pool, wcon-w2-launch-flow, wcon-w3-session-monitor]
---

# Coding Spec — Mutation Testing Pipeline

## Table of Contents

1. Purpose
2. Scope and non-scope
3. Workflow design
4. Per-target jobs
5. Threshold + failure semantics
6. Aggregator job + summary
7. Triage loop
8. Acceptance criteria
9. References

---

## 1. Purpose

Coverage tells us *what code ran*, not *whether the tests would catch regressions*. Mutation testing flips that: it makes small mechanical perturbations to production code (inverting conditions, replacing operators, deleting statements) and re-runs the test suite. A "killed" mutant means a test failed under the perturbation — proof the test exercises that decision point. A "surviving" mutant means no test caught the change — a coverage gap the line-coverage report doesn't expose.

The audit (`AUDIT-2026-04-15.md` §12.6, §13.7.9) flags four modules where mutation testing has the highest signal: anything that gates security or correctness behavior under a closed transition table. Those are exactly the modules `cargo-mutants` is best-suited to chew on, because they have small surface area + high test density + high consequence for surviving mutants.

This spec defines the pipeline that runs `cargo-mutants` weekly + on demand, computes per-module mutation scores, fails below a threshold, and surfaces the surviving mutants for triage.

## 2. Scope and non-scope

### 2.1 In scope

- A `ci-mutation.yml` GitHub Actions workflow with weekly schedule (`cron: "0 4 * * 1"` — Mondays 04:00 UTC) and `workflow_dispatch` for manual runs.
- Four per-target jobs running `cargo mutants` against:
  - `wacp-transport` — `src/auth_api_key.rs`, `src/auth_session.rs`, `src/auth_oauth.rs`, `src/auth.rs`. The auth path is the runtime's primary security boundary.
  - `wacp-tools` — `src/execution.rs`. Tool execution is where agent intentions become real-world side effects.
  - `console-core` — `src/session_launcher.rs`. The W2 launch flow with rollback semantics.
  - `console-core` — `src/session_monitor.rs`. The W3 session lifecycle observer.
- A small `scripts/mutation-score.py` (or shell equivalent) that parses `mutants.out/outcomes.json`, prints the score + survivors, and exits non-zero if score < 85 %.
- A 5th aggregator job (`mutation-summary`) that downloads each per-target artifact and writes a unified markdown summary to `$GITHUB_STEP_SUMMARY`. Always runs (`if: always()`) so a single failing target doesn't hide the others.
- `// mutants:skip` annotations approved as a documented escape hatch for false-positive mutants (e.g., a `>` mutated to `>=` in a loop bound where both produce the same effective behavior).

### 2.2 Out of scope

- Workflow does not block PRs. Mutation testing is too slow (~5–20 min per target) and too noisy for per-PR gating. The workflow runs on its own cadence and surfaces regressions for triage, not for blocking.
- No mutation testing on the frontend (Stryker Mutator for TS/Vitest is a separate pipeline; deferred until Vitest coverage stabilizes — `AUDIT-2026-04-15.md` §13.7.7 / §13.7.9 future scope).
- No mutation testing on the Python SDK. `mutmut` exists but the SDK surface is small enough that targeted unit tests carry the load.
- Targeting fewer files than the audit lists (e.g., omitting `auth_oauth.rs`) — the four-module set is the **minimum viable** mutation footprint per the audit's risk analysis.

## 3. Workflow design

### 3.1 Triggers

```yaml
on:
  schedule:
    - cron: "0 4 * * 1"
  workflow_dispatch:
```

Weekly cadence. Manual dispatch is the escape hatch when an author wants to run mutation testing on a branch before merging a security-sensitive change. Per-PR run is supportable via the manual dispatch form (`workflow_dispatch.inputs.ref`).

### 3.2 Topology

Five jobs:

```
mutate-wacp-transport-auth ─┐
mutate-wacp-tools           ├─→ mutation-summary
mutate-console-launcher     │
mutate-console-monitor      ─┘
```

Per-target jobs run in parallel. The summary job depends on `needs: [...]` and `if: always()` so a target that fails its threshold doesn't prevent the summary from publishing.

### 3.3 Job skeleton

Per-target shape:

```yaml
mutate-<target>:
  name: Mutants — <human-friendly>
  runs-on: ubuntu-latest
  timeout-minutes: 60
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2
      with: { key: mutants-<target> }
    - name: Install protoc
      uses: arduino/setup-protoc@v3
      with: { version: "29.3" }
    - name: Install cargo-mutants
      uses: taiki-e/install-action@v2
      with: { tool: cargo-mutants }
    - name: Run mutants
      run: cargo mutants -p <crate> --file <files...> --output mutants.out --no-shuffle
      continue-on-error: true
    - name: Compute score + post summary
      run: python3 scripts/mutation-score.py --target <target> --threshold 85 --out mutants.out
    - uses: actions/upload-artifact@v4
      if: always()
      with:
        name: mutants-<target>
        path: mutants.out/
```

Notes on the steps:
- `continue-on-error: true` on the `cargo mutants` step — cargo-mutants exits 1 when any mutant survives, but we want the score-check step to be the authoritative pass/fail signal (it can fail with a more informative summary).
- `--no-shuffle` makes the output deterministic for diffing across runs.
- `--output mutants.out` is the default location; named explicitly for the artifact step.

## 4. Per-target jobs

### 4.1 wacp-transport (auth)

```yaml
- name: Run mutants
  run: |
    cargo mutants -p wacp-transport \
      --file src/auth_api_key.rs \
      --file src/auth_session.rs \
      --file src/auth_oauth.rs \
      --file src/auth.rs \
      --output mutants.out \
      --no-shuffle
```

**Why these files.** Auth decisions in WACP are constant-time-equality comparisons of hashed tokens (per §11 #3 from the audit). A mutant that flips a `==` to `!=` or replaces `Status::Unauthenticated` with `Status::Ok` is a critical bug. Tests must catch these.

### 4.2 wacp-tools (execution)

```yaml
- name: Run mutants
  run: |
    cargo mutants -p wacp-tools \
      --file src/execution.rs \
      --output mutants.out \
      --no-shuffle
```

**Why this file.** Tool execution evaluates policy gates, applies cancellation tokens, and surfaces refusals. A mutant in cancellation logic that converts a cancellation into a no-op is silent in line coverage but a major incident in production.

### 4.3 console-core (session_launcher)

```yaml
- name: Run mutants
  run: |
    cargo mutants -p console-core \
      --file src/session_launcher.rs \
      --output mutants.out \
      --no-shuffle
```

**Why this file.** W2's rollback path is conditional on each step's success. A mutant that skips a rollback branch on Dispatch failure leaves orphan workspaces on the runtime — the kind of bug T7.5 catches if the test exists, but mutation testing catches the test gap if it doesn't.

### 4.4 console-core (session_monitor)

```yaml
- name: Run mutants
  run: |
    cargo mutants -p console-core \
      --file src/session_monitor.rs \
      --output mutants.out \
      --no-shuffle
```

**Why this file.** W3's stream drivers + completion detection. A mutant that flips the terminal-state comparison (`== Closed` to `== Conflicted`, the same bug that bit T7.3 from a different angle) survives any test that doesn't observe the terminal lifecycle frame.

## 5. Threshold + failure semantics

Pass bar: ≥ 85 % per module. Score formula:

```
killed   = caught + timeout
testable = total - unviable
score    = killed / testable * 100
```

Rationale:
- **Caught**: tests detected the mutant. Counts as killed.
- **Timeout**: tests took too long (likely infinite loop introduced). Counts as killed because the timeout signal is itself observable.
- **Unviable**: code didn't compile after mutation (e.g., type-checker rejected the change). Excluded from the denominator — these are mutants the type system already kills.
- **Missed**: tests passed despite the mutation. Counts against the score.

The 85 % threshold is the audit's stated bar. It's tight enough to surface real coverage gaps but loose enough to accommodate the ~5–10 % false-positive rate cargo-mutants exhibits in practice (mutants that produce equivalent behavior, e.g., swapping operands in a commutative operation).

When a target is below threshold, the score-check step exits non-zero. The job fails. The aggregator job runs anyway (`if: always()`) and the summary marks the failing target with ⚠ (per §6).

## 6. Aggregator job + summary

```yaml
mutation-summary:
  name: Mutation summary
  runs-on: ubuntu-latest
  needs: [mutate-wacp-transport-auth, mutate-wacp-tools, mutate-console-launcher, mutate-console-monitor]
  if: always()
  steps:
    - uses: actions/checkout@v4
    - uses: actions/download-artifact@v4
      with: { path: mutants/ }
    - name: Build summary
      run: python3 scripts/mutation-summary.py mutants/ >> "$GITHUB_STEP_SUMMARY"
```

The `mutation-summary.py` script reads each target's `outcomes.json`, formats:

```markdown
## Mutation testing — <date>

| Target | Score | Killed | Missed | Timeout | Unviable | Total |
|---|---|---|---|---|---|---|
| wacp-transport (auth) | 92.3 % | 47 | 4 | 0 | 12 | 63 |
| wacp-tools (execution) | 88.1 % | 37 | 5 | 0 | 8 | 50 |
| console-core (launcher) | 79.4 % ⚠ | 27 | 7 | 0 | 6 | 40 |
| console-core (monitor) | 86.5 % | 64 | 10 | 0 | 15 | 89 |

### Surviving mutants

#### console-core (launcher) — below threshold

- `src/session_launcher.rs:298` — `if !rollback_workspaces.is_empty()` → `if true` (Missed)
- ...
```

The aggregator is the authoritative output. Per-job summaries also exist (in their own job's `Summary` panel) so an author triaging a single failing target can drill in directly.

## 7. Triage loop

When a target falls below threshold or surfaces a surviving mutant that should be killed:

1. **Reproduce locally.** `cargo mutants -p <crate> --file <file> --no-shuffle` on the same code reproduces the survivors. Output goes to `mutants.out/`.
2. **Classify the mutant.**
   - **Real coverage gap** — write a killer test. Run `cargo test` to confirm green pre-mutation. Run mutants again to confirm the mutant is now caught.
   - **Equivalent mutant** — the mutation produces semantically-equivalent behavior (e.g., `>` → `>=` in a loop bound that's never hit at the boundary). Add `// mutants:skip` directly above the line with a one-line comment justifying why.
3. **Land the fix.** Single commit per mutant batch is fine; reference the mutation-testing run id in the commit body.
4. **Update the spec table.** When a target's score moves above threshold permanently, update §4 with the new floor value and remove any temporary `// mutants:skip` annotations that were placeholder guards.

The triage loop sits with the on-call dev-tools owner. Default cadence: triage the Monday run by Wednesday EOD.

`// mutants:skip` annotations are reviewed during quarterly tech-debt sweeps. Each annotation must justify the equivalence; placeholder skips ("TODO: write a test") are tracked as their own issue.

## 8. Acceptance criteria

- `.github/workflows/ci-mutation.yml` exists and parses (validated by `actionlint` if present, otherwise by `yamllint`).
- `scripts/mutation-score.py` and `scripts/mutation-summary.py` exist, are executable, and their unit tests (if any) pass.
- This spec lives at `wacp-console/specs/coding/wcon-mutation-testing.md`.
- `AUDIT-2026-04-15.md` §13.5 mutation-testing entry is removed once the first weekly run completes (deferred to that run; the deliverables in §13.7.9 are this commit's scope).
- First weekly scheduled run produces a per-module score and either passes the threshold or files a triage task inline in the summary.

## 9. References

| ID | Title | Relationship |
|----|-------|--------------|
| AUDIT-2026-04-15 | Codebase Health Audit — §12.6, §13.7.9 | parent audit + deliverable |
| wcon-w2-launch-flow | W2 — Launch flow + rollback | constrains (session_launcher target) |
| wcon-w3-session-monitor | W3 — Session monitor | constrains (session_monitor target) |
| cargo-mutants | https://mutants.rs | tool reference |

*WACP Platform — authored by Akil Abderrahim and Claude Opus 4.7 (1M context).*
