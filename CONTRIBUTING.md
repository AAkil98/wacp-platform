# Contributing

wacp-platform is v0.1 and actively accepting contributors, maintainers, and co-maintainers. The three tiers + what each looks like are in [`README.md` §Looking for help](README.md#looking-for-help). This file covers the workflow: picking work, shipping a PR, and what reviewers look for.

---

## First PR

### 1. Pick something to work on

- **[`good first issue`](https://github.com/AAkil98/wacp-platform/labels/good%20first%20issue)** — scoped, reviewable within a day, no design discussion required.
- **[`help wanted`](https://github.com/AAkil98/wacp-platform/labels/help%20wanted)** — larger scope; open a comment on the issue with your approach before coding.
- **Nothing fits?** Open an issue first. Describe the problem and your proposed fix. We'll confirm scope, then you code.

Don't ship a large PR that nobody agreed to review.

### 2. Fork + branch

Branch naming per [`impl/git-strategy.md`](impl/git-strategy.md) §4: `{scope}/{slug}`, where scope is one of `feat`, `fix`, `docs`, `test`, `refactor`, `ci`, `chore`. Examples:

```
fix/launcher-rollback-on-decompose-error
feat/gate-bulk-approve
docs/api-pagination-example
```

Branch off `dev`, not `main`. PRs land on `dev` first; `dev` → `main` is a batched fast-forward on maintainer cadence.

### 3. Build + test locally

Full-workspace gates:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Frontend (if touching wacp-console/frontend/)
cd wacp-console/frontend
pnpm install --frozen-lockfile
pnpm lint && pnpm typecheck && pnpm test
```

Want CI to catch drift before you push? Opt into the pre-push hook:

```bash
./scripts/install-hooks.sh
```

(Adds ~2–5 min per push. Uninstall: `git config --unset core.hooksPath`.)

### 4. Commit conventions

Format (enforced by `.gitmessage` template):

```
<type>(<scope>): <subject>

<body — wrap at 72, explain the why>

Co-Authored-By: Someone <email>
```

- **Type**: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`.
- **Scope**: crate / module / subsystem. Examples: `console-core`, `wacp-transport`, `frontend`, `ci`, `seed`.
- **Subject**: imperative mood, lowercase, no trailing period, ≤70 chars.
- **Body** *(optional for trivial changes)*: explain the motivation, not the diff. The diff is in the commit; the body is for why.

Set the template with `git config commit.template .gitmessage`.

### 5. Open the PR

Title mirrors the commit-header format. Body explains what changed, why, and what reviewers should scrutinize. For larger PRs, include a checklist of manual verification steps.

Required before merge:
- All CI workflows green (`ci-lint`, `ci-wacp`, `ci-console`, `coverage`).
- At least one maintainer approval.
- No merge commits — PRs are squashed or fast-forwarded; your branch history is preserved on `dev` as the squash base, not as a merge tree.

---

## Code conventions

### Rust

- `cargo fmt` — non-negotiable; CI fails on drift.
- `cargo clippy --workspace -- -D warnings` — same.
- Tests alongside the code they cover: `#[cfg(test)] mod tests` inline, or `#[path = "foo_tests.rs"] mod tests;` for files > ~500 lines.
- New public APIs get doc comments (`///`). Non-obvious internal behavior gets inline explanation.
- No unused `#[allow]` attributes to silence lints. Fix the underlying issue or justify in a comment.

### TypeScript / React

- Strict mode on. `pnpm typecheck` fails on `any` leakage, implicit returns, and missing prop types.
- `pnpm lint` enforces eslint + react-hooks + jsx-a11y rules.
- Components co-located with their tests: `FooBar.tsx` + `FooBar.test.tsx`.
- No imports of `wacp-console/frontend/src/**` from outside that tree.

### Tests

- Bug fixes need a regression test. If you can't write one, explain why in the PR.
- New features need tests covering the happy path + at least one failure path.
- Integration tests (`wacp-console/integration/tests/*.rs`) spin up a real runtime child — use them when the change touches a cross-binary boundary.

### Docs

- Protocol / design changes update the relevant spec under `wacp-console/specs/` or sibling [`AAkil98/wacp-protocol`](https://github.com/AAkil98/wacp-protocol) before shipping code.
- Public-API changes update `wacp-console/openapi.yaml` (CI has a drift check).
- Architecture drift gets logged in [`HEALTH-LOG.md`](HEALTH-LOG.md) under an appropriate `## N.M` subsection.

---

## Larger changes

### Design-scale work

Open an issue prefixed `design:` before writing code. Include:

- The problem, with a concrete failure mode or unmet need.
- What you're proposing, at architecture-level detail (not pseudocode).
- What's out of scope.
- Which specs or ADRs would need to change.

Maintainers will either agree, push back, or suggest an alternate framing — *before* you've invested in implementation.

### Protocol-level changes

The protocol spec lives in the sibling [`AAkil98/wacp-protocol`](https://github.com/AAkil98/wacp-protocol) repo under CC BY-SA 4.0. Implementation-side changes that depend on protocol evolution should be paired with a PR (or at minimum, an issue) in that repo first.

---

## What reviewers look for

In roughly this order:

1. **Does the change do what the PR description says?** If the description says "fix X" and the diff also refactors Y, reviewers will ask for Y to be split into a separate PR.
2. **Is the test coverage proportional to the change?** A one-line logic change might need one new assertion; a new RPC path needs a full integration test.
3. **Does the code match the surrounding style?** Not a taste question — the existing style was chosen for a reason; ask before diverging.
4. **Are the commit messages useful six months from now?** The "why" matters more than the "what".
5. **Does CI pass cleanly?** Flaky CI is treated as a bug — if your PR exposes flake, surfacing it in an issue is valuable even if the PR itself needs to land some other way.

Review turnaround is usually 1–3 days for a scoped PR; slower for design-scale work. If a PR is sitting for more than a week without comment, nudge on the PR thread — it's fine.

---

## Questions

- **Technical / project questions** → open an issue. Don't worry about it being a "real" issue; if it's a question, a maintainer will answer or convert it into a discussion.
- **Maintainer-interest conversations** → open an issue titled `maintainer interest: {your context}` or email `aakilabderr22@gmail.com` with a link to your recent work.
- **Protocol-spec questions** → file in [`AAkil98/wacp-protocol/issues`](https://github.com/AAkil98/wacp-protocol/issues).

---

## References

- [`README.md`](README.md) — project overview + quick start
- [`impl/git-strategy.md`](impl/git-strategy.md) — branching, commits, merges in depth
- [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) — community standards
- [`SEED.md`](SEED.md) — umbrella session-context doc (useful for orienting on active work)
