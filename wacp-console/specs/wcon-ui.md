---
id: wcon-ui
type: design
status: final
created: 2026-04-10T00:00:00
revised: 2026-04-14T00:00:00
authors: [AKIL Abderrahim, Claude Opus 4.6]
tags: [ui, design, frontend, interaction, vertical-context, auth]
depends_on: [wcon-auth, wcon-api]
---

# WACP Console — UI Design

## Table of Contents

1. Overview
2. Navigation Structure
3. Screen Inventory
4. Discovery Browser
5. Profile Studio
6. Session Launcher
7. Oversight Dashboard
7A. Auth Screens
8. Settings
9. Component Patterns
10. Responsive Design
11. State Management
12. Invariants

---

## 1. Overview

The Console frontend is a single-page application with four primary surfaces: the discovery browser, the profile studio, the session launcher, and the oversight dashboard. Each surface maps to a core capability defined in `wcon-vision` §2 and consumes the API defined in `wcon-api`.

This spec defines what the user sees and how they interact with it. It does not specify implementation technology (framework, component library) — those decisions belong to the implementation phase. It defines screens, layouts, interaction patterns, and the behavioral rules the frontend must follow.

### 1.1 Design Principles

1. **Task-oriented.** Each surface serves a distinct workflow. The user navigates to a surface to accomplish a specific task (browse taxonomy, create a profile, launch a session, oversee agents). Surfaces do not blend responsibilities.
2. **Progressive disclosure.** Lists show summaries; clicking reveals detail. Editors show required fields first; advanced settings are collapsed by default. The user sees complexity only when they reach for it.
3. **Real-time when active.** The oversight dashboard streams events continuously. All other surfaces use request-response — they fetch data when the user navigates to them and do not auto-refresh.
4. **Keyboard accessible.** All interactive elements are reachable via keyboard. Gate approval and escalation response — the most time-sensitive actions — have keyboard shortcuts.

## 2. Navigation Structure

### 2.1 Top-Level Navigation

The application shell has a persistent navigation sidebar (left edge) visible on all screens:

```
┌──────────┬──────────────────────────────────────────────┐
│          │                                              │
│ ◆ WACP   │          [Active Surface]                    │
│ Console  │                                              │
│          │                                              │
│ ─────    │                                              │
│          │                                              │
│ 🔍 Discover│                                              │
│ 📋 Profiles│                                              │
│ ▶ Sessions│                                              │
│ 👁 Oversight│                                             │
│          │                                              │
│ ─────    │                                              │
│          │                                              │
│ ⚙ Settings│                                              │
│          │                                              │
│ ─────    │                                              │
│ ● Runtime│                                              │
│   Connected│                                             │
│          │                                              │
│ ─────    │                                              │
│ ○ jane   │                                              │
│   operator│                                             │
└──────────┴──────────────────────────────────────────────┘
```

| Nav item | Surface | Badge |
|----------|---------|-------|
| Discover | Discovery browser | — |
| Profiles | Profile studio | Profile count |
| Sessions | Session launcher + session list | Active session count |
| Oversight | Oversight dashboard | Pending gates + escalations + refusals |
| Settings | Settings panel | — |
| Users | User management (admin only) | — |
| Audit Log | Audit log viewer (admin only) | — |

**User menu** (bottom of sidebar): shows the authenticated user's username and console role. Clicking opens a popover with: "Change Password", "API Tokens", and "Logout". Admins also see "Users" and "Audit Log" as navigation items in the sidebar; these items are hidden for operators and viewers (`wcon-auth` §4.2).

**Vertical filter** (optional, shown at the top of the Sessions and Oversight lists when multiple sessions span more than one vertical): a compact dropdown labelled "Vertical: [All ▼]" with options for `All` and each vertical currently represented by at least one session. Selecting a vertical filters the session list in-place. The filter state is preserved across navigation.

The Oversight badge aggregates three counts — gates, escalations, and refusals — displayed as a single number on the nav sidebar. The breakdown is visible when hovering: "2 gates · 1 escalation · 3 refusals".

### 2.2 Navigation Behavior

- Clicking a nav item loads the corresponding surface in the main content area.
- The active nav item is highlighted.
- Navigation preserves surface state: switching from Profiles to Discover and back retains the profile list's scroll position, filters, and any open editor.
- The oversight badge count updates in real-time via the WebSocket connection. All other badges update on navigation.

### 2.3 Runtime Status Indicator

The bottom of the sidebar shows the WACP runtime connection status:

| Status | Display | Color |
|--------|---------|-------|
| Connected | "Runtime Connected" | Green |
| Reconnecting | "Runtime Reconnecting..." | Yellow |
| Disconnected | "Runtime Disconnected" | Red |

Clicking the indicator navigates to Settings with the runtime configuration section focused.

## 3. Screen Inventory

| Screen | Parent surface | URL path | Description |
|--------|---------------|----------|-------------|
| Role list | Discovery | `/discover/roles` | Filtered list of roles |
| Role detail | Discovery | `/discover/roles/:id` | Full role definition |
| Tool list | Discovery | `/discover/tools` | Filtered list of tools |
| Tool detail | Discovery | `/discover/tools/:name` | Full tool definition |
| Type list | Discovery | `/discover/types` | Envelope and checkpoint types |
| Vertical list | Discovery | `/discover/verticals` | List of verticals |
| Vertical detail | Discovery | `/discover/verticals/:id` | Vertical contents |
| Workflow detail | Discovery | `/discover/verticals/:id/workflows/:wf_id` | Workflow stage DAG (Mode A per `wcon-sessions` §2.4) or summary card (Mode B) |
| Search results | Discovery | `/discover/search?q=...` | Cross-entity search |
| Profile library | Profiles | `/profiles` | List of saved profiles |
| Profile editor | Profiles | `/profiles/new` | Create new profile |
| Profile editor | Profiles | `/profiles/:id/edit` | Edit existing profile |
| Profile detail | Profiles | `/profiles/:id` | View profile with version history |
| Profile import | Profiles | `/profiles/import` | Import from YAML |
| Session list | Sessions | `/sessions` | All sessions (active + historical) |
| Session configure | Sessions | `/sessions/new` | New session wizard |
| Session detail | Sessions | `/sessions/:id` | Session configuration and status |
| Dashboard | Oversight | `/oversight` | Active session selector + dashboard |
| Dashboard (session) | Oversight | `/oversight/:id` | Dashboard for specific session |
| Settings | Settings | `/settings` | Console configuration |
| Login | — | `/login` | Username/password form (unauthenticated) |
| Change password | — | `/change-password` | Forced or voluntary password change |
| API tokens | User menu | `/tokens` | Own token list and creation |
| User list | Users | `/admin/users` | User management (admin only) |
| User detail | Users | `/admin/users/:id` | Edit user, reset password (admin only) |
| Audit log | Audit Log | `/admin/audit-log` | Filterable audit log (admin only) |

## 4. Discovery Browser

### 4.1 Layout

Two-panel layout: entity list on the left, detail view on the right.

```
┌────────────────────┬─────────────────────────────────────┐
│ [Search............]│                                     │
│                    │  Role: finance:portfolio_manager     │
│ Tabs: Roles | Tools│                                     │
│   | Types | Verts  │  Base: worker                        │
│                    │  Vertical: Finance                   │
│ Filters:           │                                      │
│  Base: [All ▼]     │  Tools available in this vertical:   │
│  Vertical: [All ▼] │  ├─ market_data_fetch                │
│                    │  ├─ financial_model_build            │
│ ┌────────────────┐ │  ├─ risk_calc                        │
│ │ coordinator    │ │  ├─ compliance_check                 │
│ │ worker         │ │  ├─ kyc_screen                       │
│ │ observer       │ │  ├─ 🔒 trade_execute  ───────[link]─▶│
│ │─── Finance ───│ │  ├─ portfolio_rebalance              │
│ │ analyst        │ │  ├─ audit_trail_export               │
│ │▸portfolio_mgr  │ │  ├─ regulatory_filing_prepare        │
│ │ risk_officer   │ │  └─ disclosure_review                │
│ │ compliance_o.. │ │                                      │
│ │ auditor        │ │  Envelope types:                     │
│ │─── SWE ───────│ │  ├─ Can send: directive, feedback    │
│ │ swe:planner    │ │  └─ Can receive: directive, query    │
│ │ ...            │ │                                      │
│ └────────────────┘ │  🔒 = policy-gated (see §3.5)        │
└────────────────────┴─────────────────────────────────────┘
```

The "Tools available in this vertical" label reflects the §3.4 relaxation in wcon-discovery — the Console presents the full set of tools declared by the role's vertical rather than a per-role-filtered subset (which would require metadata not in the manifest). Lock icons mark tools with runtime policies (§3.5 / `wcon-discovery` §3.5).

### 4.2 Tab Navigation

The left panel has tabs for each entity category:

| Tab | Content | Default sort |
|-----|---------|-------------|
| Roles | Role list: base roles section, then derived roles grouped by vertical | Alphabetical within groups |
| Tools | Tool list grouped by vertical; lock icon for policy-gated tools (`wcon-discovery` §3.5) | Alphabetical within groups |
| Types | Three collapsible sections: envelope types (protocol-level), checkpoint types (protocol-level), vertical checkpoint types (grouped by vertical) | Alphabetical within sections |
| Verticals | Vertical list | Alphabetical |

The Types tab's third section — vertical checkpoint types — mirrors `wcon-discovery` §6.1 navigation structure. Each vertical-specific checkpoint type (e.g., Finance `compliance_check`) is shown under its owning vertical header with a compact preview (description + field count); clicking opens the full field schema in the detail panel.

The active tab and its filter state are preserved when switching between tabs.

### 4.3 Search

The search box at the top of the left panel searches across all entity types (`GET /api/search`). When a search query is active:

- The tab bar is replaced by a "Search results" header showing the query.
- Results are grouped by entity type with counts.
- Clicking a result navigates to the appropriate tab and selects the entity.
- Clearing the search restores the previous tab and filter state.

### 4.4 Detail View

Clicking an entity in the list loads its detail in the right panel. Detail views contain:

- All entity fields in a structured layout.
- Cross-reference links (e.g., clicking a tool in a role's tool list navigates to that tool's detail).
- A "Use in profile" action on role details — opens the profile editor with the role pre-selected.

### 4.5 Vertical Detail

Vertical detail is the richest entity view. It is driven entirely by the `VerticalManifest` (`wcon-data-model` §6.1) served by the runtime. Sections:

**Header.**
- `name` (large)
- `defining_constraint` (body text, prominent)
- Summary counts: roles, task types, workflows, tools, context fields, tool policies, vertical checkpoint types

**Roles section.**
Collapsible. Lists `VerticalEntry.roles` with autonomy from `default_profiles` ("gated" / "autonomous"). Each entry links to the role detail view.

**Task Types section.**
Collapsible table with columns: `id`, `name`, `description`, target workflow (`workflow_id`), and keywords (rendered as chips for search/detection). The table renders one row per `TaskTypeDescriptor`.

**Workflows section.**
Each workflow is rendered as a card from the `WorkflowSummary`:

```
┌────────────────────────────────────────────────────┐
│ Trade Execution                                    │
│ Analyze → compliance → execute → record            │
│ 4 stages  •  2 gated                               │
└────────────────────────────────────────────────────┘
```

When per-stage detail is available (via a supplementary endpoint or upstream source projection), the card expands to a horizontal DAG diagram on click:

```
[Analyze]──────▶[Compliance]──────▶[Execute]──────▶[Record]
 analyst         compliance         portfolio_mgr   auditor
                 🔒 gated             🔒 gated
```

The layout engine handles any `stage_count`. The stage list is rendered as a horizontal row with arrows between stages; if the number of stages exceeds the container width, the row wraps to a second line without repeating arrows. Gate indicators (lock icon) appear below stages whose `gated: true`. When every stage is gated (e.g., Finance `client-onboarding`, Healthcare `patient-assessment`), the card gets an "All-gated workflow" banner rather than repeating the lock icon below every stage.

Clicking a stage navigates to the role detail for that stage's role.

**Context Schema section.**
Renders the vertical's `context_schema` as a table (empty for SWE — the section is hidden when empty):

| Field | Type | Required | Description | Enum values |
|-------|------|----------|-------------|-------------|
| compliance_scope | string | yes | Regulatory scope for trades in this session | — |
| jurisdiction | enum | yes | Regulatory jurisdiction governing trades | SEC, FINRA, MiFID II, FCA, other |

Users inspecting a vertical before launching can see exactly what the launch wizard will ask them for in step 4 (§6.2).

**Tool Policies section.**
Renders `tool_policies` as a table (empty for SWE — hidden when empty):

| Tool | Kind | Summary |
|------|------|---------|
| trade_execute | requires_checkpoint | Requires `compliance_check` checkpoint with matching `trade_id` (expires after 5 min) |
| train_launch | budget_limited | Blocked if `max_hours` exceeds session `compute_budget` |

Each row has a lock icon indicating the tool is policy-gated. Clicking expands to the full `ToolPolicy` structure (description + kind-specific fields).

**Vertical Checkpoint Types section.**
Renders `checkpoint_types` as a collapsible list. Each entry shows:
- Checkpoint type name (e.g., `compliance_check`)
- `description`
- Field table (`name`, `type`, `description`, `enum_values` where applicable)
- `required_by`: the list of tools whose policy references this type (cross-linked to the Tool Policies section)

Example for Finance `compliance_check`:

| Field | Type | Description |
|-------|------|-------------|
| trade_id | string | Unique identifier for the trade being checked |
| instrument | string | Financial instrument (ticker, ISIN, CUSIP) |
| side | enum | Trade direction (buy / sell) |
| quantity | number | Trade quantity |
| status | enum | Compliance decision (approved / rejected) |
| regulation_cited | string | Applicable regulation(s) checked |
| forbidden_pattern_screened | boolean | Whether the forbidden-pattern screen ran |
| suitability_verified | boolean | Whether suitability was verified for the client |
| kyc_current | boolean | Whether KYC is current for the counterparty |
| expires_at | number | Unix timestamp (ms) after which this check is stale |

This section is the source of truth the oversight dashboard's trail stream uses to render vertical checkpoint events with structured field views (§7.2 / `wcon-highway` §8).

**Quality Criteria section.**
Renders `quality_criteria` as a table with columns: `id`, `name`, `description`, `weight`. Each row is one dimension of the vertical's quality rubric. Example for Finance (6 criteria, all weight 1): `regulatory_compliance`, `audit_trail_integrity`, `fiduciary_duty`, `risk_disclosure`, `data_provenance`, `documentation`.

**Tools section.**
Simple list of `tools[]` with `name` and `description`. Tools that have a policy in the Tool Policies section are marked with a lock badge.

## 5. Profile Studio

### 5.1 Profile Library

The default view for the Profiles surface. A table of saved profiles.

```
┌──────────────────────────────────────────────────────────────┐
│ Profiles                                    [Import] [+ New] │
│                                                              │
│ [Search............]  Role: [All ▼]  Vertical: [All ▼]      │
│                                                              │
│ ┌────┬──────────────────┬──────────────┬──────────┬────────┐ │
│ │ ☐  │ Name             │ Role         │ Autonomy │ v      │ │
│ ├────┼──────────────────┼──────────────┼──────────┼────────┤ │
│ │ ☐  │ Fast Implementer │ implementer  │ auto     │ v5     │ │
│ │ ☐  │ Careful Reviewer │ reviewer     │ super    │ v2     │ │
│ │ ☐  │ Budget Planner   │ planner      │ assisted │ v1     │ │
│ └────┴──────────────────┴──────────────┴──────────┴────────┘ │
│                                                              │
│ Selected: 0   [Clone] [Export] [Delete]                      │
└──────────────────────────────────────────────────────────────┘
```

| Column | Content |
|--------|---------|
| Checkbox | For bulk selection |
| Name | Profile name (click to view detail) |
| Role | Role label (derived from taxonomy) |
| Autonomy | Autonomy preset (abbreviated) |
| Version | Current version number |

**Actions:**
- Row click → profile detail view.
- "New" button → profile editor (create mode).
- "Import" button → file picker for YAML import.
- Bulk actions (enabled when checkboxes are selected): Clone, Export (ZIP), Delete.

**Invalid profile indicator:** profiles whose role or tools no longer resolve in the taxonomy show a warning icon next to the name. Hovering shows "Role or tools no longer valid — edit to fix."

### 5.2 Profile Editor

Form-based editor for creating and editing profiles. Used for both `/profiles/new` and `/profiles/:id/edit`.

```
┌──────────────────────────────────────────────────────────────┐
│ Profile Editor                              [Cancel] [Save]  │
│                                                              │
│ Name: [Fast Implementer          ]                           │
│ Description: [High-autonomy implementer    ]                 │
│ Tags: [swe] [fast] [+ add tag]                               │
│                                                              │
│ ── Role ──────────────────────────────────────────────────── │
│ Role: [swe:implementer ▼]                                    │
│   Vertical: SWE   Base: worker                               │
│                                                              │
│ ── LLM Configuration ────────────────────────────────────── │
│ Provider: [anthropic ▼]    Model: [claude-sonnet-4-20250514 ▼] │
│ Temperature: [0.3    ]     Max tokens: [8192     ]           │
│                                                              │
│ ── Autonomy ─────────────────────────────────────────────── │
│ (○) Autonomous  (●) Assisted  (○) Supervised                │
│                                                              │
│ ▸ Tools (click to expand)                                    │
│ ▸ Budget (click to expand)                                   │
└──────────────────────────────────────────────────────────────┘
```

The role selector shows the role ID, its owning vertical (if any), and its base role. The upstream manifest does not carry a free-text "concern" description per role — the role detail view (§4) exposes whatever metadata the manifest provides (name, base role, vertical, associated tools); the editor shows a compact version of the same.

**Field behavior:**

| Field | Widget | Source |
|-------|--------|--------|
| Name | Text input | User-typed |
| Description | Text area | User-typed |
| Tags | Tag chips with autocomplete | User-typed, existing tags suggested |
| Role | Searchable dropdown | `GET /api/roles` (taxonomy index) |
| Provider | Dropdown | Hardcoded list of known providers |
| Model | Dropdown | Provider-dependent model list |
| Temperature | Number input (0.0–2.0, step 0.1) | User-typed |
| Max tokens | Number input | User-typed |
| Autonomy | Radio buttons | Three presets |
| Tool allowlist | Checkbox list | `GET /api/tools?vertical=<role's vertical>` — all tools in the vertical that owns the selected role (per `wcon-discovery` §3.4 vertical-coarse mapping). Base roles (coordinator, worker, observer) have no owning vertical; for these the list is empty. |
| Tool denylist | Checkbox list | Same source as allowlist |
| Budget fields | Number inputs | User-typed |

**Progressive disclosure:**
- Tools section is collapsed by default. Expanding shows the tool list as a checkbox list. Checked tools form the allowlist. A separate "deny" toggle per tool adds it to the denylist.
- Budget section is collapsed by default. Expanding shows the three budget fields and the warning threshold slider.

**Tool-policy indicator.** Tools that have an entry in `ToolEntry.policy` (`wcon-discovery` §3.5) are displayed with a small lock icon (🔒) to the left of the tool name in the allowlist checkbox list. Hovering shows a tooltip with a one-line policy summary — for example:

| Tool | Indicator | Tooltip |
|------|-----------|---------|
| `code_edit` | — | — |
| `trade_execute` | 🔒 | Runtime will refuse without a prior approved `compliance_check` checkpoint (trade_id matching, expires in 5 min) |
| `clinical_report_generate` | 🔒 | Runtime will refuse without a valid `phi_access_grant` checkpoint (consent or de-identified basis) |
| `train_launch` | 🔒 | Runtime will refuse if `max_hours` exceeds session `compute_budget` |
| `deploy_execute` | 🔒 | Runtime will require gate clearance for production deployment |

The lock icon is informational — enabling a policy-gated tool in the allowlist still succeeds, but the editor shows a non-blocking warning at save time ("This profile enables N tools that have runtime policies — review the highlighted tools before launch") and `wcon-profiles` §3.2 records the policy warning in the validation response. The policy is enforced at runtime, not in the editor.

Clicking the lock icon opens a side panel with the full policy details: kind, description, checkpoint type (if any), matching field (if any), expiry window (if any), budget field (if any), blocked classifications (if any). This lets users understand what they are enabling without leaving the editor.

**Inline validation:**
- Changing the role *to a role in a different vertical* clears the tool selections (the tool set comes from the new vertical, not the previous one). Changing to a different role within the same vertical preserves tool selections.
- Invalid fields show red borders with error text below the field.
- The Save button is disabled when any required field is empty or any hard validation error is present. Non-blocking warnings (`TOOL_HAS_RUNTIME_POLICY`, autonomous-worker warning — see `wcon-profiles` §3.2, §3.3) do not disable Save — they surface as inline notices below the relevant field.
- Validation runs on blur (field loses focus) and on save attempt.

### 5.3 Profile Detail

Read-only view of a profile with version history.

```
┌──────────────────────────────────────────────────────────────┐
│ Fast Implementer (v5)                 [Edit] [Clone] [Export]│
│ "High-autonomy implementer with aggressive budget"           │
│ Tags: [swe] [fast]                                           │
│                                                              │
│ Role: swe:implementer ──────────────────────────────[link]──▶│
│ Provider: anthropic    Model: claude-sonnet-4-20250514       │
│ Temperature: 0.3       Max tokens: 8192                      │
│ Autonomy: autonomous                                         │
│                                                              │
│ Tools (4): code_edit, file_read, file_write, terminal        │
│ Denied: browser                                              │
│                                                              │
│ Budget: $0.50 max cost │ 100k tokens │ 5m wall time          │
│ Warning at: 80%                                              │
│                                                              │
│ ── Version History ──────────────────────────────────────── │
│ v5  Apr 10  autonomy: autonomous → assisted  [Rollback]     │
│ v4  Apr 9   temperature: 0.5 → 0.3                          │
│ v3  Apr 9   added tool: terminal                             │
│ v2  Apr 8   budget: $1.00 → $0.50                            │
│ v1  Apr 8   created                                          │
└──────────────────────────────────────────────────────────────┘
```

**Version history:** shows a changelog-style diff summary for each version. Computed by comparing adjacent versions field-by-field. Clicking a version row expands to show the full profile state at that version. The "Rollback" button next to each version triggers the rollback API (`wcon-profiles` §5.3).

### 5.4 Profile Import

**Flow:**
1. User clicks "Import" in the profile library.
2. File picker opens. User selects a YAML file.
3. The frontend reads the file and sends it to `POST /api/profiles/import`.
4. If valid: the imported profile appears in the library, and the profile detail view opens. If the response carries non-blocking warnings (e.g., `TOOL_HAS_RUNTIME_POLICY` for tools that have policies in the current taxonomy — `wcon-profiles` §3.2), the detail view shows them as inline notices below the affected tools. These are informational — the profile is saved and usable; the notices explain what the runtime will enforce at session launch.
5. If invalid: the profile editor opens pre-populated with the parsed fields, validation errors shown inline. The user fixes the errors and saves manually.

**Policy warnings on cross-instance import.** A profile exported on instance A and imported on instance B may pick up new `TOOL_HAS_RUNTIME_POLICY` warnings if B's taxonomy has policies for tools that did not have them on A. The import still succeeds — the Console treats policy warnings as informational, not blocking. This is the mechanism by which profile portability (`wcon-vision` G5) coexists with vertical-specific policy enforcement: the profile bundles operational parameters and travels unchanged, while runtime policies are looked up from the local taxonomy at load time.

## 6. Session Launcher

### 6.1 Session List

The default view for the Sessions surface. Shows all sessions with status indicators.

```
┌──────────────────────────────────────────────────────────────┐
│ Sessions                                        [+ New]      │
│                                                              │
│ State: [All ▼]  Vertical: [All ▼]                           │
│                                                              │
│ ┌────────────────────┬──────────┬──────────┬────────────────┐│
│ │ Session            │ Vertical │ State    │ Created        ││
│ ├────────────────────┼──────────┼──────────┼────────────────┤│
│ │ Auth Feature Build │ SWE      │ ● Active │ Apr 10 14:30  ││
│ │ Refactor DB Layer  │ SWE      │ ✓ Done   │ Apr 9 10:00   ││
│ │ Bug Fix #421       │ SWE      │ ✗ Failed │ Apr 8 16:45   ││
│ └────────────────────┴──────────┴──────────┴────────────────┘│
└──────────────────────────────────────────────────────────────┘
```

Active sessions show a live state indicator. Clicking an active session navigates to the oversight dashboard. Clicking a historical session shows the session detail view (configuration + final state).

### 6.2 Session Configuration Wizard

A multi-step flow for creating and launching a session. Accessed via "New" button or `/sessions/new`.

**Step indicators:** a horizontal progress bar showing all six steps, with the current step highlighted. Step 4 is dynamically skipped when the selected vertical has an empty `context_schema`.

```
  ● Vertical ── ● Workflow ── ● Assign ── ● Context ── ○ Overrides ── ○ Review
```

**Step 1: Select Vertical**

Each vertical from `GET /api/verticals` is rendered as a selectable card. The card's content is driven by the vertical manifest (`wcon-discovery` §2.2):

```
┌──────────────────────────────────────────────────────────────┐
│ Step 1: Select Vertical                                      │
│                                                              │
│ ┌──────────────────────────────────────────────────────────┐ │
│ │ Finance                                                  │ │
│ │ Regulatory pre-check + fiduciary duty — trade_execute    │ │
│ │ refuses without an approved compliance_check checkpoint  │ │
│ │ for the same trade_id (expires after 5 minutes).         │ │
│ │ Tasks: 9  Workflows: 4  Tools: 10                        │ │
│ └──────────────────────────────────────────────────────────┘ │
│ ┌──────────────────────────────────────────────────────────┐ │
│ │ Healthcare                                               │ │
│ │ HIPAA PHI access grant (consent/de-id) — clinical tools  │ │
│ │ refuse without a valid phi_access_grant checkpoint.      │ │
│ │ Tasks: 8  Workflows: 4  Tools: 9                         │ │
│ └──────────────────────────────────────────────────────────┘ │
│ ┌──────────────────────────────────────────────────────────┐ │
│ │ SWE                                                      │ │
│ │ DAG validation — software engineering vertical with      │ │
│ │ plan → implement → test → review workflow.               │ │
│ │ Tasks: 7  Workflows: 4  Tools: 8                         │ │
│ └──────────────────────────────────────────────────────────┘ │
│ ... (one card per vertical in the registry)                  │
│                                                              │
│                                                    [Next ▶]  │
└──────────────────────────────────────────────────────────────┘
```

| Field | Source | Rendering |
|-------|--------|-----------|
| Title | `VerticalSummary.name` | Large |
| Body | `VerticalSummary.defining_constraint` | Small, up to 4 lines, truncated with ellipsis |
| Counts line | `task_type_count`, `workflow_count`, `tool_count` | Single line at bottom of card |

The body text is the card's primary differentiator — what makes this vertical distinct, not what it contains. Count lines are secondary information.

Cards are listed alphabetically by id. Single selection. The list supports scrolling when more than four verticals are present. A search box at the top of the step filters cards by name or substring match against the defining constraint text.

**Step 2: Select Workflow**

Workflows from `VerticalEntry.workflows` are rendered as cards. Because the manifest carries only summary (`stage_count`, `gated_stage_count`) and not per-stage structure, the card shows counts and description rather than a stage diagram:

```
┌──────────────────────────────────────────────────────────────┐
│ Step 2: Select Workflow              Vertical: Finance       │
│                                                              │
│ ┌──────────────────────────────────────────────────────────┐ │
│ │ ● Trade Execution                                        │ │
│ │   Analyze → compliance → execute → record                │ │
│ │   4 stages  •  2 gated                                   │ │
│ ├──────────────────────────────────────────────────────────┤ │
│ │ ○ Full Report                                            │ │
│ │   Collect → analyze → risk → compliance → publish        │ │
│ │   5 stages  •  2 gated                                   │ │
│ ├──────────────────────────────────────────────────────────┤ │
│ │ ○ Client Onboarding                                      │ │
│ │   KYC → suitability → approve                            │ │
│ │   3 stages  •  3 gated  ⚠ All stages gated              │ │
│ ├──────────────────────────────────────────────────────────┤ │
│ │ ○ Portfolio Rebalance                                    │ │
│ │   Assess → propose → compliance → execute                │ │
│ │   4 stages  •  2 gated                                   │ │
│ └──────────────────────────────────────────────────────────┘ │
│                                              [◀ Back] [Next ▶]│
└──────────────────────────────────────────────────────────────┘
```

- The description line shows `WorkflowSummary.description` verbatim — this is the source of the arrow-separated stage name list used above.
- The counts line shows `stage_count` and `gated_stage_count`. When `gated_stage_count == stage_count` and the count is non-trivial (> 1), the card gets an "⚠ All stages gated" annotation — this signals to the user that every transition will require human approval and may be heavyweight (e.g., Finance `client-onboarding`, Healthcare `patient-assessment`).
- The wizard supports workflows with any `stage_count`. The earlier SWE-specific ASCII DAG format (hardcoded 4-stage Plan→Implement→Test→Review) is not used anywhere in the wizard; it exists only in vertical detail view (§4.5) when per-stage detail is available.

**Step 3: Assign Profiles**

```
┌──────────────────────────────────────────────────────────────┐
│ Step 3: Assign Profiles              Workflow: Trade Exec.   │
│                                                              │
│ Stage: Analyze                                               │
│ Role: finance:analyst                                        │
│ Profile: [Equity Analyst ▼]                   [+ Create New] │
│                                                              │
│ Stage: Compliance                                            │
│ Role: finance:compliance_officer                             │
│ Profile: [Compliance Lead ▼]                  [+ Create New] │
│                                                              │
│ Stage: Execute                                               │
│ Role: finance:portfolio_manager                              │
│ Profile: [Senior PM ▼]                        [+ Create New] │
│                                                              │
│ Stage: Record                                                │
│ Role: finance:auditor                                        │
│ Profile: [Audit Observer ▼]                   [+ Create New] │
│                                                              │
│                                              [◀ Back] [Next ▶]│
└──────────────────────────────────────────────────────────────┘
```

This layout is **Mode A** rendering per `wcon-sessions` §2.4 — each workflow stage is one row with its stage name and role. The profile selector dropdown is filtered to profiles matching the stage's role. Unassigned slots show a warning indicator. "Create New" opens an inline profile editor modal with the role pre-selected.

**Mode B rendering** (when per-stage metadata is not available, which is the current state of the REST contract): the Console shows one row per distinct role in `VerticalEntry.roles`, without the "Stage: X" header. Example:

```
┌──────────────────────────────────────────────────────────────┐
│ Step 3: Assign Profiles              Vertical: Finance       │
│                                                              │
│ Role: finance:analyst                                        │
│ Profile: [Equity Analyst ▼]                   [+ Create New] │
│                                                              │
│ Role: finance:compliance_officer                             │
│ Profile: [Compliance Lead ▼]                  [+ Create New] │
│                                                              │
│ Role: finance:portfolio_manager                              │
│ Profile: [Senior PM ▼]                        [+ Create New] │
│                                                              │
│ Role: finance:risk_officer                                   │
│ Profile: [Risk Officer ▼]                     [+ Create New] │
│                                                              │
│ Role: finance:auditor (autonomous observer)                  │
│ Profile: [Audit Observer ▼]                   [+ Create New] │
│                                                              │
│                                              [◀ Back] [Next ▶]│
└──────────────────────────────────────────────────────────────┘
```

The Console picks Mode A if per-stage metadata is available for the selected workflow; otherwise Mode B. The two modes produce different assignment record shapes (stage-scoped vs role-scoped) but the same validation and launch behavior (`wcon-sessions` §3.1, §4.1).

The "Next" button is disabled until all slots are filled.

**Step 4: Vertical Context** *(skipped when `context_schema` is empty)*

Dynamically generated from `VerticalEntry.context_schema` for the selected vertical. Each `ContextField` becomes one form field. The widget is chosen by `ContextField.type`; layout is vertically stacked, one field per row, with the field `description` as helper text.

```
┌──────────────────────────────────────────────────────────────┐
│ Step 4: Vertical Context             Vertical: Finance       │
│                                                              │
│ ── Required ──────────────────────────────────────────────── │
│ Compliance scope *                                           │
│ [equities                                                  ] │
│   Regulatory scope for trades in this session (e.g.          │
│   equities, fixed-income, derivatives).                      │
│                                                              │
│ Jurisdiction *                                               │
│ [ SEC ▼ ]                                                    │
│   Regulatory jurisdiction governing trades in this session.  │
│   Choices: SEC, FINRA, MiFID II, FCA, other                  │
│                                                              │
│                                              [◀ Back] [Next ▶]│
└──────────────────────────────────────────────────────────────┘
```

**Per-field widget mapping:**

| `ContextField.type` | Widget | Notes |
|---------------------|--------|-------|
| `"string"` | Text input | Single-line; multi-line only if `description` hints at it |
| `"number"` | Number input | Integer unless `description` says decimal; arrow buttons for step adjustment |
| `"boolean"` | Toggle switch | Labelled with the field name, helper text as the field description |
| `"enum"` | Dropdown | Choices populated from `enum_values`; always include the default value if present |

**Per-vertical widget notes** (guidance for known verticals; all content is manifest-driven, these are adaptations when the generic widget is not expressive enough):

- **DevOps — `environment` (enum).** Rendered as large radio buttons (`dev` / `staging` / `production`) rather than a dropdown — the choice is load-bearing and should be visible at a glance. When `production` is selected, a warning banner appears above the buttons: "Production environment: deploy_execute, rollback, and secret_rotate tools require explicit gate clearance at runtime."
- **MLOps — `compute_budget` (number).** Number input with a unit label ("GPU-hours") pulled from the field's description. Optional: show an estimated cost preview alongside the input, based on a configurable per-GPU-hour rate. The estimate is informational only.
- **Analytics — `data_snapshot_id` (string).** A searchable dropdown backed by an endpoint the runtime provides (or a fallback text input if no such endpoint exists). The dropdown lets the user pick from recent snapshot IDs; the text input allows manual entry of an unknown ID.
- **Healthcare — `phi_access_basis` (enum).** Two-option selector (card-style, side-by-side): `consent` vs `de_identified`. Each card describes the downstream implication so the user can choose informedly. Follow-up fields (patient_id, consent_scope for the consent basis; deidentification_method for the de-id basis) are **not** collected in this step — they are runtime-scoped and captured when the agent actually creates the `phi_access_grant` checkpoint.
- **Finance — `compliance_scope` and `jurisdiction`.** Text input for scope, dropdown for jurisdiction. Both live in the same step.
- **DataSci — `hypothesis_framework`.** Template dropdown backed by a known list of frameworks, or a link to a "Declare hypothesis" editor for free-form declarations. The declaration is stored as context text; the runtime creates the formal `declared_hypothesis` checkpoint separately.

For verticals the Console has not seen before (no hardcoded adaptation), the generic type-driven widgets above are used without any adaptation. The step is always driven by the manifest — the Console never assumes a field name.

**Validation:**
- Client-side: `required: true` fields must be non-null; `enum` fields must be in `enum_values`; `number` fields must parse; `boolean` fields must be explicitly toggled (no tri-state).
- Server-side (at launch): `MISSING_CONTEXT` / `INVALID_CONTEXT` per `wcon-sessions` §3.1.

**Step 5: Set Overrides (optional)**

```
┌──────────────────────────────────────────────────────────────┐
│ Step 5: Budget Overrides (optional)                          │
│                                                              │
│ ── Session-wide ──────────────────────────────────────────── │
│ Max cost: [$______]  Max tokens: [________]  Max time: [____]│
│                                                              │
│ ── Per-assignment (expand to override) ──────────────────── │
│ ▸ finance:analyst (Equity Analyst)                           │
│ ▸ finance:compliance_officer (Compliance Lead)               │
│ ▸ finance:portfolio_manager (Senior PM)                      │
│ ▸ finance:auditor (Audit Observer)                           │
│                                                              │
│                                              [◀ Back] [Next ▶]│
└──────────────────────────────────────────────────────────────┘
```

Session-level budget fields. Each assignment is expandable to reveal per-assignment overrides. All fields are optional — empty means "use profile default."

Note: MLOps `compute_budget` is **not** in this step. It was captured in step 4 as vertical context. Attempting to override it here would be a mistake — the override fields in step 5 are Console-enforced resource limits, not vertical-specific compute metrics.

**Step 6: Review and Launch**

```
┌──────────────────────────────────────────────────────────────┐
│ Step 6: Review                                               │
│                                                              │
│ Session name (optional): [Q1 Trade Pipeline              ]   │
│                                                              │
│ Vertical: Finance                                            │
│ Workflow: Trade Execution (4 stages, 2 gated)                │
│                                                              │
│ Context:                                                     │
│   compliance_scope: equities                                 │
│   jurisdiction: SEC                                          │
│                                                              │
│ Assignments:                                                 │
│ ┌────────────────────────┬────────────────┬────────────────┐ │
│ │ Stage                  │ Profile        │ Budget         │ │
│ ├────────────────────────┼────────────────┼────────────────┤ │
│ │ Analyze                │ Equity Analyst │ profile default│ │
│ │ Compliance             │ Compliance Lead│ profile default│ │
│ │ Execute                │ Senior PM v3   │ $2.00 (override│ │
│ │ Record                 │ Audit Obs v1   │ profile default│ │
│ └────────────────────────┴────────────────┴────────────────┘ │
│                                                              │
│ Session budget: $5.00 max cost, 30m max time                 │
│                                                              │
│ ⚠ Runtime: Connected                                         │
│                                                              │
│                            [Discard] [◀ Back] [🚀 Launch]    │
└──────────────────────────────────────────────────────────────┘
```

**Session name** is an optional text field at the top of the review step. When left empty, the dashboard derives a display name as `"{vertical} / {workflow}"` per `wcon-data-model` §4.1. The name is stored in `sessions.name` and shown in the oversight dashboard header, session list, and notification events.

**Discard button** cancels the session (transitions to `cancelled` state per `wcon-data-model` §4.3) and returns to the session list. Present on every wizard step, not just step 6. Labelled "Discard" in pre-launch states (`configuring`, `validating`) since no runtime resources exist yet.

Summary of the full configuration. The Context section is present only when `context_schema` is non-empty — it lists each field and its value verbatim. The launch button is prominent and distinct. Runtime connectivity is confirmed before enabling launch.

On launch click:
1. The wizard transitions to a "Launching..." state with a progress indicator.
2. If validation fails: the wizard returns to the relevant step with errors highlighted. `MISSING_CONTEXT` / `INVALID_CONTEXT` errors send the user back to step 4.
3. If launch succeeds: the wizard closes and the oversight dashboard opens for the new session.

## 7. Oversight Dashboard

Defined in detail in `wcon-highway` §8. This section specifies additional UX behavior.

### 7.1 Session Selector

When navigating to `/oversight`, the user sees a session selector if multiple sessions are active:

```
┌─────────────────────────────────────────────────────────────────────┐
│ Oversight — Select Session                                          │
│                                                                     │
│ Vertical filter: [All ▼]                                            │
│                                                                     │
│ ┌────────────────────┬────────────┬──────┬──────┬────────┬────────┐ │
│ │ Session            │ Vertical   │ Gates│ Esc. │ Refs.  │ Elapsed│ │
│ ├────────────────────┼────────────┼──────┼──────┼────────┼────────┤ │
│ │ Q1 Trade Pipeline  │ Finance    │ 1    │ 0    │ 1      │ 22m    │ │
│ │ Patient Intake     │ Healthcare │ 0    │ 0    │ 2      │ 8m     │ │
│ │ Auth Feature Build │ SWE        │ 2    │ 1    │ 0      │ 12m    │ │
│ └────────────────────┴────────────┴──────┴──────┴────────┴────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

The **Refs.** column counts `pending_refusals` (tool-layer refusals, `wcon-sessions` §6.1). The **Vertical filter** dropdown narrows the list to sessions of a selected vertical — useful when a team is running many concurrent Finance sessions and wants to ignore SWE sessions, or vice versa.

If only one session is active, the selector is skipped and the dashboard opens directly.

### 7.2 Dashboard Panels

The dashboard layout is defined in `wcon-highway` §8.1. Panel-specific behaviors:

**Session header — vertical context badges.** The session header shows the session name (or derived `"{vertical} / {workflow}"` if unnamed), state, elapsed time, a [Cancel Session] button, and a row of context badges derived from `session.context` (`wcon-sessions` §2.1 step 4). Each non-null field becomes one badge. The cancel button is visible only to the session owner and admins (`wcon-auth` §4.2), and only while the session is in a non-terminal state. Clicking it triggers `POST /api/sessions/:id/cancel` with a confirmation dialog. Examples:

```
Session: "Q1 Trade Pipeline"    State: ACTIVE    ⏱ 22m 15s    [Cancel Session]
[finance]  [scope=equities]  [jurisdiction=SEC]
```

```
Session: "Prod Deploy v4.2"    State: ACTIVE    ⏱ 3m 47s
[devops]  [env=production ⚠]
```

```
Session: "Training Run 017"    State: ACTIVE    ⏱ 41m 02s
[mlops]  [compute_budget=50 GPU-h]  [used=31.2 GPU-h]
```

```
Session: "Patient Intake"    State: ACTIVE    ⏱ 8m 33s
[healthcare]  [phi_basis=consent]
```

Badge rules:
- First badge is always the vertical name (clickable — navigates to vertical detail).
- Subsequent badges are context field values. The display uses `field=value` format. For booleans, show the field name when `true`, absent when `false`. For enums, show the value. For long strings, truncate with tooltip.
- A warning modifier (⚠) is appended for values the Console knows are sensitive — today this is the heuristic "env=production" for DevOps; other verticals can add their own heuristics.
- The header's usage counter (e.g., "used=31.2 GPU-h" for MLOps) is derived from trail state, not from `context`. It updates in real-time. The session monitor accumulates the metric per `wcon-sessions` §6.4 (Vertical-specific resource metrics) by scanning trail entries for tool invocations whose tool has a `budget_limited` policy and summing the `budget_field` argument values. The frontend reads the accumulated value from `GET /api/sessions/:id/state`.

**Workspace tree:**
- Clicking a workspace filters the trail stream to that workspace's entries.
- Double-clicking opens a workspace detail flyout showing full state, resource usage, and recent checkpoints.
- Resource usage bars fill toward the budget limit. Color changes: green (< 60%), yellow (60–80%), red (> 80%).
- **Refusal badge.** A workspace that is `BLOCKED` due to a tool-layer refusal (`wcon-sessions` §6.2) gets a red "🚫" badge next to its label. The badge color is distinct from the gate-blocked yellow and escalation-blocked orange — it needs its own treatment so the user can tell at a glance why a workspace is blocked.

**Trail stream:**
- Filter chips above the stream show active filters. Each chip has an X to remove it.
- The pause button shows a count of buffered entries while paused: "▶ Resume (47 new)".
- Entry expansion shows formatted JSON for envelope payloads and checkpoint content.
- **Vertical-specific checkpoint rendering.** When a trail entry records creation of a checkpoint whose type appears in the session's `VerticalEntry.checkpoint_types` (e.g., Finance `compliance_check`, Healthcare `phi_access_grant`, MLOps `reproducibility_checkpoint`, DataSci `declared_hypothesis`, Analytics `data_snapshot`), the entry is rendered with a structured field view rather than a raw JSON dump. The rendering is driven by the checkpoint type's `fields[]` schema (from `wcon-data-model` §6.1 `CheckpointSchema`):

  ```
  14:42:08 [finance:analyst (Equity Analyst)]  Checkpoint: compliance_check
  ┌─────────────────────────┬─────────────────────────────────────┐
  │ trade_id                │ TXN-2026-Q1-00847                   │
  │ instrument              │ AAPL                                │
  │ side                    │ buy                                 │
  │ quantity                │ 1200                                │
  │ status                  │ approved                            │
  │ regulation_cited        │ SEC Rule 15c3-5                     │
  │ forbidden_pattern_...   │ ✓                                   │
  │ suitability_verified    │ ✓                                   │
  │ kyc_current             │ ✓                                   │
  │ expires_at              │ 14:47:08 (in 5m)                    │
  └─────────────────────────┴─────────────────────────────────────┘
  ```

  Field types drive the rendering: booleans show ✓ / ✗, enums show the value verbatim, numbers formatted per locale, strings shown as-is, timestamps rendered relative to current time. The field order follows the `fields[]` array in the schema — not alphabetical, not JSON key order.

- **Tool-layer refusal rendering.** Trail entries with event type `tool_call_refused` (and matching status codes like `COMPLIANCE_NOT_APPROVED`, `PHI_ACCESS_NOT_GRANTED`, `HYPOTHESIS_NOT_DECLARED`, `COMPUTE_BUDGET_EXCEEDED`, `ENVIRONMENT_GATE_REQUIRED`) are rendered with a red left border, a 🚫 icon, and the refusal reason. Expanding the entry shows the policy that triggered the refusal, the tool-arg values, and a link to the Refusal panel (below) for the actionable view.

**Gate queue:**
- Gates pulse when timeout is below 20% remaining.
- Clicking a gate opens the gate detail overlay (`wcon-highway` §8.3).
- **Vertical rationale.** Gates carry vertical-specific rationale (from `wcon-highway` §4.7 gate rationale enrichment) — e.g., "Production environment: deploy requires gate clearance" for DevOps, "Insider-trading pattern detected" for Finance. The queue entry shows the rationale as a subtitle below the gate type.
- Keyboard shortcut: `G` focuses the gate queue. `Enter` opens the selected gate. `A` approves, `R` rejects (with confirmation dialog).

**Escalation inbox:**
- Clicking an escalation opens the escalation detail overlay.
- Keyboard shortcut: `E` focuses the escalation inbox. `Enter` opens the selected escalation.

**Refusals panel** (new):
- Lists entries from `pending_refusals` (`wcon-sessions` §6.1 / §6.5). Each row shows: workspace label, tool name, policy kind, error code, and a short description of what the user should do to unblock.
- Example rows:

  ```
  🚫 finance:portfolio_manager (Senior PM) — trade_execute
     COMPLIANCE_NOT_APPROVED
     Missing: compliance_check checkpoint with trade_id=TXN-00847 (expired 42s ago)
     Action: Agent must invoke compliance_check and obtain an approved checkpoint
     [View trail entry] [View policy]
  ```

  ```
  🚫 mlops:trainer (Training Runner) — train_launch
     COMPUTE_BUDGET_EXCEEDED
     Request: max_hours=80 exceeds session compute_budget=50
     Action: Cancel the session and relaunch with a higher compute_budget,
             or reduce the requested max_hours
     [View trail entry] [View policy]
  ```

- **The Console does not provide an "override" button for refusals.** The refusal panel's purpose is to explain *why* a workspace is blocked and *what needs to happen* to unblock it. Unblocking a `requires_checkpoint` refusal requires the agent to create the prerequisite checkpoint (usually via directive injection to the relevant workspace). Unblocking a `budget_limited` refusal typically requires cancelling and relaunching the session. Unblocking a `requires_gate` refusal requires resolving the corresponding gate. None of these are one-click operations; they require user judgment.
- Keyboard shortcut: `F` focuses the refusals panel.

**Quality report panel** (new, end-of-session only):
- When a session reaches `completed`, the dashboard shows a quality report panel below the trail stream showing the per-criterion verdict (pass/warn/fail) for each dimension in the vertical's `quality_criteria`. Example for Finance:

  ```
  ┌─ Quality Report (finance) ──────────────────────────────┐
  │ Regulatory Compliance    ✓ pass   weight 1              │
  │ Audit Trail Integrity    ✓ pass   weight 1              │
  │ Fiduciary Duty           ⚠ warn   weight 1              │
  │ Risk Disclosure          ✓ pass   weight 1              │
  │ Data Provenance          ✓ pass   weight 1              │
  │ Documentation            ✓ pass   weight 1              │
  │                                                         │
  │ Overall: 5 pass, 1 warn, 0 fail                         │
  └─────────────────────────────────────────────────────────┘
  ```

- Verdicts come from the runtime's quality report (a trail entry typically emitted by the vertical's autonomous observer agent at session end — e.g., Finance `auditor`, Healthcare `compliance`, etc.). The Console does not compute verdicts itself; it renders them from the trail. If no quality report trail entry arrived by session completion, the panel shows "Quality report not available" and is collapsible.

**Injection bar:**
- The target workspace is set by clicking a workspace in the tree, then clicking the injection bar.
- The bar shows the target workspace label: "Inject to: finance:portfolio_manager (Senior PM)".
- `Ctrl+Enter` sends the directive (with confirmation if targeting the coordinator).

### 7.3 Session Terminal State

When a session reaches a terminal state (completed, failed, cancelled):

1. The dashboard header updates with the final state and a color indicator.
2. The trail stream shows a final "Session [completed/failed/cancelled]" entry.
3. The gate queue, escalation inbox, and refusal panel are all cleared. Outstanding gates/escalations that the runtime did not resolve are recorded in the trail as "unresolved at session end"; outstanding refusals are simply gone because the workspaces they blocked no longer exist.
4. The injection bar is disabled.
5. The workspace tree shows final states for all workspaces.
6. A summary banner appears: elapsed time, total tokens consumed, total cost, tasks completed. For verticals with vertical-specific resource metrics (`wcon-sessions` §6.4), the banner also shows per-metric totals (e.g., "GPU-hours consumed: 31.2 / 50").
7. If a quality report trail entry arrived during the session, the Quality Report panel (§7.2) is displayed below the summary banner.

The dashboard remains viewable after terminal state — the user can scroll through the trail buffer and inspect workspace final states. The WebSocket connection closes.

## 7A. Auth Screens

Auth surfaces follow `wcon-auth` for behavior and `wcon-api` §3 for endpoints.

### 7A.1 Login Screen

Full-page centered form (no sidebar). Username + password fields, "Log In" button. Error messages appear inline: "Invalid credentials", "Account locked — try again in N minutes". The login screen is the only surface visible to unauthenticated users. All other routes redirect here when the session cookie is absent or expired.

### 7A.2 Forced Password Change

Full-page form (no sidebar). Shown when the user's `must_change_password` flag is true (bootstrap flow or admin-initiated reset). Fields: current password, new password, confirm new password. Inline validation enforces the password policy (`wcon-auth` §7.2). The user cannot navigate away — all other routes return `403 PASSWORD_CHANGE_REQUIRED` until the change succeeds.

### 7A.3 API Tokens Screen

Accessed from the user menu. Lists the authenticated user's tokens with name, created date, and last-used date. Actions: "Create Token" (name input → displays the full token once → dismiss), "Revoke" (confirmation required). Revoked tokens are shown with a strikethrough for the current page load, then disappear on refresh.

### 7A.4 User Management (Admin Only)

Accessed from the "Users" sidebar nav item (visible to admins only). Entity list of all users with: username, display name, console role, status (active/disabled), created date. Actions per user: edit (display name, console role), disable/enable, reset password. Guard: disabling or demoting the last active admin shows a warning and the action is blocked (`wcon-auth` §2.2).

### 7A.5 Audit Log Viewer (Admin Only)

Accessed from the "Audit Log" sidebar nav item (visible to admins only). Filterable table of audit log entries (`wcon-api` §3.8). Filters: user, action, target type, date range. Each row shows: timestamp, username, action, target, IP. Clicking a row expands to show the `detail` JSON payload. Paginated via the standard cursor model.

### 7A.6 Permission-Gated Affordances

The frontend hides UI controls the user lacks permission for. This is a convenience, not a security boundary — the backend enforces access independently (`wcon-auth` §4.3).

| Console role | Hidden affordances |
|-------------|-------------------|
| `viewer` | "New Profile" button, profile edit/delete/clone/import, "New Session" button, session launch/cancel, gate approve/reject, escalation respond, directive inject, settings save |
| `operator` | "Users" and "Audit Log" nav items, user management screens, other users' private profiles, other users' sessions, settings save |
| `admin` | — (nothing hidden) |

The permission check uses the `console_role` from the `GET /api/auth/whoami` response, fetched once at app startup and cached in client state. If the whoami call fails (expired session), the app redirects to the login screen.

## 8. Settings

### 8.1 Settings Screen

A single-page form organized by category.

```
┌──────────────────────────────────────────────────────────────┐
│ Settings                                                     │
│                                                              │
│ ── Runtime Connection ───────────────────────────────────── │
│ AgentService:      [[::1]:9090               ]               │
│ HighwayService:    [[::1]:9091               ]               │
│ CoordinatorService:[[::1]:9092               ]               │
│ REST gateway:      [http://[::1]:9093        ]               │
│   (GET /v1/verticals — per ADR-001)                          │
│ Auth method: [None ▼]                                        │
│ Status: ● All 4 endpoints connected          [Test Connection]│
│                                                              │
│ ── Data Sources ─────────────────────────────────────────── │
│ Taxonomy path: [../wacp/protocol/taxonomy   ]                │
│   (protocol-level base/derived roles and envelope types)     │
│ Export directory: [./exports                 ]                │
│                                              [Reload Taxonomy]│
│                                                              │
│ ── Interface ────────────────────────────────────────────── │
│ Theme: (●) System  (○) Light  (○) Dark                       │
│ Trail buffer size: [1000  ]                                  │
│                                                              │
│                                                    [Save]    │
└──────────────────────────────────────────────────────────────┘
```

The Runtime Connection section has **two addresses**: gRPC (for sessions, agents, highway) and REST (for vertical manifests, per ADR-001 in `SPEC_BUILD.md`). Both are required for full functionality; the Console can start with either missing but session launch and discovery respectively require them.

There is **no vertical manifests path** in Console settings. Vertical manifests are served by the runtime's REST endpoint and are not a Console filesystem concern (`wcon-discovery` §2.2, ADR-001). Earlier drafts included a `Verticals path` field — it was removed when the runtime took responsibility for serving the registry.

**Behaviors:**
- "Test Connection" sends a gRPC health check and a REST `GET /v1/verticals` probe to the configured addresses and shows both results inline (OK / unreachable / auth failed).
- "Reload Taxonomy" triggers `POST /api/taxonomy/reload` and shows the reload status (success, partial, failed) with entity counts per `wcon-discovery` §7.4. Reload fetches both protocol-taxonomy files and vertical manifests atomically.
- Changes are saved on "Save" click, not on blur. Unsaved changes show a warning when navigating away.

## 9. Component Patterns

### 9.1 Shared Components

| Component | Usage | Behavior |
|-----------|-------|----------|
| Entity list | Role list, tool list, profile library, session list | Paginated table with filters, sort, and cursor-based loading. "Load more" button at bottom. |
| Detail panel | Role detail, tool detail, profile detail | Structured field layout with cross-reference links. Consistent header with action buttons. |
| Form | Profile editor, session overrides, settings | Label-above-input layout. Inline validation on blur. Disabled submit when invalid. |
| Searchable dropdown | Role selector, profile selector | Dropdown with text filter. Shows matching items from the API. Keyboard navigable. |
| Confirmation dialog | Delete, launch, inject | Modal dialog with action description, cancel button, and confirm button. Destructive confirms use red button. |
| Toast notification | All surfaces | Transient notification banner. Auto-dismiss (5s normal, sticky for high priority). Stacks vertically. |
| Empty state | Any list with no results | Illustration + message + action link (e.g., "No profiles yet. Create one."). |
| Loading state | Any data-dependent view | Skeleton placeholders matching the layout shape. No spinners on individual fields. |
| Error state | API failures | Inline error banner with retry action. Does not replace the entire screen. |

### 9.2 Overlay Pattern

Gate detail and escalation detail use a slide-in overlay from the right edge. The overlay covers roughly 60% of the width, leaving the left sidebar and a portion of the underlying dashboard visible. Clicking outside the overlay or pressing `Escape` closes it.

Refusal detail uses the same slide-in overlay pattern as gates and escalations — each refusal row in the Refusals panel is clickable, opening the overlay with the full `RefusalEvent` (see `wcon-highway` §4A.2) including the policy reference, the tool args, the unblock hint, and a link to the originating trail entry. Unlike gate/escalation overlays which have "Approve / Reject" or "Respond" actions, the refusal overlay has only navigational actions — "View trail entry", "View policy detail", and (if applicable) "Inject directive to this workspace" or "Cancel and clone session". There is no "resolve refusal" action because the Console cannot directly resolve refusals (see `wcon-highway` §4A.4).

### 9.3 Modal Pattern

Confirmation dialogs and inline profile creation use centered modals with a backdrop. The modal traps focus (keyboard users cannot tab out). `Escape` closes the modal (unless it contains unsaved changes, in which case a secondary confirmation is shown).

## 10. Responsive Design

### 10.1 Target Environment

The Console is designed for desktop use (`wcon-vision` §3, NG6). The primary target is a 1280px+ viewport width. The oversight dashboard requires significant screen real estate for its multi-panel layout.

### 10.2 Breakpoints

| Breakpoint | Width | Layout adaptation |
|-----------|-------|-------------------|
| Desktop (primary) | ≥ 1280px | Full layout — sidebar nav, two-panel discovery, multi-panel dashboard |
| Small desktop | 1024–1279px | Sidebar collapses to icon-only rail. Dashboard panels stack vertically below workspace tree. |
| Tablet | 768–1023px | Sidebar becomes a hamburger menu. Discovery uses single-panel (list or detail, not both). Dashboard panels are tabbed instead of side-by-side. |
| Below 768px | Not supported | Banner: "WACP Console requires a wider viewport" |

### 10.3 Dashboard Adaptations

At the small desktop breakpoint, the oversight dashboard reorganizes:

```
┌────────────────────────────────────────────────────────┐
│ Session header (with context badges)                   │
├────────────┬───────────────────────────────────────────┤
│ Workspace  │ Trail stream                              │
│ tree       │                                           │
│            │                                           │
│            ├───────────────────────────────────────────┤
│            │ Tabs: [Gates (2)] [Escalations (1)]       │
│            │        [Refusals (1)]                     │
│            │                                           │
│            │ ⚠ task_approval — timeout 4m              │
│            │ ○ integration — timeout 9m                │
│            │                                           │
│            ├───────────────────────────────────────────┤
│            │ [Inject directive...]                     │
└────────────┴───────────────────────────────────────────┘
```

Gate queue, escalation inbox, and refusal panel merge into a tabbed container below the trail stream. Each tab shows its count as a badge. Empty tabs are dimmed but still visible (they do not disappear) so the user can see at a glance that there are no pending items in that category.

The Refusals tab is hidden entirely when the session has no pending refusals AND the session's vertical has no `tool_policies` (e.g., SWE sessions — the tab would never have content, so hiding it reduces clutter). In verticals with tool policies (Finance, Healthcare, MLOps, DevOps, Analytics, DataSci), the tab is always shown, with count 0 when empty.

## 11. State Management

### 11.1 Client-Side State

The frontend manages three categories of state:

| Category | Examples | Storage | Lifetime |
|----------|----------|---------|----------|
| Server state | Profiles, sessions, taxonomy data, settings | Fetched from API, cached in memory | Until stale or navigated away |
| UI state | Active tab, selected entity, scroll position, filter selections, open/closed panels | In-memory | Surface lifetime (preserved across tab switches) |
| Persistent preferences | Theme, notification settings, trail buffer size, panel sizes | Browser local storage | Cross-session (survives page reload) |

### 11.2 Data Fetching

| Pattern | Used by | Behavior |
|---------|---------|----------|
| Fetch on mount | Lists, detail views, settings | Fetch data when the component mounts. Show loading skeleton. Cache for the surface lifetime. |
| Fetch on action | Profile save, session launch, gate resolve, refusal navigation | Submit to API on user action. Show loading state on the action button. On success, update the local cache. |
| Real-time push | Oversight dashboard | Connect WebSocket on mount. Process incoming events from the `trail`, `gates`, `escalations`, `refusals`, `workspaces`, `session`, and `notification` channels (`wcon-api` §12.2). Update local state per channel. Reconnect on disconnect. |
| Optimistic update | Profile delete, setting change | Update local state immediately, send API request. On failure, revert and show error. |

### 11.3 Cache Invalidation

- Profile list cache is invalidated on profile create, update, delete, import, or clone.
- Session list cache is invalidated on session create, launch, cancel, or clone.
- Taxonomy data cache is invalidated on taxonomy reload.
- The frontend does not poll for changes — invalidation is triggered by user actions or WebSocket events.

## 12. Invariants

### 12.1 No Silent Data Loss

Every destructive action (delete, cancel, overwrite) requires explicit confirmation. Bulk actions show a count of affected items. No action results in data loss without the user's knowledge.

### 12.2 Validation Before Submission

No form submits invalid data to the API. Client-side validation mirrors server-side rules. The Save/Launch/Submit button is disabled when validation errors are present. Server-side validation is the authority — if the server rejects a request that the client allowed, the error is shown inline.

### 12.3 Keyboard Accessibility

Every interactive element is reachable via Tab. Every action is triggerable via keyboard (Enter, Space, or documented shortcut). Focus is managed correctly through modals and overlays (trap focus in modal, restore focus on close).

### 12.4 Real-Time Fidelity

The oversight dashboard displays every event received through the WebSocket. No events are silently dropped by the frontend. Filtering is a display concern — filtered events are received and buffered, just not shown.

### 12.5 State Preservation

Navigating away from a surface and back restores the surface's state (scroll position, filters, open editors) as the user left it. The exception is the oversight dashboard, which always shows the current real-time state on return.

### 12.6 Manifest-Driven Rendering

Every screen that displays vertical-scoped content is rendered from the current `VerticalEntry` (via `wcon-api`), not from hardcoded per-vertical logic in the frontend:

- Session wizard step 1 cards are driven by `VerticalSummary` fields.
- Session wizard step 2 workflow cards are driven by `WorkflowSummary` fields.
- Session wizard step 4 form fields are driven by `ContextField` entries in `context_schema`.
- Vertical detail sections (§4.5) are driven by the full `VerticalEntry` projection.
- Dashboard context badges (§7.2) are driven by the session's `config.context` map.
- Dashboard vertical-specific checkpoint rendering (§7.2) is driven by `CheckpointSchema.fields` for the matching type.
- Tool-policy indicators (§5.2) are driven by `ToolEntry.policy`.

A new vertical added to the runtime — with a new context field, new tool policy, or new checkpoint type — is rendered correctly on the next taxonomy reload without any Console code change. The per-vertical widget notes in §6.2 step 4 are refinements that add polish (radio buttons vs dropdown, warning banners, custom helpers); they are not required for correctness, and a vertical without a matching refinement uses the generic type-driven widget.

This invariant is the UI-layer enforcement of `wcon-vision` SC8 and BC4 and is verified by the `wcon-test` fixture strategy (one simple vertical without extended fields, one complex vertical with all extended fields).

### 12.7 Permission-Gated Rendering

The frontend never renders actionable UI controls for operations the authenticated user cannot perform. Viewers never see "New Profile"; operators never see "Users". The permission check uses the `console_role` from the whoami response (§7A.6). The backend independently enforces every check — the frontend gating is a UX convenience, not a security boundary (`wcon-auth` §4.3).

### 12.8 Refusal Non-Actionability

The frontend never presents UI affordances that would override a tool-layer refusal (`wcon-highway` §4A). Refusal rows, overlays, and detail views are strictly informational with navigational actions (view trail, view policy, inject directive, cancel+clone). No "Override refusal" button, no "Force retry" button, no "Grant exception" button. The reason: enforcement is exclusively the runtime's responsibility, and the Console's role is to explain the refusal, not to short-circuit it.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-auth | Authentication & Authorization | defines login flow, permission matrix, and audit log — §7A of this spec implements the auth UI surfaces |
| wcon-api | API Surface | defines all endpoints consumed by the frontend (§3, §6–§12) |
| wcon-highway | Highway Integration | defines oversight dashboard layout (§8), gate resolution UX (§4), escalation UX (§5), refusal rendering (§4A, §8), notification model (§9) |
| wcon-discovery | Agent & Role Discovery | defines browsing UX model (§6), search model (§5), vertical manifest schema (§2.2), tool policy surfacing (§3.5) |
| wcon-data-model | Data Model | defines `VerticalEntry.context_schema`, `tool_policies`, `checkpoint_types`, `quality_criteria` (§6.1) consumed by vertical detail and wizard rendering |
| wcon-profiles | Profile System | defines profile lifecycle (§2), validation (§3), versioning (§5), import/export (§7), policy-aware tool validation (§3.2) |
| wcon-sessions | Session Lifecycle | defines configuration steps (§2), launch sequence (§4), monitoring (§6), vertical context validation (§3.1), refusal tracking (§6.1, §6.5) |
| wcon-architecture | System Architecture | defines frontend surfaces (§4.2) |
| wcon-vision | Product Vision | establishes desktop-first (NG6), four core capabilities (§2), vertical-agnosticism (BC4) |

*WACP Console -- authored by AKIL Abderrahim and Claude Opus 4.6*
