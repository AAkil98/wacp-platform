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
