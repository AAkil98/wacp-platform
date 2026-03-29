use crate::*;

fn manual_clock(us: u64) -> Clock<ManualTimeSource> {
    Clock::new(ManualTimeSource::new(us))
}

#[test]
fn local_event_monotonic() {
    let mut clock = manual_clock(1000);
    let a = clock.now();
    let b = clock.now();
    let c = clock.now();
    assert!(a < b);
    assert!(b < c);
}

#[test]
fn local_event_advances_physical() {
    let source = ManualTimeSource::new(1000);
    let mut clock = Clock::new(source);

    let a = clock.now();
    assert_eq!(a.physical_us(), 1000);
    assert_eq!(a.logical(), 0);

    clock.time_source.set(2000);
    let b = clock.now();
    assert_eq!(b.physical_us(), 2000);
    assert_eq!(b.logical(), 0);
}

#[test]
fn local_event_increments_logical() {
    let mut clock = manual_clock(1000);

    let a = clock.now();
    assert_eq!(a.logical(), 0);

    let b = clock.now();
    assert_eq!(b.physical_us(), 1000);
    assert_eq!(b.logical(), 1);

    let c = clock.now();
    assert_eq!(c.logical(), 2);
}

#[test]
fn recv_merges_remote() {
    let mut clock = manual_clock(1000);
    let local = clock.now();

    let remote = Timestamp::new(1500, 5);
    let merged = clock.recv(remote);

    assert!(merged > local);
    assert!(merged > remote);
}

#[test]
fn recv_advances_from_future_remote() {
    let mut clock = manual_clock(1000);
    let _ = clock.now();

    let remote = Timestamp::new(5000, 3);
    let merged = clock.recv(remote);

    assert_eq!(merged.physical_us(), 5000);
    assert_eq!(merged.logical(), 4); // remote.logical + 1
}

#[test]
fn recv_same_physical() {
    let mut clock = manual_clock(1000);
    let _ = clock.now(); // (1000, 0)
    let _ = clock.now(); // (1000, 1)

    let remote = Timestamp::new(1000, 5);
    let merged = clock.recv(remote);

    assert_eq!(merged.physical_us(), 1000);
    // max(local.logical=1, remote.logical=5) + 1 = 6
    assert_eq!(merged.logical(), 6);
}

#[test]
fn to_bytes_roundtrip() {
    let ts = Timestamp::new(123456789, 42);
    let bytes = ts.to_bytes();
    let back = Timestamp::from_bytes(bytes);
    assert_eq!(ts, back);
}

#[test]
fn bytes_lexicographic_order() {
    let pairs = [
        (Timestamp::new(100, 0), Timestamp::new(200, 0)),
        (Timestamp::new(100, 5), Timestamp::new(100, 6)),
        (Timestamp::new(100, 65535), Timestamp::new(101, 0)),
        (Timestamp::ZERO, Timestamp::new(0, 1)),
    ];
    for (a, b) in pairs {
        assert!(a < b, "{a} should be < {b}");
        assert!(a.to_bytes() < b.to_bytes(), "{a}.to_bytes() should be < {b}.to_bytes()");
    }
}

#[test]
fn succ_increments() {
    let ts = Timestamp::new(100, 42);
    let next = ts.succ();
    assert!(next > ts);
    assert_eq!(next.physical_us(), 100);
    assert_eq!(next.logical(), 43);
}

#[test]
fn succ_wraps_physical_on_logical_overflow() {
    let ts = Timestamp::new(100, u16::MAX);
    let next = ts.succ();
    assert!(next > ts);
    assert_eq!(next.physical_us(), 101);
    assert_eq!(next.logical(), 0);
}

#[test]
fn ordering() {
    let a = Timestamp::new(100, 5);
    let b = Timestamp::new(100, 6);
    let c = Timestamp::new(101, 0);
    assert!(a < b);
    assert!(b < c);
    assert!(a < c);
    assert_eq!(a, Timestamp::new(100, 5));
}

#[test]
fn display_format() {
    let ts = Timestamp::new(1000, 42);
    assert_eq!(ts.to_string(), "1000.42");
}

#[test]
fn manual_time_source() {
    let src = ManualTimeSource::new(100);
    assert_eq!(src.now_us(), 100);

    src.set(500);
    assert_eq!(src.now_us(), 500);

    src.advance(100);
    assert_eq!(src.now_us(), 600);
}

#[test]
fn clock_with_initial() {
    let initial = Timestamp::new(5000, 10);
    let mut clock = Clock::with_initial(ManualTimeSource::new(5000), initial);

    let ts = clock.now();
    assert!(ts > initial);
}

// ── Phase 18a.3: Clock edge cases ──

#[test]
fn succ_from_zero() {
    let ts = Timestamp::ZERO.succ();
    assert_eq!(ts.physical_us(), 0);
    assert_eq!(ts.logical(), 1);
    assert!(ts > Timestamp::ZERO);
}

#[test]
fn succ_at_max_physical_low_logical() {
    let ts = Timestamp::new(u64::MAX, 0);
    let next = ts.succ();
    assert_eq!(next.physical_us(), u64::MAX);
    assert_eq!(next.logical(), 1);
    assert!(next > ts);
}

#[test]
#[should_panic]
fn succ_at_absolute_max_overflows() {
    // (u64::MAX, u16::MAX) has no successor — physical + 1 overflows.
    let ts = Timestamp::new(u64::MAX, u16::MAX);
    let _ = ts.succ();
}

#[test]
fn bytes_roundtrip_zero() {
    let ts = Timestamp::ZERO;
    let bytes = ts.to_bytes();
    assert_eq!(bytes, [0u8; 10]);
    assert_eq!(Timestamp::from_bytes(bytes), ts);
}

#[test]
fn bytes_roundtrip_max_physical() {
    let ts = Timestamp::new(u64::MAX, 0);
    let bytes = ts.to_bytes();
    assert_eq!(&bytes[..8], &[0xFF; 8]);
    assert_eq!(&bytes[8..10], &[0, 0]);
    assert_eq!(Timestamp::from_bytes(bytes), ts);
}

#[test]
fn bytes_roundtrip_max_logical() {
    let ts = Timestamp::new(0, u16::MAX);
    let bytes = ts.to_bytes();
    assert_eq!(&bytes[..8], &[0u8; 8]);
    assert_eq!(&bytes[8..10], &[0xFF, 0xFF]);
    assert_eq!(Timestamp::from_bytes(bytes), ts);
}

#[test]
fn bytes_roundtrip_all_max() {
    let ts = Timestamp::new(u64::MAX, u16::MAX);
    let bytes = ts.to_bytes();
    assert_eq!(bytes, [0xFF; 10]);
    assert_eq!(Timestamp::from_bytes(bytes), ts);
}

#[test]
fn send_is_equivalent_to_now() {
    let source = ManualTimeSource::new(1000);
    let mut c1 = Clock::new(source);
    let source2 = ManualTimeSource::new(1000);
    let mut c2 = Clock::new(source2);

    let a = c1.send();
    let b = c2.now();
    assert_eq!(a, b);

    let a2 = c1.send();
    let b2 = c2.now();
    assert_eq!(a2, b2);
}

#[test]
fn send_monotonic() {
    let mut clock = manual_clock(1000);
    let a = clock.send();
    let b = clock.send();
    let c = clock.send();
    assert!(a < b);
    assert!(b < c);
}

#[test]
fn now_logical_saturation_at_max() {
    // Set clock state to just below logical max.
    let near_max = Timestamp::new(1000, u16::MAX - 1);
    let mut clock = Clock::with_initial(ManualTimeSource::new(1000), near_max);

    let a = clock.now(); // logical = MAX (saturating_add of MAX-1 + 1)
    assert_eq!(a.logical(), u16::MAX);

    let b = clock.now(); // logical = MAX (saturating_add of MAX + 1 = MAX)
    assert_eq!(b.logical(), u16::MAX);
    assert_eq!(b.physical_us(), 1000);
}

#[test]
fn recv_logical_saturation_three_way_tie() {
    // All three physical times equal, both logicals at MAX.
    let near_max = Timestamp::new(1000, u16::MAX - 1);
    let mut clock = Clock::with_initial(ManualTimeSource::new(1000), near_max);

    // Advance logical to MAX.
    let _ = clock.now(); // (1000, MAX)

    let remote = Timestamp::new(1000, u16::MAX);
    let merged = clock.recv(remote);

    // max(MAX, MAX).saturating_add(1) = MAX
    assert_eq!(merged.physical_us(), 1000);
    assert_eq!(merged.logical(), u16::MAX);
}

#[test]
fn recv_remote_far_future_with_max_logical() {
    let mut clock = manual_clock(1000);
    let _ = clock.now();

    let remote = Timestamp::new(999_999_999, u16::MAX);
    let merged = clock.recv(remote);

    // Remote physical is max → remote.logical.saturating_add(1) = MAX
    assert_eq!(merged.physical_us(), 999_999_999);
    assert_eq!(merged.logical(), u16::MAX);
}

#[test]
fn recv_physical_advances_resets_logical() {
    let source = ManualTimeSource::new(1000);
    let mut clock = Clock::new(source);
    let _ = clock.now(); // (1000, 0)
    let _ = clock.now(); // (1000, 1)

    // Advance physical time beyond both local and remote.
    clock.time_source.set(5000);
    let remote = Timestamp::new(3000, 100);
    let merged = clock.recv(remote);

    // local_physical (5000) > last.physical (1000) && > remote.physical (3000) → logical = 0
    assert_eq!(merged.physical_us(), 5000);
    assert_eq!(merged.logical(), 0);
}

#[test]
fn recv_local_physical_is_max() {
    // Last physical is the max, remote is lower.
    let near_max = Timestamp::new(1000, 5);
    let mut clock = Clock::with_initial(ManualTimeSource::new(500), near_max);

    let remote = Timestamp::new(800, 10);
    let merged = clock.recv(remote);

    // last.physical (1000) is max → last.logical.saturating_add(1) = 6
    assert_eq!(merged.physical_us(), 1000);
    assert_eq!(merged.logical(), 6);
}
