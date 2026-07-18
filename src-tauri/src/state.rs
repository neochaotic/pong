//! Shared application state and the snapshot handed to the UI.

use crate::config::Config;
use crate::health::{HealthReport, Phase, ProbePayload};
use crate::scheduler;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::oneshot;

/// RAII guard proving a check holds the exclusive right to drive the webview.
///
/// Releasing on `Drop` means every early return in the pipeline — timeout,
/// missing webview, 401 — frees the slot without a manual unlock.
pub struct CheckGuard<'a> {
    running: &'a AtomicBool,
}

impl Drop for CheckGuard<'_> {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// How many past checks are kept. Enough to see a pattern (a flapping session,
/// a slow degradation) without turning the popover into a log viewer or growing
/// unbounded in a process that runs for weeks.
pub const HISTORY_LIMIT: usize = 50;

/// Everything the popover needs to render, in one message.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MonitorSnapshot {
    pub phase: Phase,
    pub target_url: String,
    pub cron: String,
    /// Unix seconds of the next scheduled check, if the cron is valid.
    pub next_run_unix: Option<i64>,
    /// Seconds remaining until that run — the UI ticks this down locally.
    pub seconds_until_next: Option<i64>,
    pub last_report: Option<HealthReport>,
    /// True after a 401, until a check succeeds again.
    pub needs_relogin: bool,
    /// Whether the dashboard window is currently on screen.
    #[serde(default)]
    pub dashboard_visible: bool,
}

/// Lock a mutex, tolerating a poisoned one.
///
/// Every value behind these mutexes is a plain snapshot (a phase, a report, a
/// config). A panic elsewhere cannot leave them logically inconsistent, so
/// recovering the inner value is strictly better than propagating the panic and
/// bricking the monitor for the rest of the process's life.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Process-wide state, owned by Tauri and reachable from every command.
pub struct AppState {
    config: Mutex<Config>,
    pub config_path: PathBuf,
    phase: Mutex<Phase>,
    last_report: Mutex<Option<HealthReport>>,
    needs_relogin: Mutex<bool>,
    /// Most recent checks, newest first.
    history: Mutex<VecDeque<HealthReport>>,
    nonce: AtomicU64,
    /// Probes waiting for their report to come back from the webview.
    pending: Mutex<HashMap<u64, oneshot::Sender<ProbePayload>>>,
    /// Guards against two checks typing into the dashboard at once.
    running: AtomicBool,
}

impl AppState {
    pub fn new(config: Config, config_path: PathBuf) -> Self {
        Self {
            config: Mutex::new(config),
            config_path,
            phase: Mutex::new(Phase::Ready),
            last_report: Mutex::new(None),
            needs_relogin: Mutex::new(false),
            history: Mutex::new(VecDeque::with_capacity(HISTORY_LIMIT)),
            nonce: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            running: AtomicBool::new(false),
        }
    }

    /// Claim the right to run a check, or `None` if one is already in flight.
    ///
    /// A cron tick and a manual "Force Check" can otherwise overlap and type
    /// into the dashboard simultaneously.
    pub fn try_begin_check(&self) -> Option<CheckGuard<'_>> {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| CheckGuard {
                running: &self.running,
            })
    }

    /// Allocate a fresh correlation id for a probe.
    pub fn next_nonce(&self) -> u64 {
        self.nonce.fetch_add(1, Ordering::Relaxed)
    }

    pub fn phase(&self) -> Phase {
        *lock(&self.phase)
    }

    pub fn set_phase(&self, phase: Phase) {
        *lock(&self.phase) = phase;
    }

    pub fn config_snapshot(&self) -> Config {
        lock(&self.config).clone()
    }

    /// Replace the live configuration.
    pub fn set_config(&self, config: Config) {
        *lock(&self.config) = config;
    }

    /// Record a finished check and derive the resulting phase.
    pub fn record_report(&self, report: HealthReport) {
        *lock(&self.needs_relogin) = report.verdict.needs_relogin();
        self.set_phase(report.phase());

        let mut history = lock(&self.history);
        history.push_front(report.clone());
        // Bounded: this process is expected to run for weeks.
        while history.len() > HISTORY_LIMIT {
            history.pop_back();
        }
        drop(history);

        *lock(&self.last_report) = Some(report);
    }

    /// Past checks, newest first.
    pub fn history(&self) -> Vec<HealthReport> {
        lock(&self.history).iter().cloned().collect()
    }

    pub fn needs_relogin(&self) -> bool {
        *lock(&self.needs_relogin)
    }

    pub fn clear_relogin(&self) {
        *lock(&self.needs_relogin) = false;
    }

    /// Register interest in the report for `nonce`, returning its receiver.
    pub fn expect_report(&self, nonce: u64) -> oneshot::Receiver<ProbePayload> {
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(nonce, tx);
        rx
    }

    /// Route an incoming report to whoever is waiting for it.
    ///
    /// Returns `false` for stale or unknown nonces, which are simply dropped.
    pub fn resolve_report(&self, payload: ProbePayload) -> bool {
        let waiter = lock(&self.pending).remove(&payload.nonce);
        match waiter {
            Some(tx) => tx.send(payload).is_ok(),
            None => false,
        }
    }

    /// Stop waiting for `nonce` (used when a probe times out).
    pub fn forget_report(&self, nonce: u64) {
        lock(&self.pending).remove(&nonce);
    }

    pub fn snapshot(&self) -> MonitorSnapshot {
        let cfg = self.config_snapshot();
        let now = Utc::now();
        let next: Option<DateTime<Utc>> = scheduler::next_occurrence(&cfg.cron, now);

        MonitorSnapshot {
            phase: self.phase(),
            target_url: cfg.target_url,
            cron: cfg.cron,
            next_run_unix: next.map(|t| t.timestamp()),
            seconds_until_next: next.map(|t| (t - now).num_seconds().max(0)),
            last_report: lock(&self.last_report).clone(),
            needs_relogin: self.needs_relogin(),
            // Filled in by the caller that has an AppHandle; the state layer
            // deliberately knows nothing about windows.
            dashboard_visible: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Verdict;

    fn state() -> AppState {
        AppState::new(Config::default(), PathBuf::from("/tmp/pongllm-test.json"))
    }

    #[test]
    fn starts_ready_with_no_history() {
        let s = state();
        assert_eq!(s.phase(), Phase::Ready);
        assert!(!s.needs_relogin());
        assert!(s.snapshot().last_report.is_none());
    }

    #[test]
    fn nonces_are_unique_and_increasing() {
        let s = state();
        let a = s.next_nonce();
        let b = s.next_nonce();
        assert!(b > a, "{b} should follow {a}");
    }

    #[test]
    fn recording_an_unauthorized_report_raises_the_relogin_flag() {
        let s = state();
        s.record_report(HealthReport::new(401, "login screen", 20));

        assert_eq!(s.phase(), Phase::Error);
        assert!(s.needs_relogin());
        let snap = s.snapshot();
        assert_eq!(snap.last_report.unwrap().verdict, Verdict::Unauthorized);
        assert!(snap.needs_relogin);
    }

    #[test]
    fn a_healthy_report_clears_the_relogin_flag() {
        let s = state();
        s.record_report(HealthReport::new(401, "login screen", 20));
        s.record_report(HealthReport::new(200, "responded", 500));

        assert_eq!(s.phase(), Phase::Ready);
        assert!(!s.needs_relogin());
    }

    #[tokio::test]
    async fn a_report_reaches_the_waiting_probe() {
        let s = state();
        let nonce = s.next_nonce();
        let rx = s.expect_report(nonce);

        let delivered = s.resolve_report(ProbePayload {
            code: 200,
            detail: "ok".into(),
            latency_ms: 10,
            nonce,
        });

        assert!(delivered);
        assert_eq!(rx.await.unwrap().code, 200);
    }

    #[test]
    fn stale_reports_are_dropped() {
        let s = state();
        let nonce = s.next_nonce();
        let _rx = s.expect_report(nonce);

        let delivered = s.resolve_report(ProbePayload {
            code: 200,
            detail: "late".into(),
            latency_ms: 10,
            nonce: nonce + 999,
        });

        assert!(!delivered, "an unknown nonce must not resolve any waiter");
    }

    #[test]
    fn forgetting_a_probe_prevents_later_delivery() {
        let s = state();
        let nonce = s.next_nonce();
        let _rx = s.expect_report(nonce);
        s.forget_report(nonce);

        assert!(!s.resolve_report(ProbePayload {
            code: 200,
            detail: "too late".into(),
            latency_ms: 1,
            nonce,
        }));
    }

    #[test]
    fn only_one_check_may_run_at_a_time() {
        let s = state();
        let first = s.try_begin_check().expect("first caller wins");
        assert!(
            s.try_begin_check().is_none(),
            "second caller must be turned away"
        );

        drop(first);
        assert!(
            s.try_begin_check().is_some(),
            "slot frees once the guard drops"
        );
    }

    #[test]
    fn the_check_slot_is_released_on_early_return() {
        let s = state();
        // Simulate a pipeline that bails out early via `?`-style return.
        fn bail(state: &AppState) {
            let _guard = state.try_begin_check().unwrap();
            // early return with the guard still alive
        }
        bail(&s);
        assert!(s.try_begin_check().is_some(), "Drop must free the slot");
    }

    #[test]
    fn state_survives_a_poisoned_mutex() {
        let state = std::sync::Arc::new(state());
        let victim = state.clone();

        // Panic while holding the lock — this poisons the mutex for good.
        let died = std::thread::spawn(move || {
            let _held = victim.phase.lock().unwrap();
            panic!("simulated panic while holding the lock");
        })
        .join();
        assert!(died.is_err(), "the helper thread was supposed to panic");

        // A long-running monitor must not be bricked by one poisoned lock.
        assert_eq!(state.phase(), Phase::Ready);
        state.set_phase(Phase::Pinging);
        assert_eq!(state.phase(), Phase::Pinging);
        assert!(state.snapshot().next_run_unix.is_some());
    }

    #[test]
    fn history_records_checks_newest_first() {
        let s = state();
        s.record_report(HealthReport::new(200, "first", 10));
        s.record_report(HealthReport::new(401, "second", 20));

        let history = s.history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].detail, "second", "newest entry should lead");
        assert_eq!(history[1].detail, "first");
    }

    #[test]
    fn history_is_bounded() {
        let s = state();
        for i in 0..(HISTORY_LIMIT + 25) {
            s.record_report(HealthReport::new(200, format!("check {i}"), 1));
        }

        let history = s.history();
        assert_eq!(history.len(), HISTORY_LIMIT, "must not grow without bound");
        // The oldest entries are the ones dropped.
        assert_eq!(history[0].detail, format!("check {}", HISTORY_LIMIT + 24));
    }

    #[test]
    fn history_starts_empty() {
        assert!(state().history().is_empty());
    }

    #[test]
    fn snapshot_projects_the_schedule() {
        let s = state();
        let snap = s.snapshot();
        assert!(
            snap.next_run_unix.is_some(),
            "default cron must be schedulable"
        );
        assert!(snap.seconds_until_next.unwrap() <= 15 * 60);
        assert_eq!(snap.target_url, Config::default().target_url);
    }
}
