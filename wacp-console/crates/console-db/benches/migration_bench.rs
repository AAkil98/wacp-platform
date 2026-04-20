//! One-shot `create_test_pool()` cost measurement.
//!
//! HEALTH-LOG §9.3 / backend-perf-baseline-plan C7. The note flagged that
//! each test re-runs all 9 migrations against a fresh in-memory DB; bench
//! this to confirm the current cost before deciding whether to amortize
//! via a `lazy_static!` migrated-template + `ATTACH DATABASE` pattern.
//!
//! Threshold: >10 ms mean triggers the amortization work. Under 10 ms,
//! keep the simple-per-test pattern.

use std::time::Duration;

use console_db::create_test_pool;
use criterion::{Criterion, criterion_group, criterion_main};
use tokio::runtime::Runtime;

fn bench_create_test_pool(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("console_db_migration");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);
    group.bench_function("create_test_pool", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = create_test_pool().await.unwrap();
            });
        });
    });
    group.finish();
}

criterion_group!(benches, bench_create_test_pool);
criterion_main!(benches);
