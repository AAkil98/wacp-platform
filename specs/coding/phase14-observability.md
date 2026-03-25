# Task 14.6: Observability

## Scope

Add Prometheus metrics endpoint and HTTP health checks. Both are lightweight axum HTTP servers on their own ports, separate from gRPC. Metrics serve Prometheus text format at a configurable path. Health returns JSON with status and subsystem checks.

## Dependencies

- `prometheus` (new workspace dep) — metric types, text encoding
- `axum` (new workspace dep, already transitive dep via tonic)

## Types

### `HealthState`

```rust
pub enum HealthState { Starting, Ready, Draining }
```

Stored as `AtomicU8`. Transitions: Starting → Ready → Draining. No backward transitions.

## Functions

### Metrics

```rust
pub fn register_metrics() -> RuntimeMetrics
pub async fn start_metrics_server(config: &MetricsConfig) -> Result<(), ObservabilityError>
```

`RuntimeMetrics` holds handles to all registered metrics (gauges, counters, histograms from deployment.md §7.2). The metrics server serves the prometheus text encoding at the configured path, 404 on all other paths.

### Health

```rust
pub fn start_health_server(config: &HealthConfig, state: Arc<AtomicU8>) -> Result<(), ObservabilityError>
```

Returns JSON: `{"status": "ready"|"starting"|"draining", "uptime_seconds": N}`.
HTTP 200 when Ready, 503 otherwise.

## Tests

| Test | Verifies |
|------|----------|
| `metrics_registration` | All metric families registered without panic |
| `health_starting_returns_503` | HealthState::Starting → 503 |
| `health_ready_returns_200` | HealthState::Ready → 200 |
| `health_draining_returns_503` | HealthState::Draining → 503 |

## Acceptance Criteria

- Metrics endpoint serves valid Prometheus text format.
- Health endpoint returns correct status for each state.
- Both servers disabled when their config `enabled` is false.
- No per-workspace label cardinality in metrics.
- All tests pass, clippy clean.
