---
id: wacp-closeout-plan
type: impl
status: draft
created: 2026-04-19T15:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [plan, pre-launch, tech-debt, tooling, refactor]
depends_on: [tech-debt-2026-04-18, wacp-git-strategy, HEALTH-LOG-12.5]
---

# Closeout Plan — Nine Open Items

> **Triggering finding:** `tech-debt-2026-04-18.md` §7 (three user decisions), `impl/git-strategy.md` §13 (four tooling items), `HEALTH-LOG.md` §12.5 (one investigation) — closes the last nine items blocking a v0.1.0-eligible state.
> **Target branches:** P1 on `ci/pre-launch-closeout`; P2 is repo-admin (no commit); P3 is `dev`→`main` ff; P4 on `refactor/file-splits`; P5 direct to `dev`.
> **Rough effort:** ~13–17 h total — high confidence on P1/P2/P3/P5, medium on P4 (per tech-debt §3.2).
> **Not in scope:** Codecov monthly ratchet (§13.7.10, awaiting baseline settle); four §13.7.8 deferred sub-scenarios (infrastructure-blocked); OCI release workflow changes (separate launch gate); v0.1.0 tagging ceremony itself.

## 1. Goal & Motivation

As of `d0be941` (dev+main aligned post-2026-04-18 ff), every AUDIT §13.7 work package except 13.7.10 is closed. What remains is a residue of open questions and pre-launch hygiene items that surfaced during §11.4 (2200-line files → copy-paste bug) and during the tech-debt + git-strategy doc drafts that followed.

The user has now answered the three tech-debt §7 decisions (highway-ui: **delete**; lint thresholds: **agree**; refactor: **single PR**) and greenlit the five smaller items. This plan sequences the work so that:

1. **Pre-launch items** (Bucket A + Bucket C + git-strategy §13 tooling) land in **one PR** on a topic branch — minimizes churn, keeps every drift answerable against one green CI run.
2. **Repo admin** (branch protection) happens once, out-of-band from the PR.
3. **dev → main ff** closes the pre-launch batch cleanly before refactor work starts.
4. **Bucket B refactor** happens post-ff on a dedicated branch — single blame event, behavior-preserving.
5. **§12.5 bisect** is orthogonal and parallelizable.

If not done: the 2200-line-file pattern recurs, the legacy `highway-ui` subtree keeps burning CI minutes + contributor mental model, `main` stays force-pushable until someone accidentally does it, and the §12.5 unmount stays a known-bad in the backlog.

## 2. Phases

| Phase | Deliverable | Effort | Blocker | Success signal |
|---|---|---|---|---|
| **P1** | Pre-launch PR on `ci/pre-launch-closeout`: sourcemap flip + highway-ui delete + file-size lint + `.gitmessage` + opt-in pre-push hook + rerere note | ~2.5–3 h | — | all 4 CI workflows green on draft PR; ff'd to `dev` |
| **P2** | GitHub branch protection on `main` + `dev` | ~10 min | — (parallel to P1) | repo settings show linear-history + disallow-force-push on both |
| **P3** | `dev` → `main` ff ceremony | ~15–30 min | P1 landed, P2 configured | all 4 workflows green on `main` post-push |
| **P4** | Bucket B refactor per tech-debt §3.2 — 9 oversized files split on `refactor/file-splits` | ~8–12 h | P3 ff'd | no Rust file >800 lines in refactor scope; `cargo test --workspace` same pass count |
| **P5** | §12.5 ProfilesPage Create-New unmount bisect + fix or scope | ~30–60 min | — (parallel to any phase) | HEALTH-LOG §12.5 resolved or follow-up plan scaffolded |

## 3. Deliverables — per phase

### 3.1 Phase P1 — pre-launch PR on `ci/pre-launch-closeout`

Branch from `dev`. Draft PR → `main` for CI per git-strategy §7.2. Ff topic → `dev` when CI green.

**Commit sequence:**

1. **`fix(frontend): §3.1 A.1 — vite sourcemap off in release build`**
   - `wacp-console/frontend/vite.config.ts:24` — `sourcemap: true` → `sourcemap: false`.
   - Verify: `pnpm -C wacp-console/frontend build` → no `.map` in `dist/`.
   - Payoff: −1.7 MB per binary + per OCI image; info-leak closed.

2. **`chore(highway-ui): §3.1 A.2 — delete legacy webapp subtree`**
   - `git rm -r wacp/highway-ui/`.
   - Remove `TypeScript — highway-ui` stage from `.github/workflows/ci-wacp.yml:81–97`.
   - Grep-sweep + update references in: `SEED.md`, `AUDIT-2026-04-15.md`, `README.md`, `tech-debt-2026-04-18.md` §2.2/§2.4/§5, `wacp/impl/highway-ui.md` (move to `wacp/impl/archive/` with "superseded by W4" note).
   - Use the `blast-radius` skill pre-commit to catch stragglers.

3. **`ci(lint): §3.3 C.1+C.2 — file-size guardrail + allowlist`**
   - New `.file-size-allowlist` at repo root (content per tech-debt §5, minus the four `wacp/highway-ui/src/gen/*` entries since that subtree was just deleted in commit 2).
   - New `file-size` job in `.github/workflows/ci-lint.yml`:
     - Rust: warn >1000 lines (`.rs`, excluding `**/tests.rs`, `**/tests/*.rs`, files listed in allowlist); fail >1500 unless allowlisted.
     - TS: warn >500 lines (`.ts`, `.tsx`, excluding `**/*.test.{ts,tsx}`, generated files listed in allowlist); fail >1000 unless allowlisted.
   - Verify job runs green on this PR (allowlist protects current oversized files).

4. **`chore(git): §13 — .gitmessage template + rerere recommendation`**
   - New `.gitmessage` at repo root: `<type>(<scope>): <subject>` skeleton + Co-Authored-By trailer placeholder + §scope hint.
   - README section (or new `CONTRIBUTING.md` §"Development setup") with opt-in commands: `git config commit.template .gitmessage` + `git config --global rerere.enabled true`.

5. **`chore(hooks): §13 — opt-in pre-push fmt+clippy hook`**
   - New `.githooks/pre-push` running `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings`.
   - New `scripts/install-hooks.sh` setting `core.hooksPath = .githooks` for the local repo.
   - README section: "opt-in local hook — run `./scripts/install-hooks.sh`". Default behavior unchanged.

6. **`docs: close tech-debt §7 + git-strategy §13 + seed refresh`**
   - `tech-debt-2026-04-18.md` §7 — mark Q1 **delete** / Q2 **agree** / Q3 **single PR** with date + commit anchor.
   - `impl/git-strategy.md` §11.5 — update "Not currently configured" → points at P2 configuration (done separately).
   - `impl/git-strategy.md` §13 — strike through landed items.
   - Invoke `seed-refresh` skill: append commit table rows, update "dev ahead of main" count, update state paragraph.

### 3.2 Phase P2 — repo admin (no commit)

Three clicks per branch in GitHub:
- `aakil98/wacp-platform` → Settings → Branches → Add rule for `main`:
  - Require linear history: **on**.
  - Allow force pushes: **off**.
  - Allow deletions: **off**.
- Same rule for `dev`.

Can run any time relative to P1. Flag the configuration date in `impl/git-strategy.md` §11.5 as a small commit (merges into P1.6 above if still in flight).

### 3.3 Phase P3 — dev → main ff

Per git-strategy §9.3. Uses the `ff-main` skill:

```bash
git fetch aakil98
git switch main
git merge --ff-only aakil98/dev
git push aakil98 main
```

Verify post-push:
- `git log --oneline main..dev` → empty.
- All 4 CI workflows green on `main` within ~15 min.
- Refresh SEED.md "Current State" paragraph via `seed-refresh` skill.

### 3.4 Phase P4 — Bucket B refactor

**Authoritative plan: `tech-debt-2026-04-18.md` §3.2 Bucket B.** This phase executes that plan; do not restate its deliverables here. Summary only:

- Branch: `refactor/file-splits` from `main` (post-P3 ff).
- Scope: B.1 `init.rs`, B.2 `session_monitor.rs`, B.3 `session_launcher.rs`, B.4 `routes/highway.rs`, B.5 `config`/`recovery`/`rest_gateway`/`routes/sessions`/`tools/execution`.
- Behavior-preserving — extraction + `pub(crate)` tweaks + import fix-ups only.
- Single PR review unit, ff (not squash) to `dev` to preserve §X.Y anchors per git-strategy §5.2.
- Shrink `.file-size-allowlist` Rust entries as files split. Target: allowlist Rust section empty or near-empty.

Consider invoking `new-plan` for a Bucket-B-specific plan if the work sprawls past one session; tech-debt §3.2 is the default source of truth.

### 3.5 Phase P5 — ProfilesPage Create-New unmount bisect

Per HEALTH-LOG §12.5. Orthogonal to all other phases.

- Reproduce on current `dev` (or post-P3 `main`) — click "Create New" on `/profiles`, observe full React unmount.
- Bisect (manual or `git bisect run`) to identify the commit that introduced the regression.
- If fix scope ≤1 commit: land directly on `dev`.
- If structural: scaffold a follow-up plan via `new-plan`, close this phase with "scoped forward".
- Update HEALTH-LOG §12.5 with root cause + resolution status. Use the `health` skill if the drift evolved mid-investigation.

## 4. Acceptance Criteria

### P1
- [ ] `wacp-console/frontend/vite.config.ts` has `sourcemap: false`.
- [ ] `ls wacp/highway-ui 2>/dev/null; echo $?` → `1` (directory removed).
- [ ] `.github/workflows/ci-wacp.yml` has no `TypeScript — highway-ui` job.
- [ ] No active (non-archived, non-historical) reference to `wacp/highway-ui` in `SEED.md`, `AUDIT-2026-04-15.md`, `README.md`.
- [ ] `.file-size-allowlist` exists at repo root with entries per tech-debt §5 minus deleted highway-ui rows.
- [ ] `file-size` job in `.github/workflows/ci-lint.yml` passes on the PR (allowlist covers current oversized files).
- [ ] `.gitmessage` exists + README/CONTRIBUTING documents setup command.
- [ ] `.githooks/pre-push` + `scripts/install-hooks.sh` exist + README documents opt-in install.
- [ ] `tech-debt-2026-04-18.md` §7 Q1/Q2/Q3 marked answered.
- [ ] `impl/git-strategy.md` §13 items struck through with landing-commit anchors.
- [ ] Draft PR `ci/pre-launch-closeout` → `main`: all 4 CI workflows green.
- [ ] `git merge --ff-only ci/pre-launch-closeout` to `dev` succeeds.
- [ ] Topic branch deleted local + remote.
- [ ] `SEED.md` refreshed via `seed-refresh` skill.

### P2
- [ ] GitHub repo settings for `aakil98/wacp-platform`:
  - `main` rule: require linear history; disallow force-push; disallow deletion.
  - `dev` rule: same.
- [ ] `impl/git-strategy.md` §11.5 "Not currently configured" note updated to reflect configured state + date.

### P3
- [ ] `git log --oneline main..dev` empty post-ff.
- [ ] `git push aakil98 main` succeeds.
- [ ] All 4 CI workflows green on `main` within 20 min of push.
- [ ] `SEED.md` "dev/main state" paragraph updated to reflect new ff SHA + date.

### P4 — gated by tech-debt §3.2 acceptance
- [ ] Every file in tech-debt §3.2 B.1–B.5 split per plan (no Rust file >800 lines in refactor scope).
- [ ] `cargo test --workspace` pass count unchanged from pre-refactor baseline (recorded pre-branch).
- [ ] `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `.file-size-allowlist` Rust production section reduced to ≤1 entry (or empty).
- [ ] `refactor/file-splits` ff'd to `dev`; PR closed.
- [ ] `SEED.md` refreshed with refactor landing table entry.

### P5
- [ ] HEALTH-LOG §12.5 root cause documented (specific file + commit that introduced regression).
- [ ] Fix landed on `dev` OR follow-up plan scaffolded via `new-plan`.
- [ ] HEALTH-LOG §12.5 status line updated: resolved / scoped-forward / won't-fix with rationale.

**All five phases ticked → plan eligible for `archive-plan` skill.**

## 5. Risks / Open Questions

- **P1 branch scope name.** git-strategy §3.2 lists `ci/*`, `refactor/*`, `audit/*`, `fix/*`, `feat/*`, `hotfix/*` — no `chore/*`. Using `ci/pre-launch-closeout` since file-size lint + hook install are CI-adjacent and drive the largest chunk. Alternative: `audit/pre-launch-closeout` (frames as closing tech-debt §7). Either works; sticking with `ci/*` unless user prefers `audit/*`.
- **P1 commit-6 doc race.** `tech-debt-2026-04-18.md` is edited in P1.6 to mark §7 answered. If the doc is touched in parallel (e.g., a new health finding filed mid-PR), rebase is mechanical but worth watching.
- **P2 linear-history + ff interaction.** GitHub's "require linear history" rejects merge-commits but accepts ff-only merges. Our flow uses `git merge --ff-only` so this should be compatible, but test once (use the P3 ff as the first test — if it rejects, we learn before tagging).
- **P4 effort variance.** tech-debt §3.2 estimates 8–12 h; actual could extend if import reshuffles trigger clippy lints that the file-size reality has been masking. Budget 12 h, accept up to 16 h before re-scoping.
- **P5 bisect blast-radius.** May reveal the unmount was introduced by a TanStack Query / React 19 upgrade that affects other surfaces. If so, P5 scope-cuts and a follow-up plan is scaffolded.
- **`wacp/impl/highway-ui.md` disposition.** Recommend archive to `wacp/impl/archive/highway-ui.md` with a "superseded by W4" note rather than delete — preserves the "this was the prior approach" history. Flag if user prefers delete.
- **Pre-push hook opt-in discoverability.** Default contributor behavior is unchanged (hook only runs if installed). Make the README note prominent so new contributors notice the opt-in path.

## 6. References

| ID | Title | Relationship |
|----|-------|--------------|
| tech-debt-2026-04-18 | Tech-debt baseline + triage (platform root) | informs Buckets A/C + §7 decisions; P4 implements Bucket B |
| wacp-git-strategy | Git Strategy — wacp-platform (`impl/git-strategy.md`) | informs P1 branch + commit conventions, §13 tooling items, §9.3 ff ceremony, §11.5 risk class P2 closes |
| HEALTH-LOG-12.5 | ProfilesPage Create-New unmount (platform root HEALTH-LOG §12.5) | P5 closes |
| AUDIT-2026-04-15 | Codebase audit + §13.7 work-package tracker | P1–P4 close post-audit loose ends referenced in §11 punch list + §13.7 |
| wcon-vision | Product Vision (`wacp-console/specs/`) | implicit — highway-ui delete is OK because `wcon-vision` W4 supersedes it |

## 7. Execution log

| Phase | Commit | Date | Note |
|---|---|---|---|
| (plan scaffold, on `dev`) | `6197263` | 2026-04-19 | direct-to-dev per git-strategy §3.1 |
| P1.1 sourcemap | `03d195d` | 2026-04-19 | verified `pnpm build` emits no `.map` |
| P1.2 highway-ui delete | `f75979e` | 2026-04-19 | 74 files / -11,745 / +11; spec archived w/ `status: superseded` |
| P1.3 file-size lint | `08ff88f` | 2026-04-19 | allowlist +1 vs tech-debt §5 (api/hooks/index.test.ts, 1065 lines) |
| P1.4 gitmessage + rerere | `3364098` | 2026-04-19 | README Development Setup section added |
| P1.5 pre-push hook | `7e83da5` | 2026-04-19 | opt-in; README line 3 added |
| P1.6 docs + seed | — | 2026-04-19 | in progress — tech-debt §7 answered, git-strategy §13 struck, SEED manual refresh (15th pass) |
| P2 branch protection | — | — | repo admin, no commit |
| P3 ff dev→main | — | — | uses `ff-main` skill |
| P4 Bucket B refactor | — | — | per tech-debt §3.2; may spawn its own sub-plan |
| P5 §12.5 bisect | — | — | update HEALTH-LOG.md with outcome |

---

*Scaffolded 2026-04-19 via `new-plan` skill after user answered tech-debt §7 decisions + greenlit git-strategy §13 tooling + §12.5 investigation. Consolidates nine items into one sequenced doc so future sessions have a single resumption point.*
