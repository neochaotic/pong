//! Claude.ai usage-limits scraping: session/weekly usage percentages and
//! their reset countdowns, read from `claude.ai/settings/usage`.
//!
//! This is deliberately not part of the generic `Selectors`/check pipeline:
//! the page has no stable `data-testid` hooks for these numbers, and its
//! wording changes with the account's language (English "Resets in 3 hr 43
//! min" vs Portuguese "Reinicia em 3 h 48 min"), so the scraper in `agent.js`
//! locates elements structurally (a `%` character, specific CSS classes)
//! rather than by locale-specific text, and the parsing here is tolerant of
//! word order — it only looks for number-then-unit pairs.

use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, TimeZone, Utc, Weekday};

/// One metric's worth of the usage panel: a percentage and, best-effort, when
/// it resets.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MetricSnapshot {
    pub percent: u8,
    /// `None` when the percent was scraped but the reset countdown text
    /// couldn't be parsed — a metric with a known percent and an unknown
    /// reset time is still worth showing, not worth discarding.
    pub reset_at: Option<DateTime<Utc>>,
    /// Set only when `reset_at` is `None` because parsing failed (as opposed
    /// to the text never being scraped) — the raw text, so an unfamiliar
    /// format shows up in the UI instead of requiring a screenshot to
    /// diagnose.
    pub reset_note: Option<String>,
}

/// A resolved snapshot of the usage panel, safe to render directly. Session
/// and weekly are independent: one metric failing to scrape or parse never
/// blanks the other.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UsageSnapshot {
    pub session: Option<MetricSnapshot>,
    pub weekly: Option<MetricSnapshot>,
    /// When this snapshot was scraped — the UI extrapolates countdowns from
    /// here rather than re-fetching every second.
    pub fetched_at: DateTime<Utc>,
}

/// One entry in the usage-check history, mirroring `HealthReport`'s shape
/// closely enough to render in the same `HistoryView` component.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UsageLogEntry {
    pub ok: bool,
    pub detail: String,
    pub latency_ms: u64,
    pub at: DateTime<Utc>,
}

/// Raw payload posted back by the injected scraper. Every field is optional
/// because the scraper reports whatever it found rather than failing outright
/// on a partial page.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct UsageProbePayload {
    #[serde(default)]
    pub session_percent: Option<u8>,
    #[serde(default)]
    pub session_reset_text: Option<String>,
    #[serde(default)]
    pub weekly_percent: Option<u8>,
    #[serde(default)]
    pub weekly_reset_text: Option<String>,
    /// True when the scraper found `selectors.login_indicator` instead of
    /// the usage panel — the session expired, not "the page redesigned".
    #[serde(default)]
    pub logged_out: bool,
    pub nonce: u64,
}

impl UsageProbePayload {
    /// Resolve the scraped strings into a snapshot, anchoring relative
    /// "resets in" offsets to `now`. Each metric is resolved independently:
    /// a percent with no parseable reset text still produces a metric (with
    /// `reset_at: None`) rather than failing the whole check. Only fails
    /// closed — an `Err` — when neither metric could be scraped at all (a
    /// redesigned page, a still-loading panel).
    pub fn into_snapshot(self, now: DateTime<Utc>) -> Result<UsageSnapshot, String> {
        let session = self
            .session_percent
            .map(|percent| resolve_metric(percent, self.session_reset_text.as_deref(), now));
        let weekly = self
            .weekly_percent
            .map(|percent| resolve_metric(percent, self.weekly_reset_text.as_deref(), now));

        if session.is_none() && weekly.is_none() {
            return Err("no usage data found on the page".into());
        }

        Ok(UsageSnapshot {
            session,
            weekly,
            fetched_at: now,
        })
    }
}

fn resolve_metric(percent: u8, reset_text: Option<&str>, now: DateTime<Utc>) -> MetricSnapshot {
    match reset_text.and_then(|text| parse_reset_offset(text, now)) {
        Some(offset) => MetricSnapshot {
            percent,
            reset_at: Some(now + offset),
            reset_note: None,
        },
        None => MetricSnapshot {
            percent,
            reset_at: None,
            reset_note: reset_text.map(|t| t.to_string()),
        },
    }
}

/// Parses free text like "Resets in 3 hr 43 min", "Reinicia em 3 h 48 min",
/// "Resets in 6 days", "Resets in 1 week", or "Resets Sun 7:00 AM" into a
/// duration from `now`.
///
/// Two shapes are tried, since claude.ai switches between them by distance to
/// reset: a relative "N unit" scan (locale-tolerant — the shape, digits then
/// a unit, is not localized even though the phrase around it is), and an
/// absolute "weekday + clock time" shape used when the reset is far enough
/// out that a relative countdown would be more confusing than a date.
fn parse_reset_offset(text: &str, now: DateTime<Utc>) -> Option<ChronoDuration> {
    parse_relative_offset(text).or_else(|| parse_weekday_time_offset(text, now, Local))
}

/// The relative "N unit" shape: "3 hr 43 min", "6 days", "1 semana", etc.
fn parse_relative_offset(text: &str) -> Option<ChronoDuration> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut weeks: i64 = 0;
    let mut days: i64 = 0;
    let mut hours: i64 = 0;
    let mut minutes: i64 = 0;
    let mut found = false;

    for i in 0..tokens.len() {
        let Ok(n) = tokens[i].parse::<i64>() else {
            continue;
        };
        let Some(unit) = tokens.get(i + 1) else {
            continue;
        };
        let unit = unit.to_lowercase();
        if unit.starts_with("min") {
            minutes = n;
            found = true;
        } else if unit.starts_with("week") || unit.starts_with("semana") {
            weeks = n;
            found = true;
        } else if unit.starts_with("day") || unit.starts_with("dia") {
            // English "day(s)" and Portuguese "dia(s)" both start with 'd',
            // ahead of the "h" check below so neither is ever mistaken for it.
            days = n;
            found = true;
        } else if unit.starts_with('h') {
            hours = n;
            found = true;
        }
    }

    found.then(|| {
        ChronoDuration::weeks(weeks)
            + ChronoDuration::days(days)
            + ChronoDuration::hours(hours)
            + ChronoDuration::minutes(minutes)
    })
}

/// The absolute "weekday + clock time" shape: "Sun 7:00 AM", "Reinicia dom
/// 07:00". `tz` is the zone the clock time is assumed to be expressed in —
/// production uses `Local` (the same machine renders the page and runs this
/// process), tests pin `Utc` so the result doesn't depend on the machine
/// running them.
fn parse_weekday_time_offset<Tz: TimeZone>(
    text: &str,
    now: DateTime<Utc>,
    tz: Tz,
) -> Option<ChronoDuration> {
    let weekday = find_weekday(text)?;
    let (hour, minute) = find_clock_time(text)?;

    let local_now = now.with_timezone(&tz);
    let mut date = local_now.date_naive();
    loop {
        if date.weekday() == weekday {
            break;
        }
        date = date.succ_opt()?;
    }

    let naive = date.and_hms_opt(hour, minute, 0)?;
    let mut target = tz.from_local_datetime(&naive).single()?;
    // The countdown is always to the *next* occurrence — if today matches the
    // weekday but the time already passed, the reset is a full week out.
    if target <= local_now {
        target += ChronoDuration::weeks(1);
    }

    Some(target.with_timezone(&Utc) - now)
}

/// Finds a weekday name, tolerant of English ("Sun", "Sunday") and
/// Portuguese ("dom", "domingo") abbreviations by matching a distinctive
/// 3-letter prefix rather than the full word.
fn find_weekday(text: &str) -> Option<Weekday> {
    for tok in text.to_lowercase().split_whitespace() {
        let key: String = tok.chars().filter(|c| c.is_alphabetic()).collect();
        let day = if key.starts_with("sun") || key.starts_with("dom") {
            Weekday::Sun
        } else if key.starts_with("mon") || key.starts_with("seg") {
            Weekday::Mon
        } else if key.starts_with("tue") || key.starts_with("ter") {
            Weekday::Tue
        } else if key.starts_with("wed") || key.starts_with("qua") {
            Weekday::Wed
        } else if key.starts_with("thu") || key.starts_with("qui") {
            Weekday::Thu
        } else if key.starts_with("fri") || key.starts_with("sex") {
            Weekday::Fri
        } else if key.starts_with("sat") || key.starts_with("sab") || key.starts_with("sáb") {
            Weekday::Sat
        } else {
            continue;
        };
        return Some(day);
    }
    None
}

/// Finds a clock time token like "7:00" and, if present, an adjoining
/// AM/PM marker (English 12-hour) — Portuguese renders 24-hour and is used
/// as-is.
fn find_clock_time(text: &str) -> Option<(u32, u32)> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for (i, tok) in tokens.iter().enumerate() {
        let digits_and_colon: String = tok
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == ':')
            .collect();
        let Some((h, m)) = digits_and_colon.split_once(':') else {
            continue;
        };
        if h.is_empty() || m.is_empty() {
            continue;
        }
        let Ok(mut hour) = h.parse::<u32>() else {
            continue;
        };
        let Ok(minute) = m.parse::<u32>() else {
            continue;
        };
        if hour > 23 || minute > 59 {
            continue;
        }

        let same_token = tok.to_lowercase();
        let next_token = tokens
            .get(i + 1)
            .map(|t| t.to_lowercase())
            .unwrap_or_default();
        let marker = if same_token.contains("am") || same_token.contains("pm") {
            same_token
        } else {
            next_token
        };
        if marker.starts_with("pm") {
            if hour < 12 {
                hour += 12;
            }
        } else if marker.starts_with("am") && hour == 12 {
            hour = 0;
        }

        return Some((hour, minute));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offset(text: &str) -> Option<ChronoDuration> {
        parse_reset_offset(text, Utc::now())
    }

    #[test]
    fn parses_english_hours_and_minutes() {
        assert_eq!(
            offset("Resets in 3 hr 43 min"),
            Some(ChronoDuration::hours(3) + ChronoDuration::minutes(43))
        );
    }

    #[test]
    fn parses_portuguese_hours_and_minutes() {
        assert_eq!(
            offset("Reinicia em 3 h 48 min"),
            Some(ChronoDuration::hours(3) + ChronoDuration::minutes(48))
        );
    }

    #[test]
    fn parses_minutes_only() {
        assert_eq!(
            offset("Resets in 45 min"),
            Some(ChronoDuration::minutes(45))
        );
    }

    #[test]
    fn parses_days_only_english() {
        // What the weekly limit shows right after it resets: a full week
        // away again, with no hour/minute component at all.
        assert_eq!(offset("Resets in 6 days"), Some(ChronoDuration::days(6)));
    }

    #[test]
    fn parses_days_only_portuguese() {
        assert_eq!(offset("Reinicia em 6 dias"), Some(ChronoDuration::days(6)));
    }

    #[test]
    fn parses_days_combined_with_hours() {
        assert_eq!(
            offset("Resets in 1 day 4 hr"),
            Some(ChronoDuration::days(1) + ChronoDuration::hours(4))
        );
    }

    #[test]
    fn parses_weeks_only_english() {
        assert_eq!(offset("Resets in 1 week"), Some(ChronoDuration::weeks(1)));
    }

    #[test]
    fn parses_weeks_only_portuguese() {
        assert_eq!(
            offset("Reinicia em 1 semana"),
            Some(ChronoDuration::weeks(1))
        );
    }

    #[test]
    fn returns_none_for_text_without_a_number() {
        assert_eq!(offset("You haven't used Fable yet"), None);
    }

    #[test]
    fn parses_weekday_and_time_pinned_to_utc() {
        // Wed 2026-07-15 12:00 UTC -> next "Sun 7:00 AM" is 2026-07-19 07:00 UTC.
        let now = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap();
        let got = parse_weekday_time_offset("Resets Sun 7:00 AM", now, Utc).unwrap();
        assert_eq!(
            now + got,
            Utc.with_ymd_and_hms(2026, 7, 19, 7, 0, 0).unwrap()
        );
    }

    #[test]
    fn rolls_over_a_week_when_todays_weekday_matches_but_the_time_already_passed() {
        // Sun 2026-07-19 08:00 UTC: today is Sunday, but 7:00 AM is behind us.
        let now = Utc.with_ymd_and_hms(2026, 7, 19, 8, 0, 0).unwrap();
        let got = parse_weekday_time_offset("Resets Sun 7:00 AM", now, Utc).unwrap();
        assert_eq!(
            now + got,
            Utc.with_ymd_and_hms(2026, 7, 26, 7, 0, 0).unwrap()
        );
    }

    #[test]
    fn parses_weekday_and_24_hour_time_portuguese() {
        let now = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap();
        let got = parse_weekday_time_offset("Reinicia dom 07:00", now, Utc).unwrap();
        assert_eq!(
            now + got,
            Utc.with_ymd_and_hms(2026, 7, 19, 7, 0, 0).unwrap()
        );
    }

    #[test]
    fn parses_pm_times() {
        let now = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap();
        let got = parse_weekday_time_offset("Resets Wed 3:30 PM", now, Utc).unwrap();
        assert_eq!(
            now + got,
            Utc.with_ymd_and_hms(2026, 7, 15, 15, 30, 0).unwrap()
        );
    }

    #[test]
    fn builds_a_snapshot_from_a_complete_payload() {
        let now = Utc::now();
        let payload = UsageProbePayload {
            session_percent: Some(26),
            session_reset_text: Some("Resets in 3 hr 43 min".into()),
            weekly_percent: Some(40),
            weekly_reset_text: Some("Resets in 7 hr 23 min".into()),
            logged_out: false,
            nonce: 1,
        };

        let snapshot = payload.into_snapshot(now).unwrap();
        let session = snapshot.session.unwrap();
        let weekly = snapshot.weekly.unwrap();
        assert_eq!(session.percent, 26);
        assert_eq!(weekly.percent, 40);
        assert_eq!(
            session.reset_at,
            Some(now + ChronoDuration::hours(3) + ChronoDuration::minutes(43))
        );
        assert_eq!(snapshot.fetched_at, now);
    }

    #[test]
    fn a_metric_with_no_parseable_reset_text_still_reports_its_percent() {
        let now = Utc::now();
        let payload = UsageProbePayload {
            session_percent: Some(26),
            session_reset_text: Some("Resets in 3 hr 43 min".into()),
            weekly_percent: Some(40),
            // A hypothetical future format the parser doesn't understand yet.
            weekly_reset_text: Some("Resets next month".into()),
            logged_out: false,
            nonce: 1,
        };

        let snapshot = payload.into_snapshot(now).unwrap();
        let weekly = snapshot.weekly.unwrap();
        assert_eq!(weekly.percent, 40);
        assert_eq!(weekly.reset_at, None);
        assert_eq!(weekly.reset_note.as_deref(), Some("Resets next month"));
        // The session metric is untouched by the weekly metric's failure.
        assert!(snapshot.session.unwrap().reset_at.is_some());
    }

    #[test]
    fn one_missing_metric_does_not_blank_the_other() {
        let now = Utc::now();
        let payload = UsageProbePayload {
            session_percent: Some(26),
            session_reset_text: Some("Resets in 3 hr 43 min".into()),
            weekly_percent: None,
            weekly_reset_text: None,
            logged_out: false,
            nonce: 1,
        };

        let snapshot = payload.into_snapshot(now).unwrap();
        assert!(snapshot.session.is_some());
        assert!(snapshot.weekly.is_none());
    }

    #[test]
    fn fails_only_when_neither_metric_could_be_scraped() {
        let payload = UsageProbePayload {
            session_percent: None,
            session_reset_text: None,
            weekly_percent: None,
            weekly_reset_text: None,
            logged_out: false,
            nonce: 1,
        };

        assert!(payload.into_snapshot(Utc::now()).is_err());
    }
}
