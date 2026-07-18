//! System tray icon, menu and popover toggling.

use crate::monitor::{self, POPOVER_LABEL};
use crate::state::{AppState, MonitorSnapshot};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "pongllm-tray";

/// Build the tray icon, its menu and the click handlers.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let force = MenuItem::with_id(app, "force_check", "Force Check", true, None::<&str>)?;
    let relogin = MenuItem::with_id(app, "relogin", "Reconnect Dashboard…", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide_dashboard", "Hide Dashboard", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Pong", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &force,
            &relogin,
            &hide,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        // Renders as a monochrome template icon on macOS, matching the menu bar.
        .icon_as_template(true)
        .tooltip("Pong — idle")
        .menu(&menu)
        // Left click toggles the popover instead of opening the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "force_check" => {
                let app = app.clone();
                let state = app.state::<Arc<AppState>>().inner().clone();
                tauri::async_runtime::spawn(monitor::run_health_check(app.clone(), state));
            }
            "relogin" => {
                let _ = monitor::show_relogin(app);
            }
            "hide_dashboard" => {
                let _ = monitor::hide_relogin(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;

    Ok(())
}

/// Show the popover if hidden, hide it if already visible.
///
/// `anchor` is the tray icon's screen rect, used to place the panel under it.
pub fn toggle_popover(app: &AppHandle, anchor: Option<tauri::Rect>) {
    let Some(popover) = app.get_webview_window(POPOVER_LABEL) else {
        return;
    };

    if popover.is_visible().unwrap_or(false) {
        let _ = popover.hide();
        return;
    }

    if let Some(rect) = anchor {
        anchor_under(&popover, rect);
    }
    let _ = popover.show();
    let _ = popover.set_focus();
}

/// Centre the popover horizontally under the tray icon.
fn anchor_under(popover: &tauri::WebviewWindow, anchor: tauri::Rect) {
    let (Ok(size), Ok(scale)) = (popover.outer_size(), popover.scale_factor()) else {
        return;
    };

    let position = match anchor.position {
        tauri::Position::Physical(p) => p,
        tauri::Position::Logical(p) => p.to_physical(scale),
    };
    let icon = match anchor.size {
        tauri::Size::Physical(s) => s,
        tauri::Size::Logical(s) => s.to_physical(scale),
    };

    let x = position.x + icon.width as i32 / 2 - size.width as i32 / 2;
    let y = position.y + icon.height as i32;

    // Never let the panel spill off the left edge of the screen.
    let _ = popover.set_position(tauri::PhysicalPosition::new(x.max(0), y.max(0)));
}

/// Push the latest state into the tray tooltip.
pub fn refresh(app: &AppHandle, snapshot: &MonitorSnapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    let tooltip = monitor::tooltip_for(
        snapshot.phase,
        snapshot.last_report.as_ref().map(|r| r.verdict),
        snapshot.seconds_until_next,
    );
    let _ = tray.set_tooltip(Some(tooltip));
}
