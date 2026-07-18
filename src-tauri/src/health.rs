//! Health-check domain types shared between Rust, the hidden webview and the UI.

use chrono::{DateTime, Utc};

/// What the injected probe concluded about the dashboard.
///
/// The numeric codes mirror HTTP semantics on purpose: the webview reports a
/// status code and Rust maps it back to one of these variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Authenticated and the dashboard reacted to the synthetic interaction.
    Healthy,
    /// A login screen was detected — the persisted session expired.
    Unauthorized,
    /// Reached the page but neither marker was found, or the DOM never settled.
    Degraded,
    /// Navigation failed outright (offline, DNS, TLS...).
    Unreachable,
}

impl Verdict {
    /// Map a probe status code onto a verdict.
    pub fn from_code(code: u16) -> Self {
        match code {
            200..=299 => Verdict::Healthy,
            401 | 403 => Verdict::Unauthorized,
            408 | 500..=599 => Verdict::Degraded,
            _ => Verdict::Unreachable,
        }
    }

    /// Whether this verdict should prompt the user to re-authenticate.
    pub fn needs_relogin(&self) -> bool {
        matches!(self, Verdict::Unauthorized)
    }
}

/// Coarse lifecycle state driving the UI badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Phase {
    /// Idle, waiting for the next cron tick.
    Ready,
    /// A check is currently running.
    Pinging,
    /// The last check failed or came back unauthorized.
    Error,
}

/// The outcome of a single health check.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HealthReport {
    pub code: u16,
    pub verdict: Verdict,
    /// Short human-readable explanation, surfaced in the popover.
    pub detail: String,
    /// End-to-end duration of the injected probe.
    pub latency_ms: u64,
    pub at: DateTime<Utc>,
}

impl HealthReport {
    pub fn new(code: u16, detail: impl Into<String>, latency_ms: u64) -> Self {
        Self {
            code,
            verdict: Verdict::from_code(code),
            detail: detail.into(),
            latency_ms,
            at: Utc::now(),
        }
    }

    /// The phase the UI should show after this report lands.
    pub fn phase(&self) -> Phase {
        match self.verdict {
            Verdict::Healthy => Phase::Ready,
            _ => Phase::Error,
        }
    }
}

/// Raw payload posted back by the injected JavaScript agent.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProbePayload {
    pub code: u16,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub latency_ms: u64,
    /// Echoed back so late reports from a previous run can be discarded.
    pub nonce: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_success_codes_to_healthy() {
        assert_eq!(Verdict::from_code(200), Verdict::Healthy);
        assert_eq!(Verdict::from_code(204), Verdict::Healthy);
    }

    #[test]
    fn maps_auth_codes_to_unauthorized() {
        assert_eq!(Verdict::from_code(401), Verdict::Unauthorized);
        assert_eq!(Verdict::from_code(403), Verdict::Unauthorized);
    }

    #[test]
    fn maps_timeout_and_server_errors_to_degraded() {
        assert_eq!(Verdict::from_code(408), Verdict::Degraded);
        assert_eq!(Verdict::from_code(503), Verdict::Degraded);
    }

    #[test]
    fn maps_unknown_codes_to_unreachable() {
        assert_eq!(Verdict::from_code(0), Verdict::Unreachable);
        assert_eq!(Verdict::from_code(302), Verdict::Unreachable);
    }

    #[test]
    fn only_unauthorized_triggers_relogin() {
        assert!(Verdict::Unauthorized.needs_relogin());
        assert!(!Verdict::Healthy.needs_relogin());
        assert!(!Verdict::Degraded.needs_relogin());
        assert!(!Verdict::Unreachable.needs_relogin());
    }

    #[test]
    fn healthy_report_returns_to_ready_phase() {
        let report = HealthReport::new(200, "dashboard responded", 812);
        assert_eq!(report.verdict, Verdict::Healthy);
        assert_eq!(report.phase(), Phase::Ready);
    }

    #[test]
    fn failing_report_moves_to_error_phase() {
        assert_eq!(HealthReport::new(401, "login", 12).phase(), Phase::Error);
        assert_eq!(
            HealthReport::new(503, "no marker", 12).phase(),
            Phase::Error
        );
    }

    #[test]
    fn phase_serializes_uppercase_for_the_badge() {
        let json = serde_json::to_string(&Phase::Pinging).unwrap();
        assert_eq!(json, "\"PINGING\"");
    }

    #[test]
    fn probe_payload_tolerates_missing_optional_fields() {
        let payload: ProbePayload = serde_json::from_str(r#"{"code":200,"nonce":7}"#).unwrap();
        assert_eq!(payload.code, 200);
        assert_eq!(payload.nonce, 7);
        assert_eq!(payload.detail, "");
        assert_eq!(payload.latency_ms, 0);
    }
}
