//! Placeholder bench for `SessionLauncher` coordinator sequence (C3).
//!
//! Writing the real bench requires an `InjectableCoordinator` mock from
//! `console-integration::mock_coordinator` — that crate is a dev-dep chain
//! away from `console-core`, so wiring it in requires either (a) moving the
//! mock to `console-test-support` or (b) duplicating a minimal stub here.
//!
//! This placeholder proves the harness compiles + emits results; the real
//! scenario lands in a follow-up (`backend-perf-baseline-plan` §3 follow-up
//! row) once the mock location is decided.

use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn bench_placeholder(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_launcher");
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("placeholder", |b| {
        b.iter(|| {
            // Stand-in cost — a noop `black_box` to validate the harness.
            // Real N=1/4/16/64 sweep lives in the follow-up commit.
            black_box(1 + 1)
        });
    });
    group.finish();
}

criterion_group!(benches, bench_placeholder);
criterion_main!(benches);
