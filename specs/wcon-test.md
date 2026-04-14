---
id: wcon-test
type: design
status: final
created: 2026-04-10T00:00:00
revised: 2026-04-11T00:00:00
authors: [AKIL Abderrahim, Claude Opus 4.6]
tags: [testing, quality, ci, verticals]
depends_on: [wcon-architecture, wcon-api, wcon-auth, wcon-profiles, wcon-sessions, wcon-highway, wcon-data-model]
---

# WACP Console — Test Strategy

## Table of Contents

1. Overview
2. Test Layers
3. Backend Unit Tests
4. Frontend Unit Tests
5. Integration Tests
6. End-to-End Tests
7. Test Data Management
8. CI Pipeline
9. Coverage and Quality Gates
10. Invariants

---

## 1. Overview

The Console is a two-tier application (Rust backend + browser SPA) that integrates with an external system (WACP runtime) via gRPC. This creates three testing boundaries:

1. **Backend logic** — profile validation, session state machines, taxonomy parsing, query handling. Testable in isolation with no runtime dependency.
2. **Backend ↔ runtime** — gRPC calls that create workspaces, deliver directives, stream events. Requires either a live runtime or a mock gRPC server.
3. **Frontend ↔ backend ↔ runtime** — full user flows from UI action to runtime effect. Requires all three tiers running.

The test strategy allocates effort where bugs are most likely and most costly:

| Layer | Volume | Speed | Catches |
|-------|--------|-------|---------|
| Backend unit | High | Fast (ms) | Logic errors, validation bugs, state machine violations, parsing failures |
| Frontend unit | Medium | Fast (ms) | Component rendering, form validation, state management |
| Integration | Medium | Moderate (s) | gRPC contract mismatches, serialization errors, stream handling |
| End-to-end | Low | Slow (10s+) | User flow breakage, cross-tier regressions, real-time event delivery |

## 2. Test Layers

### 2.1 Layer Diagram

```
┌─────────────────────────────────────────────────────────┐
│ E2E Tests                                               │
│  Browser ──▶ Backend ──▶ Runtime (or mock runtime)      │
│  Full user flows: discover → profile → session → gate   │
├─────────────────────────────────────────────────────────┤
│ Integration Tests                                       │
│  Backend ──▶ Mock gRPC Server                           │
│  gRPC call correctness, stream handling, reconnection   │
├─────────────────────────────────────────────────────────┤
│ Frontend Unit Tests          │ Backend Unit Tests        │
│  Component render + behavior │ Service logic, validation │
│  Mock API responses          │ In-memory DB, mock index  │
└──────────────────────────────┴──────────────────────────┘
```

### 2.2 Test Independence

Each layer runs independently:

- Backend unit tests require no running services — no runtime, no database process (SQLite is embedded).
- Frontend unit tests require no running backend — API responses are mocked.
- Integration tests require no running runtime — a mock gRPC server is started in-process.
- E2E tests require all tiers but use a dedicated test configuration.

## 3. Backend Unit Tests

### 3.1 Scope

Backend unit tests cover the four services (`wcon-architecture` §4.1) and the infrastructure layer.

| Module | What to test | What not to test |
|--------|-------------|-----------------|
| Profile Store | Validation logic, version management, CRUD operations, import/export parsing, tool allowlist/denylist resolution | gRPC calls (none in this module) |
| Session Manager | State machine transitions, configuration validation, assignment logic, budget precedence resolution | gRPC workspace creation (integration layer) |
| Taxonomy Index | YAML parsing, index construction, query filtering, search ranking, role-tool resolution, reload atomicity | Filesystem access patterns (use in-memory strings) |
| Highway Bridge | Event enrichment logic, gate/escalation state tracking, notification generation | gRPC stream consumption (integration layer) |
| Infrastructure | Configuration loading, health check logic, authentication middleware, pagination cursor encoding/decoding | HTTP server binding, WebSocket frame handling |

### 3.2 Test Patterns

**Profile validation tests:**

Test each validation rule from `wcon-profiles` §3 in isolation:

```
test_valid_profile_passes_validation
test_unknown_role_rejected                                // UNKNOWN_ROLE
test_unknown_tool_in_allowlist_rejected                   // UNKNOWN_TOOL
test_tool_from_different_vertical_rejected                // TOOL_NOT_IN_ROLE_VERTICAL (violation in allowlist)
test_tool_from_different_vertical_in_denylist_warns       // TOOL_NOT_IN_ROLE_VERTICAL (warning in denylist)
test_base_role_allowlist_rejected                         // base roles have no vertical tools; any non-empty allowlist fails
test_empty_effective_tool_set_rejected                    // EMPTY_TOOL_SET for vertical roles
test_base_role_empty_tool_set_accepted                    // EMPTY_TOOL_SET check skipped for base roles
test_duplicate_name_rejected_among_own_live                // DUPLICATE_NAME scoped to owner's own live profiles
test_duplicate_name_of_soft_deleted_accepted              // reusing a soft-deleted profile's name succeeds
test_same_name_different_users_allowed                   // per-user uniqueness; two users can have "Default SWE"
test_shared_profile_display_name_prefixed                // shared profile shows "{owner}'s {name}" to other users
test_temperature_out_of_range_rejected
test_null_optional_fields_accepted
test_multiple_violations_returned
```

**Session state machine tests:**

Test every valid transition and every invalid transition from `wcon-data-model` §4.3:

```
test_configuring_to_validating
test_validating_to_launching
test_validating_to_configuring_on_failure
test_launching_to_active
test_configuring_to_cancelled
test_validating_to_cancelled
test_launching_to_failed
test_launching_to_cancelled
test_active_to_completed
test_active_to_failed
test_active_to_cancelled
test_terminal_state_rejects_transition  // for each terminal state × each target state
```

**Taxonomy parsing and ingestion tests:**

```
test_parse_protocol_taxonomy_yaml                         // filesystem path
test_ingest_vertical_manifest_simple                      // fixture-simple: empty maps
test_ingest_vertical_manifest_complex                     // fixture-complex: all extended fields
test_base_roles_always_present
test_base_roles_have_empty_tools                          // base roles have no vertical, no tools
test_derived_role_inherits_base
test_role_tool_vertical_coarse_consistency                // bidirectional within a vertical, not cross-vertical
test_vertical_list_endpoint_empty_yields_empty_registry   // GET /v1/verticals returns []
test_vertical_detail_endpoint_404_recorded_as_stub        // one vertical fails to fetch, others succeed
test_runtime_unreachable_on_startup_yields_warning        // Console starts with empty registry
test_malformed_manifest_yaml_skipped_with_warning         // partial reload
test_duplicate_vertical_id_rejected                        // runtime should not serve duplicates, but defensive check
test_deterministic_build                                   // same REST responses → same index
test_console_sorts_verticals_by_id                         // determinism does not depend on runtime sort

// Extended manifest schema tests
test_context_schema_field_types_parsed                    // string, number, boolean, enum
test_context_schema_enum_values_preserved
test_context_schema_default_values_preserved
test_tool_policy_requires_checkpoint_parsed               // checkpoint_type, matching_field, expires_after_ms
test_tool_policy_requires_gate_parsed                     // gate_condition
test_tool_policy_budget_limited_parsed                    // budget_field
test_tool_policy_classification_gated_parsed              // blocked_classifications, override_flag
test_checkpoint_schema_field_list_parsed
test_checkpoint_schema_enum_field_parsed
test_tool_policy_cross_reference_resolution               // tool → checkpoint type link (within vertical)
test_tool_policy_unresolved_reference_warning             // policy refers to undeclared checkpoint type
test_tool_policy_cross_vertical_reference_not_resolved    // reference to another vertical's checkpoint type is unresolved
test_unknown_manifest_field_preserved_opaque              // forward compatibility — unknown fields in raw_manifest
test_unknown_enum_value_preserved_opaque                  // e.g., ToolPolicyKind beyond the known four
test_vertical_entry_roles_synthesized_from_profiles       // profiles[].role_id dedup → VerticalEntry.roles
test_vertical_roles_sorted_lexicographically              // determinism
test_vertical_with_empty_context_schema_valid             // fixture-simple baseline
test_vertical_with_all_extended_fields_valid              // fixture-complex
test_same_named_checkpoint_types_in_two_verticals         // no collision — vertical-scoped
```

**Vertical context validation tests (new):**

```
test_session_launch_missing_required_context_rejected   // MISSING_CONTEXT on fixture-complex without scope/jurisdiction
test_session_launch_context_enum_out_of_range_rejected  // INVALID_CONTEXT: jurisdiction="Z" rejected (not in fixture-complex enum)
test_session_launch_context_wrong_type_rejected         // INVALID_CONTEXT: scope=42 (number sent for string field)
test_session_launch_context_strict_type_no_coercion     // "50" (string) sent for a number field → INVALID_CONTEXT
test_session_launch_context_empty_schema_accepts_null   // fixture-simple has no context
test_session_launch_extra_unknown_context_field_ignored // forward compatibility — soft warning, not violation
test_session_create_with_context                        // POST /api/sessions with context body
test_session_create_without_context_ok_on_empty_schema  // fixture-simple — context field omitted from create body
test_session_patch_replaces_context_wholesale           // partial update not supported
test_session_patch_context_after_launch_rejected        // 409 Conflict once session left configuring
test_session_clone_default_policy_resets_context        // fixture-complex uses the default "reset all required" per wcon-sessions §9.5
test_session_clone_simple_no_context_to_copy            // fixture-simple clone has no context field
```

**Tool-layer refusal relay tests (new):**

```
test_refusal_trail_entry_detected_by_monitor           // COMPLIANCE_NOT_APPROVED
test_refusal_added_to_pending_refusals
test_refusal_event_forwarded_via_websocket             // channel "refusals"
test_session_channel_emits_lifecycle_events            // session_active, session_completed, session_cancelled
test_workspaces_channel_emits_state_changes            // workspace created, state transition, closed
test_notification_channel_emits_gate_alert             // new gate → notification event
test_notification_channel_emits_escalation_alert       // new escalation → notification event
test_notification_channel_emits_refusal_alert          // new refusal → notification event
test_notification_channel_emits_timeout_warning        // gate timeout approaching → high-priority notification
test_refusal_policy_reference_resolved_from_index
test_refusal_unblock_hint_generated_per_policy_kind
test_refusal_cleared_when_prerequisite_checkpoint_created
test_refusal_cleared_when_workspace_transitions_out
test_refusal_cleared_when_tool_retry_succeeds
test_refusal_cleared_when_session_cancelled
test_refusal_budget_limited_rendering                  // COMPUTE_BUDGET_EXCEEDED
test_refusal_requires_gate_rendering                   // ENVIRONMENT_GATE_REQUIRED
test_refusal_classification_gated_rendering            // CLASSIFICATION_BLOCKED
test_unknown_refusal_code_surfaces_as_generic_refusal  // forward compatibility
test_workspace_blocked_classification_correct          // gate vs escalation vs refusal
```

**Autonomous observer profile validation tests (new):**

```
test_autonomous_observer_profile_saves_without_warning       // fixture-complex:auditor
test_autonomous_worker_with_policy_gated_tool_warns          // narrowed rule fires
test_autonomous_worker_with_high_impact_tool_warns           // narrowed rule fires
test_autonomous_worker_with_read_only_tools_no_warning       // narrowed rule does not fire
test_assisted_worker_with_policy_gated_tool_no_autonomy_warn // rule is autonomy-gated
```

**Policy-aware tool validation tests (new):**

```
test_policy_gated_tool_allowlist_saves_with_warning     // TOOL_HAS_RUNTIME_POLICY
test_policy_gated_tool_warning_includes_policy_fields
test_profile_save_never_blocks_on_policy_gated_tool
test_export_omits_policy_metadata                       // round-trip fidelity
test_import_surfaces_new_policy_warnings                // profile imported on instance with new policy for an allowed tool
```

**Soft delete tests (new):**

```
test_delete_profile_sets_deleted_at                     // soft delete updates all version rows
test_delete_profile_preserves_session_assignment_fk     // FK still resolves to the deleted rows
test_list_profiles_excludes_deleted                     // GET /api/profiles filters deleted_at IS NULL
test_get_profile_by_id_excludes_deleted                 // GET /api/profiles/:id returns 404 for deleted
test_session_detail_shows_deleted_profile_name          // historical session detail still displays the profile
test_delete_active_session_profile_returns_409          // UX safeguard even though soft delete would technically succeed
test_duplicate_name_after_soft_delete_allowed           // reusing a deleted profile's name succeeds
test_rollback_on_deleted_profile_returns_404            // cannot rollback a deleted profile
test_versions_endpoint_deleted_profile_returns_404      // version history hidden for deleted profiles
```

**Authentication and authorization tests (`wcon-auth`):**

```
test_login_valid_credentials_returns_session_cookie      // 200 + Set-Cookie: wcon_sid
test_login_invalid_password_returns_401
test_login_disabled_user_returns_401
test_login_forces_password_change_when_flagged           // must_change_password=1 → 403 PASSWORD_CHANGE_REQUIRED
test_logout_clears_session
test_api_token_auth_valid_bearer                         // Authorization: Bearer wcon_t_...
test_api_token_auth_revoked_token_rejected
test_csrf_required_on_cookie_state_changing_request      // POST without CSRF → 403
test_csrf_not_required_on_bearer_auth                    // API tokens exempt
test_rate_limit_per_ip_20_attempts                       // 429 after 20 failed logins from same IP
test_rate_limit_per_account_5_failed                     // 401 ACCOUNT_LOCKED after 5 failed
test_rate_limit_auto_unlock_after_window                 // lockout expires after 15 min
test_bootstrap_first_launch_creates_admin                // empty users → bootstrap credential generated
test_bootstrap_credential_forces_password_change
test_viewer_cannot_create_profile                        // 403
test_viewer_cannot_launch_session                        // 403
test_operator_cannot_manage_users                        // 403
test_operator_cannot_view_audit_log                      // 403
test_operator_cannot_view_others_private_profiles        // filtered from list
test_operator_cannot_cancel_others_sessions              // 403
test_admin_can_view_all_sessions                         // includes other users' sessions
test_admin_can_manage_all_profiles                       // edit/delete regardless of ownership
test_last_admin_cannot_be_demoted                        // LAST_ADMIN error
test_audit_log_entry_created_on_profile_create
test_audit_log_entry_created_on_session_launch
test_audit_log_entry_created_on_gate_resolution
test_audit_log_append_only                               // no UPDATE/DELETE on audit_log table
```

**Session cancellation from all non-terminal states:**

```
test_launch_with_deleted_profile_rejected                 // DELETED_PROFILE_IN_ASSIGNMENT
test_delete_profile_warns_about_configuring_sessions     // 200 with warnings array
test_delete_profile_notifies_session_owner               // notification channel event

test_cancel_from_configuring                             // immediate discard, no runtime cleanup
test_cancel_from_validating                              // immediate discard
test_cancel_from_launching                               // best-effort workspace cleanup
test_cancel_from_active                                  // abort via CoordinatorService
test_cancel_from_completed_returns_409                   // terminal state
test_cancel_from_failed_returns_409
test_cancel_from_cancelled_returns_409
```

**Session assignment stage_id tests (new):**

```
test_mode_a_assignment_carries_stage_id                 // per-stage slot has stage_id populated
test_mode_b_assignment_has_null_stage_id                // per-role fallback slot has stage_id NULL
test_assignment_slot_position_unique_per_session        // unique index enforces ordering
test_assignment_slot_iteration_order_deterministic      // launch iterates in slot_position order
```

**Discovery browser tests for non-SWE verticals (new):**

```
test_vertical_detail_renders_defining_constraint
test_vertical_detail_renders_context_schema_table       // fixture-complex
test_vertical_detail_omits_context_schema_when_empty    // fixture-simple
test_vertical_detail_renders_tool_policies_table
test_vertical_detail_renders_checkpoint_type_schemas
test_vertical_detail_renders_quality_criteria_with_weight
test_workflow_card_5_stages_layout                      // handles variable stage count
test_workflow_card_all_gated_shows_banner               // all-gated workflow
test_session_wizard_step1_shows_defining_constraint_body
test_session_wizard_step2_card_from_summary_only        // without per-stage detail
test_session_wizard_step4_skipped_when_context_empty
test_session_wizard_step4_generates_form_from_schema
test_session_wizard_step4_enum_field_renders_dropdown
test_session_wizard_step4_required_field_blocks_next
```

**Event enrichment tests:**

```
test_workspace_id_enriched_to_label
test_unknown_workspace_id_passes_through_raw
test_task_id_enriched_to_name
test_cache_miss_triggers_lookup
```

### 3.3 Database Tests

Backend unit tests that involve persistence use an in-memory SQLite database. Each test creates a fresh database, applies the schema, and runs against it. No shared state between tests.

```rust
// Pattern: each test gets a clean database
fn test_db() -> Database {
    let db = Database::open_in_memory().unwrap();
    db.apply_migrations().unwrap();
    db
}
```

### 3.4 Taxonomy Index Tests

Tests that need a taxonomy index build one from inline data structures — no filesystem access, no REST calls. The protocol-taxonomy parser is tested separately with string inputs. The vertical manifest ingester is tested with pre-built `VerticalManifest` objects (as if freshly deserialized from the REST endpoint). The index builder is tested with pre-parsed data.

```rust
// Pattern: build index from test data (using fixture-complex shape for vertical roles/tools)
fn test_index() -> TaxonomyIndex {
    TaxonomyIndexBuilder::new()
        // Protocol taxonomy: base/derived roles, protocol envelope/checkpoint types
        .add_base_roles()  // always present
        .add_derived_role("swe:implementer", "worker")
        // Vertical manifest: add a full VerticalManifest; the builder takes care of
        // populating RoleEntry.tools and ToolEntry.roles vertically-coarse per §3.4.
        .add_vertical(fixture_simple_manifest())
        .add_vertical(fixture_complex_manifest())
        .build()
}

// Helper: build a test vertical manifest
fn fixture_simple_manifest() -> VerticalManifest {
    VerticalManifest {
        id: "fixture-simple".into(),
        name: "Fixture Simple".into(),
        defining_constraint: "SWE-like baseline".into(),
        context_schema: HashMap::new(),  // empty
        tool_policies: HashMap::new(),   // empty
        checkpoint_types: HashMap::new(),// empty
        task_types: vec![...],
        workflows: vec![...],
        profiles: vec![profile_summary("fixture-simple:implementer", "gated"), ...],
        tools: vec![tool_summary("simple_exec"), ...],
        quality_criteria: vec![...],
    }
}
```

Tests should **not** construct `RoleEntry.tools` directly — that field is populated by the builder from the vertical manifest's `tools[]` per `wcon-discovery` §3.4. Directly populating it bypasses the §3.4 relaxation semantics and will produce a bogus index that does not match what the Console builds at runtime.

## 4. Frontend Unit Tests

### 4.1 Scope

Frontend unit tests cover component rendering, user interaction, and state management. They do not test API integration — API responses are mocked.

| Area | What to test |
|------|-------------|
| Components | Rendering with various props, conditional display, empty/loading/error states |
| Forms | Field validation, disabled state logic, form submission with valid/invalid data |
| State management | Cache invalidation triggers, optimistic update + rollback, filter/sort state |
| Navigation | Route matching, surface state preservation, breadcrumb generation |
| WebSocket handling | Event dispatch to correct handlers, reconnection logic, channel subscription |

### 4.2 Test Patterns

**Component rendering tests:**

```
test_profile_library_renders_empty_state
test_profile_library_renders_profile_list
test_profile_library_shows_invalid_indicator_for_stale_profile
test_gate_queue_orders_by_urgency_then_timeout
test_trail_entry_expansion_shows_payload
test_workspace_tree_color_codes_states
```

**Form validation tests:**

```
test_profile_editor_disables_save_when_name_empty
test_profile_editor_clears_tools_on_role_change
test_profile_editor_shows_inline_error_for_invalid_temperature
test_session_wizard_disables_next_when_slots_unfilled
```

**State management tests:**

```
test_profile_cache_invalidated_on_create
test_navigation_preserves_profile_list_filters
test_websocket_reconnect_fetches_current_state
test_optimistic_delete_reverts_on_api_failure
```

### 4.3 Mock API

Frontend tests use a mock API layer that returns predefined responses. The mock matches request patterns (method + path) and returns configured responses.

```typescript
// Pattern: configure mock API per test
const api = mockApi()
  .on("GET", "/api/profiles", { items: [testProfile()], has_more: false })
  .on("POST", "/api/profiles", { status: 201, body: testProfile() })
  .on("DELETE", "/api/profiles/uuid-1", { status: 204 });
```

## 5. Integration Tests

### 5.1 Scope

Integration tests verify the Console backend's interaction with the WACP runtime — both the gRPC API (sessions, agents, highway) and the REST API (vertical manifests, per ADR-001). They test client code, request serialization, response deserialization, and streaming behavior on both transports.

| Interaction | Transport | Tests |
|------------|-----------|-------|
| Vertical registry load | REST | Backend sends correct `GET /v1/verticals` and `GET /v1/verticals/{id}` requests; handles success, 404 on one vertical, 500, connection error |
| Vertical manifest deserialization | REST | Backend correctly deserializes `VerticalManifest` including all extended fields (context_schema, tool_policies, checkpoint_types); tolerates unknown fields |
| Workspace creation | gRPC | Backend sends correct `CreateSession` / `Dispatch` requests; handles success and error responses |
| Directive delivery | gRPC | Backend constructs correct directive envelopes from profile data (including `context` pass-through); handles delivery acknowledgment and rejection |
| Trail streaming | gRPC | Backend subscribes, receives events, processes them correctly, handles stream interruption and reconnection |
| Gate streaming | gRPC | Backend receives gate events, enriches them with `vertical_context`, routes to correct session |
| Gate resolution | gRPC | Backend translates user decisions into correct `RespondToGate` calls |
| Escalation handling | gRPC | Backend translates responses into correct `RespondToEscalation` calls |
| Directive injection | gRPC | Backend translates injection requests into correct `InjectEnvelope` calls |
| Refusal detection | Internal | Backend observes refusal trail entries on StreamTrail, constructs RefusalEvents, resolves policy from index, emits on `refusals` channel |
| Health check | Both | Backend detects runtime availability on both transports and reports correct health status |

### 5.2 Mock Runtime

Integration tests run against a mock runtime that implements both the gRPC service interfaces (`AgentService`, `CoordinatorService`, `HighwayService`) and the REST endpoints (`GET /v1/verticals`, `GET /v1/verticals/{id}`). The mock:

- Runs in the same process as the test (in-process Tonic server for gRPC, in-process Axum or equivalent for REST).
- Accepts configurable responses per RPC method and per REST endpoint.
- Records received requests on both transports for assertion.
- Supports streaming RPCs with configurable event sequences.
- Can simulate failures on either transport: connection refused, timeout, error status codes (gRPC) or HTTP 4xx/5xx (REST).
- Preloads fixture verticals (fixture-simple, fixture-complex) so the backend's vertical registry load at startup works out of the box.

```rust
// Pattern: configure mock runtime (both transports)
let mock = MockRuntime::new()
    // REST side: preload verticals
    .with_vertical(fixture_simple_manifest())
    .with_vertical(fixture_complex_manifest())
    // gRPC side: session lifecycle
    .on_create_session(Ok(CreateSessionResponse { session_id: "s1".into(), ... }))
    .on_dispatch(Ok(DispatchResponse { workspace_id: "ws1".into(), ... }))
    .on_stream_trail(vec![trail_entry_1, trail_entry_2])
    .on_stream_gates(vec![gate_event_1]);

let server = mock.start().await;  // binds gRPC + REST on random ports
let backend = start_backend_with_runtime(
    server.grpc_addr(),
    server.rest_addr(),
).await;
```

The mock's REST and gRPC surfaces are independent — a test can simulate "gRPC down, REST up" (to exercise the degraded health state) or "REST down, gRPC up" (to exercise the empty-vertical-registry path).

### 5.3 Integration Test Cases

**Vertical registry load (REST):**

```
test_registry_load_fetches_list_endpoint           // backend calls GET /v1/verticals at startup
test_registry_load_fetches_detail_per_vertical     // for each listed id, GET /v1/verticals/{id}
test_registry_load_empty_list_valid                // runtime returns [], backend starts with empty registry
test_registry_load_single_vertical_404_stubbed    // one vertical fails, others succeed, stub entry recorded
test_registry_load_all_verticals_succeed          // happy path with fixture-simple + fixture-complex
test_registry_load_runtime_unreachable_degraded   // connection refused — backend starts with empty registry, health=degraded
test_registry_load_invalid_json_logged_skipped    // malformed response for one vertical, others succeed
test_registry_load_console_sorts_by_id            // backend sorts regardless of runtime response order
test_taxonomy_reload_refreshes_both_sources       // protocol taxonomy parse + REST fetch, atomic swap
test_taxonomy_reload_rest_failure_retains_old     // protocol parse succeeds but REST fails — reload fails, old index kept
```

**Session launch sequence:**

```
test_launch_creates_coordinator_workspace
test_launch_creates_worker_workspaces_in_order
test_launch_delivers_directives_with_correct_payloads
test_launch_subscribes_to_all_four_streams
test_launch_fails_if_coordinator_creation_fails
test_launch_fails_if_directive_delivery_fails
test_launch_maps_workspace_ids_to_assignments
```

**Stream handling:**

```
test_trail_events_forwarded_to_websocket_clients
test_gate_events_added_to_pending_queue
test_escalation_events_added_to_inbox
test_workspace_state_change_updates_session_state
test_stream_reconnects_on_disconnect
test_stream_reconnect_fails_after_max_attempts
test_session_fails_on_coordinator_workspace_failure
```

**Gate resolution:**

```
test_approve_gate_sends_correct_grpc_request
test_reject_gate_sends_correct_grpc_request
test_modify_gate_includes_modifications
test_batch_resolve_sends_individual_requests
test_gate_resolve_failure_returns_502
```

**Profile-to-WACP mapping:**

```
test_directive_payload_contains_llm_config
test_directive_payload_contains_effective_tool_set
test_workspace_budget_uses_assignment_override
test_workspace_budget_uses_session_override_when_no_assignment_override
test_workspace_budget_uses_profile_default_when_no_overrides
test_system_prompt_from_vertical_default_profile
```

### 5.4 Backend API Tests

Integration tests also cover the REST API layer — HTTP request handling, routing, serialization, and error responses. These tests start the full backend (with mock runtime) and make HTTP requests.

```
test_create_profile_returns_201
test_create_profile_with_invalid_role_returns_422
test_delete_profile_assigned_to_active_session_returns_409
test_launch_session_with_missing_assignments_returns_422
test_inject_directive_rate_limit_returns_429
test_health_endpoint_reports_degraded_when_runtime_unreachable
```

## 6. End-to-End Tests

### 6.1 Scope

E2E tests exercise full user flows through a real browser against a running backend and runtime (or comprehensive mock runtime). They verify that the tiers work together and that user-visible behavior matches the spec.

### 6.2 Test Environment

| Component | E2E setup |
|-----------|-----------|
| Frontend | Served by the backend (production-like) |
| Backend | Full binary with test configuration |
| Runtime | Mock runtime with scripted scenarios (or a real WACP runtime for a small set of smoke tests) |
| Database | Temporary SQLite file, fresh per test suite run |
| Taxonomy | Fixture taxonomy files with known roles, tools, verticals |
| Browser | Headless browser driven by test framework |

### 6.3 User Flow Tests

**Discovery flow:**

```
test_e2e_browse_roles_and_view_detail
  1. Navigate to /discover/roles
  2. Assert role list shows expected roles from fixture-simple AND fixture-complex
     (base roles + 4 fixture-simple roles + 5 fixture-complex roles)
  3. Click "fixture-simple:implementer"
  4. Assert detail panel shows role definition, tools (all tools in fixture-simple),
     and vertical membership (fixture-simple)

test_e2e_browse_vertical_detail_complex
  1. Navigate to /discover/verticals/fixture-complex
  2. Assert defining_constraint banner visible
  3. Assert context_schema table shows 2 fields (scope, jurisdiction)
  4. Assert tool_policies table shows 3 rows
  5. Assert checkpoint_types section shows compliance_check with its 8 fields
  6. Assert quality_criteria section shows 5 criteria
  7. Assert all 3 workflows rendered with correct stage/gate counts

test_e2e_browse_vertical_detail_simple
  1. Navigate to /discover/verticals/fixture-simple
  2. Assert defining_constraint banner visible
  3. Assert context_schema table is HIDDEN (empty schema)
  4. Assert tool_policies table is HIDDEN (empty policies)
  5. Assert checkpoint_types section is HIDDEN (empty)
  6. Assert workflows render with correct stage counts

test_e2e_search_across_entities
  1. Type "analyst" in search box
  2. Assert results show fixture-complex:analyst (role)
  3. Click role result
  4. Assert navigates to role detail
  5. Clear search, type "compliance"
  6. Assert results show fixture-complex:compliance_check (task type or checkpoint type)
     and fixture-complex:compliance_check (vertical checkpoint type)
```

**Profile flow:**

```
test_e2e_create_edit_export_import_profile
  1. Navigate to /profiles/new
  2. Fill in name, select role, configure LLM settings
  3. Save
  4. Assert profile appears in library
  5. Click profile, click Edit
  6. Change temperature
  7. Save
  8. Assert version is 2
  9. Export profile
  10. Delete profile
  11. Import the exported YAML
  12. Assert imported profile has same fields, version 1, new ID
```

**Session flow — simple vertical (SWE-like baseline):**

```
test_e2e_configure_launch_monitor_session_simple
  1. Navigate to /sessions/new
  2. Select vertical: fixture-simple
  3. Select workflow: implement-feature
  4. Assign profiles to all role slots
  5. Assert step 4 (vertical context) is skipped (empty context_schema)
  6. Click Launch
  7. Assert redirected to oversight dashboard
  8. Assert workspace tree shows all workspaces
  9. Assert trail stream receives events
  10. Wait for gate event from mock runtime
  11. Approve gate
  12. Assert gate removed from queue
  13. Assert workspace resumes
  14. Mock runtime completes session
  15. Assert session shows "completed" state
```

**Session flow — complex vertical (finance/healthcare-like):**

```
test_e2e_configure_launch_monitor_session_complex
  1. Navigate to /sessions/new
  2. Select vertical: fixture-complex
  3. Select workflow: trade-execution
  4. Assign profiles to all role slots (including autonomous observer — no warning should fire)
  5. Step 4: vertical context appears
  6. Fill scope="equities"
  7. Attempt Next without selecting jurisdiction → assert Next disabled
  8. Select jurisdiction="A"
  9. Click Next
  10. Launch
  11. Assert oversight dashboard header shows badges: [fixture-complex] [scope=equities] [jurisdiction=A]
  12. Wait for compliance_check checkpoint creation in trail stream
  13. Assert checkpoint rendered with structured field table (not generic JSON)
  14. Assert complex_execute succeeds and session completes
  15. Assert quality_report panel renders with per-criterion verdicts

test_e2e_tool_layer_refusal_complex
  1. Launch fixture-complex trade-execution session
  2. Mock runtime: agent invokes complex_execute without prior compliance_check
  3. Assert refusal appears in refusals panel
  4. Assert refusal panel shows: tool_name=complex_execute, error_code=COMPLIANCE_NOT_APPROVED,
     unblock_hint mentions compliance_check checkpoint
  5. Inject directive to fixture-complex:officer asking for compliance_check
  6. Mock runtime: officer creates compliance_check checkpoint
  7. Mock runtime: agent retries complex_execute — succeeds
  8. Assert refusal removed from refusals panel
  9. Assert trail stream shows both the refusal and the resolution

test_e2e_all_gated_workflow_complex
  1. Navigate to /sessions/new
  2. Select vertical: fixture-complex
  3. Select workflow: client-onboarding
  4. Assert workflow card shows "All-gated workflow" banner (all 3 stages gated)
  5. Continue through wizard, launch session
  6. Assert every transition raises a gate event
  7. Approve all gates in order
  8. Session completes

test_e2e_variable_stage_workflow_complex
  1. Navigate to fixture-complex → full-report workflow (5 stages)
  2. Assert workflow card shows "5 stages • 2 gated"
  3. Assert wizard step 3 generates assignment slots for all 5 stages
  4. Launch
  5. Assert oversight dashboard workspace tree shows 5 worker workspaces plus coordinator
```

**Gate resolution flow:**

```
test_e2e_gate_approval_with_modification
  1. Launch session (setup)
  2. Mock runtime sends task_approval gate
  3. Assert gate appears in queue
  4. Click gate to open detail
  5. Select "Modify"
  6. Edit task description
  7. Submit
  8. Assert gate resolved, trail shows modification
```

**Escalation flow:**

```
test_e2e_escalation_response
  1. Launch session (setup)
  2. Mock runtime sends escalation event
  3. Assert escalation appears in inbox
  4. Click escalation to open detail
  5. Type response
  6. Submit
  7. Assert escalation resolved, workspace resumes
```

### 6.4 Negative Flow Tests

```
test_e2e_launch_with_runtime_disconnected
  1. Stop mock runtime
  2. Attempt to launch session
  3. Assert validation error: "Runtime unreachable"

test_e2e_profile_validation_in_editor
  1. Navigate to profile editor
  2. Select a role
  3. Deny all tools for that role
  4. Assert Save button disabled, error: "Empty tool set"

test_e2e_session_cancel
  1. Launch session
  2. Click Cancel in dashboard header
  3. Confirm in dialog
  4. Assert session shows "cancelled" state
  5. Assert injection bar disabled
```

## 7. Test Data Management

### 7.1 Fixture Taxonomy

A fixture taxonomy provides deterministic, known data for all test layers. It is a minimal but complete taxonomy with enough entities to exercise all query, filter, and validation paths. It is **not** the SWE vertical — it is a synthetic, controlled dataset designed for test predictability.

**Two fixture verticals are the minimum.** When only a single fixture vertical existed, the Console's SWE-shaped assumptions (no context schema, no tool policies, no vertical-specific checkpoint types) leaked into code paths that would break on any other vertical. Two fixtures — one simple, one complex — are required to catch those leaks.

**Fixture vertical 1: `fixture-simple` (SWE-like baseline).**

| Entity | Count | Examples |
|--------|-------|---------|
| Roles | 4 | `fixture-simple:planner`, `fixture-simple:implementer`, `fixture-simple:tester`, `fixture-simple:reviewer` |
| Tools | 6 | `simple_read`, `simple_write`, `simple_exec`, `simple_search`, `simple_test`, `simple_deploy` |
| Task types | 3 | `fixture-simple:implement`, `fixture-simple:refactor`, `fixture-simple:debug` |
| Workflows | 2 | `implement-feature` (4 stages, 2 gated), `refactor` (3 stages, 0 gated) |
| Quality criteria | 3 | `correctness`, `test_coverage`, `documentation` |
| Default profiles | 4 | one per role, mix of `gated` and `autonomous` |
| Context schema | **empty** | — |
| Tool policies | **empty** | — |
| Checkpoint types | **empty** | — |

Purpose: the "simple path." Tests that exercise discovery browsing, profile CRUD, SWE-style session launch, and gate-based oversight can use this vertical. Anything that assumes an SWE-shaped vertical should work against `fixture-simple` without special casing.

**Fixture vertical 2: `fixture-complex` (finance/healthcare-like).**

| Entity | Count | Examples |
|--------|-------|---------|
| Roles | 5 | `fixture-complex:analyst`, `fixture-complex:officer`, `fixture-complex:executor`, `fixture-complex:auditor` (autonomous observer), `fixture-complex:reviewer` |
| Tools | 7 | `complex_read`, `complex_analyze`, `complex_pre_check`, `complex_execute`, `complex_deploy`, `complex_train`, `complex_audit` |
| Task types | 3 | `fixture-complex:trade`, `fixture-complex:report`, `fixture-complex:audit` |
| Workflows | 3 | `trade-execution` (4 stages, 2 gated), `full-report` (5 stages, 2 gated), `client-onboarding` (3 stages, **3 gated — all-gated**) |
| Quality criteria | 5 | `regulatory_compliance`, `audit_trail_integrity`, `fiduciary_duty`, `risk_disclosure`, `documentation` |
| Default profiles | 5 | four `gated`, one `autonomous` (auditor) |
| Context schema | 2 fields | `scope` (string, required), `jurisdiction` (enum, required: `A` / `B` / `C` / `other`) |
| Tool policies | 3 | `complex_execute` (requires_checkpoint → `compliance_check`, matching `trade_id`, expires 300000ms); `complex_train` (budget_limited → budget_field `max_hours`); `complex_deploy` (requires_gate → `environment_gate_clearance`) |
| Checkpoint types | 2 | `compliance_check` (8 fields mirroring Finance's real shape), `environment_gate_record` (3 fields) |

Purpose: the "complex path." Tests that exercise the vertical context wizard step, tool-layer refusals, vertical-specific checkpoint rendering, autonomous observer validation, policy-gated tool warnings, and non-trivial workflow shapes (including an all-gated workflow) use this vertical. This fixture is designed to exercise every code path that §3 and §4 of the vertical-expansion review identified as load-bearing.

**Base fixtures used by both.**

| Entity | Count | Purpose |
|--------|-------|---------|
| Base roles | 3 | coordinator, worker, observer (protocol constants, always present) |
| Envelope types | 2 | `directive` (base), `test:report` (custom, taxonomy-registered) |
| Protocol-level checkpoint types | 2 | `artifact` (base), `test:review` (custom) |

**Fixture location and delivery.** The fixture taxonomy YAMLs for base roles and protocol-level types live in the test fixtures directory (read directly from the filesystem during tests). The two fixture verticals are served via the mock runtime's REST endpoints (`GET /v1/verticals` and `GET /v1/verticals/{id}`) — they do not live on the Console's filesystem, because the Console no longer reads vertical manifests from disk (ADR-001). The test harness starts the mock runtime with both fixture verticals pre-loaded before each test that needs them.

### 7.2 Fixture Profiles

Pre-built profiles for test scenarios. Roles map to the fixture verticals defined in §7.1; no legacy `test:alpha` / `test:beta` names are used.

| Name | Role | Autonomy | Purpose |
|------|------|----------|---------|
| simple-valid-profile | `fixture-simple:implementer` | assisted | Standard valid profile for CRUD tests against the simple baseline |
| simple-autonomous-profile | `fixture-simple:reviewer` | autonomous | Tests autonomous observer behavior (no warning should fire — reviewer's base_role is observer) |
| simple-budget-profile | `fixture-simple:implementer` | assisted | All budget fields set, for budget precedence tests |
| complex-policy-profile | `fixture-complex:executor` | assisted | Allowlist includes `complex_execute` (policy-gated); tests `TOOL_HAS_RUNTIME_POLICY` warning path |
| complex-autonomous-observer | `fixture-complex:auditor` | autonomous | Autonomous observer — the narrowed rule (`wcon-profiles` §3.3) should not fire |
| complex-autonomous-worker | `fixture-complex:executor` | autonomous | Autonomous worker with policy-gated tool — the narrowed rule SHOULD fire as a warning |
| invalid-role-profile | `nonexistent:role` | — | YAML fixture for import validation failure tests (UNKNOWN_ROLE) |
| cross-vertical-tool-profile | `fixture-simple:implementer` with allowlist=`["complex_execute"]` | assisted | YAML fixture for TOOL_NOT_IN_ROLE_VERTICAL violation on import |

### 7.3 Fixture Sessions

Pre-configured session states for tests that need to start from a specific point in the session lifecycle:

| Fixture | State | Purpose |
|---------|-------|---------|
| configured-session | configuring | Has vertical, workflow, and full assignments |
| active-session | active | Launched with mock workspace IDs |
| completed-session | completed | Terminal state for history view tests |
| failed-session | failed | Terminal state with failure reason |

### 7.4 Mock Runtime Scenarios

Scripted event sequences for integration and E2E tests:

| Scenario | Vertical | Events | Purpose |
|----------|----------|--------|---------|
| simple-happy | fixture-simple | launch → workspaces active → checkpoints → complete | SWE-style normal lifecycle |
| simple-gate | fixture-simple | launch → gate event → resolve → resume → complete | Gate resolution with fixture-simple |
| simple-escalation | fixture-simple | launch → escalation → respond → resume | Escalation handling |
| simple-failure | fixture-simple | launch → coordinator fails | Session failure path |
| simple-reconnect | fixture-simple | launch → stream disconnect → reconnect → resume | Stream reconnection |
| complex-happy | fixture-complex | launch (with context) → workspaces active → compliance_check checkpoint → complex_execute succeeds → quality_report → complete | Full complex lifecycle exercising context and checkpoint types |
| complex-refusal-checkpoint | fixture-complex | launch → complex_execute called without prior compliance_check → refusal event (COMPLIANCE_NOT_APPROVED) → compliance_check checkpoint created → complex_execute retried and succeeds → refusal cleared → complete | requires_checkpoint refusal resolution |
| complex-refusal-budget | fixture-complex | launch (compute_budget=50) → complex_train called with max_hours=80 → refusal event (COMPUTE_BUDGET_EXCEEDED) | budget_limited refusal — no automatic recovery, user must cancel+clone |
| complex-refusal-gate | fixture-complex | launch → complex_deploy called → refusal event (ENVIRONMENT_GATE_REQUIRED) → gate appears → gate approved → refusal cleared → complete | requires_gate refusal resolution |
| complex-autonomous-observer | fixture-complex | launch → fixture-complex:auditor watches workspaces read-only → session completes → quality_report emitted | Autonomous observer pattern end-to-end |
| complex-all-gated-workflow | fixture-complex | launch `client-onboarding` → every stage gate appears → all approved → complete | Variable-stage-count + all-gated workflow rendering |

Each scenario is defined as a scripted sequence the mock runtime replays when the test harness calls the corresponding setup helper. The scenarios cover both fixture verticals so that regressions in either path (simple baseline, complex extended) are caught in CI.

## 8. CI Pipeline

### 8.1 Pipeline Stages

```
┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐
│  Lint    │──▶│  Unit    │──▶│ Integr.  │──▶│  E2E     │
│          │   │  Tests   │   │  Tests   │   │  Tests   │
└──────────┘   └──────────┘   └──────────┘   └──────────┘
     │              │              │              │
     ▼              ▼              ▼              ▼
   fail-fast     fail-fast     fail-fast     report
```

| Stage | Contents | Duration target | Blocking |
|-------|----------|----------------|----------|
| Lint | Rust clippy (zero warnings), frontend linter, type check, format check | < 2 min | Yes — all must pass |
| Unit tests | Backend unit tests + frontend unit tests (parallel) | < 5 min | Yes — all must pass |
| Integration tests | Backend integration tests with mock runtime (gRPC + REST in-process) | < 5 min | Yes — all must pass |
| E2E tests | Full-stack E2E tests with headless browser against mock runtime (gRPC + REST) | < 15 min | Yes — all must pass |

Each stage is fail-fast: if any check fails, later stages do not run.

### 8.2 Parallel Execution

Within each stage, tests run in parallel where possible:

- Backend unit tests: Rust's `cargo test` runs tests in parallel by default. Each test uses its own in-memory SQLite database — no shared state.
- Frontend unit tests: test runner parallelizes across test files.
- Integration tests: each test starts its own mock runtime (both gRPC and REST surfaces) on random ports — no port conflicts.
- E2E tests: run sequentially (shared browser state, shared backend instance).

### 8.3 CI Environment

| Concern | Approach |
|---------|----------|
| Rust toolchain | Pinned via `rust-toolchain.toml` |
| Node/frontend toolchain | Pinned via `.node-version` or equivalent |
| Database | In-memory SQLite (unit), temporary file (integration, E2E) |
| Runtime | Mock runtime with both gRPC (Tonic in-process) and REST (Axum or equivalent in-process) surfaces. Preloaded with fixture-simple and fixture-complex manifests. No real WACP runtime in CI. |
| Protocol taxonomy | Fixture YAML files checked into the repo under `tests/fixtures/taxonomy/` |
| Vertical manifests | Fixture `VerticalManifest` structs constructed in test code and served by the mock runtime's REST endpoints — no fixture YAMLs for verticals on disk (mirrors the ADR-001 architecture) |
| Browser | Headless browser installed in CI image |
| Caching | Cargo build cache + frontend dependency cache between runs |

## 9. Coverage and Quality Gates

### 9.1 Coverage Targets

| Layer | Target | Measurement |
|-------|--------|-------------|
| Backend unit | 80% line coverage | `cargo-llvm-cov` or equivalent |
| Frontend unit | 70% line coverage | Framework coverage reporter |
| Integration | No line coverage target — measured by scenario coverage | All gRPC RPCs exercised, all error paths tested |
| E2E | No line coverage target — measured by flow coverage | All primary user flows covered |

Coverage is measured and reported but not gated in CI initially. The target is a guideline, not a hard gate — coverage is useful for identifying untested code, not for proving correctness.

### 9.2 Quality Gates

| Gate | Threshold | Blocking |
|------|-----------|----------|
| Lint (Rust clippy) | Zero warnings | Yes |
| Lint (frontend) | Zero warnings | Yes |
| Type check (frontend) | Zero errors | Yes |
| Format check | `cargo fmt --check` + frontend formatter | Yes |
| Backend unit tests | 100% pass | Yes |
| Frontend unit tests | 100% pass | Yes |
| Integration tests | 100% pass | Yes |
| E2E tests | 100% pass | Yes |
| Coverage regression | Coverage must not decrease by more than 2% from main branch | Warning only |

### 9.3 Test Quality Rules

1. **No `#[ignore]` without a tracking issue.** Ignored tests must have a comment with a link to an issue explaining why they are disabled and when they will be re-enabled.
2. **No `sleep` in tests.** Tests that need to wait for async events use channels, condition variables, or test-framework-provided waiting utilities — not wall-clock delays.
3. **No shared mutable state between tests.** Each test constructs its own fixtures. No global test setup that mutates state across tests.
4. **Assertion messages.** Assertions include descriptive messages that identify what failed and what was expected, not just `assert!(x)`.

## 10. Invariants

### 10.1 Test Isolation

Every test is independent. Running tests in any order, running a single test in isolation, or running all tests in parallel produces the same results. No test depends on the side effects of another test.

### 10.2 Deterministic Results

Tests produce the same pass/fail result on every run given the same code. No flaky tests. Tests that depend on timing use deterministic event sequences, not wall-clock waits. Tests that exhibit non-determinism are either fixed or removed — never ignored.

### 10.3 Fixture Completeness

The fixture taxonomy contains at least one instance of every entity type: base role, derived role, tool, envelope type, protocol-level checkpoint type, vertical-specific checkpoint type, vertical (two, with different shapes), workflow (with varying stage counts including a 5-stage and an all-gated workflow), task type, quality criterion, context field (at least one per `ContextField.type` variant), and tool policy (at least one per `ToolPolicyKind` variant).

No test should need to construct its own taxonomy from scratch unless testing taxonomy parsing specifically. Tests that depend on a specific policy kind (e.g., `classification_gated` when fixture-complex does not ship one) are expected to construct targeted fixtures inline using the taxonomy index builder, documenting why the shared fixture is insufficient.

**The two-vertical rule.** Every test that touches vertical-scoped behavior (context schema, tool policies, checkpoint types, workflows, quality criteria) must run against `fixture-complex` unless it is explicitly verifying SWE-baseline behavior (then it uses `fixture-simple`). No test should run against only one fixture vertical without justification — the vertical expansion taught the project that single-vertical testing hides leaky assumptions.

### 10.4 Mock Fidelity

The mock runtime implements the same protobuf contracts as the real WACP runtime for gRPC services AND the same REST contracts (`GET /v1/verticals`, `GET /v1/verticals/{id}`) defined in `wcon-discovery` §2.2. Request validation in the mock matches the real service on both transports. If the upstream protobuf or REST schema changes, the mock must be updated in the same commit — compile-time type checking via generated protobuf bindings enforces this for gRPC; for REST, the mock uses the same `VerticalManifest` Rust struct as the runtime (from the `wacp-taxonomy` crate), so schema drift is caught at compile time.

### 10.5 CI Parity

Tests that pass locally must pass in CI, and vice versa. The CI environment mirrors the local development environment in toolchain versions, database configuration, and fixture data. Environment-specific test failures are treated as bugs in the test infrastructure, not in the application.

## References

| ID | Title | Relationship |
|----|-------|--------------|
| wcon-architecture | System Architecture | defines backend components (§4.1), concurrency model (§7), infrastructure layer (§4.1) |
| wcon-api | API Surface | defines all REST endpoints and WebSocket protocol tested by integration and E2E layers |
| wcon-data-model | Data Model | defines schemas tested by backend unit tests (§3–§5), `VerticalEntry` with extended fields tested by §3.2 parsing tests |
| wcon-discovery | Agent & Role Discovery | defines the REST-based vertical ingestion pipeline (§2.2) exercised by the mock-runtime test fixtures |
| wcon-profiles | Profile System | defines validation rules (§3) tested exhaustively in backend unit tests, including policy-aware tool warnings and narrowed autonomous-observer rule |
| wcon-sessions | Session Lifecycle | defines state machine (§4.3), launch sequence (§4.1), context validation (§3.1), and refusal tracking (§6.3) tested in integration tests |
| wcon-highway | Highway Integration | defines gate resolution (§4), escalation handling (§5), refusal events (§4A), event enrichment (§7) tested across all layers |
| wcon-ui | UI Design | defines wizard steps, oversight dashboard panels, and context badges tested by frontend unit tests and E2E flows |

*WACP Console -- authored by AKIL Abderrahim and Claude Opus 4.6*
