# WACP Ecosystem: SWE Vertical

```yaml
id: wacp-eco-swe
type: ecosystem-spec
status: complete
created: 2026-04-02
revised: 2026-04-02
lineage: LAYER-MAPPING.md (E1)
depends_on:
  - wacp-impl-cli-agent
  - wacp-impl-tool-framework
  - wacp-impl-local-sdk
  - wacp-impl-coordinator-sdk
  - wacp-impl-runtime
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, ecosystem, swe, software-engineering, vertical, multi-agent, workflows]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Role Taxonomy](#2-role-taxonomy)
3. [Task Taxonomy](#3-task-taxonomy)
4. [Tool Catalog](#4-tool-catalog)
5. [Agent Profiles](#5-agent-profiles)
6. [Workflows](#6-workflows)
7. [Execution Model](#7-execution-model)
8. [Quality Criteria](#8-quality-criteria)
9. [Gate Policies](#9-gate-policies)
10. [Package Structure](#10-package-structure)
11. [Test Requirements](#11-test-requirements)
12. [References](#12-references)

---

## 1. Purpose

This spec defines the SWE (Software Engineering) ecosystem vertical — the first domain parameterization of WACP. It answers "how does the platform behave when the task is software engineering" — not "how does the platform work" (that's the runtime + middleware).

**What the vertical provides:** Roles (who does what), task types (what kinds of work exist), tools (what capabilities are available), profiles (how each role behaves), workflows (how work decomposes into protocol-level workspaces), quality criteria (how to evaluate output), gate policies (when to ask the human).

**Execution model:** The CLI agent loads the SWE vertical at boot. When a goal is submitted, the CLI detects the task type, selects the matching workflow, and executes it through the WACP runtime — each stage is a real workspace with its own role profile, tool whitelist, signals, and checkpoints. The vertical defines the decomposition; the protocol enforces it.

---

## 2. Role Taxonomy

Four derived roles, each with a specific concern:

| Role | Extends | Concern | Tool access | Autonomy |
|------|---------|---------|-------------|----------|
| `swe:planner` | worker | Analyze requirements, produce plans, assign scope | Read-only + search | Gated |
| `swe:implementer` | worker | Write/edit code, produce diffs | Read + write + search + exec | Gated |
| `swe:tester` | worker | Write tests, run tests, report results | Read + write + test execution | Gated |
| `swe:reviewer` | observer | Review changes, evaluate quality | Read-only + search | Autonomous |

**Protocol mapping:** Each role maps to a workspace role at dispatch time. The coordinator creates a workspace with the role's tool whitelist and the profile's system prompt as the directive. The agent binds to the workspace and operates within its permissions.

---

## 3. Task Taxonomy

Seven task types with default decomposition:

| Type | Description | Default workflow | Roles involved |
|------|-------------|------------------|----------------|
| `swe:implement` | Build new functionality | `swe:implement-feature` (4 stages) | All 4 |
| `swe:refactor` | Restructure existing code | `swe:refactor` (4 stages) | All 4 |
| `swe:debug` | Find and fix a bug | `swe:fix-bug` (3 stages) | 3 (no reviewer) |
| `swe:test` | Write or improve tests | `swe:write-tests` (2 stages) | 2 |
| `swe:review` | Review existing changes | Direct (1 stage) | 1 (reviewer) |
| `swe:document` | Write/update documentation | Direct (1 stage) | 1 (implementer) |
| `swe:investigate` | Research a question | Direct (1 stage) | 1 (planner) |

**Detection:** The CLI classifies user goals into task types via keyword heuristics: "fix bug" → `swe:debug`, "refactor" → `swe:refactor`, "add tests" → `swe:test`, default → `swe:implement`.

---

## 4. Tool Catalog

SWE-specific tools beyond the CLI's built-in 7. All 14 tools are registered at boot and filtered per stage by the profile's whitelist.

| Tool | Description | Operation type |
|------|-------------|----------------|
| `code_search` | Regex search across codebase (grep) | `file_read` |
| `code_edit` | Find-and-replace in a file | `file_write` |
| `test_run` | Run test suite or specific file | `shell_exec` |
| `lint_check` | Run project linter | `shell_exec` |
| `type_check` | Run type checker (tsc, mypy, cargo check) | `shell_exec` |
| `git_branch` | Create or switch git branch | `git_write` |
| `git_commit` | Stage files and commit | `git_write` |
| `dependency_check` | Check outdated/vulnerable deps | `shell_exec` |

Tool executors auto-detect the project's toolchain (npm/pytest/cargo for test_run, eslint/biome/ruff for lint_check, tsc/mypy for type_check).

---

## 5. Agent Profiles

One profile per role. Each profile provides: system prompt, tool whitelist, autonomy level.

**`swe:planner`** — Read-only analysis. System prompt instructs: analyze codebase, produce numbered plan with files/steps/risks/test strategy. Tools: `read_file`, `list_dir`, `search_files`, `code_search`, `git_status`, `git_diff`.

**`swe:implementer`** — Full read-write. System prompt instructs: minimal focused changes, follow existing style, use `code_edit` for small changes, run code after changes, commit working changes. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `code_search`, `code_edit`, `run_command`, `git_status`, `git_diff`, `git_branch`, `git_commit`, `test_run`, `type_check`.

**`swe:tester`** — Read-write + test execution. System prompt instructs: test through public interfaces, cover edge cases, run existing tests for regressions, do not modify implementation code. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `code_search`, `code_edit`, `test_run`, `run_command`, `git_status`, `git_diff`.

**`swe:reviewer`** — Read-only, autonomous. System prompt instructs: produce review with summary, categorized issues (critical/important/suggestion), verdict (APPROVE or REQUEST_CHANGES). Tools: `read_file`, `list_dir`, `search_files`, `code_search`, `git_diff`, `test_run`.

---

## 6. Workflows

Four workflow DAGs. Each defines stages with role assignments, dependencies, and gate policies.

**`swe:implement-feature`** (4 stages):
```
plan (planner) ──→ implement (implementer) ──→ test (tester) ──→ review (reviewer)
                        [gated]                                      [gated]
```

**`swe:refactor`** (4 stages): Same structure. Tests verify behavior preservation.

**`swe:fix-bug`** (3 stages):
```
plan (planner) ──→ implement (implementer) ──→ test (tester)
                        [gated]
```
No review — urgency. Tests verify the fix.

**`swe:write-tests`** (2 stages):
```
plan (planner) ──→ test (tester)
```
Planner identifies gaps. Tester fills them.

**DAG validation:** `validateWorkflow()` checks that all dependencies exist and no cycles are present.

---

## 7. Execution Model

The SWE vertical executes through the WACP protocol — not as a simulation.

**Goal submission:**
```
1. User submits goal: "implement authentication"
2. CLI detects task type: swe:implement
3. CLI selects workflow: swe:implement-feature (4 stages)
4. CoordinatorService.SubmitGoal → runtime creates root workspace
5. CoordinatorService.Decompose → runtime creates task graph (4 tasks)
```

**Per-stage execution:**
```
1. CoordinatorService.Dispatch(task, role, tools) → runtime creates child workspace
2. AgentService.Bind(workspace) → agent connects to workspace
3. AgentService.EmitSignal(STARTED) → trail records stage start
4. LLM loop:
   a. Call LLM with stage profile (system prompt + filtered tools)
   b. Stream tokens to terminal
   c. For each tool call:
      - Autonomy gate check
      - Execute tool via LocalResources
      - AgentService.CreateCheckpoint(observation, tool result)
   d. Feed tool results back to LLM
5. AgentService.CreateCheckpoint(artifact, FINAL, stage output)
6. AgentService.EmitSignal(COMPLETE)
7. Stage output flows as context to next stage
```

**Gate transitions:** Gated stages pause between decompose and dispatch. The CLI prompts the human. Approval → proceed. Rejection → workflow stops.

**Trail:** Every signal, checkpoint, and workspace transition is recorded in the Rust runtime's trail — hash-chained, tamper-evident, recoverable.

---

## 8. Quality Criteria

Six dimensions for evaluating SWE output:

| Dimension | Definition | Evaluation |
|-----------|-----------|------------|
| **Correctness** | Code does what the directive requires | Test pass rate |
| **Type safety** | Type errors eliminated | `type_check` exit code |
| **Style** | Follows project conventions | `lint_check` exit code |
| **Coverage** | Tests cover the changed code | Test count delta |
| **Scope** | Changes limited to declared scope | Changed files vs. planned |
| **Design** | Architecture sound, maintainable | Reviewer verdict |

**Evaluation function:** Each dimension returns `pass`, `warn`, or `fail`. Overall: `pass` if all pass, `warn` if any warn, `fail` if any fail.

---

## 9. Gate Policies

| Transition | Gate | Rationale |
|-----------|------|-----------|
| Goal → plan | None | Planning is read-only |
| Plan → implement | **Human approval** | Scope verification before writes |
| Implement → test | Optional (auto if tests exist) | Low risk |
| Test → review | Auto on test pass | Tests are the gate |
| Review → deliver | **Human approval** | Final sign-off |

---

## 10. Package Structure

```
ecosystem/swe/
├── SWE.md              # This spec
├── package.json        # @wacp/swe
├── tsconfig.json
├── src/
│   ├── index.ts        # Public exports
│   ├── taxonomy.ts     # 4 roles + 7 task types with lookup functions
│   ├── tools/
│   │   └── swe-tools.ts    # 8 tool definitions + executors (auto-detect toolchain)
│   ├── profiles/
│   │   └── profiles.ts     # 4 profiles with system prompts + tool whitelists
│   ├── workflows/
│   │   └── workflows.ts    # 4 workflow DAGs + validation (topological sort, cycle detection)
│   └── quality/
│       └── quality.ts      # 6 dimensions + evaluateQuality() → QualityReport
└── tests/
    ├── taxonomy.test.ts    # 11 tests
    ├── tools.test.ts       # 8 tests
    ├── profiles.test.ts    # 11 tests
    ├── workflows.test.ts   # 14 tests
    └── quality.test.ts     # 13 tests
```

---

## 11. Test Requirements

| Module | Tests | Count |
|--------|-------|-------|
| `taxonomy.ts` | 4 roles unique, correct extends/access/autonomy. 7 task types unique, correct workflow mapping. Lookup functions. | 11 |
| `tools/swe-tools.ts` | 8 definitions unique, valid schemas. Required fields present. | 8 |
| `profiles/profiles.ts` | 4 profiles with non-empty prompts. Tool whitelist matches role. Planner has no write tools. Reviewer is autonomous. All others gated. | 11 |
| `workflows/workflows.ts` | 4 workflows unique, correct stage counts. Dependency order correct. Gated stages marked. DAG validation passes. Missing dependency caught. | 14 |
| `quality/quality.ts` | 6 dimensions unique. All-pass → pass. Test failure → fail. Type check failure → fail. Lint failure → warn. Scope overflow → warn. Review rejection → fail. No review → pass. | 13 |
| **Total** | | **57** |

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| CLI agent spec | §6–7 | §7 | Workflow execution, stage agent loop |
| Coordinator SDK spec | §3–5 | §7 | SubmitGoal, Decompose, Dispatch RPCs |
| Agent SDK v2 spec | §3 | §7 | Bind, EmitSignal, CreateCheckpoint |
| Runtime spec | §3 (process model) | §7 | Workspace lifecycle, trail recording |
| Tool framework spec | §3 | §4 | ToolDefinition schema |
| Local SDK spec | §4 (autonomy) | §9 | Gate policies, trust surface |
| LAYER-MAPPING.md | E1 | §1 | SWE vertical design |

---

*WACP ecosystem specification — authored by Akil Abderrahim and Claude Opus 4.6*
