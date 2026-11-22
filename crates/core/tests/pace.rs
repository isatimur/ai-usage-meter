use aium_core::pace::{compute, window_ms_for_label, PaceStatus};

const H: i64 = 3_600_000;
const W5: i64 = 5 * H;

#[test]
fn fast_spend_runs_out_before_reset() {
    // 2 h in, 60% used: 100% at 3 h 20 min in, reset at 5 h.
    let now = 1_000_000_000;
    let p = compute(60.0, now + 3 * H, W5, now).unwrap();
    assert_eq!(p.status, PaceStatus::RunsOut);
    let in_ms = p.runs_out_at_ms.unwrap() - now;
    assert!((in_ms - 80 * 60_000).abs() < 1000, "got {in_ms}");
}

#[test]
fn slow_spend_is_ok() {
    let now = 1_000_000_000;
    let p = compute(20.0, now + 3 * H, W5, now).unwrap();
    assert_eq!(p.status, PaceStatus::Ok);
    assert_eq!(p.runs_out_at_ms, None);
    assert!((p.expected_percent - 40.0).abs() < 0.01);
}

#[test]
fn above_even_pace_always_runs_out() {
    // 3 h in: even pace is 60%. 70% used projects to about 117% at the reset.
    let now = 1_000_000_000;
    let p = compute(70.0, now + 2 * H, W5, now).unwrap();
    assert_eq!(p.status, PaceStatus::RunsOut);
}

#[test]
fn too_early_gives_no_forecast() {
    let now = 1_000_000_000;
    // 5 minutes into a 5 hour window.
    let p = compute(30.0, now + W5 - 5 * 60_000, W5, now).unwrap();
    assert_eq!(p.status, PaceStatus::Ok);
}

#[test]
fn full_window_runs_out_now() {
    let now = 1_000_000_000;
    let p = compute(100.0, now + H, W5, now).unwrap();
    assert_eq!(p.status, PaceStatus::RunsOut);
    assert_eq!(p.runs_out_at_ms, Some(now));
}

#[test]
fn bad_times_give_none() {
    let now = 1_000_000_000;
    assert!(compute(10.0, now - 1, W5, now).is_none());
    assert!(compute(10.0, now + 10 * H, W5, now).is_none());
}

#[test]
fn window_lengths_from_labels() {
    assert_eq!(window_ms_for_label("5 hours"), Some(W5));
    assert_eq!(window_ms_for_label("Week · Fable"), Some(7 * 24 * H));
    assert_eq!(window_ms_for_label("something"), None);
}
