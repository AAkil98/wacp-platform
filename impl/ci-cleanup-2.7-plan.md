---
id: wacp-ci-cleanup-2.7-plan
type: impl
status: draft
created: 2026-04-17T23:00:00
authors: [AAkil98, Claude Opus 4.7 (1M context)]
tags: [ci, cleanup, drift, 13.7.7, no-tech-debt]
depends_on: [wacp-ci-health-2026-04-17]
---

# §2.7 CI-Drift Full-Fix Plan

> Sequel to `impl/ci-health-2026-04-17.md` §2.7. §2.1–§2.6 landed on `dev` via `efd23e9..0845acd`; the cleanup restored the Rust + fmt + deny + Frontend-lint + Integration + Coverage-Rust surface. Nine pre-existing drift items (§2.7.1–§2.7.9) still keep four workflows red. This plan closes them all — no `#[allow]` bandaids, no `tsconfig` exclusions as deferrals, no config relaxations. The one unavoidable `#[allow(clippy::collapsible_match)]` from §2.6 stays because the alternative requires an allocation; every other shortcut gets repaid.
>
> The plan also absorbs two debt items carried forward from §2.1–§2.6:
> - **D.1** `deny.toml` `wildcards = "warn"` (relaxation in §2.5) → restored to `"deny"` after adding `version = "0.1.0"` alongside each internal path dep in the root Cargo.toml. No `publish = false` sweep needed; cargo-deny accepts `{ path = "...", version = "..." }` as non-wildcard.
> - **D.2** `eslint-plugin-react-hooks` never installed (the orphan `// eslint-disable-line react-hooks/exhaustive-deps` in `SettingsPage.tsx` was removed, not replaced) → install the plugin, configure the rule, re-apply the disable comment on the one intentional case.

## Table of Contents

- 1. Goal
- 2. Principles
- 3. Per-item plan
  - 3.1 §2.7.2 / §2.7.3 Regenerate `openapi.yaml` (console + runtime)
  - 3.2 §2.7.9 Exclude `e2e/**` from Vitest
  - 3.3 §2.7.4 `highway-ui` `pnpm-workspace.yaml` packages field
  - 3.4 §2.7.6 `@wacp/local` dist build → ecosystem typecheck cascade
  - 3.5 §2.7.5 `wacp-cli` `OperationType` / `Workflow` narrowings
  - 3.6 §2.7.7 / §2.7.8 Python `wacp.proto` codegen via `buf generate`
  - 3.7 §2.7.1 Frontend Typecheck — 54 strict-mode errors in tests
  - 3.8 D.1 `deny.toml` — restore `wildcards = "deny"` via path+version pattern
  - 3.9 D.2 `eslint-plugin-react-hooks`
- 4. Phasing + commit strategy
- 5. Verification matrix
- 6. Non-goals / explicitly deferred
- 7. References

---

## 1. Goal

Every `pull_request: branches: [main]` and `push: branches: [main, dev]` CI run across `ci-lint`, `ci-wacp`, `ci-console`, and `coverage` reaches **green** on `dev`. No steps left with `continue-on-error`, no rules relaxed to warning, no test files excluded from typecheck. Cascading into §13.7.7 D3 (Playwright CI stage) that can land on a fully-green base.

## 2. Principles

1. **No bypasses.** A fix that works by hiding the failure (excluding files from typecheck, downgrading a deny-check to warn, `@ts-expect-error` on prod code) is not a fix. If an existing bandaid can't be removed without breaking CI, it becomes its own work item in this plan.
2. **Verify locally where possible.** Each item has a local verification command. Items that can't be verified locally (e.g., pnpm-workspace interactions, cargo-deny behavior) are explicitly marked as "CI-only verification".
3. **Commit per item** (or per tight cluster). Reviewers should be able to bisect to the specific fix. The two openapi regens can share a commit; everything else is its own commit.
4. **Commit message style** follows the recent log: `<scope>(<area>): §2.7.<n> — <summary>`. Body explains *why* the fix is shaped this way.
5. **Update `ci-health-2026-04-17.md`** as items close: mark `Status:` per subsection, flip the frontmatter to `status: resolved` once every item in §2.7 + D.1 + D.2 lands.

## 3. Per-item plan

### 3.1 §2.7.2 / §2.7.3 Regenerate `openapi.yaml` (console + runtime)

**Failure shape.** Both `ci-wacp` and `ci-console` run an "OpenAPI drift check" step that pipes `cargo run -p <crate> --bin <gen-bin>` to a temp file and `diff`s against the checked-in `openapi.yaml`. Both currently differ — the generator output has evolved but the checked-in YAML hasn't.

**Fix.**

```bash
cargo run -p wacp-transport --bin gen_openapi > wacp/openapi.yaml
cargo run -p console-api --bin gen-openapi > wacp-console/openapi.yaml
```

Then `git add` both, inspect the diff to make sure it looks like schema evolution (not a regression), and commit.

**Local verification.**

```bash
cargo run -p wacp-transport --bin gen_openapi > wacp/openapi.yaml.gen
diff wacp/openapi.yaml wacp/openapi.yaml.gen && rm wacp/openapi.yaml.gen
cargo run -p console-api --bin gen-openapi > wacp-console/openapi.yaml.gen
diff wacp-console/openapi.yaml wacp-console/openapi.yaml.gen && rm wacp-console/openapi.yaml.gen
```

Both diffs must return empty.

**Commit.** `chore(openapi): §2.7.2+§2.7.3 — regenerate wacp + console openapi.yaml`.

**Effort.** ~10 min (compile time dominates).

**Regression risk.** Low. OpenAPI generation is deterministic. The diff is read-only artefact alignment.

---

### 3.2 §2.7.9 Exclude `e2e/**` from Vitest

**Failure shape.** `coverage > frontend` runs `pnpm test:ci` → `vitest run --coverage`. Since §13.7.7 D2 (`385ba71`) landed, `wacp-console/frontend/e2e/*.spec.ts` exists. Vitest's default include pattern matches these, and Playwright's `test.skip()` is called at top-level (not inside `test()`/`describe()`), which Vitest rejects:

```
Error: test.skip() can only be called inside test, describe block or fixture
```

**Fix.** Extend `wacp-console/frontend/vitest.config.ts` with:

```ts
test: {
  // ... existing ...
  exclude: [
    ...configDefaults.exclude,
    "e2e/**",
    "playwright-report/**",
    "test-results/**",
  ],
}
```

(Pull `configDefaults` from `vitest/config`.) The `e2e/**` glob is the primary target; the other two are defensive in case vitest-coverage ever scans generated artefact dirs.

**Local verification.**

```bash
cd wacp-console/frontend
pnpm test:ci 2>&1 | tail -20
```

Must complete with `Test Files NNN passed (NNN)` and zero spec files from `e2e/` listed.

**Commit.** `test(frontend): §2.7.9 — exclude playwright e2e specs from vitest`.

**Effort.** ~15 min.

**Regression risk.** Low. Only changes which files Vitest picks up.

---

### 3.3 §2.7.4 `highway-ui` `pnpm-workspace.yaml` packages field

**Failure shape.** `ci-wacp > TypeScript — highway-ui > Install` errors with:

```
ERROR  packages field missing or empty
##[error]Process completed with exit code 1.
```

**Root cause.** `wacp/highway-ui/pnpm-workspace.yaml` contains only:

```yaml
onlyBuiltDependencies: ["@bufbuild/buf", "esbuild"]
```

pnpm v10 requires a `packages:` field in any file named `pnpm-workspace.yaml`. The file appears to be a misplaced/vestigial config; highway-ui is a standalone package, not a workspace root (it has its own `package.json` with its own deps + lock).

**Fix.** Move the `onlyBuiltDependencies` entry out of `pnpm-workspace.yaml` and into `wacp/highway-ui/package.json` under the `pnpm` key (pnpm's standard location for this option):

```json
"pnpm": {
  "onlyBuiltDependencies": ["@bufbuild/buf", "esbuild"]
}
```

Then `git rm wacp/highway-ui/pnpm-workspace.yaml`. No functional change — both locations are pnpm-native.

**Local verification.**

```bash
cd wacp/highway-ui
rm -rf node_modules
pnpm install --frozen-lockfile 2>&1 | tail -5
```

Must exit 0. The "packages field missing" error is gone.

**Commit.** `fix(highway-ui): §2.7.4 — move onlyBuiltDependencies into package.json; remove orphan pnpm-workspace.yaml`.

**Effort.** ~10 min.

**Regression risk.** Low. `onlyBuiltDependencies` is honored identically in both locations.

---

### 3.4 §2.7.6 `@wacp/local` dist build → ecosystem typecheck cascade

**Failure shape.** `ci-wacp > TypeScript — wacp/ecosystem/{swe,devops,mlops,finance,healthcare,analytics,datasci}` (7 jobs) + `wacp/packages/wacp-cli` fail at Typecheck with:

```
node_modules/.pnpm/@wacp+local@file+..+..+packages+wacp-local/node_modules/@wacp/local/src/resources.ts(1,21):
  error TS2307: Cannot find module 'node:fs/promises'
```

**Root cause.** `wacp/packages/wacp-local/package.json` has `"main": "src/index.ts"` and `"types": "src/index.ts"` — consumers import raw TS. `src/resources.ts` imports `node:fs/promises`, `node:path`, `node:child_process`, `node:util`. Consumers have `"moduleResolution": "bundler"` + `"strict": true` + no `@types/node`, so `node:*` is unresolved. This is shipping-source-without-types and it bleeds into every consumer.

**Root fix (chosen).** Build `@wacp/local` to `dist/` and point the entry points at the compiled output. No consumer-level workarounds.

Steps:

1. **`wacp/packages/wacp-local/package.json`** — change `main` + `types` + add `exports`:
   ```json
   "main": "dist/index.js",
   "types": "dist/index.d.ts",
   "exports": {
     ".": {
       "types": "./dist/index.d.ts",
       "import": "./dist/index.js"
     }
   },
   "files": ["dist"],
   "devDependencies": {
     "typescript": "^5.8.0",
     "vitest": "^3.1.0",
     "@types/node": "^22"
   }
   ```
2. **`wacp/packages/wacp-local/tsconfig.json`** — already emits `dist/` (`outDir: "dist"`, `declaration: true`). Verify `"noEmit"` is not set.
3. **`wacp/packages/wacp-local/src/`** — verify `node:*` imports are all listed in the existing types dep scope. `@types/node` covers all four.
4. **`ci-wacp.yml > typescript-packages`** — the job needs to build `@wacp/local` before any ecosystem package typechecks. Either:
   - (A) Add `needs: wacp-local-build` where `wacp-local-build` is a new job that runs `pnpm install && pnpm build` in `wacp/packages/wacp-local/` and uploads `dist/` as an artifact; downstream jobs download it before typecheck.
   - (B) Make every matrix entry run `pnpm -C ../../packages/wacp-local build` as a `before: typecheck` step.
   - **Preferred: (B)**. Simpler, no artifact shuffling; extra compile is ~3s per matrix cell, acceptable.
5. **Local verification (per-ecosystem)**:
   ```bash
   cd wacp/packages/wacp-local && pnpm install && pnpm build
   cd wacp/ecosystem/swe && pnpm install --frozen-lockfile && pnpm typecheck
   # ...repeat for each ecosystem. Must all exit 0.
   ```

**Commit.** `build(wacp-local): §2.7.6 — compile to dist/; ecosystem + wacp-cli pick up emitted types`. Includes the ci-wacp.yml pre-build step.

**Effort.** 60–90 min. Includes touching 9 downstream package-lock files if pnpm decides to update them (it shouldn't, since only `@wacp/local`'s published entry points changed, not its deps from the consumer's view).

**Regression risk.** Moderate. Every ecosystem package's import path stays the same (`import { LocalResources } from '@wacp/local'`). TS types move from source to `.d.ts`. Runtime behavior unchanged because the JS emit is a straight transpile. The one failure mode to watch: if any ecosystem package has `"moduleResolution": "bundler"` AND the exports map's condition ordering matters, test imports early.

---

### 3.5 §2.7.5 `wacp-cli` `OperationType` / `Workflow` narrowings

**Failure shape.** `ci-wacp > TypeScript — wacp/packages/wacp-cli` at Typecheck:

```
src/agent.ts(187,41): error TS2345: Argument of type 'string' is not assignable to parameter of type 'OperationType'.
src/agent.ts(192,32): error TS2345: Argument of type 'string' is not assignable to parameter of type 'OperationType'.
src/agent.ts(194,33): error TS2345: ...
src/agent.ts(199,38): error TS2345: ...
src/repl.ts(108,17): error TS2345: Argument of type 'VerticalWorkflow' is not assignable to parameter of type 'Workflow'.
```

**Root cause (hypothesized; confirm during execution).** `toolToOperation(call.name, ecosystem)` returns `string | undefined` but `session.autonomy.check(opType)` expects `OperationType`. Either `toolToOperation` should return `OperationType | undefined` (its full SDK-typed return), or each call site should narrow via a type predicate.

**Root fix.** Fix `toolToOperation`'s return type. Grep the function signature in `@wacp/local` or `@wacp/cli`; change its return type to `OperationType | undefined`. The runtime value is already an `OperationType` string literal by construction.

For the `VerticalWorkflow` vs `Workflow` issue at `repl.ts:108` — likely a similar narrowing. `VerticalWorkflow` is probably a subset of `Workflow`. If structurally assignable, cast; if not, fix the source type.

**Local verification.**

```bash
cd wacp/packages/wacp-cli
pnpm typecheck 2>&1 | grep "error TS" | wc -l   # expect 0
```

**Commit.** `fix(wacp-cli): §2.7.5 — OperationType + Workflow narrowings in agent.ts / repl.ts`.

**Effort.** 30–45 min (mostly reading adjacent code to pick the right fix shape).

**Regression risk.** Low. Type-only changes.

---

### 3.6 §2.7.7 / §2.7.8 Python `wacp.proto` codegen via `buf generate`

**Failure shape.** `ci-wacp > Python 3.{11,12,13}` and `coverage > sdk-python` fail at test collection with:

```
ModuleNotFoundError: No module named 'wacp.proto'
```

`wacp/sdk-python/src/wacp/agent.py:14–31` imports 13 symbols from `wacp.proto.v1`; 4 test files do the same. The `wacp.proto` subpackage does not exist on disk. `pyproject.toml` lists `betterproto>=1.2.5` and `grpclib>=0.4.9` but no codegen is wired.

**Root fix.** Wire `buf generate` with the Python betterproto plugin and commit the generated stubs under `src/wacp/proto/v1/`. Consumers never need to run codegen; `pip install -e .` Just Works.

Steps:

1. **Add `wacp/sdk-python/buf.gen.yaml`**:
   ```yaml
   version: v2
   plugins:
     - remote: buf.build/community/danielgtaylor-betterproto:latest
       out: src
       opt: ["python_betterproto_opt=client_generation=async_no_pydantic"]
   ```
   (Or pin an explicit betterproto version; remote plugins work but pinned is more reproducible.)
2. **Add a `Makefile` in `wacp/sdk-python/`** with `generate` and `clean` targets:
   ```make
   .PHONY: generate
   generate:
   	cd .. && buf generate --template sdk-python/buf.gen.yaml
   ```
3. **Run `buf generate`** and commit `src/wacp/proto/v1/` + `src/wacp/proto/__init__.py`.
4. **Add `src/wacp/proto/v1/` to `.gitignore`** — **No.** We commit the generated code. Rationale: keeps `pip install` free of protoc/buf requirements; matches the TS approach in `highway-ui/src/gen/`; treats the stubs as part of the published surface.
5. **`pyproject.toml`** — add `wacp.proto` and `wacp.proto.v1` to the packages list (either via `[tool.setuptools.packages.find]` with `include = ["wacp*"]` which should already catch them, or explicit under `packages`).
6. **Add a generator-drift CI step** (optional but recommended): in `ci-wacp.yml > python`, add `buf generate --template wacp/sdk-python/buf.gen.yaml && git diff --exit-code wacp/sdk-python/src/wacp/proto/` so stale `.proto` vs generated is caught.

**Local verification.**

```bash
cd wacp/sdk-python
pip install -e ".[dev]"
python -c "from wacp.proto.v1 import AgentServiceStub; print('OK')"
python -m pytest tests/ -v 2>&1 | tail -10
```

Tests should collect without `ModuleNotFoundError`; any remaining failures are actual behavioral issues to address separately.

**Commit.** `feat(sdk-python): §2.7.7 — wire buf generate for python proto stubs`. Generated stubs land in the same commit so history is self-contained.

**Effort.** 60–90 min including first-run verification + any downstream test fixups.

**Regression risk.** Moderate. Generated code is large (probably ~1000–2000 lines); it becomes part of the repo. Review must scan for any source-wrapping issues. Buf plugin version pinning important to avoid silent regen diffs.

---

### 3.7 §2.7.1 Frontend Typecheck — 54 strict-mode errors in tests

**Failure shape.** `ci-console > Frontend > Typecheck` runs `pnpm typecheck` (`tsc -b --noEmit` against the default `tsconfig.json`). 54 errors remain, all in `src/**/*.test.ts(x)` + `src/**/test-helpers.tsx`:

- ~30× `TS2345: HTMLElement | undefined` → passed into `within()` / `fireEvent` which expects `Element`.
- ~12× `TS2532: Object is possibly 'undefined'` on array-indexing / hook-destructure.
- ~6× `TS2322: GateEvent.subject` shape mismatch (`string` vs `Record<string, unknown>`).
- 1× `TS2488: Type 'any[] | undefined' must have '[Symbol.iterator]()'` (for-of on possibly-undefined array).
- 4× `TS2345` on `UseMutationResult` generic arguments in helper `runMutationErrorBattery`.
- 1× `TS2578: Unused '@ts-expect-error' directive`.
- 1× `TS2322: Location vs string & Location`.
- 2× `TS6133: declared but never read` (after my `inputs` cleanup in §2.2, 1 remains: `data` in `ProfilesPage.test-helpers.tsx:54`).

**Root fix (chosen).** Fix each of the 54 errors at its source. Do NOT reintroduce the bypass of using `tsconfig.build.json` to exclude test files from typecheck. `tsconfig.build.json` stays (it keeps `pnpm build` fast + scoped), but the default `tsconfig.json` + `pnpm typecheck` must go green with tests included.

Mechanical recipe:

| Pattern | Fix |
|---|---|
| `rows[0]` → `HTMLElement \| undefined` passed to `within()` | `rows[0]!` (non-null assertion — we just asserted `rows.length > 0` on the preceding line). If no preceding assertion, add one. |
| `const ref = map.get("x")` → `ref` possibly undefined | `const ref = map.get("x")!` after validating the key was just inserted; otherwise narrow with `if (!ref) throw` or `expect(ref).toBeDefined()` + `!`. |
| `GateEvent.subject` shape mismatch | Update the test fixture to produce the actual `Record<string, unknown>` shape (`{ subject: { kind: "...", detail: "..." } }`) rather than `subject: "..."`. Source-of-truth is `src/realtime/events.ts` `GateEvent` type. |
| `runMutationErrorBattery` UseMutationResult variance | The helper is over-specified. Widen its generic to `UseMutationResult<unknown, Error, unknown, unknown>` and cast at call sites, OR accept a factory for each test that builds a typed client. Preferred: widen. |
| `TS2578 unused @ts-expect-error` | The error the directive was suppressing no longer fires (probably because strictness changed). Delete the directive. |
| `TS6133 unused` | Delete the variable. |
| `TS2322 Location vs string & Location` | `window.location` assignment in a test. Use `vi.stubGlobal('location', { ... } as Location)` or equivalent. |

Approach: fix file-by-file, running `pnpm typecheck 2>&1 \| grep "<file>"` between edits to watch the count drop.

**Local verification.**

```bash
pnpm typecheck 2>&1 | grep "error TS" | wc -l   # expect 0
pnpm test:isolated 2>&1 | tail -5                # expect all passing (no behavior change)
```

**Commit.** Split into logical groups if diff grows: one commit per rule cluster (`test(frontend): §2.7.1 — narrow UseMutationResult helper`, `test(frontend): §2.7.1 — non-null assertions on getAllByRole[n]`, etc.). Or one large commit if cohesive. Err toward one commit: easier to revert, easier to audit.

**Effort.** 90–120 min mechanical.

**Regression risk.** Low. Type-only changes, no runtime behavior change. Re-run `pnpm test:isolated` after each cluster to catch accidental logic changes.

---

### 3.8 D.1 `deny.toml` — restore `wildcards = "deny"` via path+version pattern

**Current state.** `deny.toml` has `wildcards = "warn"` after the §2.5 relaxation, because `allow-wildcard-paths = true` doesn't apply to "public" crates (those without `publish = false`). A workspace-wide `publish = false` sweep is one option but conflicts with any crate that *should* eventually publish to crates.io (wacp-sdk, wacp-types, wacp-coordinator-sdk likely candidates).

**Root fix (chosen).** Add an explicit `version` alongside each path in the root `[workspace.dependencies]`. cargo-deny treats `{ path = "...", version = "..." }` as non-wildcard. This is the accepted idiomatic form when a crate is published AND used as a workspace member.

Steps:

1. **Root `Cargo.toml` `[workspace.dependencies]`** — change each of the 20 internal deps from:
   ```toml
   wacp-clock = { path = "wacp/crates/wacp-clock" }
   ```
   to:
   ```toml
   wacp-clock = { path = "wacp/crates/wacp-clock", version = "0.1.0" }
   ```
   Do this for all 20 (15 wacp + 5 console). Each crate's `Cargo.toml` already pins `version = "0.1.0"` at `[package]`, so the workspace dep line just needs to match.

2. **`deny.toml`** — revert `wildcards = "warn"` to `wildcards = "deny"`. Keep `allow-wildcard-paths = true` — it's now redundant but harmless and future-proofs us against adding path-only deps for test fixtures etc.

3. **Update `impl/ci-health-2026-04-17.md` §2.5** — append the D.1 closure note.

**Local verification.** Need `cargo install cargo-deny` (one-time, ~5 min). Then:

```bash
cargo deny check advisories bans licenses sources --all-features --workspace
```

Must exit 0. CI verification on push is the backup.

**Commit.** `ci(deny): D.1 — restore wildcards=deny via path+version pattern`.

**Effort.** 20 min.

**Regression risk.** Low. `cargo` treats `{ path, version }` the same as `{ path }` at resolve time — the version gates are only checked when publishing. No behavioral change for the workspace.

---

### 3.9 D.2 `eslint-plugin-react-hooks`

**Current state.** The orphan `// eslint-disable-line react-hooks/exhaustive-deps` comment at `SettingsPage.tsx:137` was removed in §2.2 because the plugin isn't configured, but the *intent* of the disable is valid: that `useEffect` deliberately uses `settingsQuery.dataUpdatedAt` as a change-sentinel rather than listing `settings` in deps. Without the rule enforced, future drift won't catch someone omitting deps elsewhere.

**Root fix (chosen).** Install `eslint-plugin-react-hooks`, configure `react-hooks/rules-of-hooks` (error) and `react-hooks/exhaustive-deps` (warn — too noisy as error for an existing codebase; ratchet to error later). Re-add the disable comment at the one deliberately-overridden site.

Steps:

1. **`wacp-console/frontend/package.json`** — add `"eslint-plugin-react-hooks": "^5.2.0"` to devDependencies, run `pnpm install`.

2. **`wacp-console/frontend/eslint.config.js`** — wire the plugin:
   ```js
   import reactHooks from "eslint-plugin-react-hooks";
   // ...
   extends: [
     js.configs.recommended,
     ...tseslint.configs.recommended,
   ],
   plugins: { "react-hooks": reactHooks },
   rules: {
     "react-hooks/rules-of-hooks": "error",
     "react-hooks/exhaustive-deps": "warn",
     "@typescript-eslint/no-unused-vars": [...]
   }
   ```

3. **Re-add at `SettingsPage.tsx:137`**: `// eslint-disable-next-line react-hooks/exhaustive-deps` with a terse comment explaining the deliberate override.

4. **Run `pnpm lint`** and address anything new the plugin catches. Since we set `exhaustive-deps: "warn"`, we won't fail CI, but surfacing warnings in the log is the intent. (Future ratchet to `"error"` is a separate work item; not in this plan.)

**Local verification.**

```bash
pnpm lint 2>&1 | grep -c "error"   # must stay 0
pnpm lint 2>&1 | grep "warn" | wc -l   # expected >0 but non-blocking
```

**Commit.** `chore(frontend): D.2 — reinstate eslint-plugin-react-hooks`.

**Effort.** 30 min.

**Regression risk.** Low. Warns don't block CI. If a `rules-of-hooks` error fires, it's a real bug and should be fixed in the same commit or a follow-up.

---

## 4. Phasing + commit strategy

Ordered by cost × unblock-impact (cheap early so CI signal recovers fast):

| Phase | Items | Commits | Est effort |
|---|---|---|---|
| **A** (cheap wins) | §2.7.2, §2.7.3, §2.7.9, §2.7.4 | 3 (one combined OpenAPI, one vitest, one highway-ui) | ~35 min |
| **B** (types) | §2.7.5, §2.7.6 | 2 (wacp-cli types, wacp-local build + ecosystem wire-up) | ~90–135 min |
| **C** (Python) | §2.7.7 + §2.7.8 | 1 (buf.gen + generated stubs + pyproject fix) | ~60–90 min |
| **D** (mechanical strict-mode) | §2.7.1 | 1 (possibly split; see 3.7 notes) | ~90–120 min |
| **E** (debt) | D.1, D.2 | 2 | ~50 min |

**Total wall: 5.5–7.5 h.** Single-sitting feasible; natural pause points after A and after C.

Every commit lands on the same branch — `ci/cleanup-2.7` (new branch, cut from current `dev` tip). A fresh draft PR to `main` for trigger-based CI verification (same pattern as §2.1–§2.6). Merge to `dev` via fast-forward once all 9 items are green on the PR.

## 5. Verification matrix

| Workflow / job | Pass criterion | Items that unblock |
|---|---|---|
| `ci-lint/fmt` | green (already) | — |
| `ci-lint/deny` | green with `wildcards = "deny"` | D.1 |
| `ci-wacp/rust/Build+Clippy+Test` | green (already) | — |
| `ci-wacp/rust/OpenAPI drift` | green | §2.7.3 |
| `ci-wacp/typescript-highway` | `pnpm install` + typecheck + test + build all green | §2.7.4 |
| `ci-wacp/typescript-packages × {wacp-cli, wacp-local}` | typecheck green | §2.7.5, §2.7.6 |
| `ci-wacp/typescript-packages × ecosystem × 7` | typecheck green | §2.7.6 |
| `ci-wacp/python × 3` | test collection + all tests pass | §2.7.7 |
| `ci-wacp/proto` | green (already) | — |
| `ci-console/integration` | green (already) | — |
| `ci-console/rust/Build+Clippy+Test` | green (already) | — |
| `ci-console/rust/OpenAPI drift` | green | §2.7.2 |
| `ci-console/frontend/Lint` | green (already) | D.2 must not introduce errors |
| `ci-console/frontend/Typecheck` | green | §2.7.1 |
| `ci-console/frontend/Test` | green | §2.7.1 (indirectly — typecheck runs first) |
| `ci-console/frontend/Build` | green (already) | — |
| `coverage/rust-runtime` | green (already) | — |
| `coverage/rust-console` | green (already) | — |
| `coverage/frontend` | green | §2.7.9 |
| `coverage/python` | green | §2.7.7 |

## 6. Non-goals / explicitly deferred

- **Node.js 20 deprecation warnings** on `actions/checkout@v4`, `actions/setup-python@v5`, `actions/upload-artifact@v4`, `arduino/setup-protoc@v3`, `actions/setup-node@v4`, `pnpm/action-setup@v4`. These are GitHub Actions lifecycle warnings, not failures. GitHub forces Node 24 by June 2, 2026; we'll pick up updated action versions by then. Not debt — external deprecation.
- **Ratcheting `react-hooks/exhaustive-deps` from `warn` to `error`** once the codebase is clean. Tracked informally in D.2 body.
- **`#[allow(clippy::collapsible_match)]` in `ws.rs`.** Kept. The alternative (match guard with `data.clone()`) introduces a `Bytes` allocation on every ping, which is a real runtime cost to satisfy a lint. Lint-limit edge case, not debt.
- **`wildcards = "allow"` for tests-only dev-deps.** Not needed — the path+version pattern (D.1) works for every workspace member uniformly.
- **Re-running `cargo-dist` / `cargo-deny` v0.18 upgrade.** External; out of scope.
- **Upgrading `tsc` / `typescript` version across packages.** §2.7.1 fixes the existing errors against the current TS version; upgrades are a separate work package.

## 7. References

| Doc | Relationship |
|---|---|
| `impl/ci-health-2026-04-17.md` | Source doc; §2.7 subsection frames the 9 drift items. |
| `AUDIT-2026-04-15.md` §13.7.7 | Playwright CI stage (D3) is unblocked by this plan. |
| `SEED.md` | Current-state narrative; will refresh to "§2.7 closed" at plan's acceptance. |
| `wacp-console/performance-optimization.md` §12 | Cross-referenced for the pre-cleanup drift findings. |
| Commits `efd23e9..0845acd` (on `dev`) | §2.1–§2.6 landing this plan builds on. |

*wacp-platform — §2.7 full-fix plan, drafted 2026-04-17 evening. Plan status: draft; execution pending.*
