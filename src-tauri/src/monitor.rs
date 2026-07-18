//! The health-check pipeline: heartbeat, synthetic interaction, reporting.

use crate::health::{HealthReport, Phase, ProbePayload, Verdict};
use crate::injection::{build_check_call, build_heartbeat_call, InjectionParams};
use crate::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

/// Label of the hidden webview that hosts the monitored dashboard.
pub const MONITOR_LABEL: &str = "monitor";
/// Label of the tray popover.
pub const POPOVER_LABEL: &str = "popover";
/// Event name the popover listens on for state updates.
pub const UPDATE_EVENT: &str = "monitor://update";

/// A read-only heartbeat should answer almost instantly.
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(8);
/// Slack added on top of the configured typing + settle budget.
const CHECK_OVERHEAD: Duration = Duration::from_secs(15);

/// Broadcast the current snapshot to the popover and refresh the tray tooltip.
pub fn emit_snapshot(app: &AppHandle, state: &AppState) {
    let mut snapshot = state.snapshot();
    snapshot.dashboard_visible = dashboard_visible(app);
    let _ = app.emit(UPDATE_EVENT, &snapshot);
    crate::tray::refresh(app, &snapshot);
}

/// Run one full health check: heartbeat first, then the synthetic interaction.
pub async fn run_health_check(app: AppHandle, state: Arc<AppState>) {
    // A cron tick landing while "Force Check" is still running would have two
    // agents typing into the same page; the second caller simply steps aside.
    let Some(_guard) = state.try_begin_check() else {
        log::debug!("check already in flight — skipping this run");
        return;
    };

    state.set_phase(Phase::Pinging);
    emit_snapshot(&app, &state);

    let Some(webview) = app.get_webview_window(MONITOR_LABEL) else {
        finish(
            &app,
            &state,
            HealthReport::new(0, "monitor webview is not running", 0),
        );
        return;
    };

    let cfg = state.config_snapshot();

    // Step 1 — ask the page what it currently is before touching anything.
    let hb_nonce = state.next_nonce();
    let hb_params = InjectionParams::from_config(&cfg, hb_nonce);
    let heartbeat = probe(
        &webview,
        &state,
        build_heartbeat_call(&hb_params),
        hb_nonce,
        HEARTBEAT_TIMEOUT,
    )
    .await;

    let heartbeat_code = heartbeat.as_ref().map(|p| p.code).unwrap_or(0);
    if let Next::Abort(report) = decide_after_heartbeat(heartbeat) {
        finish(&app, &state, report);
        return;
    }

    // Probe-only: the heartbeat already told us the session is alive, and
    // driving the page could mutate the account. Stop here.
    if cfg.interaction == crate::config::Interaction::ProbeOnly {
        finish(
            &app,
            &state,
            HealthReport::new(heartbeat_code, "session alive (probe only)", 0),
        );
        return;
    }

    // Step 2 — drive the dashboard like a user would.
    let nonce = state.next_nonce();
    let params = InjectionParams::from_config(&cfg, nonce);
    let budget = check_budget(&cfg);

    let outcome = probe(&webview, &state, build_check_call(&params), nonce, budget).await;
    finish(&app, &state, report_from_probe(outcome, budget));
}

/// How long a full check may take: typing + settle + fixed overhead.
fn check_budget(cfg: &crate::config::Config) -> Duration {
    let typing = cfg
        .typing_delay_ms
        .saturating_mul(cfg.payload.chars().count() as u64);
    Duration::from_millis(typing + cfg.settle_ms) + CHECK_OVERHEAD
}

#[derive(Debug)]
pub enum ProbeError {
    Timeout,
    Eval(String),
}

/// What the pipeline should do once the heartbeat comes back.
#[derive(Debug, PartialEq)]
pub enum Next {
    /// Stop and publish this report — nothing worth interacting with.
    Abort(HealthReport),
    /// The dashboard looks authenticated; run the synthetic interaction.
    Proceed,
}

/// Decide whether a heartbeat result justifies driving the page.
///
/// Split out from `run_health_check` so the branching is testable without a
/// live webview; the async function keeps only the I/O.
pub fn decide_after_heartbeat(result: Result<ProbePayload, ProbeError>) -> Next {
    match result {
        Err(ProbeError::Timeout) => Next::Abort(HealthReport::new(
            408,
            "heartbeat timed out",
            HEARTBEAT_TIMEOUT.as_millis() as u64,
        )),
        Err(ProbeError::Eval(e)) => {
            Next::Abort(HealthReport::new(0, format!("injection failed: {e}"), 0))
        }
        // A login screen means there is nothing to interact with — stop here.
        Ok(payload) if payload.code == 401 => {
            Next::Abort(HealthReport::new(401, payload.detail, payload.latency_ms))
        }
        Ok(_) => Next::Proceed,
    }
}

/// Turn the outcome of the full check into the report the UI will show.
pub fn report_from_probe(
    result: Result<ProbePayload, ProbeError>,
    budget: Duration,
) -> HealthReport {
    match result {
        Ok(payload) => HealthReport::new(payload.code, payload.detail, payload.latency_ms),
        Err(ProbeError::Timeout) => {
            HealthReport::new(408, "check timed out", budget.as_millis() as u64)
        }
        Err(ProbeError::Eval(e)) => HealthReport::new(0, format!("injection failed: {e}"), 0),
    }
}

/// Fire the "session expired" notification only on the transition into that
/// state, so a dashboard left logged out does not nag on every cron tick.
pub fn should_notify(needs_relogin: bool, was_already_flagged: bool, enabled: bool) -> bool {
    needs_relogin && !was_already_flagged && enabled
}

/// Evaluate `js` in the webview and await the matching report.
async fn probe(
    webview: &WebviewWindow,
    state: &AppState,
    js: String,
    nonce: u64,
    timeout: Duration,
) -> Result<ProbePayload, ProbeError> {
    let rx = state.expect_report(nonce);

    if let Err(e) = webview.eval(&js) {
        state.forget_report(nonce);
        return Err(ProbeError::Eval(e.to_string()));
    }

    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(payload)) => Ok(payload),
        // Timed out, or the sender was dropped — either way, stop waiting.
        _ => {
            state.forget_report(nonce);
            // The page title is the one signal we can read without the IPC
            // bridge; it disambiguates "wrong page" from "IPC never arrived".
            log::warn!(
                "probe {nonce} timed out after {timeout:?} (page title: {:?})",
                webview.title().unwrap_or_default()
            );
            Err(ProbeError::Timeout)
        }
    }
}

/// Persist the outcome, notify the user if the session died, refresh the UI.
fn finish(app: &AppHandle, state: &AppState, report: HealthReport) {
    let needs_relogin = report.verdict.needs_relogin();
    let notify = state.config_snapshot().notifications_enabled;

    log::info!(
        "check finished: {} {:?} ({}ms) — {}",
        report.code,
        report.verdict,
        report.latency_ms,
        report.detail
    );

    // Only fire the notification on the transition into the unauthorized state,
    // so a dashboard that stays logged out does not nag every cron tick.
    let was_already_flagged = state.needs_relogin();
    state.record_report(report);

    if needs_relogin && !was_already_flagged {
        if should_notify(needs_relogin, was_already_flagged, notify) {
            // The official notification plugin exposes no click handler on
            // desktop (plugins-workspace#2150), so the copy points at the tray
            // rather than promising a click that does nothing.
            let _ = app
                .notification()
                .builder()
                .title("Pong")
                .body("Dashboard session expired — reconnect from the menu bar.")
                .show();
        }

        // Surface the recovery UI directly: the popover renders a Reconnect
        // button whenever `needs_relogin` is set. Shown without stealing focus.
        if let Some(popover) = app.get_webview_window(POPOVER_LABEL) {
            let _ = popover.show();
        }
    }

    emit_snapshot(app, state);
}

/// Show the dashboard window if hidden, hide it if visible.
///
/// Returns the resulting visibility, so the UI can label its button.
pub fn toggle_dashboard(app: &AppHandle) -> Result<bool, String> {
    let webview = app
        .get_webview_window(MONITOR_LABEL)
        .ok_or_else(|| "monitor webview is not running".to_string())?;

    if webview.is_visible().unwrap_or(false) {
        webview.hide().map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        webview.show().map_err(|e| e.to_string())?;
        webview.set_focus().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// Whether the dashboard window is currently on screen.
pub fn dashboard_visible(app: &AppHandle) -> bool {
    app.get_webview_window(MONITOR_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Reveal the hidden webview so the user can log in again.
pub fn show_relogin(app: &AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview_window(MONITOR_LABEL)
        .ok_or_else(|| "monitor webview is not running".to_string())?;

    webview.show().map_err(|e| e.to_string())?;
    webview.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Hide the webview again once the user is done logging in.
pub fn hide_relogin(app: &AppHandle) -> Result<(), String> {
    if let Some(webview) = app.get_webview_window(MONITOR_LABEL) {
        webview.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Verdict-driven tray tooltip text.
pub fn tooltip_for(phase: Phase, verdict: Option<Verdict>, countdown: Option<i64>) -> String {
    let status = match (phase, verdict) {
        (Phase::Pinging, _) => "checking…".to_string(),
        (_, Some(Verdict::Unauthorized)) => "session expired".to_string(),
        (_, Some(Verdict::Degraded)) => "degraded".to_string(),
        (_, Some(Verdict::Unreachable)) => "unreachable".to_string(),
        (_, Some(Verdict::Healthy)) => "healthy".to_string(),
        (_, None) => "idle".to_string(),
    };

    match countdown {
        Some(secs) => format!(
            "Pong — {status} · next in {}",
            crate::scheduler::format_countdown(secs)
        ),
        None => format!("Pong — {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn budget_covers_typing_plus_settle_plus_overhead() {
        let cfg =
            Config::from_json(r##"{"payload":"ping","typing_delay_ms":100,"settle_ms":3000}"##)
                .unwrap();
        // 4 chars * 100ms + 3000ms + 15s overhead
        assert_eq!(
            check_budget(&cfg),
            Duration::from_millis(3400) + CHECK_OVERHEAD
        );
    }

    #[test]
    fn budget_handles_an_empty_payload() {
        let cfg = Config::from_json(r##"{"payload":"","settle_ms":0}"##).unwrap();
        assert_eq!(check_budget(&cfg), CHECK_OVERHEAD);
    }

    #[test]
    fn budget_counts_unicode_characters_not_bytes() {
        let cfg = Config::from_json(r##"{"payload":"héllo","typing_delay_ms":10,"settle_ms":0}"##)
            .unwrap();
        assert_eq!(
            check_budget(&cfg),
            Duration::from_millis(50) + CHECK_OVERHEAD
        );
    }

    fn payload(code: u16) -> ProbePayload {
        ProbePayload {
            code,
            detail: "detail".into(),
            latency_ms: 42,
            nonce: 1,
        }
    }

    #[test]
    fn heartbeat_proceeds_when_authenticated() {
        assert_eq!(decide_after_heartbeat(Ok(payload(200))), Next::Proceed);
    }

    #[test]
    fn heartbeat_proceeds_even_when_markers_are_missing() {
        // A 503 is worth investigating with the full interaction, not aborting.
        assert_eq!(decide_after_heartbeat(Ok(payload(503))), Next::Proceed);
    }

    #[test]
    fn heartbeat_aborts_on_a_login_screen() {
        match decide_after_heartbeat(Ok(payload(401))) {
            Next::Abort(report) => {
                assert_eq!(report.code, 401);
                assert_eq!(report.verdict, Verdict::Unauthorized);
                assert_eq!(report.latency_ms, 42);
            }
            Next::Proceed => panic!("a login screen must abort the check"),
        }
    }

    #[test]
    fn heartbeat_aborts_on_timeout() {
        match decide_after_heartbeat(Err(ProbeError::Timeout)) {
            Next::Abort(report) => {
                assert_eq!(report.code, 408);
                assert_eq!(report.verdict, Verdict::Degraded);
            }
            Next::Proceed => panic!("a timeout must abort the check"),
        }
    }

    #[test]
    fn heartbeat_aborts_when_injection_fails() {
        match decide_after_heartbeat(Err(ProbeError::Eval("no webview".into()))) {
            Next::Abort(report) => {
                assert_eq!(report.code, 0);
                assert_eq!(report.verdict, Verdict::Unreachable);
                assert!(report.detail.contains("no webview"), "{}", report.detail);
            }
            Next::Proceed => panic!("a failed eval must abort the check"),
        }
    }

    #[test]
    fn report_mirrors_a_successful_probe() {
        let report = report_from_probe(Ok(payload(200)), Duration::from_secs(20));
        assert_eq!(report.code, 200);
        assert_eq!(report.verdict, Verdict::Healthy);
        assert_eq!(report.latency_ms, 42);
    }

    #[test]
    fn report_uses_the_budget_as_latency_on_timeout() {
        let budget = Duration::from_secs(20);
        let report = report_from_probe(Err(ProbeError::Timeout), budget);
        assert_eq!(report.code, 408);
        assert_eq!(report.latency_ms, 20_000);
    }

    #[test]
    fn report_marks_a_failed_injection_unreachable() {
        let report = report_from_probe(Err(ProbeError::Eval("boom".into())), Duration::ZERO);
        assert_eq!(report.verdict, Verdict::Unreachable);
    }

    #[test]
    fn notifies_only_on_the_transition_into_unauthorized() {
        assert!(should_notify(true, false, true), "first 401 should notify");
        assert!(
            !should_notify(true, true, true),
            "a dashboard left logged out must not nag every tick"
        );
        assert!(
            !should_notify(false, false, true),
            "a healthy check is silent"
        );
        assert!(!should_notify(true, false, false), "respects the opt-out");
    }

    #[test]
    fn tooltip_reports_checking_while_pinging() {
        let t = tooltip_for(Phase::Pinging, Some(Verdict::Healthy), Some(90));
        assert!(t.contains("checking"), "{t}");
    }

    #[test]
    fn tooltip_surfaces_an_expired_session() {
        let t = tooltip_for(Phase::Error, Some(Verdict::Unauthorized), Some(60));
        assert!(t.contains("session expired"), "{t}");
        assert!(t.contains("1m 00s"), "{t}");
    }

    #[test]
    fn tooltip_without_history_reads_idle() {
        let t = tooltip_for(Phase::Ready, None, None);
        assert_eq!(t, "Pong — idle");
    }
}
