# WACP Ecosystem: DevOps Vertical

```yaml
id: wacp-eco-devops
type: ecosystem-spec
status: draft
created: 2026-04-10
lineage: LAYER-MAPPING.md (E2)
depends_on:
  - wacp-impl-cli-agent
  - wacp-impl-tool-framework
  - wacp-impl-local-sdk
  - wacp-impl-coordinator-sdk
  - wacp-impl-runtime
  - wacp-eco-swe
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, ecosystem, devops, infrastructure, deployment, incident-response, vertical, multi-agent, workflows]
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

This spec defines the DevOps ecosystem vertical — the second domain parameterization of WACP. It answers "how does the platform behave when the task is infrastructure and operations" — not "how does the platform work" (that's the runtime + middleware).

**What the vertical provides:** Roles (who does what), task types (what kinds of work exist), tools (what capabilities are available), profiles (how each role behaves), workflows (how work decomposes into protocol-level workspaces), quality criteria (how to evaluate output), gate policies (when to ask the human).

**Key constraint — blast radius model:** DevOps actions have environment-scoped impact. A misconfigured deployment to production is categorically different from one to dev. The vertical enforces environment-scaled gating: production mutations always require human approval, staging gates on health checks, dev auto-approves. This constraint permeates every workflow, tool, and gate policy.

**Execution model:** The CLI agent loads the DevOps vertical at boot. When a goal is submitted, the CLI detects the task type, selects the matching workflow, and executes it through the WACP runtime — each stage is a real workspace with its own role profile, tool whitelist, signals, and checkpoints. The vertical defines the decomposition; the protocol enforces it.

---

## 2. Role Taxonomy

Five derived roles, each with a specific concern:

| Role | Extends | Concern | Tool access | Autonomy |
|------|---------|---------|-------------|----------|
| `devops:architect` | worker | Design infrastructure, write IaC, plan changes | Read + write + plan | Gated |
| `devops:deployer` | worker | Execute deployments, manage rollbacks, verify health | Execute + rollback | Gated |
| `devops:monitor` | observer | Observe system health, query metrics/logs, manage alerts | Read + query + alert | Autonomous |
| `devops:responder` | worker | Triage incidents, perform mitigation, contain blast radius | Execute + query | Gated |
| `devops:auditor` | observer | Review compliance, security posture, policy adherence | Read-only + scan | Autonomous |

**Protocol mapping:** Each role maps to a workspace role at dispatch time. The coordinator creates a workspace with the role's tool whitelist and the profile's system prompt as the directive. The agent binds to the workspace and operates within its permissions.

**Environment context:** Every workspace carries an `environment` tag (`dev`, `staging`, `production`). Tool executors read this tag to determine target environment. Gate policies read it to determine approval requirements.

---

## 3. Task Taxonomy

Nine task types with default decomposition:

| Type | Description | Default workflow | Roles involved |
|------|-------------|------------------|----------------|
| `devops:provision` | Set up new infrastructure | `devops:provision` (3 stages) | 3 (architect, deployer, auditor) |
| `devops:deploy` | Deploy application changes | `devops:deploy` (4 stages) | 4 (architect, auditor, deployer, monitor) |
| `devops:monitor` | Set up or review monitoring | Direct (1 stage) | 1 (monitor) |
| `devops:respond` | Incident response | `devops:respond` (3 stages) | 2 (responder, architect) |
| `devops:audit` | Compliance/security audit | `devops:audit` (2 stages) | 1 (auditor) |
| `devops:migrate` | Migrate infrastructure or services | `devops:deploy` (4 stages) | 4 (architect, auditor, deployer, monitor) |
| `devops:configure` | Configuration management | `devops:provision` (3 stages) | 3 (architect, deployer, auditor) |
| `devops:secure` | Security hardening | `devops:secure` (3 stages) | 2 (auditor, architect) |
| `devops:optimize` | Performance/cost optimization | `devops:optimize` (3 stages) | 3 (monitor, architect, auditor) |

**Detection:** The CLI classifies user goals into task types via keyword heuristics: "deploy" → `devops:deploy`, "provision"/"set up infrastructure" → `devops:provision`, "incident"/"outage"/"down" → `devops:respond`, "audit"/"compliance" → `devops:audit`, "migrate" → `devops:migrate`, "configure" → `devops:configure`, "harden"/"secure" → `devops:secure`, "optimize"/"cost"/"performance" → `devops:optimize`, "monitor"/"alert" → `devops:monitor`, default → `devops:deploy`.

---

## 4. Tool Catalog

DevOps-specific tools beyond the CLI's built-in 7. All 17 tools (7 built-in + 10 DevOps) are registered at boot and filtered per stage by the profile's whitelist.

| Tool | Description | Operation type |
|------|-------------|----------------|
| `infra_plan` | Generate/validate IaC plan (Terraform plan, Pulumi preview, CloudFormation changeset) | `shell_exec` |
| `deploy_execute` | Execute deployment (apply IaC, push container, run migration) | `shell_exec` |
| `rollback` | Rollback to previous version (previous state, rollout undo) | `shell_exec` |
| `health_check` | Check service health (HTTP endpoints, TCP ports, process state) | `network_read` |
| `log_query` | Query logs (structured search across log files/aggregators) | `data_read` |
| `metric_query` | Query metrics (time-series: CPU, memory, latency, error rate) | `data_read` |
| `alert_manage` | Manage alerts (create, update, silence, acknowledge) | `config_write` |
| `config_validate` | Validate configuration (schema/syntax for YAML, JSON, HCL, Dockerfile) | `file_read` |
| `secret_rotate` | Rotate secrets (generate credentials, update store, invalidate old) | `secret_write` |
| `compliance_scan` | Scan for compliance violations (policy-as-code: OPA, Checkov, tfsec) | `shell_exec` |

Tool executors auto-detect the project's infrastructure toolchain (`terraform`/`pulumi`/`cdk` for IaC, `kubectl`/`docker`/`helm` for containers, project-local log/metric endpoints for observability).

**Environment awareness:** `deploy_execute`, `rollback`, and `secret_rotate` read the workspace's `environment` tag. Production targets require the workspace to have passed the environment-scaled gate. Tools refuse to execute against production without a gate clearance checkpoint in the trail.

---

## 5. Agent Profiles

One profile per role. Each profile provides: system prompt, tool whitelist, autonomy level.

**`devops:architect`** — Read + write + plan. System prompt instructs: analyze existing infrastructure, produce IaC changes with blast radius assessment, evaluate tradeoffs (cost, availability, complexity), document rollback strategy. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `code_search`, `infra_plan`, `config_validate`, `compliance_scan`, `git_status`, `git_diff`.

**`devops:deployer`** — Execute + rollback. System prompt instructs: execute deployment plan exactly as specified, verify health after each step, rollback immediately on failure, never skip health checks, log every action. Tools: `read_file`, `list_dir`, `search_files`, `deploy_execute`, `rollback`, `health_check`, `log_query`, `metric_query`, `config_validate`, `git_status`.

**`devops:monitor`** — Read + query + alert. System prompt instructs: observe system health, identify anomalies in metrics/logs, configure alerting thresholds, report status with evidence. Tools: `read_file`, `list_dir`, `search_files`, `log_query`, `metric_query`, `alert_manage`, `health_check`.

**`devops:responder`** — Execute + query. System prompt instructs: triage incident by severity, identify root cause from logs/metrics, apply minimal mitigation to contain blast radius, document every action for postmortem. Tools: `read_file`, `list_dir`, `search_files`, `log_query`, `metric_query`, `health_check`, `deploy_execute`, `rollback`, `config_validate`, `alert_manage`.

**`devops:auditor`** — Read-only + scan, autonomous. System prompt instructs: review infrastructure for compliance violations, security misconfigurations, policy drift, and best-practice deviations. Produce audit report with severity-ranked findings and remediation guidance. Tools: `read_file`, `list_dir`, `search_files`, `code_search`, `compliance_scan`, `config_validate`, `log_query`, `metric_query`.

---

## 6. Workflows

Five workflow DAGs plus one direct. Each defines stages with role assignments, dependencies, and gate policies.

**`devops:provision`** (3 stages):
```
plan (architect) ──→ execute (deployer) ──→ verify (auditor)
                        [gated]
```
Used by: `devops:provision`, `devops:configure`.

**`devops:deploy`** (4 stages):
```
plan (architect) ──→ validate (auditor) ──→ execute (deployer) ──→ verify (monitor)
                                               [gated]
```
Used by: `devops:deploy`, `devops:migrate`. The validate stage runs compliance scan and config validation before any mutation. Rollback-aware: if verify fails, the deployer stage's rollback strategy is available as a recovery action.

**`devops:respond`** (3 stages):
```
triage (responder) ──→ mitigate (responder) ──→ postmortem (architect)
                          [gated]
```
Used by: `devops:respond`. Triage is read-only (logs, metrics, health). Mitigate is gated — human approves the mitigation plan before execution. Postmortem produces the incident report and remediation plan.

**`devops:secure`** (3 stages):
```
scan (auditor) ──→ remediate (architect) ──→ verify (auditor)
                      [gated]
```
Used by: `devops:secure`, `devops:audit` (scan + report only, no remediate). Scan identifies violations. Remediate writes IaC/config fixes. Verify re-scans to confirm resolution.

**`devops:optimize`** (3 stages):
```
analyze (monitor) ──→ propose (architect) ──→ validate (auditor)
```
Used by: `devops:optimize`. Analyze gathers metrics/cost data. Propose writes optimization changes. Validate confirms no compliance or availability regressions.

**Direct execution:** `devops:monitor` — single-stage, monitor role. Used for ad-hoc observability tasks (check health, query metrics, review alerts).

**DAG validation:** `validateWorkflow()` checks that all dependencies exist and no cycles are present.

---

## 7. Execution Model

The DevOps vertical executes through the WACP protocol — not as a simulation.

**Goal submission:**
```
1. User submits goal: "deploy the new API version to production"
2. CLI detects task type: devops:deploy
3. CLI extracts environment: production (from goal text or prompt)
4. CLI selects workflow: devops:deploy (4 stages)
5. CoordinatorService.SubmitGoal → runtime creates root workspace (env=production)
6. CoordinatorService.Decompose → runtime creates task graph (4 tasks)
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
      - Autonomy gate check (environment-scaled)
      - Execute tool via LocalResources
      - AgentService.CreateCheckpoint(observation, tool result)
   d. Feed tool results back to LLM
5. AgentService.CreateCheckpoint(artifact, FINAL, stage output)
6. AgentService.EmitSignal(COMPLETE)
7. Stage output flows as context to next stage
```

**Blast radius enforcement:** The environment tag propagates from root workspace to all child workspaces. Tools that perform mutations (`deploy_execute`, `rollback`, `secret_rotate`) check the environment tag before execution. Production mutations without a prior gate-clearance checkpoint cause the tool to refuse execution with an `ENVIRONMENT_GATE_REQUIRED` error.

**Rollback path:** When the verify stage detects a health failure after deployment:
1. Monitor emits signal `FAILED` with health check evidence
2. Coordinator receives failure, checks rollback strategy from the plan stage's checkpoint
3. If auto-rollback is configured: dispatch rollback task to deployer (gated in production)
4. If manual rollback: escalate to human with rollback instructions

**Gate transitions:** Gated stages pause between decompose and dispatch. The CLI prompts the human. Approval → proceed. Rejection → workflow stops.

**Trail:** Every signal, checkpoint, and workspace transition is recorded in the Rust runtime's trail — hash-chained, tamper-evident, recoverable. For DevOps, the trail is the audit log — it records who approved what, when, and what changed.

---

## 8. Quality Criteria

Six dimensions for evaluating DevOps output:

| Dimension | Definition | Evaluation |
|-----------|-----------|------------|
| **Availability** | Services remain healthy after changes | `health_check` pass rate across endpoints |
| **Security posture** | No new vulnerabilities introduced | `compliance_scan` finding count (delta) |
| **Blast radius** | Changes scoped to declared target | Affected resources vs. planned resources |
| **Rollback readiness** | Rollback strategy exists and is executable | Rollback checkpoint present, strategy validated |
| **Compliance** | Changes meet regulatory/policy requirements | `compliance_scan` exit code, zero critical violations |
| **Documentation** | Runbook/playbook updated, changes documented | Changelog checkpoint present, IaC comments adequate |

**Evaluation function:** Each dimension returns `pass`, `warn`, or `fail`. Rules:
- `health_check` failure on any endpoint → `availability` = `fail`
- New critical compliance finding → `security_posture` = `fail`
- New non-critical finding → `security_posture` = `warn`
- Affected resources exceed plan → `blast_radius` = `fail`
- No rollback checkpoint → `rollback_readiness` = `fail`
- Rollback checkpoint present but untested → `rollback_readiness` = `warn`
- Critical compliance violation → `compliance` = `fail`
- No changelog checkpoint → `documentation` = `warn`

Overall: `pass` if all pass, `warn` if any warn and none fail, `fail` if any fail.

---

## 9. Gate Policies

### Environment-Scaled Gating

The defining constraint of the DevOps vertical. Gate strictness scales with environment tier:

| Environment | Mutation gate | Rollback gate | Description |
|-------------|---------------|---------------|-------------|
| `dev` | Auto-approve | Auto-approve | Fast iteration, no human in loop |
| `staging` | Auto-approve | Auto-approve | Auto-gated, health-check verified |
| `production` | **Human approval** | **Human approval** | All mutations require sign-off |

### Per-Workflow Gates

| Transition | Gate | Rationale |
|-----------|------|-----------|
| Goal → plan | None | Planning is read-only |
| Plan → validate | None | Validation is read-only |
| Validate → execute | **Environment-scaled** | Mutation gate — auto in dev/staging, human in production |
| Execute → verify | Auto on completion | Verification is read-only |
| Verify failure → rollback | **Environment-scaled** | Rollback is a mutation — same gate as deploy |
| Triage → mitigate | **Human approval** | Incident mitigation always needs sign-off regardless of environment |
| Scan → remediate | **Human approval** | Security remediations change infrastructure |
| Remediate → verify | Auto | Re-scan is read-only |

---

## 10. Package Structure

```
ecosystem/devops/
├── DEVOPS.md              # This spec
├── package.json           # @wacp/devops
├── tsconfig.json
├── src/
│   ├── index.ts           # Public exports
│   ├── taxonomy.ts        # 5 roles + 9 task types with lookup functions
│   ├── tools/
│   │   └── devops-tools.ts    # 10 tool definitions + executors (auto-detect toolchain)
│   ├── profiles/
│   │   └── profiles.ts        # 5 profiles with system prompts + tool whitelists
│   ├── workflows/
│   │   └── workflows.ts       # 5 workflow DAGs + validation (topological sort, cycle detection)
│   └── quality/
│       └── quality.ts         # 6 dimensions + evaluateQuality() → QualityReport
└── tests/
    ├── taxonomy.test.ts       # 14 tests
    ├── tools.test.ts          # 10 tests
    ├── profiles.test.ts       # 11 tests
    ├── workflows.test.ts      # 14 tests
    └── quality.test.ts        # 10 tests
```

---

## 11. Test Requirements

| Module | Tests | Count |
|--------|-------|-------|
| `taxonomy.ts` | 5 roles unique, correct extends/access/autonomy. 9 task types unique, correct workflow mapping. Lookup by type returns correct workflow. Lookup by role returns correct profile. Environment tiers validated. | 14 |
| `tools/devops-tools.ts` | 10 definitions unique, valid schemas. Required fields present. Environment-aware tools identified. Operation types correct. | 10 |
| `profiles/profiles.ts` | 5 profiles with non-empty prompts. Tool whitelist matches role access. Architect has no execute tools. Monitor is autonomous. Deployer has rollback. Responder has both query and execute. Auditor is read-only. | 11 |
| `workflows/workflows.ts` | 5 workflows unique, correct stage counts. Dependency order correct. Gated stages marked. DAG validation passes. Missing dependency caught. Cycle detected. Task type → workflow mapping complete. Deploy and migrate share workflow. Provision and configure share workflow. | 14 |
| `quality/quality.ts` | 6 dimensions unique. All-pass → pass. Health check failure → fail. New critical finding → security fail. Non-critical finding → security warn. Blast radius exceeded → fail. No rollback checkpoint → fail. Untested rollback → warn. Compliance violation → fail. No changelog → warn. No audit findings → pass. | 10 |
| **Total** | | **59** |

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| SWE vertical spec | §1–12 | §1 | Pattern template — structure, execution model |
| CLI agent spec | §6–7 | §7 | Workflow execution, stage agent loop |
| Coordinator SDK spec | §3–5 | §7 | SubmitGoal, Decompose, Dispatch RPCs |
| Agent SDK v2 spec | §3 | §7 | Bind, EmitSignal, CreateCheckpoint |
| Runtime spec | §3 (process model) | §7 | Workspace lifecycle, trail recording |
| Tool framework spec | §3 | §4 | ToolDefinition schema, environment context |
| Local SDK spec | §4 (autonomy) | §9 | Gate policies, trust surface |
| Security spec | §3 (content filter) | §4 | Secret rotation, audit events |
| LAYER-MAPPING.md | E2 | §1 | DevOps vertical design, role/task enumeration |

---

*WACP ecosystem specification — authored by Akil Abderrahim and Claude Opus 4.6*
