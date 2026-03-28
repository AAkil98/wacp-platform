# Task 17.3: E2E Highway Integration Tests

## Scope

E2E tests for highway interactions: gate approval, envelope injection, and migration lifecycle.

## Crate

`wacp-coordinator` (tests)

## Tests

| Test | Verifies |
|------|----------|
| `e2e_gate_approval_flow` | GateController opens gate → handler resolves with Approve → gate resolved. |
| `e2e_gate_rejection` | Gate opened → Reject → gate resolved with rejection. |
| `e2e_envelope_injection` | Handler injects envelope to workspace → envelope ID allocated. |
| `e2e_migration_full_lifecycle` | Active workspace → start_migration → MigrateBegin → snapshot event → bind with correct agent → MigrationComplete → Active. Migration context cleaned up. |
| `e2e_migration_timeout` | Start migration → timeout expires → fail_migration → workspace Failed. |

## Acceptance Criteria

- Gate approval/rejection via handler works E2E.
- Envelope injection via handler works.
- Migration runs end-to-end: start → snapshot → bind → complete.
