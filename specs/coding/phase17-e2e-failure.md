# Task 17.2: E2E Failure Scenario Tests

## Scope

E2E tests for failure paths: timeout, budget, failure cascade with ownership boundaries, and conflict escalation.

## Crate

`wacp-coordinator` (tests)

## Tests

| Test | Verifies |
|------|----------|
| `e2e_timeout_expiry` | Register workspace with timeout, advance time past limit, check_expired returns workspace, abort → Failed. |
| `e2e_budget_exceeded` | Workspace with budget, resource usage exceeds limit, BudgetEnforcer returns Exceeded, abort → Failed. |
| `e2e_failure_cascade_same_owner` | Parent fails → same-owner child marked Failed in tree via cascade_failure. |
| `e2e_failure_cascade_cross_owner` | Parent fails → cross-owner child reparented to root, stays active. |
| `e2e_conflict_to_resolution` | Workspace completes → Integrating → ConflictDetected → Conflicted → ConflictResolved → Closed. |
| `e2e_conflict_unresolvable` | Workspace in Conflicted → ConflictUnresolvable → Failed. |
| `e2e_integration_failure` | Workspace in Integrating → IntegrationFailed → Failed. |

## Acceptance Criteria

- Timeout + budget failures produce correct terminal states.
- Failure cascade respects ownership boundaries.
- Conflict lifecycle works end-to-end.
