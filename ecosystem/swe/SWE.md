# WACP Ecosystem: SWE Vertical

```yaml
id: wacp-eco-swe
type: ecosystem-spec
status: draft
created: 2026-04-02
lineage: LAYER-MAPPING.md (E1)
depends_on:
  - wacp-impl-cli-agent
  - wacp-impl-tool-framework
  - wacp-impl-local-sdk
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, ecosystem, swe, software-engineering, vertical]
```

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Role Taxonomy](#2-role-taxonomy)
3. [Task Taxonomy](#3-task-taxonomy)
4. [Tool Catalog](#4-tool-catalog)
5. [Agent Profiles](#5-agent-profiles)
6. [Workflows](#6-workflows)
7. [Quality Criteria](#7-quality-criteria)
8. [Gate Policies](#8-gate-policies)
9. [Package Structure](#9-package-structure)
10. [Test Requirements](#10-test-requirements)
11. [References](#11-references)

---

## 1. Purpose

This spec defines the SWE (Software Engineering) ecosystem vertical — the first domain parameterization of WACP. It answers "how does the platform behave when the task is software engineering" — not "how does the platform work" (that's the runtime + middleware).

**What the vertical provides:** Roles (who does what), task types (what kinds of work exist), tools (what capabilities are available), profiles (how each role behaves), workflows (how work decomposes), quality criteria (how to evaluate output), gate policies (when to ask the human).

**Immediate value:** The CLI agent (Phase 25) loads the SWE vertical at boot and gains a domain-optimized system prompt, SWE-specific tools, and structured workflow guidance. The single-agent CLI uses the profiles directly; multi-agent coordination (future) uses the full decomposition patterns.

---

## 2. Role Taxonomy

Four derived roles, each with a specific concern:

| Role | Extends | Concern | Tool access | Autonomy |
|------|---------|---------|-------------|----------|
| `swe:planner` | worker | Analyze requirements, produce plans, assign scope | Read-only + search | Gated |
| `swe:implementer` | worker | Write/edit code, produce diffs | Read + write + search | Gated |
| `swe:tester` | worker | Write tests, run tests, report results | Read + write + test execution | Gated |
| `swe:reviewer` | observer | Review changes, evaluate quality | Read-only + search | Autonomous |

**Single-agent mapping:** In the CLI, the agent cycles through roles within a single session. "Planning" phase uses the planner profile. "Implementation" phase switches to implementer. The role is a prompt + tool configuration, not a separate process.

---

## 3. Task Taxonomy

Seven task types with default decomposition:

| Type | Description | Default workflow | Roles involved |
|------|-------------|------------------|----------------|
| `swe:implement` | Build new functionality | plan → implement → test → review | All 4 |
| `swe:refactor` | Restructure existing code | plan → implement → test → review | All 4 |
| `swe:debug` | Find and fix a bug | plan → implement → test | 3 (no reviewer) |
| `swe:test` | Write or improve tests | plan → test | 2 |
| `swe:review` | Review existing changes | review only | 1 (reviewer) |
| `swe:document` | Write/update documentation | implement only | 1 (implementer) |
| `swe:investigate` | Research a question | plan only | 1 (planner) |

---

## 4. Tool Catalog

SWE-specific tools beyond the CLI's built-in 7:

| Tool | Description | Input | Operation type |
|------|-------------|-------|----------------|
| `code_search` | Search codebase for patterns (grep/ripgrep) | `{ pattern, path?, file_type? }` | `file_read` |
| `code_edit` | Apply targeted edits to a file (find/replace) | `{ path, old_text, new_text }` | `file_write` |
| `test_run` | Run test suite or specific test file | `{ command?, file?, filter? }` | `shell_exec` |
| `lint_check` | Run linter on files | `{ path?, fix? }` | `shell_exec` |
| `type_check` | Run type checker (tsc, mypy, etc.) | `{ path? }` | `shell_exec` |
| `git_branch` | Create or switch git branch | `{ name, create? }` | `git_write` |
| `git_commit` | Stage and commit changes | `{ message, paths? }` | `git_write` |
| `dependency_check` | Check for outdated/vulnerable deps | `{}` | `shell_exec` |

Each tool is a `ToolDefinition` with JSON Schema `input_schema`, backed by a function that calls the appropriate shell command or `LocalResources` method.

---

## 5. Agent Profiles

One profile per role. Each profile configures: system prompt, tool whitelist, context priorities.

**`swe:planner` profile:**
```
System prompt: You are a software planning agent. Analyze the codebase,
understand the requirements, and produce a structured plan. You have
read-only access — do not modify files. Focus on:
- Understanding existing code structure
- Identifying files that need changes
- Breaking work into clear, ordered steps
- Noting potential risks and edge cases

Tools: read_file, list_dir, search_files, code_search, git_status, git_diff
```

**`swe:implementer` profile:**
```
System prompt: You are a software implementation agent. Write clean,
correct code following existing conventions. You have full read/write
access. Focus on:
- Making minimal, focused changes
- Following existing code style
- Writing self-documenting code
- Not changing files outside the stated scope

Tools: read_file, write_file, list_dir, search_files, code_search,
       code_edit, run_command, git_status, git_diff, git_branch, git_commit
```

**`swe:tester` profile:**
```
System prompt: You are a software testing agent. Write and run tests
that verify the implementation is correct. Focus on:
- Testing behavior through public interfaces
- Covering edge cases and error paths
- Running existing tests to check for regressions
- Reporting clear pass/fail results

Tools: read_file, write_file, list_dir, search_files, code_search,
       code_edit, test_run, run_command, git_status, git_diff
```

**`swe:reviewer` profile:**
```
System prompt: You are a code review agent. Evaluate changes for
correctness, style, and design. You have read-only access. Focus on:
- Correctness: does the code do what it claims?
- Style: does it follow project conventions?
- Scope: are changes limited to what was planned?
- Design: are architectural decisions sound?

Produce a review with: summary, issues (if any), verdict (approve/request-changes).

Tools: read_file, list_dir, search_files, code_search, git_diff, test_run
```

---

## 6. Workflows

Four decomposition patterns. Each defines a task DAG with role assignments.

**`swe:implement-feature`:**
```
plan (planner) ──→ implement (implementer) ──→ test (tester) ──→ review (reviewer)
                      │                            │
                      └── [gate: human approves] ──┘
```

**`swe:refactor`:**
```
plan (planner) ──→ implement (implementer) ──→ test (tester) ──→ review (reviewer)
```
Same as implement-feature. Tests are critical to verify behavior preservation.

**`swe:fix-bug`:**
```
plan (planner) ──→ implement (implementer) ──→ test (tester)
```
No review — bugs are urgent. Tests verify the fix.

**`swe:write-tests`:**
```
plan (planner) ──→ test (tester)
```
Planner identifies what needs testing. Tester writes and runs the tests.

**Single-agent execution:** The CLI agent executes these workflows sequentially within one session, switching profiles at each stage. The workflow guides the agent's approach, not its process isolation.

---

## 7. Quality Criteria

Six dimensions for evaluating SWE output:

| Dimension | Definition | Evaluation |
|-----------|-----------|------------|
| **Correctness** | Code does what the directive requires. No logic errors. | Test pass rate. |
| **Type safety** | Type errors eliminated. Compiler/checker passes. | `type_check` exit code. |
| **Style** | Follows project conventions. Consistent with codebase. | `lint_check` exit code. |
| **Coverage** | Tests cover the changed code. Critical paths tested. | Test coverage delta. |
| **Scope** | Changes limited to declared scope. No unrelated edits. | Diff file count vs. plan. |
| **Design** | Architecture sound. Maintainability preserved. | Reviewer verdict. |

**Evaluation function:** Each dimension returns `pass`, `warn`, or `fail`. Overall quality is `pass` if all dimensions pass, `warn` if any warn, `fail` if any fail.

---

## 8. Gate Policies

Which transitions require human approval:

| Transition | Gate | Rationale |
|-----------|------|-----------|
| Goal → plan | None | Planning is read-only |
| Plan → implement | **Human approval** | Scope verification |
| Implement → test | Optional (auto if tests exist) | Low risk |
| Test → review | Auto on test pass | Tests are the gate |
| Review → deliver | **Human approval** | Final sign-off |

---

## 9. Package Structure

```
ecosystem/swe/
├── SWE.md              # This spec
├── package.json        # @wacp/swe
├── tsconfig.json
├── src/
│   ├── index.ts        # Registration entry point
│   ├── taxonomy.ts     # Role + task type definitions
│   ├── tools/
│   │   └── swe-tools.ts    # SWE tool definitions + executors
│   ├── profiles/
│   │   └── profiles.ts     # System prompts + tool whitelists per role
│   ├── workflows/
│   │   └── workflows.ts    # Decomposition patterns (task DAGs)
│   └── quality/
│       └── quality.ts      # Evaluation dimensions + functions
└── tests/
    ├── taxonomy.test.ts
    ├── tools.test.ts
    ├── profiles.test.ts
    ├── workflows.test.ts
    └── quality.test.ts
```

---

## 10. Test Requirements

| Module | Tests |
|--------|-------|
| `taxonomy.ts` | 4 roles defined with correct properties. 7 task types defined. Role extends correct base. Task type maps to workflow. |
| `tools/swe-tools.ts` | All 8 tool definitions have valid schemas. code_search runs grep. code_edit applies replacement. test_run executes test command. |
| `profiles/profiles.ts` | Profile per role. System prompt non-empty. Tool whitelist matches role access. Planner has no write tools. Implementer has write tools. |
| `workflows/workflows.ts` | 4 workflows defined. implement-feature has 4 stages. fix-bug has 3 stages. Stages reference correct roles. Dependencies form valid DAG. |
| `quality/quality.ts` | 6 dimensions defined. Evaluation returns pass/warn/fail. All-pass → overall pass. Any-fail → overall fail. |

**Total target: ~30 tests.**

---

## 11. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| CLI agent spec | §6, §7 | §4, §5 | Tool integration, agent loop |
| Tool framework spec | §3 | §4 | ToolDefinition schema |
| Local SDK spec | §4 | §5 | Autonomy presets |
| LAYER-MAPPING.md | E1 | §1 | SWE vertical design |

---

*WACP ecosystem specification — authored by Akil Abderrahim and Claude Opus 4.6*
