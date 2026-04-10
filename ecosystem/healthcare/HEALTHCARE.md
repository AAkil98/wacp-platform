# WACP Ecosystem: Healthcare Vertical

```yaml
id: wacp-eco-healthcare
type: ecosystem-spec
status: draft
created: 2026-04-10
lineage: IMPLEMENTATION.md (27D)
depends_on:
  - wacp-impl-cli-agent
  - wacp-impl-tool-framework
  - wacp-impl-local-sdk
  - wacp-impl-coordinator-sdk
  - wacp-impl-runtime
  - wacp-impl-security
  - wacp-eco-swe
authors:
  - Akil Abderrahim (Lead)
  - Claude Opus 4.6 (co-author)
tags: [wacp, ecosystem, healthcare, hipaa, phi, clinical, vertical, multi-agent, workflows]
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

This spec defines the Healthcare ecosystem vertical. It answers "how does the platform behave when the task is clinical assessment, patient education, biomedical research, or any work that touches Protected Health Information" — not "how does the platform work" (that's the runtime + middleware).

**What the vertical provides:** Roles (who does what), task types (what kinds of work exist), tools (what capabilities are available), profiles (how each role behaves), workflows (how work decomposes into protocol-level workspaces), quality criteria (how to evaluate output), gate policies (when to ask the human).

**Key constraint — PHI access requires a basis:** Every tool that operates on identifiable patient data refuses execution unless the caller supplies a valid `phi_access_grant` derived from one of two upstream checkpoints: (1) a `consent_verify` checkpoint proving the patient consented to this specific use, OR (2) a `de_identify` checkpoint proving the data was de-identified per HIPAA Safe Harbor or expert determination. Grants are time-bounded — default 15 minutes — and scoped to a specific patient (consent basis) or dataset hash (de-identification basis). The `clinical_report_generate`, `lab_interpret`, and `risk_score` tools enforce this contract at the tool layer; there is no way for an agent to bypass it. Additionally, every workflow that produces patient-facing output is gated for clinician sign-off — no agent can release clinical content autonomously. This is distinct from Finance (regulatory pre-check) and Data Science (hypothesis declaration): here, the constraint is **statutory** — the 18 HIPAA identifiers are the boundary, and crossing it without a basis is a federal violation, not a quality concern.

**Execution model:** The CLI agent loads the Healthcare vertical at boot. When a goal is submitted, the CLI detects the task type, selects the matching workflow, and executes it through the WACP runtime — each stage is a real workspace with its own role profile, tool whitelist, signals, and checkpoints. The vertical defines the decomposition; the protocol enforces it.

---

## 2. Role Taxonomy

Five derived roles, each with a specific concern:

| Role | Extends | Concern | Tool access | Autonomy |
|------|---------|---------|-------------|----------|
| `health:clinician` | worker | Patient assessment, clinical decision support, documentation | Clinical | Gated |
| `health:researcher` | worker | Literature review, evidence synthesis, study design | Search + de-identify | Gated |
| `health:analyst` | worker | Cohort analysis on de-identified data | De-identified analytics | Gated |
| `health:compliance` | observer | PHI audit, consent verification, regulatory adherence | Read + audit | Autonomous |
| `health:coordinator` | worker | Care coordination, patient education, workflow scheduling | Coordination + education | Gated |

**Protocol mapping:** Each role maps to a workspace role at dispatch time. The coordinator creates a workspace with the role's tool whitelist and the profile's system prompt as the directive. The agent binds to the workspace and operates within its permissions.

**PHI context:** The clinician role is the only role authorized to call clinical tools on identifiable patient data, and only after a `consent_verify` checkpoint. The researcher and analyst roles are restricted to de-identified data — their workflows produce a `de_identify` checkpoint before any analytical tool is invoked. The compliance role is read-only and autonomous — it audits the trail and produces compliance reports without ever modifying records.

---

## 3. Task Taxonomy

Eight task types with default decomposition:

| Type | Description | Default workflow | Roles involved |
|------|-------------|------------------|----------------|
| `health:assess` | Patient assessment from intake to documented plan | `health:patient-assessment` (4 stages) | 1 (clinician) |
| `health:diagnose_support` | Differential diagnosis support | `health:patient-assessment` (4 stages) | 1 (clinician) |
| `health:research` | Literature review and evidence synthesis | `health:literature-research` (4 stages) | 2 (researcher, compliance) |
| `health:analyze` | Cohort analysis on de-identified data | Direct (1 stage) | 1 (analyst) |
| `health:monitor` | Ongoing patient monitoring | Direct (1 stage) | 1 (clinician) |
| `health:report` | Patient-facing or clinician-facing report | `health:patient-assessment` (4 stages) | 1 (clinician) |
| `health:audit` | PHI audit, compliance review | `health:phi-audit` (2 stages) | 1 (compliance) |
| `health:educate` | Patient education materials | `health:patient-education` (2 stages) | 1 (coordinator) |

**Detection:** The CLI classifies user goals into task types via keyword heuristics: "assess"/"intake"/"history" → `health:assess`, "diagnose"/"differential"/"ddx" → `health:diagnose_support`, "literature"/"evidence"/"systematic review" → `health:research`, "cohort"/"population"/"de-identified" → `health:analyze`, "monitor"/"follow-up"/"vital signs" → `health:monitor`, "report"/"chart note"/"discharge" → `health:report`, "audit"/"hipaa"/"phi" → `health:audit`, "education"/"teach"/"explain to patient" → `health:educate`, default → `health:assess`.

---

## 4. Tool Catalog

Healthcare-specific tools beyond the CLI's built-in 7. All 17 tools (7 built-in + 10 healthcare) are registered at boot and filtered per stage by the profile's whitelist.

| Tool | Description | Operation type |
|------|-------------|----------------|
| `clinical_search` | Search clinical guidelines, protocols, literature (PubMed, UpToDate, Cochrane). No PHI. | `data_read` |
| `protocol_lookup` | Look up a specific clinical protocol — sepsis bundle, stroke pathway, ARDSnet, etc. No PHI. | `data_read` |
| `lab_interpret` | Interpret lab results in clinical context with reference ranges. **Requires PHI access grant.** | `data_read` |
| `risk_score` | Compute clinical risk scores — CHADS2-VASc, ASCVD, CURB-65, MELD, APACHE, etc. **Requires PHI access grant.** | `compute_exec` |
| `phi_filter` | Detect PHI in text against the 18 HIPAA identifiers and report or redact. Always allowed. | `data_read` |
| `consent_verify` | Verify patient consent for a specific use. Produces a `consent` PHI access grant. | `data_read` |
| `de_identify` | De-identify clinical data per HIPAA Safe Harbor (remove 18 identifiers) or expert determination. Produces a `de_identified` PHI access grant. | `data_write` |
| `clinical_report_generate` | Generate a clinical report — H&P, progress note, discharge summary. **Requires PHI access grant.** | `file_write` |
| `audit_export` | Export the audit trail with PHI access metadata for compliance review. Read-only. | `data_read` |
| `education_material` | Generate patient education at a specified reading level (default 6th grade). | `file_write` |

Tool executors auto-detect the project's clinical knowledge source (UpToDate, DynaMed, public guidelines) and the regulatory regime (HIPAA, GDPR, PIPEDA) from the workspace config.

**PHI access enforcement:** `clinical_report_generate`, `lab_interpret`, and `risk_score` check for a `phi_access_grant` argument and validate it. The grant must:
- Have `basis` of `"consent"` or `"de_identified"`
- For `consent`: include `patient_id`, `consent_id`, and `consent_scope` covering the proposed use
- For `de_identified`: include `deidentification_method` (`safe_harbor` or `expert_determination`) and `deidentified_data_hash`
- Have `expires_at` in the future (default grant TTL: 15 minutes)

If the grant is missing, malformed, expired, or its scope does not cover the proposed use, the tool refuses with `PHI_ACCESS_NOT_GRANTED`.

**HIPAA Safe Harbor:** The 18 identifiers screened by `phi_filter` and removed by `de_identify` are: name, geographic subdivisions smaller than state, all date elements (except year) for dates directly related to an individual, telephone numbers, fax numbers, email addresses, SSN, MRN, health plan beneficiary numbers, account numbers, certificate/license numbers, vehicle identifiers, device identifiers, web URLs, IP addresses, biometric identifiers, full-face photos, and any other unique identifying number or characteristic.

---

## 5. Agent Profiles

One profile per role. Each profile provides: system prompt, tool whitelist, autonomy level.

**`health:clinician`** — Clinical. System prompt instructs: verify consent before touching patient data. Always cite the clinical guideline you are following. Distinguish recommendation from order. Never release patient-facing content without human sign-off — the workflow gate is not a suggestion. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `clinical_search`, `protocol_lookup`, `lab_interpret`, `risk_score`, `consent_verify`, `phi_filter`, `clinical_report_generate`.

**`health:researcher`** — Search + de-identify. System prompt instructs: work on de-identified data only. Run de_identify before any analysis. Cite every source with PMID or DOI. Grade evidence (1A through 5) when summarizing. Distinguish association from causation in observational studies. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `clinical_search`, `protocol_lookup`, `phi_filter`, `de_identify`.

**`health:analyst`** — De-identified analytics. System prompt instructs: you receive only de-identified datasets — confirm the de_identification grant before computing anything. Document the data hash and the de-identification method in the report. Apply small-cell suppression for any cohort under 10 patients. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `risk_score`, `de_identify`, `phi_filter`.

**`health:compliance`** — Read + audit, autonomous. System prompt instructs: audit the workspace trail for PHI access events. Confirm every clinical tool invocation has a matching consent_verify or de_identify checkpoint. Verify grant scopes match the actual use. Run phi_filter on all generated artifacts. Produce an audit report with pass/warn/fail per dimension. You do not modify records — only verify them. Tools: `read_file`, `list_dir`, `search_files`, `phi_filter`, `consent_verify`, `audit_export`.

**`health:coordinator`** — Coordination + education. System prompt instructs: assess the patient's reading level and primary language before generating educational materials. Default to 6th-grade reading level for patient-facing content. Verify consent before personalizing materials with patient data. Reference the source clinical content. Tools: `read_file`, `write_file`, `list_dir`, `search_files`, `clinical_search`, `protocol_lookup`, `consent_verify`, `education_material`.

---

## 6. Workflows

Four workflow DAGs plus direct task types. Each defines stages with role assignments, dependencies, and gate policies.

**`health:patient-assessment`** (4 stages):
```
consent (clinician) ──→ gather (clinician) ──→ analyze (clinician) ──→ report (clinician)
   [gated]                  [gated]                                       [gated]
```
Used by: `health:assess`, `health:diagnose_support`, `health:report`. Consent stage produces the PHI access grant — gated because verifying patient consent requires human attestation. Gather stage collects history, vitals, labs and is gated. Analyze stage computes risk scores and forms differentials. Report stage generates the clinical document — gated because patient-facing output requires clinician sign-off.

**`health:literature-research`** (4 stages):
```
question (researcher) ──→ search (researcher) ──→ synthesize (researcher) ──→ review (compliance)
                                                                                  [gated]
```
Used by: `health:research`. Question formulation, literature search, and synthesis are not gated — research on public sources is read-only. Review is gated — compliance verifies no PHI leaked into the synthesis and the evidence grading is correct.

**`health:phi-audit`** (2 stages):
```
scan (compliance) ──→ report (compliance)
```
Used by: `health:audit`. Scan runs phi_filter across the workspace and exports the audit trail. Report produces a compliance summary. Both stages are autonomous — compliance is observer-only.

**`health:patient-education`** (2 stages):
```
assess_level (coordinator) ──→ generate (coordinator)
    [gated]                          [gated]
```
Used by: `health:educate`. Assess level requires reading level and language input — gated because the human confirms the patient profile. Generate is gated because patient-facing education must be reviewed before delivery.

**Direct execution:** `health:analyze` (analyst, on de-identified data only), `health:monitor` (clinician).

**DAG validation:** `validateWorkflow()` checks that all dependencies exist and no cycles are present.

---

## 7. Execution Model

The Healthcare vertical executes through the WACP protocol — not as a simulation.

**Goal submission:**
```
1. User submits goal: "do an admission H&P for the new patient in bed 4"
2. CLI detects task type: health:assess
3. CLI selects workflow: health:patient-assessment (4 stages)
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
      - PHI access check (for clinical_report_generate, lab_interpret, risk_score: verify phi_access_grant arg is valid and unexpired)
      - Execute tool via LocalResources
      - AgentService.CreateCheckpoint(observation, tool result)
   d. Feed tool results back to LLM
5. AgentService.CreateCheckpoint(artifact, FINAL, stage output)
6. AgentService.EmitSignal(COMPLETE)
7. Stage output flows as context to next stage
```

**PHI access grant contract:** The consent stage produces a checkpoint with fields: `basis: "consent"`, `patient_id`, `consent_id`, `consent_scope` (an array like `["assessment", "labs", "documentation"]`), `expires_at`. Alternatively, the de_identify stage produces a checkpoint with fields: `basis: "de_identified"`, `deidentification_method` (`safe_harbor` | `expert_determination`), `deidentified_data_hash`, `expires_at`. The downstream tools read the most recent matching grant from the workspace and reject execution if missing, expired, or scope-incompatible.

**HIPAA Safe Harbor de-identification:** The `de_identify` tool removes the 18 HIPAA identifiers from the input dataset and produces a de-identified copy plus a hash. The grant references the hash so downstream tools can verify they are operating on the de-identified version, not the original.

**Audit trail:** Every PHI access event becomes a checkpoint in the workspace trail. The compliance role can replay the trail offline and verify every clinical tool invocation has a matching grant. The hash chain provides legal admissibility — if a HIPAA breach is alleged, the trail is the evidence.

---

## 8. Quality Criteria

Six dimensions for evaluating Healthcare output:

| Dimension | Definition | Evaluation |
|-----------|-----------|------------|
| **Clinical accuracy** | Recommendations align with current clinical guidelines | Cited guideline matches recommendation |
| **PHI compliance** | No PHI exposed without a valid access grant | All tool invocations have a matching grant |
| **Evidence basis** | Citations to clinical literature, evidence levels graded | Citations present with grade |
| **Completeness** | All relevant clinical context included | Required clinical fields present |
| **Readability** | Appropriate reading level for the audience | Reading level matches target |
| **Regulatory adherence** | HIPAA, GDPR, FDA where applicable | Required statements present |

**Evaluation function:** Each dimension returns `pass`, `warn`, or `fail`. Rules:
- Recommendation contradicts cited guideline → `clinical_accuracy` = `fail`
- Off-label use without disclosure → `clinical_accuracy` = `warn`
- No guideline cited for recommendation → `clinical_accuracy` = `warn`
- Clinical tool invocation without PHI access grant → `phi_compliance` = `fail`
- PHI detected in published artifact → `phi_compliance` = `fail`
- Grant scope does not cover the use → `phi_compliance` = `fail`
- Grant expired during execution → `phi_compliance` = `fail`
- No literature citation for evidence claim → `evidence_basis` = `fail`
- Evidence level not graded → `evidence_basis` = `warn`
- Required clinical context missing (e.g., allergies, current meds) → `completeness` = `fail`
- Reading level above target by >2 grades → `readability` = `fail`
- Reading level above target by 1–2 grades → `readability` = `warn`
- HIPAA Notice of Privacy Practices not referenced in patient-facing material → `regulatory_adherence` = `warn`
- HIPAA breach indicator detected → `regulatory_adherence` = `fail`
- Required FDA disclaimer missing for off-label content → `regulatory_adherence` = `fail`

Overall: `pass` if all pass, `warn` if any warn and none fail, `fail` if any fail.

---

## 9. Gate Policies

### PHI and Clinical Gating

The defining constraint of the Healthcare vertical. Gates enforce PHI access basis and clinician sign-off — no patient-facing output without human review, no PHI access without consent or de-identification.

| Transition | Gate | Rationale |
|-----------|------|-----------|
| Goal → consent | **Human approval** | Verify patient consent or de-identification basis |
| Consent → gather | **Human approval** | Confirm consent scope before data collection |
| Gather → analyze | **Human approval** | Confirm correct data was collected |
| Analyze → report | **Human approval** | Final clinician sign-off before patient-facing output |
| Question → search | None | Public-source research is read-only |
| Search → synthesize | None | Synthesis is internal |
| Synthesize → review | **Human approval** | Compliance reviews for PHI leakage and evidence grading |
| Scan → report (audit) | None | Audit is observational |
| Assess level → generate | **Human approval** | Confirm patient profile before generating materials |
| Generate → deliver | **Human approval** | Patient-facing materials require clinician review |
| Any stage → clinical_report_generate / lab_interpret / risk_score | **PHI access check** | Tool refuses without valid PHI access grant |

---

## 10. Package Structure

```
ecosystem/healthcare/
├── HEALTHCARE.md            # This spec
├── package.json             # @wacp/healthcare
├── tsconfig.json
├── .gitignore
├── src/
│   ├── index.ts             # Public exports
│   ├── taxonomy.ts          # 5 roles + 8 task types with lookup functions
│   ├── tools/
│   │   └── healthcare-tools.ts  # 10 tool definitions + executors (PHI grant enforcement)
│   ├── profiles/
│   │   └── profiles.ts          # 5 profiles with system prompts + tool whitelists
│   ├── workflows/
│   │   └── workflows.ts         # 4 workflow DAGs + validation
│   └── quality/
│       └── quality.ts           # 6 dimensions + evaluateQuality() → QualityReport
└── tests/
    ├── taxonomy.test.ts         # 14 tests
    ├── tools.test.ts            # 18 tests
    ├── profiles.test.ts         # 13 tests
    ├── workflows.test.ts        # 15 tests
    └── quality.test.ts          # 18 tests
```

---

## 11. Test Requirements

| Module | Tests | Count |
|--------|-------|-------|
| `taxonomy.ts` | 5 roles unique, correct extends/access/autonomy. 8 task types unique, correct workflow mapping. Lookup functions. Compliance is observer. Assess and report share workflow. | 14 |
| `tools/healthcare-tools.ts` | 10 definitions unique, valid schemas. PHI access grant validation logic works for both bases. Scope check works. Expiration check works. Safe-Harbor identifier list complete. PHI detection works. | 18 |
| `profiles/profiles.ts` | 5 profiles with non-empty prompts. Tool whitelist matches role. Clinician has clinical_report_generate. Researcher lacks clinical_report_generate. Analyst restricted to de-identified tools. Compliance is read-only and autonomous. Workers are gated. | 13 |
| `workflows/workflows.ts` | 4 workflows unique, correct stage counts. Dependency order correct. Patient-assessment all stages gated. Literature-research review gated. Patient-education all stages gated. PHI audit not gated. DAG validation passes. Task type mapping complete. | 15 |
| `quality/quality.ts` | 6 dimensions unique. All-pass → pass. Tool without grant → fail. PHI in artifact → fail. Scope mismatch → fail. Expired grant → fail. No guideline → warn. Recommendation contradicts → fail. No citation → fail. Evidence ungraded → warn. Reading level mismatch → warn/fail. HIPAA breach → fail. | 18 |
| **Total** | | **78** |

---

## 12. References

| Spec | Section | Referenced in | Topic |
|------|---------|--------------|-------|
| SWE vertical spec | §1–12 | §1 | Pattern template — structure, execution model |
| Finance vertical spec | §1 | §1 | Distinction: finance gates by regulation, healthcare gates by statute (HIPAA) |
| DevOps vertical spec | §1 | §1 | Distinction: DevOps gates by environment, healthcare gates by PHI access basis |
| Data Science vertical spec | §1 | §1 | Distinction: datasci enforces hypothesis declaration, healthcare enforces PHI access grant |
| Security spec | §3 | §1, §4 | ContentFilter PHI rules, audit events |
| CLI agent spec | §6–7 | §7 | Workflow execution, stage agent loop |
| Coordinator SDK spec | §3–5 | §7 | SubmitGoal, Decompose, Dispatch RPCs |
| Agent SDK v2 spec | §3 | §7 | Bind, EmitSignal, CreateCheckpoint |
| Runtime spec | §3 (process model) | §7 | Workspace lifecycle, trail recording |
| Tool framework spec | §3 | §4 | ToolDefinition schema |
| IMPLEMENTATION.md | 27D | §1 | Healthcare vertical design |

---

*WACP ecosystem specification — authored by Akil Abderrahim and Claude Opus 4.6*
