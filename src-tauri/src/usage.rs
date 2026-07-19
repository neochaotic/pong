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

use chrono::{DateTime, Duration as ChronoDuration, Utc};

/// A resolved snapshot of the usage panel, safe to render directly.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UsageSnapshot {
    pub session_percent: u8,
    pub session_reset_at: DateTime<Utc>,
    pub weekly_percent: u8,
    pub weekly_reset_at: DateTime<Utc>,
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
    /// "resets in" offsets to `now`. Fails closed: a partially-scraped page
    /// (a redesign, a still-loading panel) produces an error instead of a
    /// snapshot with silently-wrong zeros.
    pub fn into_snapshot(self, now: DateTime<Utc>) -> Result<UsageSnapshot, String> {
        let session_percent = self.session_percent.ok_or("session percent not found")?;
        let weekly_percent = self.weekly_percent.ok_or("weekly percent not found")?;
        let session_reset_at = self
            .session_reset_text
            .as_deref()
            .and_then(parse_reset_offset)
            .map(|d| now + d)
            .ok_or("session reset time not found")?;
        let weekly_reset_at = self
            .weekly_reset_text
            .as_deref()
            .and_then(parse_reset_offset)
            .map(|d| now + d)
            .ok_or("weekly reset time not found")?;

        Ok(UsageSnapshot {
            session_percent,
            session_reset_at,
            weekly_percent,
            weekly_reset_at,
            fetched_at: now,
        })
    }
}

/// Parses free text like "Resets in 3 hr 43 min", "Reinicia em 3 h 48 min",
/// or "Resets in 6 days" into a duration, by scanning for number-then-unit
/// pairs rather than matching a fixed phrase — the phrase itself is
/// localized, the shape (digits, then a unit) is not.
///
/// Days matter because the weekly limit's countdown switches units with
/// distance: close to reset it reads in hours/minutes, but right after a
/// reset — a full week away again — the page switches to "X days" with no
/// hour/minute component at all. A parser that only understood "h"/"min"
/// silently failed once a week, immediately after every reset.
fn parse_reset_offset(text: &str) -> Option<ChronoDuration> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
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
        ChronoDuration::days(days) + ChronoDuration::hours(hours) + ChronoDuration::minutes(minutes)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_english_hours_and_minutes() {
        assert_eq!(
            parse_reset_offset("Resets in 3 hr 43 min"),
            Some(ChronoDuration::hours(3) + ChronoDuration::minutes(43))
        );
    }

    #[test]
    fn parses_portuguese_hours_and_minutes() {
        assert_eq!(
            parse_reset_offset("Reinicia em 3 h 48 min"),
            Some(ChronoDuration::hours(3) + ChronoDuration::minutes(48))
        );
    }

    #[test]
    fn parses_minutes_only() {
        assert_eq!(
            parse_reset_offset("Resets in 45 min"),
            Some(ChronoDuration::minutes(45))
        );
    }

    #[test]
    fn parses_days_only_english() {
        // What the weekly limit shows right after it resets: a full week
        // away again, with no hour/minute component at all.
        assert_eq!(
            parse_reset_offset("Resets in 6 days"),
            Some(ChronoDuration::days(6))
        );
    }

    #[test]
    fn parses_days_only_portuguese() {
        assert_eq!(
            parse_reset_offset("Reinicia em 6 dias"),
            Some(ChronoDuration::days(6))
        );
    }

    #[test]
    fn parses_days_combined_with_hours() {
        assert_eq!(
            parse_reset_offset("Resets in 1 day 4 hr"),
            Some(ChronoDuration::days(1) + ChronoDuration::hours(4))
        );
    }

    #[test]
    fn returns_none_for_text_without_a_number() {
        assert_eq!(parse_reset_offset("You haven't used Fable yet"), None);
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
        assert_eq!(snapshot.session_percent, 26);
        assert_eq!(snapshot.weekly_percent, 40);
        assert_eq!(
            snapshot.session_reset_at,
            now + ChronoDuration::hours(3) + ChronoDuration::minutes(43)
        );
        assert_eq!(snapshot.fetched_at, now);
    }

    #[test]
    fn rejects_a_partial_payload_rather_than_guessing() {
        let payload = UsageProbePayload {
            session_percent: Some(26),
            session_reset_text: Some("Resets in 3 hr 43 min".into()),
            weekly_percent: None,
            weekly_reset_text: None,
            logged_out: false,
            nonce: 1,
        };

        assert!(payload.into_snapshot(Utc::now()).is_err());
    }
}
