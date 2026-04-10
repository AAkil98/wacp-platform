# WACP Ecosystem: Data Science Vertical

```yaml
id: wacp-eco-datasci
type: ecosystem-spec
status: draft
created: 2026-04-10
lineage: IMPLEMENTATION.md (27G)
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
tags: [wacp, ecosystem, datasci, statistics, hypothesis-testing, inference, vertical, multi-agent, workflows]
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

This spec defines the Data Science ecosystem vertical. It answers "how does the platform behave when the task is exploratory analysis, statistical inference, or hypothesis testing" — not "how does the platform work" (that's the runtime + middleware).

**What the vertical provides:** Roles (who does what), task types (what kinds of work exist), tools (what capabilities are available), profiles (how each role behaves), workflows (how work decomposes into protocol-level workspaces), quality criteria (how to evaluate output), gate policies (when to ask the human).

**Key constraint — statistical rigor:** Every hypothesis test must declare its alternative hypothesis, significance level, and multiple-testing correction strategy **before execution**. Post-hoc hypothesis declaration is blocked by the tool layer — the declaration must be a checkpoint that precedes the test execution checkpoint. All point estimates must be accompanied by confidence intervals. Parametric tests require prior assumption checks (normality, independence, homoscedasticity). This is distinct from MLOps: here, the output is statistical inference, not predictive models.

**Execution model:** The CLI agent loads the Data Science vertical at boot. When a goal is submitted, the CLI detects the task type, selects the matching workflow, and executes it through the WACP runtime — each stage is a real workspace with its own role profile, tool whitelist, signals, and checkpoints. The vertical defines the decomposition; the protocol enforces it.

---

## 2. Role Taxonomy

Five derived roles, each with a specific concern:

| Role | Extends | Concern | Tool access | Autonomy |
|------|---------|---------|-------------|----------|
| `datasci:explorer` | worker | Exploratory data analysis, statistical summaries, visualization | Read + summary | Gated |
| `datasci:statistician` | worker | Hypothesis testing, statistical inference, causal reasoning | Test + inference | Gated |
| `datasci:feature_engineer` | worker | Feature extraction, transformation, selection | Extract + transform | Gated |
| `datasci:modeler` | worker | Fit statistical models (regression, GLM, GAM), diagnose fit | Fit + diagnostics | Gated |
| `datasci:reviewer` | observer | Peer review, methodology check, reproducibility audit | Read-only | Autonomous |

**Protocol mapping:** Each role maps to a workspace role at dispatch time. The coordinator creates a workspace with the role's tool whitelist and the profile's system prompt as the directive. The agent binds to the workspace and operates within its permissions.

**Rigor context:** Every workspace in a hypothesis-testing workflow carries a `declared_hypothesis` checkpoint that must precede any `hypothesis_test` tool call. The tool refuses execution if no prior declaration exists in the trail.

---

## 3. Task Taxonomy

Nine task types with default decomposition:

| Type | Description | Default workflow | Roles involved |
|------|-------------|------------------|----------------|
| `datasci:explore` | Exploratory data analysis | `datasci:exploration` (2 stages) | 2 (explorer, reviewer) |
| `datasci:hypothesize` | Formulate a hypothesis | `datasci:hypothesis-test` (3 stages) | 2 (statistician, reviewer) |
| `datasci:test` | Run a hypothesis test | `datasci:hypothesis-test` (3 stages) | 2 (statistician, reviewer) |
| `datasci:model` | Build a statistical model | `datasci:model-build` (4 stages) | 4 (explorer, feature_engineer, modeler, reviewer) |
| `datasci:feature` | Feature engineering | Direct (1 stage) | 1 (feature_engineer) |
| `datasci:validate` | Validate a model or inference | Direct (1 stage) | 1 (reviewer) |
| `datasci:interpret` | Interpret statistical results | Direct (1 stage) | 1 (statistician) |
| `datasci:report` | Report findings | `datasci:full-analysis` (5 stages) | 3 (explorer, statistician, reviewer) |
| `datasci:review` | Peer review an analysis | Direct (1 stage) | 1 (reviewer) |

**Detection:** The CLI classifies user goals into task types via keyword heuristics: "explore"/"EDA" → `datasci:explore`, "hypothesis"/"test if" → `datasci:hypothesize`, "regression"/"fit" → `datasci:model`, "feature" → `datasci:feature`, "interpret"/"mean" → `datasci:interpret`, "review" → `datasci:review`, "report"/"findings" → `datasci:report`, default → `datasci:explore`.

---

## 4. Tool Catalog

Data Science-specific tools beyond the CLI's built-in 7. All 17 tools (7 built-in + 10 datasci) are registered at boot and filtered per stage by the profile's whitelist.

| Tool | Description | Operation type |
|------|-------------|----------------|
| `stat_summary` | Compute statistical summary — mean, median, std, quartiles, skew, kurtosis | `data_read` |
| `correlation_analysis` | Correlation matrix with significance tests, handling ties and missing data | `data_read` |
| `hypothesis_test` | Run a statistical test — t-test, chi-square, ANOVA, Mann-Whitney, etc. Requires prior declaration. | `data_read` |
| `feature_extract` | Extract features from raw data — text, time-series, image features | `data_read` |
| `feature_transform` | Transform features — scale, encode, impute, reduce dimensionality | `data_write` |
| `model_fit` | Fit a statistical model — linear, logistic, GLM, GAM, mixed-effects, survival | `compute_exec` |
| `diagnostic_plots` | Generate diagnostic plots — residuals, Q-Q, influence, leverage, ACF | `file_write` |
| `bootstrap_sample` | Bootstrap resampling for confidence intervals and inference | `compute_exec` |
| `causal_inference` | Causal inference — DAG, propensity score, instrumental variables, difference-in-differences | `compute_exec` |
| `interpretation` | Generate interpretation of statistical results in plain language with caveats | `data_read` |

Tool executors auto-detect the project's statistical toolchain (`R` for classical stats, `statsmodels`/`scipy` for Python, `stan`/`pymc` for Bayesian).

**Rigor enforcement:** `hypothesis_test` checks the trail for a prior `declared_hypothesis` checkpoint. If absent, it refuses execution with `HYPOTHESIS_NOT_DECLARED`. The declaration must specify: null hypothesis, alternative hypothesis, significance level (alpha), and multiple-testing correction if more than one test is planned.

---

## 5. Agent Profiles

One profile per role. Each profile provides: system prompt, tool whitelist, autonomy level.

**`datasci:explorer`** — Read + summary. System prompt instructs: start with univariate summaries (distributions, missingness, outliers), then bivariate (correlations, cross-tabs), then multivariate. Visualize before testing. Flag data quality issues. Never run hypothesis tests during exploration — that's a separate stage. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `stat_summary`, `correlation_analysis`, `diagnostic_plots`, `git_status`.

**`datasci:statistician`** — Test + inference. System prompt instructs: declare the hypothesis before running any test — null, alternative, significance level, correction strategy. Check assumptions before parametric tests. Report effect size, confidence interval, and p-value together. Interpret results in the context of the research question, not in isolation. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `hypothesis_test`, `bootstrap_sample`, `causal_inference`, `interpretation`, `stat_summary`.

**`datasci:feature_engineer`** — Extract + transform. System prompt instructs: understand the feature's meaning before transforming it. Document every transformation with rationale. Avoid data leakage — use only training data for fitting transformers. Check distributions before and after transformation. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `feature_extract`, `feature_transform`, `stat_summary`, `correlation_analysis`, `git_status`.

**`datasci:modeler`** — Fit + diagnostics. System prompt instructs: start with the simplest reasonable model. Check residuals and diagnostic plots after fitting. Report model fit statistics (R², AIC, BIC, log-likelihood). Test for violated assumptions (heteroscedasticity, autocorrelation, multicollinearity). Never report point estimates without confidence intervals. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `model_fit`, `diagnostic_plots`, `bootstrap_sample`, `stat_summary`, `interpretation`.

**`datasci:reviewer`** — Read-only, autonomous. System prompt instructs: review methodology first, then results. Check if hypotheses were declared before tests. Check for multiple testing correction. Check for assumption violations. Check if effect sizes were reported. Verify the analysis is reproducible from the recorded artifacts. Produce a review with pass/warn/fail verdict per dimension. Tools: `read_file`, `list_dir`, `search_files`, `stat_summary`, `diagnostic_plots`, `interpretation`.

---

## 6. Workflows

Four workflow DAGs plus direct task types. Each defines stages with role assignments, dependencies, and gate policies.

**`datasci:full-analysis`** (5 stages):
```
explore (explorer) ──→ hypothesize (statistician) ──→ test (statistician) ──→ interpret (statistician) ──→ review (reviewer)
                                                                                                              [gated]
```
Used by: `datasci:report`. Full end-to-end analysis: explore data, formulate hypothesis, test, interpret results, peer review before final sign-off.

**`datasci:hypothesis-test`** (3 stages):
```
declare (statistician) ──→ test (statistician) ──→ interpret (statistician)
                              [gated]
```
Used by: `datasci:hypothesize`, `datasci:test`. Declaration stage produces the hypothesis declaration checkpoint. Test stage is gated — human confirms the declaration is correct before running. Interpret stage translates results.

**`datasci:model-build`** (4 stages):
```
explore (explorer) ──→ feature (feature_engineer) ──→ fit (modeler) ──→ validate (reviewer)
                                                         [gated]
```
Used by: `datasci:model`. Explore understands the data. Feature engineering prepares inputs. Fit is gated — human approves model choice before fitting. Validate checks fit diagnostics and assumption compliance.

**`datasci:exploration`** (2 stages):
```
profile (explorer) ──→ visualize (explorer)
```
Used by: `datasci:explore`. Profile computes summaries. Visualize produces charts. No gates — exploration is read-only and exploratory.

**Direct execution:** `datasci:feature` (feature_engineer), `datasci:validate` (reviewer), `datasci:interpret` (statistician), `datasci:review` (reviewer).

**DAG validation:** `validateWorkflow()` checks that all dependencies exist and no cycles are present.

---

## 7. Execution Model

The Data Science vertical executes through the WACP protocol — not as a simulation.

**Goal submission:**
```
1. User submits goal: "test if the new onboarding flow improves day-7 retention"
2. CLI detects task type: datasci:test
3. CLI selects workflow: datasci:hypothesis-test (3 stages)
4. CoordinatorService.SubmitGoal → runtime creates root workspace
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
      - Rigor check (for hypothesis_test: verify prior declaration exists)
      - Execute tool via LocalResources
      - AgentService.CreateCheckpoint(observation, tool result)
   d. Feed tool results back to LLM
5. AgentService.CreateCheckpoint(artifact, FINAL, stage output)
6. AgentService.EmitSignal(COMPLETE)
7. Stage output flows as context to next stage
```

**Hypothesis declaration contract:** The declare stage in `datasci:hypothesis-test` produces a checkpoint with fields: `null_hypothesis`, `alternative_hypothesis`, `alpha`, `test_type`, `multiple_testing_correction`. The `hypothesis_test` tool reads the most recent `declared_hypothesis` checkpoint from the current workspace and rejects execution if the declaration is missing or malformed.

**Confidence interval contract:** Every point estimate recorded as a checkpoint must include a `ci_lower` and `ci_upper` field. The quality evaluator fails checkpoints that report estimates without intervals.

**Reproducibility through trail:** The trail records: data snapshot hash, code version, random seed, library versions. A reviewer can re-execute any analysis from its trail alone.

---

## 8. Quality Criteria

Six dimensions for evaluating Data Science output:

| Dimension | Definition | Evaluation |
|-----------|-----------|------------|
| **Statistical rigor** | Hypotheses declared, corrections applied, assumptions checked | Declaration + correction + assumption checks present |
| **Reproducibility** | Analysis re-executable from recorded artifacts | Snapshot + seed + code version recorded |
| **Interpretation validity** | Interpretation matches the actual results | No overreach beyond what the data supports |
| **Assumptions checked** | Parametric assumptions verified before tests | Assumption check checkpoint present |
| **Effect size** | Effect sizes reported alongside p-values | Effect size present in test checkpoints |
| **Documentation** | Methodology documented, decisions justified | Methodology checkpoint present |

**Evaluation function:** Each dimension returns `pass`, `warn`, or `fail`. Rules:
- Hypothesis not declared before test → `statistical_rigor` = `fail`
- Multiple tests without correction → `statistical_rigor` = `fail`
- Hypothesis declared but correction not specified for k>1 tests → `statistical_rigor` = `warn`
- Snapshot hash missing → `reproducibility` = `fail`
- Random seed missing → `reproducibility` = `warn`
- Interpretation contains causal language for observational data → `interpretation_validity` = `fail`
- Interpretation overreaches beyond CI → `interpretation_validity` = `warn`
- Assumption checks missing for parametric test → `assumptions_checked` = `fail`
- Assumption violations present but unaddressed → `assumptions_checked` = `fail`
- Assumption violations documented with robustness discussion → `assumptions_checked` = `warn`
- Effect size missing → `effect_size` = `fail`
- Confidence interval missing → `effect_size` = `fail`
- Methodology undocumented → `documentation` = `fail`

Overall: `pass` if all pass, `warn` if any warn and none fail, `fail` if any fail.

---

## 9. Gate Policies

### Rigor Gating

The defining constraint of the Data Science vertical. Gates enforce the scientific method — hypothesis before test, assumption check before parametric inference.

| Transition | Gate | Rationale |
|-----------|------|-----------|
| Goal → explore | None | Exploration is read-only |
| Explore → hypothesize | None | Hypothesis formulation is read-only |
| Declare → test | **Human approval** | Confirm hypothesis is correct before execution — prevents post-hoc fishing |
| Test → interpret | Auto | Interpretation is read-only |
| Interpret → review | None | Review is observational |
| Review → publish | **Human approval** | Final sign-off on statistical claims |
| Explore → feature | None | Feature engineering is transformation |
| Feature → fit | **Human approval** | Model choice commitment |
| Fit → validate | Auto | Validation is observational |
| Any stage → hypothesis_test tool | **Rigor check** | Tool refuses without prior declaration checkpoint |

---

## 10. Package Structure

```
ecosystem/datasci/
├── DATASCI.md             # This spec
├── package.json           # @wacp/datasci
├── tsconfig.json
├── src/
│   ├── index.ts           # Public exports
│   ├── taxonomy.ts        # 5 roles + 9 task types with lookup functions
│   ├── tools/
│   │   └── datasci-tools.ts   # 10 tool definitions + executors (rigor enforcement)
│   ├── profiles/
│   │   └── profiles.ts        # 5 profiles with system prompts + tool whitelists
│   ├── workflows/
│   │   └── workflows.ts       # 4 workflow DAGs + validation
│   └── quality/
│       └── quality.ts         # 6 dimensions + evaluateQuality() → QualityReport
└── tests/
    ├── taxonomy.test.ts       # 14 tests
    ├── tools.test.ts          # 12 tests
    ├── profiles.test.ts       # 12 tests
    ├── workflows.test.ts      # 15 tests
    └── quality.test.ts        # 15 tests
```

---

## 11. Test Requirements

| Module | Tests | Count |
|--------|-------|-------|
| `taxonomy.ts` | 5 roles unique, correct extends/access/autonomy. 9 task types unique, correct workflow mapping. Lookup functions. Reviewer is observer. Hypothesize/test share workflow. | 14 |
| `tools/datasci-tools.ts` | 10 definitions unique, valid schemas. Required fields present. Hypothesis validation logic works. | 12 |
| `profiles/profiles.ts` | 5 profiles with non-empty prompts. Tool whitelist matches role. Statistician has hypothesis_test. Modeler has model_fit. Reviewer is read-only and autonomous. Workers are gated. | 12 |
| `workflows/workflows.ts` | 4 workflows unique, correct stage counts. Dependency order correct. Test stage gated in hypothesis-test. Fit stage gated in model-build. DAG validation passes. Task type mapping complete. | 15 |
| `quality/quality.ts` | 6 dimensions unique. All-pass → pass. No declaration → rigor fail. No correction → warn. No snapshot → reproducibility fail. No seed → warn. Causal claim on observational → interpretation fail. No effect size → fail. No CI → fail. No assumption checks → fail. No methodology → fail. | 15 |
| **Total** | | **68** |

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| SWE vertical spec | §1–12 | §1 | Pattern template — structure, execution model |
| MLOps vertical spec | §1 | §1 | Distinction: MLOps produces models, datasci produces inference |
| Analytics vertical spec | §1 | §1 | Distinction: analytics produces reports, datasci produces inference with uncertainty |
| CLI agent spec | §6–7 | §7 | Workflow execution, stage agent loop |
| Coordinator SDK spec | §3–5 | §7 | SubmitGoal, Decompose, Dispatch RPCs |
| Agent SDK v2 spec | §3 | §7 | Bind, EmitSignal, CreateCheckpoint |
| Runtime spec | §3 (process model) | §7 | Workspace lifecycle, trail recording |
| Tool framework spec | §3 | §4 | ToolDefinition schema |
| IMPLEMENTATION.md | 27G | §1 | Data Science vertical design |

---

*WACP ecosystem specification — authored by Akil Abderrahim and Claude Opus 4.6*
