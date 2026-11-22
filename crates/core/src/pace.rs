//! Pace and forecast for a limit window: "at this speed, do you hit the limit
//! before the window resets?" The forecast is a straight line from the start of
//! the window to now. It is an estimate, not a promise.

use serde::{Deserialize, Serialize};

const HOUR_MS: i64 = 3_600_000;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaceStatus {
    /// Usage is at or below an even pace.
    Ok,
    /// At this speed the limit is reached before the reset.
    RunsOut,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Pace {
    pub status: PaceStatus,
    /// Where an even spend would be now, 0 to 100.
    pub expected_percent: f64,
    /// When the limit is reached at the current speed (only for `RunsOut`).
    pub runs_out_at_ms: Option<i64>,
}

/// Window length from the label the parsers produce.
pub fn window_ms_for_label(label: &str) -> Option<i64> {
    if label.starts_with("5 hours") {
        Some(5 * HOUR_MS)
    } else if label.starts_with("Week") {
        Some(7 * 24 * HOUR_MS)
    } else {
        None
    }
}

/// Needs at least 5% of the window to have passed; before that a line through
/// one or two requests says nothing.
pub fn compute(percent: f64, resets_at_ms: i64, window_ms: i64, now_ms: i64) -> Option<Pace> {
    let remaining = resets_at_ms - now_ms;
    if window_ms <= 0 || remaining <= 0 || remaining > window_ms {
        return None;
    }
    let elapsed = window_ms - remaining;
    let expected = elapsed as f64 / window_ms as f64 * 100.0;
    if percent >= 100.0 {
        return Some(Pace {
            status: PaceStatus::RunsOut,
            expected_percent: expected,
            runs_out_at_ms: Some(now_ms),
        });
    }
    if percent <= 0.0 || elapsed * 20 < window_ms {
        return Some(Pace {
            status: PaceStatus::Ok,
            expected_percent: expected,
            runs_out_at_ms: None,
        });
    }
    let to_full = (100.0 - percent) / percent * elapsed as f64;
    if to_full < remaining as f64 {
        return Some(Pace {
            status: PaceStatus::RunsOut,
            expected_percent: expected,
            runs_out_at_ms: Some(now_ms + to_full as i64),
        });
    }
    // A straight line that stays under 100% at the reset means usage is at or below an even pace.
    Some(Pace {
        status: PaceStatus::Ok,
        expected_percent: expected,
        runs_out_at_ms: None,
    })
}
