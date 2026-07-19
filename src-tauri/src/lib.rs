//! Pong — a synthetic web health monitor that lives in the system tray.
//!
//! Architecture in one breath: a hidden webview holds a logged-in session to the
//! target dashboard; a cron job periodically injects a synthetic interaction into
//! it; the injected agent reports a status code back over IPC; the tray popover
//! renders that state.

pub mod config;
pub mod health;
pub mod injection;
pub mod monitor;
pub mod scheduler;
pub mod state;
pub mod tray;
pub mod usage;

use crate::config::Config;
use crate::health::ProbePayload;
use crate::injection::AGENT_SCRIPT;
use crate::monitor::{MONITOR_LABEL, POPOVER_LABEL};
use crate::state::{AppState, MonitorSnapshot};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;
use tokio_cron_scheduler::{Job, JobScheduler};

/// How often the tray tooltip / popover countdown is refreshed from Rust.
const TICK: Duration = Duration::from_secs(15);

/// Owns the live cron job so the schedule can be swapped without a restart.
#[derive(Default)]
pub struct CronHandle {
    inner: tokio::sync::Mutex<Option<(JobScheduler, uuid::Uuid)>>,
}

impl CronHandle {
    /// Replace the running job with one driven by `cron`.
    ///
    /// Creates the underlying scheduler on first use.
    pub async fn install(
        &self,
        app: tauri::AppHandle,
        state: Arc<AppState>,
        cron: &str,
    ) -> Result<(), String> {
        self.install_with(cron, move || {
            let app = app.clone();
            let state = state.clone();
            Box::pin(async move {
                monitor::run_health_check(app, state).await;
            })
        })
        .await
    }

    /// Stop the running job, if any, without installing a replacement.
    ///
    /// Used when the user turns the cron toggle off: the schedule config
    /// stays put on disk (so turning it back on restores the same cadence),
    /// but nothing fires until they do.
    pub async fn uninstall(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().await;
        if let Some((scheduler, previous)) = guard.take() {
            scheduler
                .remove(&previous)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// The scheduling half of `install`, independent of what the job does.
    ///
    /// Taking the task as a closure keeps this testable: a scheduler that
    /// silently stops firing, or that leaves the previous job running after a
    /// cron change, is a failure the user would never see — the app just looks
    /// idle forever.
    pub async fn install_with<F>(&self, cron: &str, task: F) -> Result<(), String>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let mut guard = self.inner.lock().await;

        let scheduler = match guard.take() {
            Some((scheduler, previous)) => {
                // Drop the old job first so both schedules never overlap.
                scheduler
                    .remove(&previous)
                    .await
                    .map_err(|e| e.to_string())?;
                scheduler
            }
            None => {
                let scheduler = JobScheduler::new().await.map_err(|e| e.to_string())?;
                scheduler.start().await.map_err(|e| e.to_string())?;
                scheduler
            }
        };

        let job = Job::new_async(cron, move |_uuid, _lock| task()).map_err(|e| e.to_string())?;

        let id = scheduler.add(job).await.map_err(|e| e.to_string())?;
        *guard = Some((scheduler, id));
        Ok(())
    }
}

#[cfg(test)]
mod cron_tests {
    use super::CronHandle;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Count invocations of a job scheduled every second.
    fn counting_task(
        counter: Arc<AtomicUsize>,
    ) -> impl Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
           + Send
           + Sync
           + 'static {
        move || {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fires_repeatedly_on_schedule() {
        let handle = CronHandle::default();
        let hits = Arc::new(AtomicUsize::new(0));

        handle
            .install_with("* * * * * *", counting_task(hits.clone()))
            .await
            .expect("every-second cron should install");

        tokio::time::sleep(Duration::from_millis(3500)).await;

        let count = hits.load(Ordering::SeqCst);
        assert!(
            (2..=5).contains(&count),
            "expected roughly 3 ticks in 3.5s, got {count}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reinstalling_replaces_the_previous_job() {
        let handle = CronHandle::default();
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));

        handle
            .install_with("* * * * * *", counting_task(first.clone()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(2200)).await;
        assert!(
            first.load(Ordering::SeqCst) > 0,
            "the first job should have fired"
        );

        handle
            .install_with("* * * * * *", counting_task(second.clone()))
            .await
            .expect("reinstall should succeed");

        // Sample *after* the swap has settled. Reading the counter before
        // `install_with` returns races the outgoing job's final tick, which is
        // a property of the test harness rather than of the scheduler.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let settled = first.load(Ordering::SeqCst);

        tokio::time::sleep(Duration::from_millis(2200)).await;

        // The replaced job must be gone, not merely shadowed: two live jobs
        // would drive the dashboard twice per tick.
        assert_eq!(
            first.load(Ordering::SeqCst),
            settled,
            "the replaced job kept firing after being swapped out"
        );
        assert!(
            second.load(Ordering::SeqCst) > 0,
            "the new job never fired after reinstall"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rejects_an_invalid_cron_without_disturbing_the_running_job() {
        let handle = CronHandle::default();
        let hits = Arc::new(AtomicUsize::new(0));

        handle
            .install_with("* * * * * *", counting_task(hits.clone()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1200)).await;

        let err = handle
            .install_with("not a cron", counting_task(Arc::new(AtomicUsize::new(0))))
            .await
            .expect_err("garbage cron must be rejected");
        assert!(!err.is_empty());
    }
}

// ---------------------------------------------------------------- IPC commands

#[tauri::command]
fn get_snapshot(state: tauri::State<'_, Arc<AppState>>) -> MonitorSnapshot {
    state.snapshot()
}

/// Past checks, newest first, for the history view.
#[tauri::command]
fn get_history(state: tauri::State<'_, Arc<AppState>>) -> Vec<crate::health::HealthReport> {
    state.history()
}

#[tauri::command]
fn get_config(state: tauri::State<'_, Arc<AppState>>) -> Config {
    state.config_snapshot()
}

/// Validate, persist and hot-apply a new configuration.
#[tauri::command]
async fn save_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    cron_handle: tauri::State<'_, CronHandle>,
    config: Config,
) -> Result<MonitorSnapshot, String> {
    config.validate().map_err(|e| e.to_string())?;
    config.save(&state.config_path).map_err(|e| e.to_string())?;

    let previous = state.config_snapshot();
    let state = state.inner().clone();
    state.set_config(config.clone());

    // Point the hidden webview at the new dashboard if it moved.
    if previous.target_url != config.target_url {
        if let (Some(webview), Ok(url)) = (
            app.get_webview_window(MONITOR_LABEL),
            config.target_url.parse::<tauri::Url>(),
        ) {
            let _ = webview.navigate(url);
        }
    }

    // A new cron string, or a toggle flip, only takes effect once the job is
    // reinstalled (or torn down).
    if previous.cron != config.cron || previous.cron_enabled != config.cron_enabled {
        if config.cron_enabled {
            cron_handle
                .install(app.clone(), state.clone(), &config.cron)
                .await?;
        } else {
            cron_handle.uninstall().await?;
        }
    }

    if previous.autostart_enabled != config.autostart_enabled {
        sync_autostart(&app, config.autostart_enabled);
    }

    monitor::emit_snapshot(&app, &state);
    Ok(state.snapshot())
}

/// Run a check right now, out of band with the cron schedule.
#[tauri::command]
async fn force_check(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    monitor::run_health_check(app, state).await;
    Ok(())
}

/// Called by the injected agent inside the hidden webview.
#[tauri::command]
fn report_health(state: tauri::State<'_, Arc<AppState>>, payload: ProbePayload) {
    state.resolve_report(payload);
}

/// The most recently scraped claude.ai usage panel, if any.
#[tauri::command]
fn get_usage(state: tauri::State<'_, Arc<AppState>>) -> Option<crate::usage::UsageSnapshot> {
    state.usage_snapshot()
}

/// Past usage-scrape attempts, newest first, for the dash's own history view.
#[tauri::command]
fn get_usage_history(state: tauri::State<'_, Arc<AppState>>) -> Vec<crate::usage::UsageLogEntry> {
    state.usage_history()
}

/// Scrape the usage panel right now — used on popover open, on a timer while
/// it stays visible, and by the dash's manual refresh button.
#[tauri::command]
async fn force_usage_check(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    monitor::run_usage_check(app, state).await;
    Ok(())
}

/// Called by the injected agent's usage scraper.
#[tauri::command]
fn report_usage(state: tauri::State<'_, Arc<AppState>>, payload: crate::usage::UsageProbePayload) {
    state.resolve_usage_report(payload);
}

/// Sign out: erase cookies and storage, then reload the login page.
#[tauri::command]
fn clear_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<MonitorSnapshot, String> {
    // Wiping cookies and navigating the webview while a health/usage check is
    // mid-flight on that same webview is a real race, not a theoretical one —
    // it has been observed to leave the webview unusable until restart. The
    // same guard the checks use makes the two mutually exclusive.
    let Some(_guard) = state.try_begin_check() else {
        return Err("a check is currently running — try again in a moment".into());
    };

    let target = state.config_snapshot().target_url;
    monitor::clear_session(&app, &target)?;

    // The session is gone, so the next check will legitimately report 401.
    // Clearing the flag avoids showing a stale "expired" banner in the meantime.
    state.clear_relogin();
    monitor::emit_snapshot(&app, &state);
    Ok(state.snapshot())
}

/// Show or hide the dashboard window, for manual sign-in.
#[tauri::command]
fn toggle_dashboard(app: tauri::AppHandle) -> Result<bool, String> {
    monitor::toggle_dashboard(&app)
}

#[tauri::command]
fn open_relogin(app: tauri::AppHandle) -> Result<(), String> {
    monitor::show_relogin(&app)
}

/// "I'm signed in — resume monitoring": re-verifies the session with a real
/// heartbeat before clearing the recovery banner, rather than trusting the
/// click alone. See `monitor::confirm_relogin` for why that trust broke.
#[tauri::command]
async fn close_relogin(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    monitor::confirm_relogin(&app, &state).await
}

#[tauri::command]
fn hide_popover(app: tauri::AppHandle) {
    if let Some(popover) = app.get_webview_window(POPOVER_LABEL) {
        let _ = popover.hide();
    }
}

// ---------------------------------------------------------------------- setup

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            // A monitor with no history is hard to trust: keep a rolling log of
            // every verdict, on disk and (in dev) on stdout.
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("pongllm".into()),
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_config,
            get_history,
            save_config,
            force_check,
            report_health,
            get_usage,
            get_usage_history,
            force_usage_check,
            report_usage,
            open_relogin,
            close_relogin,
            hide_popover,
            toggle_dashboard,
            clear_session,
        ])
        .setup(|app| {
            // Tray-only app: no dock icon, no app switcher entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            #[cfg(target_os = "macos")]
            disable_app_nap();

            let handle = app.handle().clone();

            let config_dir = app.path().app_config_dir()?;
            let config_path = config_dir.join("config.json");
            let config = Config::load_or_create(&config_path)?;

            let state = Arc::new(AppState::new(config.clone(), config_path));
            app.manage(state.clone());
            app.manage(CronHandle::default());

            // A tray-resident monitor that doesn't come back after a reboot
            // mostly defeats the point — sync the OS registration to match
            // the user's preference on every launch, in case it drifted
            // (e.g. the user removed the login item by hand).
            sync_autostart(&handle, config.autostart_enabled);

            #[cfg(target_os = "macos")]
            build_app_menu(&handle)?;

            build_popover(&handle)?;
            build_hidden_webview(&handle, &config)?;
            tray::build(&handle)?;

            if config.cron_enabled {
                start_scheduler(handle.clone(), state.clone(), config.cron.clone());
            }
            start_ticker(handle, state);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Pong")
        .run(|_app, event| {
            // Closing the popover must not terminate a tray-resident app.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// Reconciles the OS's actual login-item registration with `enabled`.
///
/// Idempotent and cheap enough to call on every launch and every settings
/// save: it only touches the registration when it disagrees with the
/// desired state, so it also self-heals if the user removed the login item
/// by hand outside of Pong.
fn sync_autostart(app: &tauri::AppHandle, enabled: bool) {
    let autostart = app.autolaunch();
    let is_enabled = autostart.is_enabled().unwrap_or(false);
    if enabled && !is_enabled {
        if let Err(e) = autostart.enable() {
            log::warn!("failed to enable autostart: {e}");
        }
    } else if !enabled && is_enabled {
        if let Err(e) = autostart.disable() {
            log::warn!("failed to disable autostart: {e}");
        }
    }
}

/// The macOS application menu.
///
/// Two reasons this is defined explicitly rather than left to the default:
/// the menu bar shows the app's name, and — more importantly — without an Edit
/// submenu the standard Cmd+C/Cmd+V shortcuts do nothing. Pasting a password
/// from a password manager is exactly what the sign-in window is for.
/// Opts the whole process out of App Nap.
///
/// A tray-only app has no visible window, no dock icon and is rarely
/// frontmost — exactly the profile macOS throttles hardest for CPU and
/// timers. That throttling was found to reach into the hidden webview's own
/// JS execution: a `full` check's typing/settle/response waits could run
/// 5-10x slower while occluded, occasionally missing the check's time budget
/// outright. `UserInitiatedAllowingIdleSystemSleep` disables App Nap for this
/// process without holding the whole Mac awake, which a background monitor
/// has no business doing.
///
/// The returned activity token must stay alive for the app's entire
/// lifetime — there is no natural point to end it, so it is deliberately
/// leaked rather than dropped.
#[cfg(target_os = "macos")]
fn disable_app_nap() {
    use objc2_foundation::{ns_string, NSActivityOptions, NSProcessInfo};

    let info = NSProcessInfo::processInfo();
    let activity = info.beginActivityWithOptions_reason(
        NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
        ns_string!("Pong runs periodic synthetic checks against a hidden webview"),
    );
    std::mem::forget(activity);
}

#[cfg(target_os = "macos")]
fn build_app_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{AboutMetadata, Menu, PredefinedMenuItem, Submenu};

    let about = AboutMetadata {
        name: Some("Pong".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        copyright: Some("Copyright (c) 2026 neochaotic".into()),
        ..Default::default()
    };

    let app_menu = Submenu::with_items(
        app,
        "Pong",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("Pong"), Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    app.set_menu(Menu::with_items(app, &[&app_menu, &edit_menu])?)?;
    Ok(())
}

/// The small frameless popover anchored near the tray icon.
fn build_popover(app: &tauri::AppHandle) -> tauri::Result<()> {
    let popover =
        WebviewWindowBuilder::new(app, POPOVER_LABEL, WebviewUrl::App("index.html".into()))
            .title("Pong")
            // One fixed size for every view. Settings/History scroll their own
            // content internally rather than resizing the window on each
            // navigation, which read as jarring — see resizePopover's removal.
            .inner_size(320.0, 360.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()?;

    // Popover semantics: clicking away dismisses it, like a menu bar panel.
    let handle = popover.clone();
    popover.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            let _ = handle.hide();
        }
    });

    Ok(())
}

/// The hidden webview holding the persistent dashboard session.
fn build_hidden_webview(app: &tauri::AppHandle, config: &Config) -> tauri::Result<()> {
    let url: tauri::Url = config
        .target_url
        .parse()
        .map_err(tauri::Error::InvalidUrl)?;

    // Session data lives next to the config so logins survive restarts.
    let data_dir = app.path().app_data_dir()?.join("webview-session");
    std::fs::create_dir_all(&data_dir)?;

    let webview = WebviewWindowBuilder::new(app, MONITOR_LABEL, WebviewUrl::External(url))
        // This window is shown to the user for manual sign-in, so it carries the
        // product name. (It previously had no title so Rust could read the
        // agent's breadcrumb from `document.title` — a diagnostic that only
        // mattered while the IPC bridge was broken.)
        .title("Pong — Dashboard")
        .inner_size(1100.0, 820.0)
        // Hidden by design — this is the real, shipped behavior. The check
        // pipeline must work occluded; see the MutationObserver-based waits
        // in agent.js, added specifically so it does.
        .visible(false)
        .skip_taskbar(true)
        .data_directory(data_dir)
        // Reinstalled on every navigation, before the page's own scripts run.
        .initialization_script(AGENT_SCRIPT)
        // Navigation history is the main clue when a dashboard silently
        // redirects to an SSO provider or a maintenance page.
        .on_page_load(|webview, payload| {
            log::info!("monitor webview {:?}: {}", payload.event(), payload.url());
            let _ = webview;
        })
        .build()?;

    // This is the one persistent webview holding the whole session — losing
    // it isn't like closing a normal window, it breaks every check until the
    // app restarts. Nothing guarded that before: the window has standard
    // native chrome (a close button) and clicking it destroyed the window
    // outright. Hide instead, exactly like the popover already does on
    // focus loss.
    let handle = webview.clone();
    webview.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = handle.hide();
        }
    });

    #[cfg(debug_assertions)]
    webview.open_devtools();

    Ok(())
}

/// Register the cron job that drives periodic checks.
fn start_scheduler(app: tauri::AppHandle, state: Arc<AppState>, cron: String) {
    tauri::async_runtime::spawn(async move {
        let handle = app.state::<CronHandle>();
        if let Err(e) = handle.install(app.clone(), state, &cron).await {
            log::error!("failed to schedule cron `{cron}`: {e}");
        }
    });
}

/// Keep the tray tooltip and popover countdown fresh between checks.
fn start_ticker(app: tauri::AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            // A no-op unless the recovery banner is showing — see
            // `poll_relogin` for why this is safe to run passively rather
            // than waiting for the user to click "I'm signed in".
            monitor::poll_relogin(&app, &state).await;
            monitor::emit_snapshot(&app, &state);
        }
    });
}
