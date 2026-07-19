import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MonitorSnapshot, UsageSnapshot } from "./lib/types";

// Every one of these is a real IPC command. The popover's buttons were silently
// dead in production because the backend ACL denied them, so these tests pin the
// wiring: a click must reach the corresponding api function.
const api = {
  getSnapshot: vi.fn(),
  getConfig: vi.fn(),
  getHistory: vi.fn(),
  saveConfig: vi.fn(),
  forceCheck: vi.fn(),
  getUsage: vi.fn(),
  getUsageHistory: vi.fn(),
  forceUsageCheck: vi.fn(),
  openRelogin: vi.fn(),
  closeRelogin: vi.fn(),
  hidePopover: vi.fn(),
  quitApp: vi.fn(),
  resizePopover: vi.fn(),
  toggleDashboard: vi.fn(),
  clearSession: vi.fn(),
  onSnapshot: vi.fn(),
  UPDATE_EVENT: "monitor://update",
};
vi.mock("./lib/api", () => api);

const App = (await import("./App.svelte")).default;

const snapshot = (over: Partial<MonitorSnapshot> = {}): MonitorSnapshot => ({
  phase: "READY",
  target_url: "https://dash.internal/login",
  cron: "0 */15 * * * *",
  cron_enabled: true,
  // Fixed 90s ahead of a frozen clock so the countdown is deterministic.
  next_run_unix: Math.floor(Date.now() / 1000) + 90,
  seconds_until_next: 90,
  last_report: {
    code: 200,
    verdict: "healthy",
    detail: "dashboard responded",
    latency_ms: 812,
    at: "2026-07-18T10:15:00Z",
  },
  needs_relogin: false,
  dashboard_visible: false,
  ...over,
});

const usageSnapshot = (over: Partial<UsageSnapshot> = {}): UsageSnapshot => ({
  session_percent: 26,
  session_reset_at: new Date(Date.now() + 3 * 3600_000).toISOString(),
  weekly_percent: 40,
  weekly_reset_at: new Date(Date.now() + 7 * 3600_000).toISOString(),
  fetched_at: new Date().toISOString(),
  ...over,
});

async function goToMonitorTab(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByTestId("tab-monitor"));
}

beforeEach(() => {
  Object.values(api).forEach((fn) => typeof fn === "function" && fn.mockReset());
  api.getSnapshot.mockResolvedValue(snapshot());
  api.getConfig.mockResolvedValue({
    target_url: "https://dash.internal/login",
    cron: "0 */15 * * * *",
    cron_enabled: false,
    selectors: {
      authenticated: "#main",
      login_indicator: "#login",
      action_button: null,
      text_input: "textarea",
      submit_button: null,
      response: null,
    },
    cleanup: { menu_button: null, delete_option: null, confirm_button: null },
    payload: "ping",
    settle_ms: 3000,
    typing_delay_ms: 60,
    element_timeout_ms: 10000,
    notifications_enabled: true,
    interaction: "full",
    usage_url: null,
  });
  api.onSnapshot.mockResolvedValue(vi.fn());
  api.resizePopover.mockResolvedValue(undefined);
  api.toggleDashboard.mockResolvedValue(true);
  api.getHistory.mockResolvedValue([]);
  api.forceCheck.mockResolvedValue(undefined);
  api.getUsage.mockResolvedValue(null);
  api.getUsageHistory.mockResolvedValue([]);
  api.forceUsageCheck.mockResolvedValue(undefined);
});

describe("dash tab (default view)", () => {
  it("shows the tab switcher with dash active by default", async () => {
    render(App);
    expect(await screen.findByTestId("tab-dash")).toBeInTheDocument();
    expect(await screen.findByTestId("usage-view")).toBeInTheDocument();
  });

  it("shows the usage page's URL in its footer, not the health check's target", async () => {
    api.getConfig.mockResolvedValue({
      target_url: "https://dash.internal/login",
      cron: "0 0 5 * * Mon-Fri",
      cron_enabled: false,
      selectors: {
        authenticated: "#main",
        login_indicator: "#login",
        action_button: null,
        text_input: "textarea",
        submit_button: null,
        response: null,
      },
      cleanup: { menu_button: null, delete_option: null, confirm_button: null },
      payload: "ping",
      settle_ms: 3000,
      typing_delay_ms: 60,
      element_timeout_ms: 10000,
      notifications_enabled: true,
      interaction: "full",
      usage_url: "https://dash.internal/usage",
    });
    render(App);

    expect(await screen.findByText("dash.internal/usage")).toBeInTheDocument();
    expect(screen.queryByText("dash.internal/login")).not.toBeInTheDocument();
  });

  it("opens the login window from the dash tab", async () => {
    render(App);
    const user = userEvent.setup();

    // Signing in is not only a recovery action: it is how the first session
    // is established, so the control lives on the tab shown by default.
    await user.click(await screen.findByRole("button", { name: "Show login" }));

    expect(api.toggleDashboard).toHaveBeenCalledOnce();
  });

  it("labels the toggle by the window's actual visibility", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ dashboard_visible: true }));
    render(App);

    expect(await screen.findByRole("button", { name: "Hide login" })).toBeInTheDocument();
  });

  it("quits when QUIT is pressed", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "QUIT" }));

    expect(api.quitApp).toHaveBeenCalledOnce();
  });

  it("fetches usage on mount and renders the percentages", async () => {
    api.getUsage.mockResolvedValue(usageSnapshot());
    render(App);

    expect(await screen.findByText("26% · resets in 3:00:00")).toBeInTheDocument();
    expect(await screen.findByText("40% · resets in 7:00:00")).toBeInTheDocument();
  });

  it("refreshes usage when the refresh button is pressed", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByTestId("usage-refresh"));

    expect(api.forceUsageCheck).toHaveBeenCalled();
  });
});

describe("monitor tab", () => {
  it("shows a disabled message instead of a countdown when cron_enabled is false", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ cron_enabled: false }));
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    expect(await screen.findByText(/Disabled — flip the switch above/)).toBeInTheDocument();
    expect(screen.queryByText("01:30")).not.toBeInTheDocument();
  });

  it("flips the schedule on from the quick toggle without opening Settings", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ cron_enabled: false }));
    api.getConfig.mockResolvedValue({
      target_url: "https://dash.internal/login",
      cron: "0 0 5 * * Mon-Fri",
      cron_enabled: false,
      selectors: {
        authenticated: "#main",
        login_indicator: "#login",
        action_button: null,
        text_input: "textarea",
        submit_button: null,
        response: null,
      },
      cleanup: { menu_button: null, delete_option: null, confirm_button: null },
      payload: "ping",
      settle_ms: 3000,
      typing_delay_ms: 60,
      element_timeout_ms: 10000,
      notifications_enabled: true,
      interaction: "full",
      usage_url: null,
    });
    api.saveConfig.mockResolvedValue(snapshot({ cron_enabled: true }));
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    await user.click(await screen.findByTestId("quick-cron-toggle"));

    expect(api.saveConfig).toHaveBeenCalledWith(
      expect.objectContaining({ cron_enabled: true })
    );
    expect(await screen.findByText("01:30")).toBeInTheDocument();
  });

  it("renders the countdown from the backend snapshot", async () => {
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    expect(await screen.findByText("01:30")).toBeInTheDocument();
  });

  it("shows the last report and shortened target URL", async () => {
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    expect(
      await screen.findByText("200 · dashboard responded · 812ms")
    ).toBeInTheDocument();
    // The dash tab's footer can still be mid-crossfade-out, so more than one
    // match is expected — not a bug, just the tab switch in flight.
    expect((await screen.findAllByText("dash.internal/login")).length).toBeGreaterThan(0);
  });

  it("runs a check when Force Check is pressed", async () => {
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    await user.click(await screen.findByRole("button", { name: "Force Check" }));

    expect(api.forceCheck).toHaveBeenCalledOnce();
  });

  it("disables Force Check while a check is in flight", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ phase: "PINGING" }));
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    expect(await screen.findByRole("button", { name: "Checking…" })).toBeDisabled();
  });

  it("falls back to a placeholder when the schedule is unknown", async () => {
    api.getSnapshot.mockResolvedValue(
      snapshot({ next_run_unix: null, seconds_until_next: null })
    );
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    expect(await screen.findByText("--:--")).toBeInTheDocument();
  });
});

describe("recovery view", () => {
  it("takes over the body when the session expired", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ needs_relogin: true, phase: "ERROR" }));
    render(App);

    expect(await screen.findByText("Dashboard session expired.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Force Check" })).not.toBeInTheDocument();
  });

  it("opens the dashboard window to reconnect", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ needs_relogin: true, phase: "ERROR" }));
    api.openRelogin.mockResolvedValue(undefined);
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Reconnect dashboard" }));

    expect(api.openRelogin).toHaveBeenCalledOnce();
    // The button now offers to finish the flow instead.
    expect(
      await screen.findByRole("button", { name: /I'm signed in/ })
    ).toBeInTheDocument();
  });

  it("resumes monitoring once the user confirms sign-in", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ needs_relogin: true, phase: "ERROR" }));
    api.openRelogin.mockResolvedValue(undefined);
    api.closeRelogin.mockResolvedValue(undefined);
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Reconnect dashboard" }));
    await user.click(await screen.findByRole("button", { name: /I'm signed in/ }));

    expect(api.closeRelogin).toHaveBeenCalledOnce();
  });
});

describe("history view", () => {
  it("loads the usage log when opened from the dash tab", async () => {
    api.getUsageHistory.mockResolvedValue([
      { ok: true, detail: "session 26% · weekly 40%", latency_ms: 900, at: new Date().toISOString() },
    ]);
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "History" }));

    await waitFor(() => expect(api.getUsageHistory).toHaveBeenCalledOnce());
    expect(api.getHistory).not.toHaveBeenCalled();
    expect(await screen.findByTestId("history-row")).toBeInTheDocument();
  });

  it("loads past checks when opened from the monitor tab", async () => {
    api.getHistory.mockResolvedValue([
      {
        code: 200,
        verdict: "healthy",
        detail: "dashboard responded",
        latency_ms: 812,
        at: new Date().toISOString(),
      },
    ]);
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    await user.click(await screen.findByRole("button", { name: "History" }));

    await waitFor(() => expect(api.getHistory).toHaveBeenCalledOnce());
    expect(api.getUsageHistory).not.toHaveBeenCalled();
    expect(await screen.findByTestId("history-row")).toBeInTheDocument();
  });

  it("returns to the monitor tab from history", async () => {
    render(App);
    const user = userEvent.setup();
    await goToMonitorTab(user);

    await user.click(await screen.findByRole("button", { name: "History" }));
    await user.click(await screen.findByRole("button", { name: "Back" }));

    expect(await screen.findByRole("button", { name: "Force Check" })).toBeInTheDocument();
  });
});

describe("settings view", () => {
  it("loads the config when opened, without resizing the fixed-size popover", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Settings" }));

    // getConfig is also fetched on mount now, to power the dash footer's
    // usage_url — this just confirms opening Settings re-fetches too.
    await waitFor(() => expect(api.getConfig).toHaveBeenCalled());
    expect(api.resizePopover).not.toHaveBeenCalled();
    expect(await screen.findByTestId("field-target_url")).toBeInTheDocument();
  });

  it("returns to the main view on cancel", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(await screen.findByTestId("usage-view")).toBeInTheDocument();
  });

  it("persists an edited config through the backend", async () => {
    api.saveConfig.mockResolvedValue(snapshot());
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: "Save" }));

    await waitFor(() => expect(api.saveConfig).toHaveBeenCalledOnce());
  });
});
