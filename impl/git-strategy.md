# Git Strategy — `wacp-platform`

> Codifies the branching, commit, merge, CI, and release conventions in use today across the `wacp-platform` monorepo. Written 2026-04-18 after the project settled into a steady dev→main fast-forward cadence and accumulated enough commit history (every commit tagged with a spec section like §11.4, §13.7.8 I3, §2.7.2) to name the pattern.
>
> Scope: this is the operational doc for how code moves through the tree. Companion docs: `impl/merge-plan.md` (M0–M7 monorepo merger, one-time event), `impl/ci-health-2026-04-17.md` (CI workflow history), `tech-debt-2026-04-18.md` (refactor plan referenced in §8).

## 1. Purpose

Two goals, in tension and both required:

1. **Preserve a truthful history.** Commits that reference `§X.Y` audit sections, co-author Claude by model revision, and never rewrite after push. `git log` should remain trustworthy two years from now for "when did that bug enter?" or "why does this file look like this?" questions.
2. **Move fast with a single maintainer.** No gated approval queue, no long-lived release branches, no backport proliferation. Work flows scratch → topic → dev → main with fast-forward merges and zero ceremony.

The rules below are the equilibrium between these two forces. They assume solo maintenance with Claude-as-co-author; §10 notes which rules tighten when real users and collaborators arrive.

## 2. Branch taxonomy

| Branch | Role | Lifetime | Force-push? | CI runs? |
|---|---|---|---|---|
| `main` | Stable trunk. Tag points for releases. The history we publish. | permanent | **never** | push + PR-target |
| `dev` | Integration branch. Green-CI invariant. Batch point before ff. | permanent | **never** | push + PR-target |
| `{scope}/{slug}` | Topic branch for multi-commit work (e.g. `ci/cleanup-2.7`, `refactor/file-splits`). | ephemeral — delete after ff to `dev` | allowed (own branch) | only via draft PR (see §7) |
| `hotfix/{slug}` | Post-launch fix for a tagged release. Cherry-picked from `main`. | ephemeral | allowed pre-merge | as topic |
| `release/{slug}` | **Not used today.** Solo maintainer + no backports = no release branches. Reserved name if multi-version support arrives post-v1.0. | — | — | — |

**Remotes:**

| Remote | Role |
|---|---|
| `aakil98` | GitHub origin (`git@github.com:AAkil98/wacp-platform.git`). Push target. |
| `wacp-origin` | Local path to the pre-merge `wacp/` repo. Read-only reference; retained from M0–M7. |
| `console-origin` | Local path to the pre-merge `wacp-console/` repo. Same. |

The `*-origin` subtree remotes are historical artifacts. They're useful for answering "what did wacp look like before the merger?" via `git log --follow`. Don't push to them; don't fetch from them for ongoing work.

## 3. Branch lifecycle

The default flow for a change — from "I have an idea" to "it's on `main`":

```
               ┌── scratch edit in working tree
               │
               ▼
        decide: one commit or many?
        ┌──────┴──────┐
        │             │
   one commit     many commits
        │             │
        │             ▼
        │       create topic branch:
        │       git switch -c {scope}/{slug} dev
        │             │
        │             ▼
        │       commit N times, push to aakil98/{scope}/{slug}
        │             │
        │             ▼
        │       open draft PR to main  ← triggers CI on branch
        │             │
        │             ▼
        │       iterate until CI green + user-review complete
        │             │
        ▼             ▼
        commit directly to dev (one-shot)   OR   ff topic → dev
                         │                              │
                         └──────────────┬───────────────┘
                                        ▼
                              dev accumulates work
                                        │
                                        ▼
                         natural batch close (see §5.3)
                                        │
                                        ▼
                                ff dev → main
                                        │
                                        ▼
                              (optional) release tag
```

### 3.1 When to use a topic branch vs commit straight to `dev`

**Commit to `dev` directly** when:
- Single commit that stands alone (docs, typo fix, lint suppression, single-file refactor).
- Test landing for an already-closed section (e.g. "test(integration): §13.7.8 I4").
- The work is so small that the topic branch overhead costs more than it buys.

**Create a topic branch** when:
- Multi-commit work that's logically one unit (a whole phase like §2.7 across 9 commits, a file-split refactor across 10 files).
- Work might need to be abandoned (topic branches delete cleanly; `git reset` on `dev` is destructive to context).
- Work needs CI feedback before landing (topic → draft PR → CI runs; see §7).

### 3.2 Topic branch naming

`{scope}/{slug}` — scope indicates the *kind* of work, slug is the specific name.

- `ci/*` — CI pipeline work (`ci/cleanup-2.7`, `ci/protoc-rate-limit`).
- `refactor/*` — behavior-preserving restructuring (`refactor/file-splits`, `refactor/init-rs-modules`).
- `audit/*` — AUDIT-plan work across multiple commits (`audit/13-7-8-integration`, `audit/14-post-v1`).
- `fix/*` — multi-commit bug fixes (rare; most fixes are one commit to `dev`).
- `feat/*` — multi-commit feature work (also rare pre-v0.1; most features are one phase on `dev`).

Slugs are stable: once named, don't rename. The branch name shows up in PR metadata and the reflog.

## 4. Commit conventions

Base convention: Conventional Commits, per the root `CLAUDE.md` §Commit Conventions. Extended below with project specifics.

### 4.1 Format

```
<type>(<scope>): <subject>

<body — optional, wraps at 72 cols>

<trailer — Co-Authored-By line>
```

**Types:** `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `ci`, `build`, `perf`.

**Scope:** one of:
- **Spec anchor:** `§2.7.4`, `§11.4`, `§13.7.8 I3` — used when the commit lands part of a numbered work package from `AUDIT-*.md` or a `impl/*.md` plan.
- **Module name:** `wacp-runtime`, `console-api`, `frontend`, `sdk-python` — used when the work is module-scoped and doesn't map to a spec anchor.
- **Workflow name:** `ci-lint`, `ci-console`, `release-runtime` — for CI-only commits.
- **Combined:** `console-integration+wacp-runtime` when a commit legitimately spans crates (rare; prefer single-scope commits).

**Subject:** imperative mood, lowercase, no period, ≤50 chars ideally, ≤72 hard.

### 4.2 Examples (all from the live history)

```
fix(wacp-runtime): §11.4 — route 7 internal-enum casts through _to_proto helpers
test(integration): §13.7.8 I3 — auth_matrix (12 tests)
ci(deny): allow CDLA-Permissive-2.0 + relax wildcards to warn
docs(seed): refresh — §2.1–§2.6 CI cleanup landed, §2.7 plan drafted
feat(wacp-runtime+wacp-sdk): §13.7.6b WA2 — EmitSignal drives workspace FSM
```

### 4.3 Body (when to write one)

Use a body when:
- The commit references a bug the *message alone* can't capture ("fixed X" — why was X broken? what was the path to discovery?).
- The commit is a non-obvious design choice that future-you will second-guess.
- The commit closes or opens a deferred item (note it explicitly so the next session finds it).

Skip the body when:
- The subject plus the diff are self-explanatory.
- The subject already names the spec section; the body would just restate the diff.

**Don't** write bodies that are:
- Paraphrases of the subject.
- Running commentary of the work session.
- Lists of files touched (the diff shows them).

### 4.4 Co-authorship

Claude sessions co-author every commit they produce. Trailer format:

```
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

Rules:
- **Always** add the trailer when Claude wrote or substantially edited the change.
- **Update the model revision** when it changes (past commits stay with the model that wrote them — co-authorship is a point-in-time attestation).
- **Don't** add the trailer for commits Claude didn't author (e.g., a manual `git revert` you ran alone).

### 4.5 What not to reference in messages

- Transient build/CI output that will rot (specific log URLs, ephemeral run IDs).
- The current working directory or shell state.
- "See the chat" or "as discussed" — the commit must stand alone.

## 5. Merge strategy

### 5.1 Fast-forward as the default

`dev` → `main` and topic → `dev` are both **fast-forward** merges.

Why:
- Preserves the commit lineage. `git log --oneline main` shows exactly the work that happened, in the order it happened.
- No merge commits cluttering history.
- Trivially bisectable: `git bisect` never has to walk a merge commit.
- Aligns with the AUDIT/SEED pattern of "every commit references one §X.Y unit".

How:
```bash
git switch main
git merge --ff-only dev
git push aakil98 main
```

If `git merge --ff-only` fails, `main` has diverged from `dev` — investigate before proceeding. Typically this means either a commit landed on `main` directly (don't do that; see §5.4) or the local `main` is stale (`git fetch aakil98` first).

### 5.2 When fast-forward is wrong

Squash-merge instead when:
- **Landing a multi-commit topic branch that reviewers viewed as a unit**, once there are real reviewers post-v0.1. Pre-v0.1 (solo maintenance), still ff — every commit in the topic branch was authored by the same person and the individual commits have context value.
- **The topic branch has churn you don't want preserved** (15 commits of "wip", "fix lint", "try again"). Rebase-squash the topic branch locally first, *then* fast-forward. Never squash-merge at the `git merge` step — that creates a single opaque commit that loses the §X.Y anchors.

Merge commits (`--no-ff`) are **not used** in this repo. If one shows up accidentally, revert and redo.

### 5.3 dev → main cadence

**Batch-close triggered, not time-based.** A "natural batch" is when:
- A coherent work unit finishes — e.g., "all of §13.7.8 I1–I5 landed", "§2.7 cleanup complete across all phases".
- Every CI workflow is green on `dev`.
- No half-finished work is interleaved with the batch (checked via `git log main..dev` — every commit should belong to the named batch).

Rough heuristic: 10–40 commits between ffs. "34 commits ahead" (the current state) is at the upper end and signals the batch is ready.

Anti-patterns:
- Daily/weekly ffs regardless of batch state → fragments coherent units across tags.
- 100+ commits between ffs → if CI ever breaks on `dev` and it's not caught immediately, the bisect surface balloons.

### 5.4 Direct commits to `main`

**Reserved for three cases:**
1. **Revert commits** undoing a bad change that landed on `main` (see §11.2). Even then, prefer reverting on `dev` and ffing forward if possible.
2. **Release tags** — tagging is not a commit, but if a release needs a `CHANGELOG` bump as a direct commit, do it on `main`.
3. **Hotfix landings** — see §11.3.

Everything else goes through `dev`. Rationale: `dev` is where CI runs before ff. Committing directly to `main` skips the verification step.

## 6. History hygiene

### 6.1 Amend

Allowed **only** before push. Once a commit is pushed to `aakil98/`, never amend it — amending creates a new commit hash, and anyone who pulled the old one (CI included) now has a divergent local copy.

Typical pre-push amend: "I forgot to stage one file, and the commit hasn't left my machine yet" → `git add` + `git commit --amend --no-edit`. Fine.

### 6.2 Rebase

Allowed on topic branches pre-PR-review. Typical uses:
- Squash 15 "wip" commits down to 3 coherent ones before opening a PR.
- Rebase a topic branch onto a newer `dev` (`git rebase dev`) when `dev` has moved forward and the topic needs the new state.
- Split a too-large commit with `git rebase -i` then `edit` then `reset HEAD^` then re-stage in pieces.

**Not allowed** on `dev` or `main`. Rebasing a shared branch is the surest way to break the "never rewrite pushed history" rule.

### 6.3 Force-push

| Target | Force-push? |
|---|---|
| `aakil98/main` | **never** |
| `aakil98/dev` | **never** |
| `aakil98/{scope}/{slug}` (topic) | allowed — own branch, no collaborators |
| `aakil98/hotfix/*` | allowed pre-merge |

Use `git push --force-with-lease` over `--force`. The `--force-with-lease` variant refuses the push if someone else has updated the remote since your last fetch, which is exactly what you want as a safeguard against races.

### 6.4 Revert

`git revert` creates a new commit that undoes the named commit. Always use revert on shared branches (`main`, `dev`). Never `git reset --hard` a pushed branch — that's force-pushing by another name and will break CI state + anyone else's working copy.

## 7. CI integration

### 7.1 Workflows + their triggers

From `.github/workflows/`:

| Workflow | Push on | PR target | Schedule |
|---|---|---|---|
| `ci-lint.yml` | `[main, dev]` | `[main]` | — |
| `ci-wacp.yml` | `[main, dev]` (path-filtered on `wacp/**`) | `[main]` (same paths) | — |
| `ci-console.yml` | `[main, dev]` (path-filtered on `wacp-console/**`) | `[main]` (same paths) | — |
| `coverage.yml` | `[main, dev]` | `[main]` | — |
| `ci-mutation.yml` | — | — | cron `0 4 * * 1` (Mondays 04:00 UTC) + `workflow_dispatch` |
| `release-runtime.yml` | tags `wacp-runtime-v*` | — | — |
| `release-console.yml` | tags `wacp-console-v*` | — | — |

### 7.2 The draft-PR-for-CI pattern

**Topic-branch pushes do not trigger CI.** The `push:` triggers above are scoped to `[main, dev]`. To get CI on a topic branch, **open a draft PR targeting `main`** — that activates the `pull_request: branches: [main]` trigger.

Practical flow:
```bash
git switch -c ci/cleanup-2.7 dev
# … commits …
git push -u aakil98 ci/cleanup-2.7
gh pr create --draft --base main --head ci/cleanup-2.7 --title "WIP: §2.7 cleanup"
```

The draft PR is a CI tripwire, not a review request. Close or merge it when the topic branch ffs to `dev`. Don't feel obligated to write a final PR body for a draft that's just running CI — the real commits go straight to `dev`.

### 7.3 The "dev stays green" invariant

`dev` must have every CI workflow green at the moment a ff to `main` happens. Not "mostly green" — **every** workflow. Reasoning: `main` inherits whatever state `dev` was in, and a broken `main` cascades into the OCI release pipeline and into any downstream clones.

If `dev` is red:
- **Fix forward** — most failures are mechanical (lint drift, test timing, CI-config rot). Fix and commit to `dev`.
- **Revert** if fix-forward looks >30 min — the offending commit reverts cleanly and we stop the bleeding.

### 7.4 Bypassing CI is never acceptable

No `--no-verify`, no `[skip ci]`, no "force merge" override, no manually flipping required checks to passed. If CI can be bypassed on this project, the §7.3 invariant is meaningless. Document exemptions as `.github/` config changes, not as per-commit skips.

## 8. Release tagging

Per M0-M7 + ADR-009, the two binaries (`wacp-runtime` and `wacp-console`) ship as independent OCI images with independent version cadences.

### 8.1 Tag format

- `wacp-runtime-vX.Y.Z` — triggers `release-runtime.yml`.
- `wacp-console-vX.Y.Z` — triggers `release-console.yml`.

Both use semver (`MAJOR.MINOR.PATCH`). The two binaries' versions are decoupled — `wacp-runtime-v0.2.0` can coexist with `wacp-console-v0.1.4`.

### 8.2 Tag ceremony

```bash
git switch main
git pull aakil98 main
git tag -a wacp-runtime-v0.1.0 -m "wacp-runtime v0.1.0"
git push aakil98 wacp-runtime-v0.1.0
```

- **Annotated tags only** (`git tag -a`, never lightweight `git tag X`). Annotated tags carry the tagger identity + message; lightweight tags are just refs and look like accidents.
- **Tag from `main`, not `dev`**. The release workflows trigger on the tag; the tag must point to a commit that's on `main` so the release reflects trunk state.
- **Tag, then push the tag.** `git push` alone doesn't push tags; need `git push <remote> <tag>` or `git push --tags` (the explicit form is safer).

### 8.3 Pre-release tags

If a release candidate is needed: `wacp-runtime-v0.1.0-rc.1`. Release workflows should accept `v*` patterns with suffixes; verify the workflow globs handle this. Not used yet (pre-v0.1.0), but reserved.

### 8.4 What gates a v0.1.0 tag

Per SEED.md "Merge strategy" section:
1. Rust branch-coverage floor ≥ 85 % (§13.7.9 ratchet).
2. First mutation run ≥ 85 % per module (§13.7.9 Monday cron).
3. All four CI workflows green on `main` (not just `dev`) after ff.
4. Tech-debt doc Bucket A landed (sourcemap flip + highway-ui decision per `tech-debt-2026-04-18.md` §3.1).

Tagging before these gates is not prevented by tooling — it's prevented by habit + this doc.

## 9. Common operations — recipes

### 9.1 Land a one-commit fix to `dev`

```bash
git switch dev
# … edit …
git add <files>
git commit -m "fix(scope): §X.Y — one-line subject

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push aakil98 dev
```

### 9.2 Run a multi-commit topic branch

```bash
git switch -c audit/13-7-8-integration dev
# … commits …
git push -u aakil98 audit/13-7-8-integration
gh pr create --draft --base main --head audit/13-7-8-integration --title "WIP: §13.7.8 integration"

# … iterate until CI green …

# Land to dev:
git switch dev
git merge --ff-only audit/13-7-8-integration
git push aakil98 dev

# Clean up:
gh pr close $PR --comment "landed via ff to dev"
git branch -d audit/13-7-8-integration
git push aakil98 --delete audit/13-7-8-integration
```

### 9.3 Fast-forward `dev` → `main`

```bash
git fetch aakil98
git switch main
git merge --ff-only aakil98/dev
git push aakil98 main
```

Verify with `git log --oneline main..dev` before the merge (should be empty after) and `git log --oneline main -5` after (should show the latest dev commits).

### 9.4 Tag a release

```bash
# After dev → main ff, with main pushed.
git switch main
git tag -a wacp-runtime-v0.1.0 -m "wacp-runtime v0.1.0 — inaugural release"
git push aakil98 wacp-runtime-v0.1.0

# Watch the release workflow kick off:
gh run watch $(gh run list --workflow=release-runtime.yml --limit 1 --json databaseId -q '.[0].databaseId')
```

## 10. Solo-maintainer defaults + post-v1.0 evolution

This doc's rules are tuned for one person + Claude. When real collaborators arrive, these tighten:

| Rule | Pre-v1.0 (today) | Post-v1.0 |
|---|---|---|
| `dev` → `main` ff | any time, batch-close discretion | CODEOWNERS + required reviews on PRs to `main` |
| Topic → `dev` ff | self-ff allowed | PR review required for topic branches |
| Squash-merge | never (preserve lineage) | allowed for noisy topic branches with many reviewers |
| Force-push | topic branches only | topic branches only; PR template discourages |
| Tagging | manual | automated via `release-please` or similar on `main` push |

The evolution isn't urgent. Don't pre-build collaboration scaffolding that has no collaborators to use it.

## 11. Failure handling

### 11.1 Broken commit on `dev`

1. Is CI red? Check which workflow.
2. Is the fix obvious + <30 min? → fix forward with a new commit.
3. Is the fix non-obvious or >30 min? → `git revert <bad-sha>` on `dev`, push, then work on the real fix as a topic branch.

### 11.2 Broken commit on `main`

Never reset `main`. Always use `git revert`:

```bash
git switch main
git revert <bad-sha>
git push aakil98 main
```

Then reconcile `dev`:
```bash
git switch dev
git merge main  # pulls the revert
git push aakil98 dev
```

If `dev` had further work on top of the bad commit, rebase that work onto the revert instead.

### 11.3 Hotfix on a released version

1. `git switch -c hotfix/desc main` (branch from `main` at the tagged release, or at `main`'s current tip if the fix applies to unreleased state too).
2. Commit the fix.
3. Open a draft PR to `main` for CI.
4. Once green, ff the hotfix to `main` (or squash-merge via PR if post-v1.0).
5. Tag the patch release (e.g., `wacp-runtime-v0.1.1`).
6. Forward-port to `dev`: `git switch dev; git merge main`.

### 11.4 Accidentally committed secrets

1. Don't push if not yet pushed. `git reset --soft HEAD~1`, unstage the secret, re-commit.
2. If already pushed: rotate the secret first (the secret is compromised the moment it hit GitHub). Then purge history — `git filter-repo` or BFG, force-push with team coordination.
3. File a process note in this doc's §14 for prevention.

### 11.5 Force-pushed `main` by accident

This is the "oh no" scenario. Recovery:
1. Every contributor has a local `main` with the pre-force state. Pick one and treat it as authoritative.
2. `git push aakil98 main --force-with-lease` from the authoritative copy.
3. Post-mortem in this doc § 14.

Prevent with branch protection on GitHub: require linear history + disallow force-pushes on `main` and `dev` via GitHub repo settings. **Configured 2026-04-20** via `gh api PUT repos/AAkil98/wacp-platform/branches/{main,dev}/protection` — `required_linear_history=true`, `allow_force_pushes=false`, `allow_deletions=false`. `enforce_admins=false` retained so the sole maintainer can bypass in emergencies (e.g., recovering from §11.5 itself); tighten to `true` when a second maintainer lands.

## 12. Relationship to other planning docs

- **`tech-debt-2026-04-18.md` §3.2 Bucket B** proposes a post-v0.1 `refactor/file-splits` branch. Per §3.2 naming, this is a `refactor/*` topic branch, single-PR review, ff to `dev` when green. Its single-blame-event goal requires ff, not squash.
- **`impl/merge-plan.md`** (M0–M7) was a one-time event. It used its own branch naming (`merge/m*`) that predates this doc. Consider the naming grandfathered.
- **`impl/archive/ci-cleanup-2.7-plan.md`** is the reference example of a `ci/*` topic branch executed under this strategy (branched from `f6efc32`, ff'd to `dev` after Phases A+B+C+D+E landed across 9 commits).
- **`SEED.md` "Merge strategy" section** is a short-form recap for session resumption. When this doc changes, update the SEED summary too.

## 13. Tooling to add pre-launch (not blocking)

| Item | Effort | Payoff | Status |
|---|---|---|---|
| ~~GitHub branch protection on `main` + `dev` (block force-push, require linear history)~~ | ~~10 min in repo settings~~ | ~~eliminates §11.5 risk class~~ | **landed 2026-04-20** — via `gh api` (see §11.5) |
| ~~`.gitmessage` template with the co-author trailer + spec-scope placeholder~~ | ~~15 min~~ | ~~cuts message-format errors~~ | **landed 2026-04-19** — `3364098` |
| ~~Pre-push hook that runs `cargo fmt --check` + `cargo clippy` (opt-in)~~ | ~~30 min~~ | ~~catches §7.3 invariant breaks earlier~~ | **landed 2026-04-19** — `7e83da5` (opt-in via `scripts/install-hooks.sh`) |
| ~~`git config --global rerere.enabled true` in the onboarding doc~~ | ~~2 min~~ | ~~cuts rebase-conflict re-resolution~~ | **landed 2026-04-19** — `3364098` (README Development Setup) |

Each is a 10–30 min standalone item. None are pre-merge gates. The four items above closed via the `ci/pre-launch-closeout` topic per `impl/archive/closeout-plan.md` P1 + P2.

## 14. Changelog for this doc

| Date | Change |
|---|---|
| 2026-04-18 | Initial draft. Codifies the pattern observed from M0 through §11.4. |
| 2026-04-19 | §13 — three of four tooling items landed on `ci/pre-launch-closeout` topic (`3364098` + `7e83da5`). Branch protection (§11.5) still pending per `closeout-plan.md` §3.2 P2. |
| 2026-04-20 | §13 — branch protection (§11.5) configured on `main` + `dev` via `gh api`; closeout-plan P2 closed. `enforce_admins=false` — admin bypass retained pending second maintainer. |

---

*Maintained by Akil Abderrahim + Claude Opus 4.7. Update when the pattern changes (e.g., first collaborator arrives → §10 tightens; first hotfix ships → §11.3 gains concrete history).*
