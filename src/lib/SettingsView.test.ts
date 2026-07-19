import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import SettingsView from "./SettingsView.svelte";
import type { Config } from "./types";

const config: Config = {
  target_url: "https://dash.internal/login",
  cron: "0 */15 * * * *",
  cron_enabled: false,
  selectors: {
    authenticated: "#dashboard-main",
    login_indicator: "input[type=password]",
    action_button: "#new-chat",
    text_input: "textarea#prompt",
    submit_button: null,
    response: null,
  },
  cleanup: { menu_button: null, delete_option: null, confirm_button: null },
  payload: "ping",
  settle_ms: 3000,
  typing_delay_ms: 60,
  element_timeout_ms: 10000,
  notifications_enabled: true,
  autostart_enabled: true,
  interaction: "full",
  usage_url: null,
};

/** Render with stub callbacks; returns them so assertions can inspect calls. */
function setup(onSaveResult: string | null = null, onClearResult: string | null = null) {
  const onSave = vi.fn().mockResolvedValue(onSaveResult);
  const onClose = vi.fn();
  const onClearSession = vi.fn().mockResolvedValue(onClearResult);
  render(SettingsView, { config, onSave, onClose, onClearSession });
  return { onSave, onClose, onClearSession, user: userEvent.setup() };
}

const field = (name: string) => screen.getByTestId(`field-${name}`);

describe("SettingsView", () => {
  it("pre-fills the form from the current config", () => {
    setup();
    expect(field("target_url")).toHaveValue("https://dash.internal/login");
    expect(field("cron")).toHaveValue("0 */15 * * * *");
    expect(field("authenticated")).toHaveValue("#dashboard-main");
    expect(field("action_button")).toHaveValue("#new-chat");
  });

  it("cancels without saving", async () => {
    const { onSave, onClose, user } = setup();
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onClose).toHaveBeenCalledOnce();
    expect(onSave).not.toHaveBeenCalled();
  });

  it("saves the edited config and closes", async () => {
    const { onSave, onClose, user } = setup();

    await user.clear(field("target_url"));
    await user.type(field("target_url"), "https://other.dev/app");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledOnce();
    expect(onSave.mock.calls[0][0].target_url).toBe("https://other.dev/app");
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("blocks an invalid URL before touching the backend", async () => {
    const { onSave, onClose, user } = setup();

    await user.clear(field("target_url"));
    await user.type(field("target_url"), "not-a-url");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId("form-errors")).toHaveTextContent(
      "Target URL is not a valid URL"
    );
  });

  it("auto-expands a classic 5-field cron with seconds=0 and saves it", async () => {
    const { onSave, user } = setup();

    await user.clear(field("cron"));
    await user.type(field("cron"), "10 5 * * *");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave.mock.calls[0][0].cron).toBe("0 10 5 * * *");
    expect(field("cron")).toHaveValue("0 10 5 * * *");
  });

  it("blocks a cron with the wrong field count before touching the backend", async () => {
    const { onSave, user } = setup();

    await user.clear(field("cron"));
    await user.type(field("cron"), "* * *");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByTestId("form-errors")).toHaveTextContent("6 fields");
  });

  it("surfaces a backend rejection and stays open", async () => {
    const { onClose, user } = setup("Error: cron rejected by validator");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByTestId("form-errors")).toHaveTextContent(
      "cron rejected by validator"
    );
    expect(onClose).not.toHaveBeenCalled();
  });

  it("toggles probe-only mode", async () => {
    const { onSave, user } = setup();

    await user.click(screen.getByTestId("field-probe_only"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave.mock.calls[0][0].interaction).toBe("probe_only");
  });

  it("toggles launch-at-login off", async () => {
    const { onSave, user } = setup();

    await user.click(screen.getByTestId("field-autostart_enabled"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave.mock.calls[0][0].autostart_enabled).toBe(false);
  });

  it("toggles the cron schedule on", async () => {
    const { onSave, user } = setup();

    await user.click(screen.getByTestId("field-cron_enabled"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave.mock.calls[0][0].cron_enabled).toBe(true);
  });

  it("resets an invalid cron to the default after a rejected save", async () => {
    const { user } = setup("Error: invalid cron expression `nope`: bad token");

    await user.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByTestId("form-errors");

    expect(field("cron")).toHaveValue("0 0 5 * * Mon-Fri");
  });

  it("resets a cron the local validator rejects, without a round trip", async () => {
    const { onSave, user } = setup();

    await user.clear(field("cron"));
    await user.type(field("cron"), "* * *");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).not.toHaveBeenCalled();
    expect(field("cron")).toHaveValue("0 0 5 * * Mon-Fri");
  });

  it("does not wipe the session on the first click", async () => {
    const { onClearSession, user } = setup();

    await user.click(screen.getByTestId("clear-session"));

    // Signing the user out has no undo, so the first click only arms it.
    expect(onClearSession).not.toHaveBeenCalled();
    expect(screen.getByTestId("clear-session")).toHaveTextContent("Confirm");
  });

  it("wipes the session once confirmed", async () => {
    const { onClearSession, user } = setup();

    await user.click(screen.getByTestId("clear-session"));
    await user.click(screen.getByTestId("clear-session"));

    expect(onClearSession).toHaveBeenCalledOnce();
  });

  it("lets the user back out before confirming", async () => {
    const { onClearSession, user } = setup();

    await user.click(screen.getByTestId("clear-session"));
    await user.click(screen.getByRole("button", { name: "cancel" }));

    expect(onClearSession).not.toHaveBeenCalled();
    expect(screen.getByTestId("clear-session")).toHaveTextContent("Clear session data");
  });

  it("does not restore defaults on the first click", async () => {
    const { onSave, user } = setup();

    await user.click(screen.getByTestId("restore-defaults"));

    expect(field("target_url")).toHaveValue("https://dash.internal/login");
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByTestId("restore-defaults")).toHaveTextContent("Confirm");
  });

  it("fills the form with defaults once confirmed, without saving", async () => {
    const { onSave, user } = setup();

    await user.click(screen.getByTestId("restore-defaults"));
    await user.click(screen.getByTestId("restore-defaults"));

    expect(field("target_url")).toHaveValue("https://github.com/login");
    expect(field("cron")).toHaveValue("0 0 5 * * Mon-Fri");
    expect(onSave).not.toHaveBeenCalled();
  });

  it("lets the user back out before confirming a restore", async () => {
    const { user } = setup();

    await user.click(screen.getByTestId("restore-defaults"));
    await user.click(screen.getByRole("button", { name: "cancel" }));

    expect(field("target_url")).toHaveValue("https://dash.internal/login");
    expect(screen.getByTestId("restore-defaults")).toHaveTextContent("Restore defaults");
  });

  it("surfaces a failure from the backend", async () => {
    const { user } = setup(null, "Error: monitor webview is not running");

    await user.click(screen.getByTestId("clear-session"));
    await user.click(screen.getByTestId("clear-session"));

    expect(await screen.findByTestId("form-errors")).toHaveTextContent(
      "monitor webview is not running"
    );
  });

  it("sends an emptied action button as null", async () => {
    const { onSave, user } = setup();

    await user.clear(field("action_button"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave.mock.calls[0][0].selectors.action_button).toBeNull();
  });

  it("shows the running app version, so a tester can tell which build they're on", () => {
    setup();

    expect(screen.getByTestId("app-version")).toHaveTextContent("Pong v");
  });
});
