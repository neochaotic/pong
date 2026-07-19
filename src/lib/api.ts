/** Thin typed wrapper over the Rust IPC surface. */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import type { Config, HealthReport, MonitorSnapshot, UsageLogEntry, UsageSnapshot } from "./types";

/** Must match `monitor::UPDATE_EVENT`. */
export const UPDATE_EVENT = "monitor://update";

export const getSnapshot = () => invoke<MonitorSnapshot>("get_snapshot");
export const getConfig = () => invoke<Config>("get_config");
/** Past checks, newest first. */
export const getHistory = () => invoke<HealthReport[]>("get_history");
export const saveConfig = (config: Config) =>
  invoke<MonitorSnapshot>("save_config", { config });

/** Grow the popover for the settings form, shrink it back afterwards. */
export const resizePopover = (height: number) =>
  getCurrentWindow().setSize(new LogicalSize(320, height));
export const forceCheck = () => invoke<void>("force_check");
export const openRelogin = () => invoke<void>("open_relogin");
/** Show/hide the dashboard window for manual sign-in; resolves to its visibility. */
export const toggleDashboard = () => invoke<boolean>("toggle_dashboard");
/** Erase cookies and storage, then reload the login page. Destructive. */
export const clearSession = () => invoke<MonitorSnapshot>("clear_session");
export const closeRelogin = () => invoke<void>("close_relogin");
export const hidePopover = () => invoke<void>("hide_popover");

/** The most recently scraped claude.ai usage panel, if any. */
export const getUsage = () => invoke<UsageSnapshot | null>("get_usage");
/** Past usage-scrape attempts, newest first. */
export const getUsageHistory = () => invoke<UsageLogEntry[]>("get_usage_history");
export const forceUsageCheck = () => invoke<void>("force_usage_check");

/** Subscribe to backend state pushes; returns the unlisten handle. */
export const onSnapshot = (
  handler: (snapshot: MonitorSnapshot) => void
): Promise<UnlistenFn> =>
  listen<MonitorSnapshot>(UPDATE_EVENT, (event) => handler(event.payload));
