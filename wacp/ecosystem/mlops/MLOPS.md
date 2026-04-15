# WACP Ecosystem: MLOps Vertical

```yaml
id: wacp-eco-mlops
type: ecosystem-spec
status: draft
created: 2026-04-10
lineage: LAYER-MAPPING.md (E3)
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
tags: [wacp, ecosystem, mlops, machine-learning, experiment-tracking, model-deployment, vertical, multi-agent, workflows]
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

This spec defines the MLOps ecosystem vertical — the third domain parameterization of WACP. It answers "how does the platform behave when the task is machine learning operations" — not "how does the platform work" (that's the runtime + middleware).

**What the vertical provides:** Roles (who does what), task types (what kinds of work exist), tools (what capabilities are available), profiles (how each role behaves), workflows (how work decomposes into protocol-level workspaces), quality criteria (how to evaluate output), gate policies (when to ask the human).

**Key constraint — compute budget and reproducibility:** ML training consumes expensive compute. The vertical enforces budget gates before any training launch. Every experiment must be checkpointed with hyperparameters, data hash, and model hash — reproducibility is a protocol-level requirement, not a best practice. Model lineage is tracked through the trail: which data trained which model, which model is deployed where.

**Execution model:** The CLI agent loads the MLOps vertical at boot. When a goal is submitted, the CLI detects the task type, selects the matching workflow, and executes it through the WACP runtime — each stage is a real workspace with its own role profile, tool whitelist, signals, and checkpoints. The vertical defines the decomposition; the protocol enforces it.

---

## 2. Role Taxonomy

Five derived roles, each with a specific concern:

| Role | Extends | Concern | Tool access | Autonomy |
|------|---------|---------|-------------|----------|
| `mlops:researcher` | worker | Design experiments, select architectures, analyze results | Read + experiment | Gated |
| `mlops:trainer` | worker | Launch training runs, manage compute, track experiments | Compute-gated execution | Gated |
| `mlops:evaluator` | observer | Benchmark models, run evaluations, compare metrics | Read + benchmark | Autonomous |
| `mlops:deployer` | worker | Deploy models, manage registry, serve endpoints | Execute + registry | Gated |
| `mlops:monitor` | observer | Monitor model performance, detect drift, track metrics | Read + alert | Autonomous |

**Protocol mapping:** Each role maps to a workspace role at dispatch time. The coordinator creates a workspace with the role's tool whitelist and the profile's system prompt as the directive. The agent binds to the workspace and operates within its permissions.

**Compute context:** Every workspace carries a `compute_budget` tag (GPU-hours or cost ceiling). The trainer role's tools check remaining budget before launching jobs. Budget exhaustion triggers an escalation signal, not a silent failure.

---

## 3. Task Taxonomy

Nine task types with default decomposition:

| Type | Description | Default workflow | Roles involved |
|------|-------------|------------------|----------------|
| `mlops:experiment` | Design and run an experiment | `mlops:experiment` (4 stages) | 3 (researcher, trainer, evaluator) |
| `mlops:train` | Train a model | `mlops:optimize` (3 stages) | 3 (evaluator, researcher, trainer) |
| `mlops:evaluate` | Evaluate model performance | Direct (1 stage) | 1 (evaluator) |
| `mlops:deploy` | Deploy model to serving | `mlops:deploy` (3 stages) | 3 (evaluator, deployer, monitor) |
| `mlops:monitor` | Monitor model performance | Direct (1 stage) | 1 (monitor) |
| `mlops:optimize` | Optimize model/training | `mlops:optimize` (3 stages) | 3 (evaluator, researcher, trainer) |
| `mlops:data-prep` | Prepare/validate datasets | Direct (1 stage) | 1 (researcher) |
| `mlops:reproduce` | Reproduce an experiment | `mlops:reproduce` (2 stages) | 2 (researcher, trainer) |
| `mlops:audit` | Audit model lineage/compliance | Direct (1 stage) | 1 (evaluator) |

**Detection:** The CLI classifies user goals into task types via keyword heuristics: "experiment"/"try" → `mlops:experiment`, "train" → `mlops:train`, "evaluate"/"benchmark" → `mlops:evaluate`, "deploy"/"serve" → `mlops:deploy`, "monitor"/"drift" → `mlops:monitor`, "optimize"/"tune"/"hyperparameter" → `mlops:optimize`, "data"/"dataset"/"prepare" → `mlops:data-prep`, "reproduce"/"replicate" → `mlops:reproduce`, "audit"/"lineage" → `mlops:audit`, default → `mlops:experiment`.

---

## 4. Tool Catalog

MLOps-specific tools beyond the CLI's built-in 7. All 17 tools (7 built-in + 10 MLOps) are registered at boot and filtered per stage by the profile's whitelist.

| Tool | Description | Operation type |
|------|-------------|----------------|
| `dataset_validate` | Validate dataset quality — schema, distributions, missing values, class balance | `data_read` |
| `experiment_track` | Log experiment parameters, metrics, and artifacts to experiment tracker (MLflow, W&B, etc.) | `data_write` |
| `train_launch` | Launch training job (local GPU, cloud instance, cluster). Budget-checked before execution | `compute_exec` |
| `eval_benchmark` | Run model benchmarks — compute metrics (accuracy, F1, BLEU, perplexity, etc.) against test sets | `compute_exec` |
| `model_register` | Register model version in model registry with metadata, metrics, and lineage | `registry_write` |
| `model_deploy` | Deploy registered model to serving endpoint (REST, gRPC, batch) | `infra_exec` |
| `drift_detect` | Detect data drift or model drift by comparing current distributions to training baseline | `data_read` |
| `compute_budget` | Check remaining compute budget (GPU-hours, cost). Returns budget status and utilization | `budget_read` |
| `reproduce_check` | Verify experiment reproducibility — compare hyperparameters, data hash, metrics within tolerance | `data_read` |
| `data_lineage` | Query data and model lineage graph — which data trained which model, deployed where | `data_read` |

Tool executors auto-detect the project's ML toolchain (`mlflow`/`wandb` for tracking, `pytorch`/`tensorflow`/`jax` for training, `triton`/`seldon`/`bentoml` for serving).

**Budget enforcement:** `train_launch` calls `compute_budget` internally before execution. If remaining budget is insufficient for the estimated job cost, the tool returns a `COMPUTE_BUDGET_EXCEEDED` error and emits an escalation signal.

---

## 5. Agent Profiles

One profile per role. Each profile provides: system prompt, tool whitelist, autonomy level.

**`mlops:researcher`** — Read + experiment. System prompt instructs: analyze the problem, review existing experiments, design experiment with clear hypothesis, select architecture and hyperparameter search space, prepare data pipeline, document decisions. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `code_search`, `dataset_validate`, `experiment_track`, `data_lineage`, `git_status`, `git_diff`.

**`mlops:trainer`** — Compute-gated execution. System prompt instructs: check compute budget before any training, launch training with experiment tracking enabled, checkpoint regularly, log all hyperparameters and metrics, stop early if budget threshold reached, register successful models. Tools: `read_file`, `list_dir`, `search_files`, `train_launch`, `experiment_track`, `compute_budget`, `model_register`, `reproduce_check`, `git_status`.

**`mlops:evaluator`** — Read + benchmark, autonomous. System prompt instructs: evaluate model against target metrics, run benchmarks on standard test sets, compare with baseline, check for overfitting/data leakage, produce evaluation report with metrics table, pass/fail verdict, and confidence intervals. Tools: `read_file`, `list_dir`, `search_files`, `eval_benchmark`, `experiment_track`, `dataset_validate`, `reproduce_check`, `data_lineage`.

**`mlops:deployer`** — Execute + registry. System prompt instructs: verify model is registered and evaluated, deploy to serving endpoint, configure autoscaling and health checks, validate serving endpoint responds correctly, roll back on serving errors. Tools: `read_file`, `list_dir`, `search_files`, `model_register`, `model_deploy`, `eval_benchmark`, `compute_budget`, `git_status`.

**`mlops:monitor`** — Read + alert, autonomous. System prompt instructs: monitor model serving metrics (latency, throughput, error rate), detect data drift and model drift, compare current performance to training baseline, report degradation with evidence, recommend retraining when drift exceeds threshold. Tools: `read_file`, `list_dir`, `search_files`, `drift_detect`, `eval_benchmark`, `data_lineage`, `experiment_track`.

---

## 6. Workflows

Four workflow DAGs plus direct task types. Each defines stages with role assignments, dependencies, and gate policies.

**`mlops:experiment`** (4 stages):
```
design (researcher) ──→ data-prep (researcher) ──→ train (trainer) ──→ evaluate (evaluator)
                                                       [gated]
```
Used by: `mlops:experiment`. Design selects architecture and hyperparameters. Data-prep validates and prepares the dataset. Train is budget-gated — human approves compute commitment. Evaluate benchmarks the result.

**`mlops:deploy`** (3 stages):
```
validate (evaluator) ──→ deploy (deployer) ──→ monitor (monitor)
                            [gated]
```
Used by: `mlops:deploy`. Validate confirms the model meets deployment criteria. Deploy is gated — human approves model promotion. Monitor verifies serving health and baseline performance.

**`mlops:reproduce`** (2 stages):
```
verify (researcher) ──→ execute (trainer)
```
Used by: `mlops:reproduce`. Verify checks that all experiment artifacts (data hash, hyperparameters, code version) are available. Execute re-runs the experiment and compares metrics within tolerance.

**`mlops:optimize`** (3 stages):
```
analyze (evaluator) ──→ propose (researcher) ──→ train (trainer)
                                                    [gated]
```
Used by: `mlops:optimize`, `mlops:train`. Analyze identifies bottlenecks or improvement opportunities. Propose designs optimization strategy (hyperparameter tuning, architecture search, data augmentation). Train is budget-gated.

**Direct execution:** `mlops:evaluate` (evaluator), `mlops:monitor` (monitor), `mlops:data-prep` (researcher), `mlops:audit` (evaluator).

**DAG validation:** `validateWorkflow()` checks that all dependencies exist and no cycles are present.

---

## 7. Execution Model

The MLOps vertical executes through the WACP protocol — not as a simulation.

**Goal submission:**
```
1. User submits goal: "train a sentiment classifier on the reviews dataset"
2. CLI detects task type: mlops:experiment
3. CLI selects workflow: mlops:experiment (4 stages)
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
      - Budget check (for compute tools)
      - Execute tool via LocalResources
      - AgentService.CreateCheckpoint(observation, tool result)
   d. Feed tool results back to LLM
5. AgentService.CreateCheckpoint(artifact, FINAL, stage output)
6. AgentService.EmitSignal(COMPLETE)
7. Stage output flows as context to next stage
```

**Budget enforcement:** The compute budget propagates from root workspace. `train_launch` checks remaining budget before execution. If insufficient, it emits `BLOCKED` signal with budget details. The coordinator escalates to human for budget increase or cancellation.

**Reproducibility contract:** Every training stage must checkpoint: hyperparameters (JSON), data hash (SHA-256), code version (git SHA), model hash (SHA-256), random seeds. The `reproduce_check` tool verifies these against a prior experiment's checkpoints.

**Model lineage:** The trail records the full lineage chain: dataset → experiment → training run → model version → deployment. `data_lineage` queries this chain from the trail.

---

## 8. Quality Criteria

Six dimensions for evaluating MLOps output:

| Dimension | Definition | Evaluation |
|-----------|-----------|------------|
| **Metric performance** | Model meets target metrics | Benchmark results vs. target thresholds |
| **Reproducibility** | Experiment can be reproduced within tolerance | `reproduce_check` result |
| **Data quality** | Dataset passes validation | `dataset_validate` result |
| **Compute efficiency** | Training within budget | Budget utilization ratio |
| **Model freshness** | Model not stale | Days since last training vs. threshold |
| **Documentation** | Experiment logged, model card present | Experiment tracking completeness |

**Evaluation function:** Each dimension returns `pass`, `warn`, or `fail`. Rules:
- Target metric not met → `metric_performance` = `fail`
- Metric improved but below target → `metric_performance` = `warn`
- Not reproducible → `reproducibility` = `fail`
- Dataset validation failed → `data_quality` = `fail`
- Budget exceeded → `compute_efficiency` = `fail`
- Budget utilization > 80% → `compute_efficiency` = `warn`
- Model age > freshness threshold → `model_freshness` = `fail`
- Model age > 75% of threshold → `model_freshness` = `warn`
- Experiment not logged → `documentation` = `fail`
- Model card missing → `documentation` = `warn`

Overall: `pass` if all pass, `warn` if any warn and none fail, `fail` if any fail.

---

## 9. Gate Policies

### Compute-Budget Gating

The defining constraint of the MLOps vertical. Training jobs cost compute resources. Gates enforce budget awareness.

| Transition | Gate | Rationale |
|-----------|------|-----------|
| Goal → design | None | Design is read-only |
| Design → data-prep | None | Data preparation is read-only |
| Data-prep → train | **Human approval** | Compute budget commitment — human reviews estimated cost |
| Train → evaluate | Auto on completion | Evaluation is read-only |
| Validate → deploy | **Human approval** | Model promotion to production serving |
| Deploy → monitor | Auto on completion | Monitoring is read-only |
| Analyze → propose | None | Proposal is read-only |
| Propose → train | **Human approval** | Compute budget commitment |
| Verify → execute | None | Reproduction runs under the original experiment's budget |

---

## 10. Package Structure

```
ecosystem/mlops/
├── MLOPS.md               # This spec
├── package.json           # @wacp/mlops
├── tsconfig.json
├── src/
│   ├── index.ts           # Public exports
│   ├── taxonomy.ts        # 5 roles + 9 task types with lookup functions
│   ├── tools/
│   │   └── mlops-tools.ts     # 10 tool definitions + executors (auto-detect toolchain)
│   ├── profiles/
│   │   └── profiles.ts        # 5 profiles with system prompts + tool whitelists
│   ├── workflows/
│   │   └── workflows.ts       # 4 workflow DAGs + validation (topological sort, cycle detection)
│   └── quality/
│       └── quality.ts         # 6 dimensions + evaluateQuality() → QualityReport
└── tests/
    ├── taxonomy.test.ts       # 14 tests
    ├── tools.test.ts          # 10 tests
    ├── profiles.test.ts       # 11 tests
    ├── workflows.test.ts      # 14 tests
    └── quality.test.ts        # 14 tests
```

---

## 11. Test Requirements

| Module | Tests | Count |
|--------|-------|-------|
| `taxonomy.ts` | 5 roles unique, correct extends/access/autonomy. 9 task types unique, correct workflow mapping. Lookup functions. Compute-gated roles identified. Train and optimize share workflow. | 14 |
| `tools/mlops-tools.ts` | 10 definitions unique, valid schemas. Required fields present. Budget-aware tools identified. Compute tools require parameters. | 10 |
| `profiles/profiles.ts` | 5 profiles with non-empty prompts. Tool whitelist matches role. Trainer has compute_budget. Evaluator is autonomous. Monitor is autonomous. Workers are gated. | 11 |
| `workflows/workflows.ts` | 4 workflows unique, correct stage counts. Dependency order correct. Train stages are gated. Deploy stage is gated. DAG validation passes. Missing dependency caught. Task type → workflow mapping complete. | 14 |
| `quality/quality.ts` | 6 dimensions unique. All-pass → pass. Metric not met → fail. Not reproducible → fail. Data validation fail → fail. Budget exceeded → fail. Budget high → warn. Model stale → fail. Model aging → warn. No experiment log → fail. No model card → warn. | 14 |
| **Total** | | **63** |

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| SWE vertical spec | §1–12 | §1 | Pattern template — structure, execution model |
| DevOps vertical spec | §9 | §9 | Environment-scaled gating pattern — adapted to compute-budget gating |
| CLI agent spec | §6–7 | §7 | Workflow execution, stage agent loop |
| Coordinator SDK spec | §3–5 | §7 | SubmitGoal, Decompose, Dispatch RPCs |
| Agent SDK v2 spec | §3 | §7 | Bind, EmitSignal, CreateCheckpoint |
| Runtime spec | §3 (process model) | §7 | Workspace lifecycle, trail recording |
| Tool framework spec | §3 | §4 | ToolDefinition schema |
| Local SDK spec | §4 (autonomy) | §9 | Gate policies, trust surface |
| LAYER-MAPPING.md | E3 | §1 | MLOps vertical design, role/task enumeration |

---

*WACP ecosystem specification — authored by Akil Abderrahim and Claude Opus 4.6*
