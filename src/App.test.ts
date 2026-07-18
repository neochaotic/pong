import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MonitorSnapshot } from "./lib/types";

// Every one of these is a real IPC command. The popover's buttons were silently
// dead in production because the backend ACL denied them, so these tests pin the
// wiring: a click must reach the corresponding api function.
const api = {
  getSnapshot: vi.fn(),
  getConfig: vi.fn(),
  getHistory: vi.fn(),
  saveConfig: vi.fn(),
  forceCheck: vi.fn(),
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

beforeEach(() => {
  Object.values(api).forEach((fn) => typeof fn === "function" && fn.mockReset());
  api.getSnapshot.mockResolvedValue(snapshot());
  api.getConfig.mockResolvedValue({
    target_url: "https://dash.internal/login",
    cron: "0 */15 * * * *",
    selectors: {
      authenticated: "#main",
      login_indicator: "#login",
      action_button: null,
      text_input: "textarea",
      submit_button: null,
    },
    payload: "ping",
    settle_ms: 3000,
    typing_delay_ms: 60,
    element_timeout_ms: 10000,
    notifications_enabled: true,
    interaction: "full",
  });
  api.onSnapshot.mockResolvedValue(vi.fn());
  api.resizePopover.mockResolvedValue(undefined);
  api.toggleDashboard.mockResolvedValue(true);
  api.getHistory.mockResolvedValue([]);
  api.forceCheck.mockResolvedValue(undefined);
});

describe("monitor view", () => {
  it("renders the countdown from the backend snapshot", async () => {
    render(App);
    expect(await screen.findByText("01:30")).toBeInTheDocument();
  });

  it("shows the last report and shortened target URL", async () => {
    render(App);
    expect(
      await screen.findByText("200 · dashboard responded · 812ms")
    ).toBeInTheDocument();
    expect(screen.getByText("dash.internal/login")).toBeInTheDocument();
  });

  it("runs a check when Force Check is pressed", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Force Check" }));

    expect(api.forceCheck).toHaveBeenCalledOnce();
  });

  it("disables Force Check while a check is in flight", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ phase: "PINGING" }));
    render(App);

    expect(await screen.findByRole("button", { name: "Checking…" })).toBeDisabled();
  });

  it("quits when QUIT is pressed", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "QUIT" }));

    expect(api.quitApp).toHaveBeenCalledOnce();
  });

  it("opens the login window from the always-visible toggle", async () => {
    render(App);
    const user = userEvent.setup();

    // Signing in is not only a recovery action: it is how the first session
    // is established, so the control must not be gated behind a 401.
    await user.click(await screen.findByRole("button", { name: "Show login" }));

    expect(api.toggleDashboard).toHaveBeenCalledOnce();
  });

  it("labels the toggle by the window's actual visibility", async () => {
    api.getSnapshot.mockResolvedValue(snapshot({ dashboard_visible: true }));
    render(App);

    expect(await screen.findByRole("button", { name: "Hide login" })).toBeInTheDocument();
  });

  it("falls back to a placeholder when the schedule is unknown", async () => {
    api.getSnapshot.mockResolvedValue(
      snapshot({ next_run_unix: null, seconds_until_next: null })
    );
    render(App);

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
  it("loads past checks when opened", async () => {
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

    await user.click(await screen.findByRole("button", { name: "History" }));

    await waitFor(() => expect(api.getHistory).toHaveBeenCalledOnce());
    expect(await screen.findByTestId("history-row")).toBeInTheDocument();
  });

  it("returns to the monitor view from history", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "History" }));
    await user.click(await screen.findByRole("button", { name: "Back" }));

    expect(await screen.findByRole("button", { name: "Force Check" })).toBeInTheDocument();
    expect(api.resizePopover).toHaveBeenLastCalledWith(260);
  });
});

describe("settings view", () => {
  it("loads the config and grows the window when opened", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Settings" }));

    await waitFor(() => expect(api.getConfig).toHaveBeenCalledOnce());
    expect(api.resizePopover).toHaveBeenCalledWith(470);
    expect(await screen.findByTestId("field-target_url")).toBeInTheDocument();
  });

  it("shrinks the window again on cancel", async () => {
    render(App);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(api.resizePopover).toHaveBeenLastCalledWith(260);
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
