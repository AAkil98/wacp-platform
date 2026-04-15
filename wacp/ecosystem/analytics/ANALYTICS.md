# WACP Ecosystem: Data Analytics Vertical

```yaml
id: wacp-eco-analytics
type: ecosystem-spec
status: draft
created: 2026-04-10
lineage: IMPLEMENTATION.md (27F)
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
tags: [wacp, ecosystem, analytics, data, sql, bi, dashboards, vertical, multi-agent, workflows]
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

This spec defines the Data Analytics ecosystem vertical. It answers "how does the platform behave when the task is business intelligence, reporting, or ad-hoc data analysis" — not "how does the platform work" (that's the runtime + middleware).

**What the vertical provides:** Roles (who does what), task types (what kinds of work exist), tools (what capabilities are available), profiles (how each role behaves), workflows (how work decomposes into protocol-level workspaces), quality criteria (how to evaluate output), gate policies (when to ask the human).

**Key constraint — query reproducibility and data integrity:** Every analytical result must be reproducible. The same query text against the same data snapshot must produce the same result. Destructive SQL operations (`DROP`, `TRUNCATE`, `DELETE` without `WHERE`, `UPDATE` without `WHERE`) are hard-gated regardless of environment. Every report cites its source queries and data-freshness timestamps — readers must be able to trace any number back to its query.

**Execution model:** The CLI agent loads the Analytics vertical at boot. When a goal is submitted, the CLI detects the task type, selects the matching workflow, and executes it through the WACP runtime — each stage is a real workspace with its own role profile, tool whitelist, signals, and checkpoints. The vertical defines the decomposition; the protocol enforces it.

---

## 2. Role Taxonomy

Five derived roles, each with a specific concern:

| Role | Extends | Concern | Tool access | Autonomy |
|------|---------|---------|-------------|----------|
| `analytics:analyst` | worker | Write queries, profile data, produce numbers | Query + profile | Gated |
| `analytics:modeler` | worker | Build data models, define metrics, design schemas | Schema + metric | Gated |
| `analytics:validator` | observer | Reconcile sources, verify data integrity, check freshness | Reconcile + profile | Autonomous |
| `analytics:reporter` | worker | Generate reports, build visualizations, cite sources | Report + viz | Gated |
| `analytics:insights` | observer | Synthesize findings, narrate implications, flag anomalies | Read + synthesize | Autonomous |

**Protocol mapping:** Each role maps to a workspace role at dispatch time. The coordinator creates a workspace with the role's tool whitelist and the profile's system prompt as the directive. The agent binds to the workspace and operates within its permissions.

**Query context:** Every workspace carries a `data_snapshot_id` tag identifying the point-in-time snapshot against which queries run. Reproducibility is enforced at the tool level — the same query against the same snapshot always produces the same result.

---

## 3. Task Taxonomy

Eight task types with default decomposition:

| Type | Description | Default workflow | Roles involved |
|------|-------------|------------------|----------------|
| `analytics:query` | Write a SQL query and return results | `analytics:query-and-report` (3 stages) | 3 (analyst, validator, reporter) |
| `analytics:report` | Produce a formatted report | `analytics:query-and-report` (3 stages) | 3 (analyst, validator, reporter) |
| `analytics:dashboard` | Build an interactive dashboard | `analytics:build-dashboard` (3 stages) | 3 (modeler, reporter, validator) |
| `analytics:model` | Build a data model (facts, dimensions, metrics) | `analytics:model-data` (3 stages) | 3 (analyst, modeler, validator) |
| `analytics:analyze` | Ad-hoc analysis | `analytics:query-and-report` (3 stages) | 3 (analyst, validator, reporter) |
| `analytics:validate` | Data validation and reconciliation | Direct (1 stage) | 1 (validator) |
| `analytics:monitor` | KPI monitoring | Direct (1 stage) | 1 (insights) |
| `analytics:investigate` | Investigate a data anomaly | `analytics:investigate` (3 stages) | 3 (analyst, validator, insights) |

**Detection:** The CLI classifies user goals into task types via keyword heuristics: "query"/"sql" → `analytics:query`, "report" → `analytics:report`, "dashboard" → `analytics:dashboard`, "metric"/"kpi" → `analytics:monitor`, "investigate"/"anomaly"/"why" → `analytics:investigate`, "validate"/"reconcile" → `analytics:validate`, "model"/"schema" → `analytics:model`, default → `analytics:analyze`.

---

## 4. Tool Catalog

Analytics-specific tools beyond the CLI's built-in 7. All 17 tools (7 built-in + 10 analytics) are registered at boot and filtered per stage by the profile's whitelist.

| Tool | Description | Operation type |
|------|-------------|----------------|
| `sql_query` | Execute a SQL query against a configured database. Read-only by default. Destructive operations hard-gated. | `data_read` |
| `dashboard_build` | Build or update a dashboard (Superset, Metabase, Tableau, Looker) | `config_write` |
| `data_profile` | Profile a dataset — compute statistics, distributions, null counts, cardinality | `data_read` |
| `kpi_calculate` | Compute a KPI from raw data using a metric definition | `data_read` |
| `report_generate` | Generate a formatted report (Markdown, PDF, HTML) with cited queries | `file_write` |
| `viz_create` | Create a visualization (bar, line, scatter, heatmap) from query results | `file_write` |
| `data_reconcile` | Reconcile two data sources — compare counts, aggregates, sample values | `data_read` |
| `schema_explore` | Explore a database schema — list tables, columns, types, relationships | `data_read` |
| `query_optimize` | Analyze and optimize a slow query — suggest indexes, rewrites | `data_read` |
| `metric_define` | Define a semantic metric in the metric store (dbt semantic layer, Cube, etc.) | `config_write` |

Tool executors auto-detect the project's analytics toolchain (`psql`/`mysql`/`duckdb` for SQL, `superset`/`metabase`/`tableau` for dashboards, `dbt` for modeling).

**SQL safety:** `sql_query` parses statements before execution. `DROP`, `TRUNCATE`, `CREATE`, `ALTER`, `DELETE` without `WHERE`, and `UPDATE` without `WHERE` are rejected unless an explicit `allow_destructive: true` parameter is set AND the workspace has gate clearance in the trail.

---

## 5. Agent Profiles

One profile per role. Each profile provides: system prompt, tool whitelist, autonomy level.

**`analytics:analyst`** — Query + profile. System prompt instructs: understand the business question, explore the schema, profile relevant tables, write a clean SQL query with explicit column selection, validate row counts against known totals, cite the query and its execution timestamp in outputs. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `sql_query`, `data_profile`, `schema_explore`, `query_optimize`, `git_status`.

**`analytics:modeler`** — Schema + metric. System prompt instructs: design dimensional models (facts, dimensions, conformed dimensions), define metrics with clear aggregation semantics, document grain and granularity, ensure metric consistency across dashboards, version metric definitions. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `schema_explore`, `metric_define`, `sql_query`, `data_profile`, `git_status`, `git_diff`.

**`analytics:validator`** — Reconcile + profile, autonomous. System prompt instructs: reconcile numbers across sources (warehouse vs. operational systems), verify data freshness against SLA, profile datasets for anomalies (nulls, cardinality shifts, distribution drift), produce a validation report with pass/warn/fail per check. Tools: `read_file`, `list_dir`, `search_files`, `data_reconcile`, `data_profile`, `sql_query`, `schema_explore`.

**`analytics:reporter`** — Report + viz. System prompt instructs: produce clear reports with an executive summary, key findings, supporting data, and source queries cited inline. Choose appropriate visualizations (bar for comparison, line for trend, scatter for correlation). Always include data freshness timestamp. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `sql_query`, `report_generate`, `viz_create`, `dashboard_build`, `kpi_calculate`.

**`analytics:insights`** — Read + synthesize, autonomous. System prompt instructs: synthesize findings across multiple analyses, identify patterns and outliers, propose hypotheses for anomalies, quantify business impact, flag findings that need investigation. Tools: `read_file`, `list_dir`, `search_files`, `sql_query`, `data_profile`, `kpi_calculate`, `report_generate`.

---

## 6. Workflows

Four workflow DAGs plus direct task types. Each defines stages with role assignments, dependencies, and gate policies.

**`analytics:query-and-report`** (3 stages):
```
query (analyst) ──→ validate (validator) ──→ report (reporter)
                                                 [gated]
```
Used by: `analytics:query`, `analytics:report`, `analytics:analyze`. Query writes and executes SQL. Validate reconciles and profiles results. Report formats output — gated before publication.

**`analytics:build-dashboard`** (3 stages):
```
model (modeler) ──→ build (reporter) ──→ validate (validator)
                       [gated]
```
Used by: `analytics:dashboard`. Model defines the semantic layer. Build constructs the dashboard — gated before publication. Validate confirms numbers match source.

**`analytics:investigate`** (3 stages):
```
explore (analyst) ──→ reconcile (validator) ──→ report (insights)
```
Used by: `analytics:investigate`. Explore queries the data. Reconcile checks if the anomaly is real or a data issue. Report narrates findings.

**`analytics:model-data`** (3 stages):
```
explore (analyst) ──→ model (modeler) ──→ validate (validator)
                          [gated]
```
Used by: `analytics:model`. Explore understands the domain. Model designs the schema — gated before creation. Validate tests the model against source data.

**Direct execution:** `analytics:validate` (validator), `analytics:monitor` (insights).

**DAG validation:** `validateWorkflow()` checks that all dependencies exist and no cycles are present.

---

## 7. Execution Model

The Analytics vertical executes through the WACP protocol — not as a simulation.

**Goal submission:**
```
1. User submits goal: "show me revenue by region for last quarter"
2. CLI detects task type: analytics:query (or analytics:analyze)
3. CLI selects workflow: analytics:query-and-report (3 stages)
4. CoordinatorService.SubmitGoal → runtime creates root workspace with data_snapshot_id
5. CoordinatorService.Decompose → runtime creates task graph (3 tasks)
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
      - SQL safety check (for sql_query)
      - Execute tool via LocalResources
      - AgentService.CreateCheckpoint(observation, tool result, query_text)
   d. Feed tool results back to LLM
5. AgentService.CreateCheckpoint(artifact, FINAL, stage output)
6. AgentService.EmitSignal(COMPLETE)
7. Stage output flows as context to next stage
```

**Reproducibility enforcement:** Every `sql_query` checkpoint records the full query text, database identifier, data snapshot ID, and result row count hash. The `reproduce` task type re-executes checkpoints against the same snapshot and verifies the result hash matches.

**Citation contract:** Every report checkpoint must include a `sources` field listing the query checkpoints it references. The reporter profile's system prompt enforces this; the quality evaluator verifies it.

**Trail:** Every query, checkpoint, and workspace transition is recorded in the Rust runtime's trail — hash-chained, tamper-evident, recoverable.

---

## 8. Quality Criteria

Six dimensions for evaluating Analytics output:

| Dimension | Definition | Evaluation |
|-----------|-----------|------------|
| **Accuracy** | Numbers reconcile with source systems | `data_reconcile` result |
| **Data freshness** | Underlying data within SLA window | Snapshot timestamp vs. SLA |
| **Reproducibility** | Queries re-executable with same result | Query hash + snapshot ID present |
| **Completeness** | Report answers the original question | Required sections present |
| **Clarity** | Results understandable to target audience | Citations + narrative present |
| **Performance** | Queries execute within budget | Query duration vs. threshold |

**Evaluation function:** Each dimension returns `pass`, `warn`, or `fail`. Rules:
- Reconciliation mismatch > 1% → `accuracy` = `fail`
- Reconciliation mismatch 0.1–1% → `accuracy` = `warn`
- Data older than SLA → `data_freshness` = `fail`
- Data older than 75% of SLA → `data_freshness` = `warn`
- Query hash missing → `reproducibility` = `fail`
- Snapshot ID missing → `reproducibility` = `fail`
- Required report sections missing → `completeness` = `fail`
- No source citations → `clarity` = `fail`
- No narrative → `clarity` = `warn`
- Query duration > hard threshold → `performance` = `fail`
- Query duration > soft threshold → `performance` = `warn`

Overall: `pass` if all pass, `warn` if any warn and none fail, `fail` if any fail.

---

## 9. Gate Policies

### SQL Safety Gating

The defining constraint of the Analytics vertical. Destructive operations bypass normal workflow gates and require explicit approval.

| Operation | Gate | Rationale |
|-----------|------|-----------|
| `SELECT` | None | Read-only |
| `INSERT` (values) | Auto in dev, **gated** in production | Data mutation |
| `UPDATE` with `WHERE` | Auto in dev, **gated** in production | Scoped mutation |
| `UPDATE` without `WHERE` | **Hard block** | Full-table update — refuse by default |
| `DELETE` with `WHERE` | Auto in dev, **gated** in production | Scoped deletion |
| `DELETE` without `WHERE` | **Hard block** | Full-table deletion — refuse by default |
| `DROP`, `TRUNCATE` | **Hard block + explicit override** | Irreversible schema change |
| `CREATE`, `ALTER` | **Gated** | Schema change |

### Per-Workflow Gates

| Transition | Gate | Rationale |
|-----------|------|-----------|
| Goal → query | None | Query stage is read-only |
| Query → validate | None | Validation is read-only |
| Validate → report | **Human approval** | Report publication — numbers leave the system |
| Model → build | **Human approval** | Dashboard publication |
| Explore → model | **Human approval** | Schema changes |
| Investigate stages | None | Investigation is read-only throughout |

---

## 10. Package Structure

```
ecosystem/analytics/
├── ANALYTICS.md           # This spec
├── package.json           # @wacp/analytics
├── tsconfig.json
├── src/
│   ├── index.ts           # Public exports
│   ├── taxonomy.ts        # 5 roles + 8 task types with lookup functions
│   ├── tools/
│   │   └── analytics-tools.ts # 10 tool definitions + executors (SQL safety + toolchain detection)
│   ├── profiles/
│   │   └── profiles.ts        # 5 profiles with system prompts + tool whitelists
│   ├── workflows/
│   │   └── workflows.ts       # 4 workflow DAGs + validation (topological sort, cycle detection)
│   └── quality/
│       └── quality.ts         # 6 dimensions + evaluateQuality() → QualityReport
└── tests/
    ├── taxonomy.test.ts       # 13 tests
    ├── tools.test.ts          # 11 tests
    ├── profiles.test.ts       # 11 tests
    ├── workflows.test.ts      # 14 tests
    └── quality.test.ts        # 14 tests
```

---

## 11. Test Requirements

| Module | Tests | Count |
|--------|-------|-------|
| `taxonomy.ts` | 5 roles unique, correct extends/access/autonomy. 8 task types unique, correct workflow mapping. Lookup functions. Query/report/analyze share workflow. Validator and insights are observers. | 13 |
| `tools/analytics-tools.ts` | 10 definitions unique, valid schemas. Required fields present. SQL safety detection works. Destructive SQL flagged. | 11 |
| `profiles/profiles.ts` | 5 profiles with non-empty prompts. Tool whitelist matches role. Analyst has sql_query. Modeler has metric_define. Reporter has report_generate. Validator and insights are autonomous. | 11 |
| `workflows/workflows.ts` | 4 workflows unique, correct stage counts. Dependency order correct. Report stage gated in query-and-report. Build stage gated in dashboard. DAG validation passes. Missing dependency caught. Task type → workflow mapping complete. | 14 |
| `quality/quality.ts` | 6 dimensions unique. All-pass → pass. Reconciliation mismatch → fail. Small mismatch → warn. Stale data → fail. Aging data → warn. Missing query hash → fail. Missing snapshot ID → fail. Missing sections → fail. No citations → fail. Slow query → warn or fail. | 14 |
| **Total** | | **63** |

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| SWE vertical spec | §1–12 | §1 | Pattern template — structure, execution model |
| DevOps vertical spec | §9 | §9 | Environment-scaled gating pattern — adapted to SQL safety gating |
| CLI agent spec | §6–7 | §7 | Workflow execution, stage agent loop |
| Coordinator SDK spec | §3–5 | §7 | SubmitGoal, Decompose, Dispatch RPCs |
| Agent SDK v2 spec | §3 | §7 | Bind, EmitSignal, CreateCheckpoint |
| Runtime spec | §3 (process model) | §7 | Workspace lifecycle, trail recording |
| Tool framework spec | §3 | §4 | ToolDefinition schema |
| Local SDK spec | §4 (autonomy) | §9 | Gate policies, trust surface |
| IMPLEMENTATION.md | 27F | §1 | Data Analytics vertical design |

---

*WACP ecosystem specification — authored by Akil Abderrahim and Claude Opus 4.6*
