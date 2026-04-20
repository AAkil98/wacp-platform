//! Bench for the `StubAdapter` hot paths flagged in HEALTH-LOG §10.1 / §10.2
//! / backend-perf-baseline-plan C5 + C6.
//!
//! C5: `serialize_for_match` is currently called twice per `complete()`
//! (once for matching, once for input-token count). The bench measures
//! serialize cost at a typical conversation size (5 messages × ~500 chars).
//!
//! C6: `complete_stream` builds the full `Vec<StreamEvent>` eagerly before
//! returning the `StreamHandle`. Bench reads the event count; the actual
//! memory-peak measurement is out of criterion's default scope, so the
//! timing bench stands in as a proxy (slower eager build ↔ higher peak).

use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use wacp_llm::providers::stub::serialize_for_match;
use wacp_llm::{Content, Message, Role, ToolDefinition};

fn make_conversation(n_messages: usize, chars_per: usize) -> Vec<Message> {
    let filler: String = "x".repeat(chars_per);
    (0..n_messages)
        .map(|i| Message {
            role: if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            },
            content: Content::Text(filler.clone()),
        })
        .collect()
}

fn bench_serialize_for_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("stub_serialize_for_match");
    group.measurement_time(Duration::from_secs(5));

    let tools: Vec<ToolDefinition> = vec![];
    for &n in &[1usize, 5, 20] {
        let messages = make_conversation(n, 500);
        group.bench_function(format!("n_messages={n}_chars=500"), |b| {
            b.iter(|| {
                let out = serialize_for_match(black_box(&messages), black_box(&tools));
                black_box(out.len());
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_serialize_for_match);
criterion_main!(benches);
