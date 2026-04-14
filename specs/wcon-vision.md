---
id: wcon-vision
type: design
status: final
created: 2026-04-09T00:00:00
revised: 2026-04-14T00:00:00
authors: [AAkil98]
tags: [vision, product, foundation]
---

# WACP Console — Product Vision

## Table of Contents

1. Problem Statement
2. Vision
3. Goals and Non-Goals
4. Target Users
5. Value Propositions
6. Dependencies
7. Success Criteria

---

## 1. Problem Statement

WACP provides a complete protocol runtime for coordinating autonomous AI agents: 12 Rust crates, three gRPC services, capability-based security, an append-only trail, and a human highway for oversight. The protocol is finished. The runtime works. But there is no product sitting on top of it.

Today, using WACP requires programmatic access. Discovering available roles means reading taxonomy YAML files by hand. Creating an agent profile means editing ecosystem definition files in a text editor. Launching a coordination session means writing code against the SDK — choosing a vertical, wiring up a workflow DAG, binding profiles to role slots, and calling gRPC endpoints directly. Monitoring a running session means consuming a trail stream and interpreting raw protocol events.

The highway UI exists as a standalone SPA fragment, but it covers only one facet of oversight (trail viewing, gate approval). It does not help users set up what they want to oversee.

The gap is between the runtime (powerful, complete, protocol-correct) and the human who wants to use it (needs to discover, configure, launch, and watch). That gap is the Console.

## 2. Vision

The WACP Console is the human's command center for the WACP ecosystem — a full-stack application where users discover what agents can do, configure how they should behave, launch coordination sessions, and oversee the work in real-time.

The Console does not replace the WACP runtime. It sits in front of it. The runtime is the engine; the Console is the cockpit. Every action the Console takes — creating a workspace, delivering a directive, resolving a gate — flows through the protocol's existing gRPC API (with vertical discovery via the runtime's REST API, per ADR-001). The Console adds no protocol-level concepts. It adds a product-level experience.

Three capabilities define the Console:

1. **Discovery** — browse the taxonomy interactively. See every role (base and derived), every tool (grouped by the vertical that owns it), every protocol-level envelope and checkpoint type, every vertical with its defining constraint, its context schema, its tool policies, its vertical-specific checkpoint types, its workflows, its task types, and its quality criteria. No YAML reading, no documentation hunting.

2. **Profile management** — create, edit, save, load, clone, export, and import agent profiles. A profile bundles a role, LLM configuration, autonomy preset, tool permissions, and budget caps into a reusable, portable unit. Profiles are the user's primary means of expressing "how this agent should behave."

3. **Session control** — configure a coordination session (pick a vertical, pick a workflow, assign profiles to roles, supply vertical context), launch it against the live runtime, and monitor it through an integrated oversight dashboard that surfaces the trail, gates, escalations, tool-layer refusals, and workspace states in real-time.

**Vertical-awareness is load-bearing.** The ecosystem ships multiple verticals today (SWE, DevOps, MLOps, Finance, Healthcare, Analytics, DataSci), each with a distinct *defining constraint*: DAG validation for SWE, environment-scoped blast radius for DevOps, compute-budget + reproducibility for MLOps, regulatory pre-check for Finance, HIPAA PHI access control for Healthcare, SQL safety + snapshot reproducibility for Analytics, pre-declared hypotheses for DataSci. The Console surfaces these constraints in configuration (the wizard's vertical context step), in oversight (context badges, refusal rendering, vertical-specific checkpoint views), and in discovery (vertical detail shows the defining constraint prominently). A Console that only knows about SWE would be unusable for six of the seven verticals.

## 3. Goals and Non-Goals

### Goals

| ID | Goal | Measured by |
|----|------|-------------|
| G1 | Users can discover every entity the ecosystem exposes: base and derived roles, tools (grouped by vertical, with policy indicators), protocol-level envelope and checkpoint types, verticals with defining constraints, context schemas, tool policies, vertical-specific checkpoint types, workflows, task types, and quality criteria | Discovery browser shows every manifest field (`wcon-discovery` §2.2, `wcon-ui` §4.5) |
| G2 | Users can create profiles without editing source files | Profile studio produces valid, saveable profiles |
| G3 | Users can launch coordination sessions without writing code, including supplying any vertical-specific context required at launch time | Session launcher creates WACP workspaces from UI configuration; vertical context wizard step generated from `context_schema` |
| G4 | Users can oversee running sessions through a unified dashboard | Oversight dashboard streams trail, surfaces gates, escalations, and tool-layer refusals; renders vertical-specific checkpoints with field schemas |
| G5 | Profiles are portable — exportable as YAML, importable from YAML | Round-trip: export → import produces identical profile |
| G6 | Profile validation catches errors before launch | Invalid role references, cross-vertical tool references, budget violations rejected at save time; policy-gated tools saved with non-blocking warning |
| G7 | The Console works with any WACP vertical, not just SWE — session launcher, discovery browser, profile editor, and oversight dashboard all render from the live `VerticalEntry` with no hardcoded per-vertical logic | Manifest-driven rendering invariant (`wcon-ui` §12.6); a new vertical added to the runtime appears correctly on the next taxonomy reload without any Console code change |

### Non-Goals

| ID | Non-Goal | Rationale |
|----|----------|-----------|
| NG1 | Replace or reimplement the WACP runtime | The runtime is complete; the Console connects to it via gRPC |
| NG2 | Modify the WACP protocol | The Console consumes the protocol as-is; protocol evolution is upstream |
| NG3 | Provide IDE integration | A separate product concern; the Console is a standalone workbench |
| NG4 | Multi-tenant SaaS hosting | The Console supports multiple authenticated users on a single instance (`wcon-auth`), but organization-level tenancy — tenant isolation, per-org billing, cross-tenant access control — is out of scope |
| NG5 | Author new verticals through the UI | Verticals are structured definition packages; authoring them is a developer task, not a Console task |
| NG6 | Provide a mobile interface | Desktop-first; the oversight dashboard requires screen real estate |
| NG7 | Replace the CLI agent | The CLI serves a different interaction model (terminal-native, scriptable); the Console serves a visual, exploratory model |

## 4. Target Users

### Primary: The Practitioner

An AI practitioner or engineer who wants to coordinate agents using WACP. They understand agent concepts (roles, tools, prompts, autonomy) but do not want to wire up gRPC calls to launch a session. They want to browse what is available, configure agent behavior through a UI, and watch the coordination unfold.

**Key need:** reduce the distance between "I have a WACP runtime running" and "agents are doing useful work."

### Secondary: The Overseer

A team lead, project manager, or domain expert responsible for reviewing agent output and making approval decisions at gates. They may not configure sessions themselves, but they interact with the oversight dashboard — approving gates, handling escalations, injecting directives when agents need course correction.

**Key need:** clear, real-time visibility into what agents are doing, with actionable controls when human judgment is required.

### Tertiary: The Explorer

A developer evaluating WACP for adoption, or an existing user exploring a new vertical. They use the discovery browser to understand what roles exist, what tools are available, what workflows are defined — before committing to integration work.

**Key need:** understand the WACP ecosystem's capabilities without reading source files or specification documents.

### Quaternary: The Administrator

The person responsible for the Console instance itself — managing user accounts, rotating credentials, reviewing the audit log, and maintaining operational health. Often the same person as the Practitioner in small teams, but a distinct function with distinct needs. Maps to the `admin` role in `wcon-auth`.

**Key need:** user lifecycle management and security oversight without requiring direct database or CLI access.

## 5. Value Propositions

### From code to interface

What today requires SDK calls and YAML editing becomes a visual workflow: browse roles in the discovery browser, create a profile in the profile studio, launch a session from the session launcher. The Console does not change what is possible — everything it does was already possible through the SDK. It changes the effort required.

### Profile portability

Profiles are first-class, user-owned artifacts. Create a profile, tune it across sessions, export it as YAML, share it with a colleague, import it on another machine. Profiles decouple "how this agent should behave" from "which session it runs in," making agent configuration reusable and transferable.

### Taxonomy as a browsable catalog

The WACP taxonomy is powerful but opaque — YAML files on the runtime host, reached only by reading source or making bespoke REST calls. The Console turns it into an interactive catalog: search for roles, filter tools by vertical or by policy, inspect envelope and checkpoint types, review context schemas and tool policies, compare quality criteria across verticals, and inspect workflow summaries with stage and gate counts. When per-stage workflow detail becomes available upstream (it is not in the manifest today), workflow cards will expand to full DAG diagrams. Discovery becomes a product feature instead of a file-reading exercise.

### Unified oversight

The highway exists as a protocol mechanism with four capabilities (visibility, gates, injection, escalation handling). The Console unifies them in a single oversight dashboard: trail streaming for visibility, a gate approval queue for control, an escalation inbox for responsiveness, and directive injection for course correction. One surface, all four capabilities.

## 6. Dependencies

### Runtime dependency: WACP runtime

The Console requires a running WACP runtime instance for session launch and oversight. It connects as a client over four network endpoints — three gRPC services (each on its own Tonic server) and one REST gateway:

| Transport | Default address | Service | Console uses it for |
|-----------|-----------------|---------|---------------------|
| gRPC | `[::1]:9090` | `AgentService` | Workspace lifecycle, directive delivery, checkpoint submission, resource management |
| gRPC | `[::1]:9091` | `HighwayService` | Trail streaming, gate management, escalation handling, injection |
| gRPC | `[::1]:9092` | `CoordinatorService` | Session orchestration, task graph management, integration, workspace state |
| REST | `http://[::1]:9093` | REST gateway | Vertical manifest loading (ADR-001), health checks |

The Console does not embed or start the runtime. The runtime is **not required** at Console startup — the Console comes up with a warning and an empty vertical registry if the runtime is unreachable (`wcon-discovery` §8.1). Discovery of base roles and protocol-level types still works; session launch is disabled until the runtime becomes reachable and the user triggers a taxonomy reload.

Connection failure on any endpoint is surfaced as a clear diagnostic in the UI and through the `/api/health` endpoint (`wcon-api` §11.1), which reports per-service reachability and an aggregate `degraded` status when one or more endpoints are unreachable.

### Data dependency: Protocol taxonomy files

The Console builds the base-role and protocol-type portion of its taxonomy index from WACP protocol-taxonomy YAML files on the local filesystem. The taxonomy is loaded once at startup and can be reloaded on demand. The Console reads but never writes taxonomy files.

### Data dependency: Vertical manifests (via the runtime REST API)

Verticals are loaded by calling `GET /v1/verticals` and `GET /v1/verticals/{id}` on the running WACP runtime (ADR-001). The runtime is the authoritative vertical registry; the Console consumes manifests through REST, not through filesystem reads. The ecosystem currently ships seven verticals (SWE, DevOps, MLOps, Finance, Healthcare, Analytics, DataSci), each with a defining constraint the Console surfaces in discovery, session launch, and oversight. At least one vertical must be loaded for the session launcher to be functional.

New verticals added to the runtime's ecosystem directory become available to the Console on the next taxonomy reload — no Console redeploy, no filesystem coordination between the two projects.

### No external service dependencies

The Console does not depend on cloud APIs, external databases, or third-party services for its core functionality. LLM provider access is the runtime's responsibility (configured in profiles, executed by the runtime's LLM adapters). The Console's own persistence is local (SQLite for profiles and session records, filesystem for YAML exports). The WACP runtime is the Console's only external dependency — accessed via gRPC (for sessions and highway) and REST (for vertical manifests).

## 7. Success Criteria

### Functional criteria

| ID | Criterion | Verification |
|----|-----------|--------------|
| SC1 | A user launching the Console for the first time can see the vertical list and base roles within 30 seconds of startup; full per-vertical indexing (all roles, tools, task types, context schemas, tool policies, checkpoint types across all loaded verticals) completes within 5 seconds of taxonomy reload for an ecosystem of up to 10 verticals and 100 tools | Manual test: fresh install → discovery browser → vertical cards visible at 30s; full index populated after reload |
| SC2 | A user can create a new profile, save it, close the Console, reopen, and load the same profile | Round-trip persistence test |
| SC3 | A user can launch a session that creates the correct WACP workspaces with the correct role and profile bindings, and the session's vertical context is delivered to each worker via the directive envelope payload (workspace-metadata delivery is an additional path pending upstream runtime support per `wcon-sessions` §4.1) | Integration test: session launch → verify workspace count, roles, directive payload including `context` field via CoordinatorService |
| SC4 | A user can approve a gate through the oversight dashboard and see the blocked workspace resume | End-to-end test: supervised session → gate appears → approve → workspace transitions to active |
| SC5 | A user can export a profile as YAML, import it on a different Console instance, and launch a session with it | Portability test: export → copy file → import → validate → launch |
| SC6 | The Console rejects a profile that references a nonexistent role (`UNKNOWN_ROLE`), a nonexistent tool (`UNKNOWN_TOOL`), or a tool that belongs to a different vertical than the role (`TOOL_NOT_IN_ROLE_VERTICAL`) at save time, not at launch time. Policy-gated tools are accepted with a non-blocking `TOOL_HAS_RUNTIME_POLICY` warning (`wcon-profiles` §3.2) | Validation test: create profile with bad role → save → error shown immediately |
| SC7 | The oversight dashboard streams trail entries with less than 2 seconds of visual latency from the protocol event | Latency measurement under normal load |
| SC8 | The session launcher's vertical-context wizard step is generated dynamically from `VerticalEntry.context_schema` — a newly added vertical (with a new context field) is handled without any Console code change | Test: start Console, add vertical manifest with new context field, reload taxonomy, verify wizard step renders the new field |
| SC9 | Tool-layer refusal events (`wcon-highway` §4A) surface in the oversight dashboard with the correct error code, policy reference, and unblock hint for every known refusal code across the seven shipping verticals | Integration test per vertical: inject synthetic refusal → verify refusal panel rendering |

### Boundary criteria

| ID | Criterion | Purpose |
|----|-----------|---------|
| BC1 | The Console never modifies taxonomy files, vertical manifests, or upstream WACP source | Enforces read-only relationship with upstream WACP data |
| BC2 | The Console never bypasses gRPC (for sessions/highway) or REST (for vertical manifests) to access runtime internals | Enforces clean separation — Console is a client, not a runtime extension |
| BC3 | The Console assumes the runtime records every protocol action in the trail and relies on this for oversight display; the Console never maintains a parallel event log that could diverge from the trail | Enforces single-source-of-truth — the trail is authoritative, the Console is a consumer |
| BC4 | The Console functions correctly with any vertical, not just SWE — evidenced by end-to-end regression tests against at least two fixture verticals with different shapes (`wcon-test` §7.1): a SWE-like baseline (no context schema, no tool policies) and a finance/healthcare-like vertical (required context, tool policies, vertical-specific checkpoint types) | Continuous verification via `wcon-test` fixtures |

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-glossary | WACP Console Glossary | informs terminology; defines `vertical`, `defining constraint`, `workspace context tag`, `tool-layer policy`, `vertical-specific checkpoint` |
| wcon-discovery | Agent & Role Discovery | implements G1 via REST-based vertical loading (§2.2); defines `VerticalEntry` schema (§2.2.2, §3.3) |
| wcon-profiles | Profile System | implements G2, G5, G6 via the profile studio; defines policy-aware validation (§3.2) |
| wcon-sessions | Session Lifecycle | implements G3 via session launcher with vertical context (§2.1, §3.1); Mode A/B slot derivation (§2.4) |
| wcon-highway | Highway Integration | implements G4 via oversight dashboard with gates, escalations, refusals (§4, §4A, §5) |
| wcon-ui | UI Design | implements the four surfaces; enforces manifest-driven rendering invariant (§12.6) which realises G7 |
| wcon-data-model | Data Model | defines the `VerticalEntry` projection (§6.1) that G1 renders against |
| wcon-api | API Surface | consolidates the contract the frontend consumes for all four capabilities |
| wcon-test | Test Strategy | verifies SC1–SC9 and BC1–BC4 via the two-fixture-vertical strategy (§7.1) |
| wacp-protocol | WACP Protocol Specification | upstream source of the gRPC API the Console consumes |
| wacp-taxonomy | WACP Taxonomy crate | upstream source of `VerticalManifest` served via REST |
| SPEC_BUILD.md | Project build log | records ADR-001 (runtime as vertical registry) which is cited throughout this spec |

*WACP Console -- authored by AAkil98*
