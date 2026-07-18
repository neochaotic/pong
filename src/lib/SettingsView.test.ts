import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import SettingsView from "./SettingsView.svelte";
import type { Config } from "./types";

const config: Config = {
  target_url: "https://dash.internal/login",
  cron: "0 */15 * * * *",
  selectors: {
    authenticated: "#dashboard-main",
    login_indicator: "input[type=password]",
    action_button: "#new-chat",
    text_input: "textarea#prompt",
  },
  payload: "ping",
  settle_ms: 3000,
  typing_delay_ms: 60,
  notifications_enabled: true,
};

/** Render with stub callbacks; returns them so assertions can inspect calls. */
function setup(onSaveResult: string | null = null) {
  const onSave = vi.fn().mockResolvedValue(onSaveResult);
  const onClose = vi.fn();
  render(SettingsView, { config, onSave, onClose });
  return { onSave, onClose, user: userEvent.setup() };
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

  it("blocks a 5-field cron before touching the backend", async () => {
    const { onSave, user } = setup();

    await user.clear(field("cron"));
    await user.type(field("cron"), "*/5 * * * *");
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

  it("sends an emptied action button as null", async () => {
    const { onSave, user } = setup();

    await user.clear(field("action_button"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave.mock.calls[0][0].selectors.action_button).toBeNull();
  });
});
