//! Cron scheduling helpers.
//!
//! The async job lives in `lib.rs`; everything here is pure so it can be unit
//! tested without spinning up a Tokio runtime or a real scheduler.

use chrono::{DateTime, Utc};
use std::str::FromStr;

/// Compute the first cron occurrence strictly after `after`.
///
/// Returns `None` when the expression is invalid or has no future occurrence.
pub fn next_occurrence(cron: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let schedule = cron::Schedule::from_str(cron).ok()?;
    schedule.after(&after).next()
}

/// Whole seconds until the next occurrence, saturating at zero.
pub fn seconds_until_next(cron: &str, now: DateTime<Utc>) -> Option<i64> {
    next_occurrence(cron, now).map(|next| (next - now).num_seconds().max(0))
}

/// Human-readable description used by the tray tooltip, e.g. "in 4m 12s".
pub fn format_countdown(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap()
    }

    #[test]
    fn computes_the_next_quarter_hour() {
        let now = at(2026, 7, 18, 10, 7, 30);
        let next = next_occurrence("0 */15 * * * *", now).unwrap();
        assert_eq!(next, at(2026, 7, 18, 10, 15, 0));
    }

    #[test]
    fn next_occurrence_is_strictly_in_the_future() {
        // Exactly on a boundary: must roll forward, never return `now` itself.
        let now = at(2026, 7, 18, 10, 15, 0);
        let next = next_occurrence("0 */15 * * * *", now).unwrap();
        assert_eq!(next, at(2026, 7, 18, 10, 30, 0));
    }

    #[test]
    fn rolls_over_the_hour_and_the_day() {
        let now = at(2026, 7, 18, 23, 58, 0);
        let next = next_occurrence("0 0 * * * *", now).unwrap();
        assert_eq!(next, at(2026, 7, 19, 0, 0, 0));
    }

    #[test]
    fn returns_none_for_an_invalid_expression() {
        assert!(next_occurrence("not a cron", Utc::now()).is_none());
        assert!(next_occurrence("*/5 * * *", Utc::now()).is_none());
    }

    #[test]
    fn seconds_until_next_counts_down_correctly() {
        let now = at(2026, 7, 18, 10, 14, 30);
        assert_eq!(seconds_until_next("0 */15 * * * *", now), Some(30));
    }

    #[test]
    fn seconds_until_next_is_none_when_expression_is_invalid() {
        assert_eq!(seconds_until_next("nope", Utc::now()), None);
    }

    #[test]
    fn formats_countdowns_at_each_magnitude() {
        assert_eq!(format_countdown(9), "9s");
        assert_eq!(format_countdown(75), "1m 15s");
        assert_eq!(format_countdown(3 * 3600 + 4 * 60), "3h 04m");
    }

    #[test]
    fn formats_negative_countdown_as_zero() {
        assert_eq!(format_countdown(-42), "0s");
    }
}
