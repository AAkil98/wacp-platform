//! Bench for the per-request auth path. Exercises the two costs that scale
//! with request rate: Argon2id password verify (login route) + CSRF double-
//! submit compare (every state-changing request).
//!
//! HEALTH-LOG §8 / backend-perf-baseline-plan C4. Baseline target:
//! Argon2id p99 < 100 ms (cost-factor-calibrated); CSRF compare < 100 µs.

use std::time::Duration;

use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use subtle::ConstantTimeEq;

fn bench_argon2_verify(c: &mut Criterion) {
    // Pre-compute one hash outside the bench loop so we measure verify only,
    // not hash + verify. Uses the Argon2 defaults `console-core::password`
    // uses in production.
    let argon2 = Argon2::default();
    let password = b"correct-horse-battery-staple";
    // Fixed salt — bench doesn't care about salt randomness, just the
    // PHC string being parseable for verify.
    let salt = SaltString::from_b64("c2FsdHNhbHRzYWx0c2FsdA").unwrap();
    let phc = argon2.hash_password(password, &salt).unwrap().to_string();

    let mut group = c.benchmark_group("middleware");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20); // Argon2 is slow; 20 samples avoids hour-long runs.
    group.bench_function("argon2_verify", |b| {
        b.iter(|| {
            let parsed = PasswordHash::new(&phc).unwrap();
            let _ = argon2.verify_password(black_box(password), &parsed);
        });
    });

    group.bench_function("csrf_compare_32_bytes", |b| {
        let expected = [0xABu8; 32];
        let provided = [0xABu8; 32];
        b.iter(|| {
            let ok: bool = black_box(expected).ct_eq(black_box(&provided)).into();
            black_box(ok);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_argon2_verify);
criterion_main!(benches);
