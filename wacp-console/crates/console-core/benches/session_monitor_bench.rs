//! Broadcast fan-out bench for `SessionMonitor`.
//!
//! Scenario: N subscribers attached to one `broadcast::channel`, publisher
//! fires M frames as fast as the channel will accept, each subscriber drains.
//! Measures end-to-end publisher→last-receiver latency per frame.
//!
//! HEALTH-LOG §8 (backend unsurveyed) / backend-perf-baseline-plan C2.
//! Baseline target: p99 < 1 ms at N=16 / M=1000 on dev box.

use std::time::Duration;

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use tokio::runtime::Runtime;
use tokio::sync::broadcast;

fn bench_broadcast_fanout(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("session_monitor_broadcast");
    group.measurement_time(Duration::from_secs(5));

    for &n_subs in &[1usize, 4, 16, 64] {
        let frames_per_iter = 1000usize;
        group.throughput(Throughput::Elements(frames_per_iter as u64));
        group.bench_function(format!("subs={n_subs}"), |b| {
            b.iter(|| {
                rt.block_on(async {
                    // capacity=256 matches SessionMonitor's cap per wiring-plan W3.
                    let (tx, _) = broadcast::channel::<u64>(256);
                    let mut subs: Vec<_> = (0..n_subs).map(|_| tx.subscribe()).collect();

                    for i in 0..frames_per_iter {
                        let _ = tx.send(black_box(i as u64));
                        for sub in subs.iter_mut() {
                            // Drain to keep channel from backing up; capacity 256
                            // tolerates bursts but not the full 1000-frame run.
                            while sub.try_recv().is_ok() {}
                        }
                    }
                });
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_broadcast_fanout);
criterion_main!(benches);
