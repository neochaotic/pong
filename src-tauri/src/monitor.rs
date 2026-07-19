//! The health-check pipeline: heartbeat, synthetic interaction, reporting.

use crate::health::{HealthReport, Phase, ProbePayload, Verdict};
use crate::injection::{build_check_call, build_heartbeat_call, build_usage_call, InjectionParams};
use crate::state::AppState;
use crate::usage::UsageSnapshot;
use chrono::{DateTime, Utc};
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
/// Fixed slack for IPC round-trips and JS engine overhead, on top of the
/// configured waits below.
const FIXED_OVERHEAD: Duration = Duration::from_secs(2);

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

/// How long a full check may take: typing + settle + the element waits the
/// agent can spend, plus fixed overhead.
///
/// The agent waits up to `element_timeout_ms` for the text input and (when
/// configured) the submit button — counted once, since those two waits are
/// mostly the same "has the SPA mounted yet" window rather than independent
/// worst cases. Each of these adds one more fully independent wait of up to
/// `element_timeout_ms`, because each runs only after the previous one
/// resolves — budgeting fewer passes than the agent can actually take would
/// let this timeout fire while it is still legitimately working:
/// - `selectors.response` (waiting for the reply to stabilize)
/// - `cleanup.menu_button`, `cleanup.delete_option`, `cleanup.confirm_button`
///   (each an independent step of the post-check teardown)
fn check_budget(cfg: &crate::config::Config) -> Duration {
    let typing = cfg
        .typing_delay_ms
        .saturating_mul(cfg.payload.chars().count() as u64);

    let mut wait_passes: u64 = 1; // text_input (+ submit_button, counted together)
    if cfg.selectors.response.is_some() {
        wait_passes += 1;
    }
    if cfg.cleanup.menu_button.is_some() {
        wait_passes += 1;
    }
    if cfg.cleanup.delete_option.is_some() {
        wait_passes += 1;
    }
    if cfg.cleanup.confirm_button.is_some() {
        wait_passes += 1;
    }
    let element_waits = cfg.element_timeout_ms.saturating_mul(wait_passes);

    Duration::from_millis(typing + cfg.settle_ms + element_waits) + FIXED_OVERHEAD
}

/// Fixed slack on top of the configured settle/element-timeout, for the
/// same reason `FIXED_OVERHEAD` exists on the check budget: IPC and JS
/// engine overhead the user's settings don't account for.
const USAGE_FIXED_OVERHEAD: Duration = Duration::from_secs(2);

/// What a scraped usage payload resolves to, decided before any I/O — the
/// login check must win over "couldn't parse the percentages", or a session
/// that expired mid-week would be reported as a confusing parse failure
/// instead of the same "please sign in again" prompt the health check shows.
#[derive(Debug, PartialEq)]
enum ScrapeOutcome {
    /// The scraper found `selectors.login_indicator` instead of the usage
    /// panel: report it as a session expiry, not a scrape failure.
    LoggedOut,
    Done(Result<UsageSnapshot, String>),
}

/// Pure decision, split out of `scrape_usage` specifically so it can be unit
/// tested without a webview — mirrors `decide_after_heartbeat` for the
/// health-check pipeline.
fn interpret_usage_payload(
    payload: crate::usage::UsageProbePayload,
    now: DateTime<Utc>,
) -> ScrapeOutcome {
    if payload.logged_out {
        return ScrapeOutcome::LoggedOut;
    }
    ScrapeOutcome::Done(payload.into_snapshot(now))
}

/// Navigate the hidden webview to claude.ai's usage panel, scrape it, and
/// navigate back to the configured target — under the same exclusivity guard
/// as a health check, since both drive the same webview and a cron tick
/// landing mid-scrape would otherwise type into the wrong page.
pub async fn run_usage_check(app: AppHandle, state: Arc<AppState>) {
    let cfg = state.config_snapshot();
    let Some(usage_url) = cfg.usage_url.clone() else {
        log::debug!("usage check skipped: usage_url not configured");
        return;
    };

    let Some(_guard) = state.try_begin_check() else {
        log::debug!("usage check skipped: a check is already in flight");
        return;
    };

    let Some(webview) = app.get_webview_window(MONITOR_LABEL) else {
        state.record_usage_result(Err("monitor webview is not running".into()), 0);
        emit_snapshot(&app, &state);
        return;
    };

    let started = std::time::Instant::now();

    let outcome = scrape_usage(&webview, &state, &cfg, &usage_url).await;

    // Always return to the configured target, regardless of outcome, so the
    // health-check pipeline resumes on the right page.
    if let Ok(url) = cfg.target_url.parse() {
        let _ = webview.navigate(url);
    }

    let latency_ms = started.elapsed().as_millis() as u64;

    match outcome {
        ScrapeOutcome::LoggedOut => {
            state.record_usage_result(Err("session expired".into()), latency_ms);
            // Same pipeline the health check uses for a 401: notifies (once,
            // on the transition) and surfaces the recovery popover. Two
            // checks watching the same session should not each have their
            // own idea of whether it is alive.
            finish(
                &app,
                &state,
                HealthReport::new(401, "session expired (seen during usage check)", latency_ms),
            );
        }
        ScrapeOutcome::Done(result) => {
            match &result {
                Ok(snapshot) => log::info!(
                    "usage check finished: {} ({}ms)",
                    crate::state::describe_usage(snapshot),
                    latency_ms
                ),
                Err(reason) => log::warn!("usage check failed: {reason} ({latency_ms}ms)"),
            }
            state.record_usage_result(result, latency_ms);
            emit_snapshot(&app, &state);
        }
    }
}

async fn scrape_usage(
    webview: &WebviewWindow,
    state: &AppState,
    cfg: &crate::config::Config,
    usage_url: &str,
) -> ScrapeOutcome {
    let url = match usage_url.parse() {
        Ok(url) => url,
        Err(e) => return ScrapeOutcome::Done(Err(format!("invalid usage_url: {e}"))),
    };
    if let Err(e) = webview.navigate(url) {
        return ScrapeOutcome::Done(Err(format!("navigation to usage page failed: {e}")));
    }

    // No known-good marker element to poll for on this page (unlike the
    // generic check pipeline), so this is a flat wait — but driven by the
    // user's own settle_ms rather than a value they cannot tune.
    tokio::time::sleep(Duration::from_millis(cfg.settle_ms)).await;

    let nonce = state.next_nonce();
    let rx = state.expect_usage_report(nonce);
    let params = InjectionParams::from_config(cfg, nonce);
    if let Err(e) = webview.eval(build_usage_call(&params)) {
        state.forget_usage_report(nonce);
        return ScrapeOutcome::Done(Err(format!("injection failed: {e}")));
    }

    let timeout = Duration::from_millis(cfg.element_timeout_ms) + USAGE_FIXED_OVERHEAD;
    let payload = match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(payload)) => payload,
        _ => {
            state.forget_usage_report(nonce);
            return ScrapeOutcome::Done(Err("usage scrape timed out".into()));
        }
    };

    interpret_usage_payload(payload, Utc::now())
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

/// Wipe every trace of the dashboard session: cookies, local storage, caches.
///
/// Used to sign out, or to recover when a half-expired session leaves the page
/// in a state neither marker matches. Afterwards the webview is sent back to
/// the configured URL, which lands on the login screen.
pub fn clear_session(app: &AppHandle, target_url: &str) -> Result<(), String> {
    let webview = app
        .get_webview_window(MONITOR_LABEL)
        .ok_or_else(|| "monitor webview is not running".to_string())?;

    webview
        .clear_all_browsing_data()
        .map_err(|e| format!("could not clear session data: {e}"))?;

    let url: tauri::Url = target_url
        .parse()
        .map_err(|e| format!("configured target URL is invalid: {e}"))?;
    webview
        .navigate(url)
        .map_err(|e| format!("could not reload the dashboard: {e}"))?;

    log::info!("session data cleared; reloaded {target_url}");
    Ok(())
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

/// Re-checks the session before trusting a manual "I'm signed in" click,
/// rather than clearing `needs_relogin` on the user's word alone.
///
/// Blind trust broke two ways in practice: a health/usage check already
/// in flight when the button was clicked could land its own (stale) 401
/// right after this cleared the flag, silently flipping it back on; and
/// clicking before sign-in actually finished "resumed" a session that
/// was never really back. Both looked identical from the outside — the
/// banner would reappear, or monitoring would silently keep failing —
/// leaving someone clicking the same button twice with no idea why the
/// first click didn't stick. Running a real heartbeat here, under the
/// same exclusivity guard every other check uses, fixes both at once.
pub async fn confirm_relogin(app: &AppHandle, state: &Arc<AppState>) -> Result<(), String> {
    let Some(_guard) = state.try_begin_check() else {
        return Err("a check is currently running — try again in a moment".into());
    };
    let Some(webview) = app.get_webview_window(MONITOR_LABEL) else {
        return Err("monitor webview is not running".into());
    };

    let cfg = state.config_snapshot();
    let nonce = state.next_nonce();
    let params = InjectionParams::from_config(&cfg, nonce);
    let heartbeat = probe(
        &webview,
        state,
        build_heartbeat_call(&params),
        nonce,
        HEARTBEAT_TIMEOUT,
    )
    .await;

    if is_confirmed_authenticated(&heartbeat) {
        hide_relogin(app)?;
        finish(
            app,
            state,
            HealthReport::new(200, "signed in — resuming monitoring", 0),
        );
        return Ok(());
    }

    // Leave the dashboard window open and the recovery banner up — the user
    // is presumably still looking at it, mid sign-in (or mid-2FA, which is
    // exactly the case `is_confirmed_authenticated` exists to not mistake
    // for success).
    let report = report_from_heartbeat(heartbeat);
    let detail = report.detail.clone();
    finish(app, state, report);
    Err(format!("still not signed in — {detail}"))
}

/// Only an explicit 200 counts as "confirmed signed in". `decide_after_heartbeat`'s
/// looser `Next::Proceed` (anything that isn't a 401) is fine for the scheduled
/// pipeline — worst case it wastes one check — but here it would hide the
/// sign-in window out from under someone who's simply mid-2FA: a verification-
/// code prompt matches neither the `authenticated` nor the `login_indicator`
/// selector, so the probe reports 503 ("neither marker found"), which
/// `Next::Proceed` would otherwise treat as success.
fn is_confirmed_authenticated(heartbeat: &Result<ProbePayload, ProbeError>) -> bool {
    matches!(heartbeat, Ok(payload) if payload.code == 200)
}

/// A report worth showing/logging for a heartbeat that didn't confirm
/// sign-in — reuses `decide_after_heartbeat`'s cases (401, timeout, eval
/// error) and fills in the one case it doesn't treat as failure (503,
/// via `Next::Proceed`) with its own.
fn report_from_heartbeat(heartbeat: Result<ProbePayload, ProbeError>) -> HealthReport {
    match decide_after_heartbeat(heartbeat) {
        Next::Abort(report) => report,
        Next::Proceed => HealthReport::new(503, "neither auth nor login marker found", 0),
    }
}

/// Passive background counterpart to `confirm_relogin`, run on every tick
/// while `needs_relogin` is set, so sign-in is detected on its own instead
/// of requiring the button click — a heartbeat is read-only (no navigation),
/// so it's safe to run even while the window is open for manual sign-in.
///
/// Deliberately quieter than `confirm_relogin` on failure: that's the normal
/// case here (most ticks land while someone is still mid-login), and logging
/// or notifying about it every 15s would spam the history for no reason.
pub async fn poll_relogin(app: &AppHandle, state: &Arc<AppState>) {
    if !state.needs_relogin() {
        return;
    }
    let Some(_guard) = state.try_begin_check() else {
        return;
    };
    let Some(webview) = app.get_webview_window(MONITOR_LABEL) else {
        return;
    };

    let cfg = state.config_snapshot();
    let nonce = state.next_nonce();
    let params = InjectionParams::from_config(&cfg, nonce);
    let heartbeat = probe(
        &webview,
        state,
        build_heartbeat_call(&params),
        nonce,
        HEARTBEAT_TIMEOUT,
    )
    .await;

    if is_confirmed_authenticated(&heartbeat) {
        let _ = hide_relogin(app);
        finish(
            app,
            state,
            HealthReport::new(200, "signed in — resuming monitoring", 0),
        );
    }
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
    use crate::usage::UsageProbePayload;

    fn usage_payload(over: UsageProbePayload) -> UsageProbePayload {
        over
    }

    #[test]
    fn a_logged_out_payload_is_flagged_before_the_percentages_are_even_looked_at() {
        let payload = usage_payload(UsageProbePayload {
            logged_out: true,
            // A page that happens to also carry stray, well-formed
            // percentage text must not accidentally look like real data.
            session_percent: Some(50),
            session_reset_text: Some("Resets in 1 hr".into()),
            weekly_percent: Some(50),
            weekly_reset_text: Some("Resets in 1 hr".into()),
            nonce: 1,
        });

        assert_eq!(
            interpret_usage_payload(payload, Utc::now()),
            ScrapeOutcome::LoggedOut
        );
    }

    #[test]
    fn a_normal_payload_resolves_to_a_snapshot() {
        let payload = usage_payload(UsageProbePayload {
            logged_out: false,
            session_percent: Some(26),
            session_reset_text: Some("Resets in 3 hr 43 min".into()),
            weekly_percent: Some(40),
            weekly_reset_text: Some("Resets in 7 hr 23 min".into()),
            nonce: 1,
        });

        match interpret_usage_payload(payload, Utc::now()) {
            ScrapeOutcome::Done(Ok(snapshot)) => {
                assert_eq!(snapshot.session.unwrap().percent, 26);
                assert_eq!(snapshot.weekly.unwrap().percent, 40);
            }
            other => panic!("expected a resolved snapshot, got {other:?}"),
        }
    }

    #[test]
    fn one_metric_missing_still_resolves_to_a_snapshot_with_the_other() {
        let payload = usage_payload(UsageProbePayload {
            logged_out: false,
            session_percent: Some(26),
            session_reset_text: Some("Resets in 3 hr 43 min".into()),
            weekly_percent: None,
            weekly_reset_text: None,
            nonce: 1,
        });

        match interpret_usage_payload(payload, Utc::now()) {
            ScrapeOutcome::Done(Ok(snapshot)) => {
                assert_eq!(snapshot.session.unwrap().percent, 26);
                assert!(snapshot.weekly.is_none());
            }
            other => panic!("expected a partial snapshot, got {other:?}"),
        }
    }

    #[test]
    fn a_payload_with_neither_metric_scraped_and_not_logged_out_still_fails_closed() {
        let payload = usage_payload(UsageProbePayload {
            logged_out: false,
            session_percent: None,
            session_reset_text: None,
            weekly_percent: None,
            weekly_reset_text: None,
            nonce: 1,
        });

        match interpret_usage_payload(payload, Utc::now()) {
            ScrapeOutcome::Done(Err(_)) => {}
            other => panic!("expected a scrape failure, got {other:?}"),
        }
    }

    #[test]
    fn budget_covers_typing_plus_settle_plus_element_wait_plus_overhead() {
        let cfg =
            Config::from_json(r##"{"payload":"ping","typing_delay_ms":100,"settle_ms":3000}"##)
                .unwrap();
        // 4 chars * 100ms + 3000ms + default 10s element wait + 2s overhead
        assert_eq!(
            check_budget(&cfg),
            Duration::from_millis(3400) + Duration::from_millis(10_000) + FIXED_OVERHEAD
        );
    }

    #[test]
    fn budget_handles_an_empty_payload() {
        let cfg = Config::from_json(r##"{"payload":"","settle_ms":0}"##).unwrap();
        assert_eq!(
            check_budget(&cfg),
            Duration::from_millis(10_000) + FIXED_OVERHEAD
        );
    }

    #[test]
    fn budget_counts_unicode_characters_not_bytes() {
        let cfg = Config::from_json(r##"{"payload":"héllo","typing_delay_ms":10,"settle_ms":0}"##)
            .unwrap();
        assert_eq!(
            check_budget(&cfg),
            Duration::from_millis(50) + Duration::from_millis(10_000) + FIXED_OVERHEAD
        );
    }

    #[test]
    fn budget_doubles_the_element_wait_when_a_response_selector_is_configured() {
        let cfg = Config::from_json(
            r##"{"payload":"","settle_ms":0,"element_timeout_ms":5000,
                 "selectors":{"response":".reply"}}"##,
        )
        .unwrap();
        // One wait for the text input/submit button, a second independent one
        // for the reply to stabilize.
        assert_eq!(
            check_budget(&cfg),
            Duration::from_millis(10_000) + FIXED_OVERHEAD
        );
    }

    #[test]
    fn budget_adds_one_wait_pass_per_configured_cleanup_step() {
        let cfg = Config::from_json(
            r##"{"payload":"","settle_ms":0,"element_timeout_ms":5000,
                 "cleanup":{"menu_button":"a","delete_option":"b","confirm_button":"c"}}"##,
        )
        .unwrap();
        // 1 (text input) + 3 (menu, delete, confirm) = 4 passes.
        assert_eq!(
            check_budget(&cfg),
            Duration::from_millis(20_000) + FIXED_OVERHEAD
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

    // `is_confirmed_authenticated` is deliberately stricter than
    // `decide_after_heartbeat`'s `Next::Proceed` — a real bug, not a
    // hypothetical: a 2FA verification-code screen matches neither the
    // `authenticated` nor `login_indicator` selector, so the heartbeat
    // reports 503, and `Next::Proceed` treats that as success. Reusing that
    // check for the relogin-confirmation paths hid the sign-in window out
    // from under someone still typing their verification code.
    #[test]
    fn confirmed_authenticated_requires_exactly_200() {
        assert!(is_confirmed_authenticated(&Ok(payload(200))));
    }

    #[test]
    fn confirmed_authenticated_rejects_the_2fa_case_that_broke_this() {
        // 503 "neither marker found" — what a verification-code prompt
        // reports. `Next::Proceed` would treat this as success.
        assert!(!is_confirmed_authenticated(&Ok(payload(503))));
    }

    #[test]
    fn confirmed_authenticated_rejects_a_login_screen() {
        assert!(!is_confirmed_authenticated(&Ok(payload(401))));
    }

    #[test]
    fn confirmed_authenticated_rejects_a_timeout_or_eval_failure() {
        assert!(!is_confirmed_authenticated(&Err(ProbeError::Timeout)));
        assert!(!is_confirmed_authenticated(&Err(ProbeError::Eval(
            "no webview".into()
        ))));
    }

    #[test]
    fn report_from_heartbeat_labels_the_503_case_explicitly() {
        // `decide_after_heartbeat` alone would silently call this Next::Proceed
        // with no report attached; the relogin-confirmation paths need
        // something to actually show/log when it isn't a real 401.
        let report = report_from_heartbeat(Ok(payload(503)));
        assert_eq!(report.code, 503);
        assert!(report.detail.contains("neither"), "{}", report.detail);
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
