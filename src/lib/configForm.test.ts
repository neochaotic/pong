import { describe, expect, it } from "vitest";
import {
  DEFAULT_CRON,
  expandFiveFieldCron,
  isCronShaped,
  toConfig,
  toForm,
  validateForm,
} from "./configForm";
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

describe("toForm / toConfig", () => {
  it("survives a round trip", () => {
    expect(toConfig(toForm(config))).toEqual(config);
  });

  it("carries cron_enabled through the round trip in both states", () => {
    expect(toConfig(toForm({ ...config, cron_enabled: true })).cron_enabled).toBe(true);
    expect(toConfig(toForm({ ...config, cron_enabled: false })).cron_enabled).toBe(false);
  });

  it("carries autostart_enabled through the round trip in both states", () => {
    expect(toConfig(toForm({ ...config, autostart_enabled: true })).autostart_enabled).toBe(true);
    expect(toConfig(toForm({ ...config, autostart_enabled: false })).autostart_enabled).toBe(
      false
    );
  });

  it("represents a missing action button as an empty box", () => {
    const form = toForm({ ...config, selectors: { ...config.selectors, action_button: null } });
    expect(form.action_button).toBe("");
  });

  it("maps an empty action button back to null", () => {
    const form = toForm(config);
    form.action_button = "   ";
    expect(toConfig(form).selectors.action_button).toBeNull();
  });

  it("represents a missing response selector as an empty box", () => {
    const form = toForm({ ...config, selectors: { ...config.selectors, response: null } });
    expect(form.response).toBe("");
  });

  it("represents missing cleanup selectors as empty boxes", () => {
    const form = toForm(config);
    expect(form.cleanup_menu_button).toBe("");
    expect(form.cleanup_delete_option).toBe("");
    expect(form.cleanup_confirm_button).toBe("");
  });

  it("represents a missing usage_url as an empty box", () => {
    const form = toForm({ ...config, usage_url: null });
    expect(form.usage_url).toBe("");
  });

  it("maps an empty usage_url back to null", () => {
    const form = toForm({ ...config, usage_url: "https://claude.ai/settings/usage" });
    form.usage_url = "   ";
    expect(toConfig(form).usage_url).toBeNull();
  });

  it("round-trips configured cleanup selectors", () => {
    const withCleanup: Config = {
      ...config,
      cleanup: {
        menu_button: "[data-testid=\"page-header\"] button",
        delete_option: "[data-testid=\"delete-chat-trigger\"]",
        confirm_button: ".text-on-danger",
      },
    };
    expect(toConfig(toForm(withCleanup))).toEqual(withCleanup);
  });

  it("maps an empty response selector back to null", () => {
    const form = toForm(config);
    form.response = "   ";
    expect(toConfig(form).selectors.response).toBeNull();
  });

  it("trims surrounding whitespace from selectors and URL", () => {
    const form = toForm(config);
    form.target_url = "  https://x.dev/  ";
    form.authenticated = "  #main  ";
    const out = toConfig(form);
    expect(out.target_url).toBe("https://x.dev/");
    expect(out.selectors.authenticated).toBe("#main");
  });

  it("converts numeric fields back to numbers", () => {
    const form = toForm(config);
    form.settle_ms = "1500";
    expect(toConfig(form).settle_ms).toBe(1500);
  });

  it("preserves payload whitespace verbatim", () => {
    const form = toForm(config);
    form.payload = "  ping  ";
    expect(toConfig(form).payload).toBe("  ping  ");
  });
});

describe("interaction mode", () => {
  it("maps probe_only both ways", () => {
    const probing: Config = { ...config, interaction: "probe_only" };
    expect(toForm(probing).probe_only).toBe(true);
    expect(toConfig(toForm(probing)).interaction).toBe("probe_only");
  });

  it("maps full interaction both ways", () => {
    expect(toForm(config).probe_only).toBe(false);
    expect(toConfig(toForm(config)).interaction).toBe("full");
  });

  it("survives a round trip in probe-only mode", () => {
    const probing: Config = { ...config, interaction: "probe_only" };
    expect(toConfig(toForm(probing))).toEqual(probing);
  });
});

describe("isCronShaped", () => {
  it("accepts the default cron", () => {
    expect(isCronShaped(DEFAULT_CRON)).toBe(true);
  });

  it("accepts 6 and 7 field expressions", () => {
    expect(isCronShaped("0 */15 * * * *")).toBe(true);
    expect(isCronShaped("0 0 9 * * Mon 2026")).toBe(true);
  });

  it("rejects the classic 5-field form", () => {
    expect(isCronShaped("*/15 * * * *")).toBe(false);
  });

  it("tolerates irregular spacing", () => {
    expect(isCronShaped("  0   */15 * * * *  ")).toBe(true);
  });

  it("rejects empty input", () => {
    expect(isCronShaped("")).toBe(false);
  });
});

describe("expandFiveFieldCron", () => {
  it("prepends seconds=0 to a classic 5-field cron", () => {
    expect(expandFiveFieldCron("10 5 * * *")).toBe("0 10 5 * * *");
  });

  it("tolerates irregular spacing", () => {
    expect(expandFiveFieldCron("  10   5 * * *  ")).toBe("0 10 5 * * *");
  });

  it("returns null for an already 6-field cron", () => {
    expect(expandFiveFieldCron(DEFAULT_CRON)).toBeNull();
  });

  it("returns null for a 7-field cron", () => {
    expect(expandFiveFieldCron("0 0 9 * * Mon 2026")).toBeNull();
  });

  it("returns null for empty input", () => {
    expect(expandFiveFieldCron("")).toBeNull();
  });
});

describe("validateForm", () => {
  it("passes a valid form", () => {
    expect(validateForm(toForm(config))).toEqual([]);
  });

  it("rejects a non-http scheme", () => {
    const form = { ...toForm(config), target_url: "file:///etc/passwd" };
    expect(validateForm(form)).toContain("Target URL must use http or https");
  });

  it("rejects an unparseable URL", () => {
    const form = { ...toForm(config), target_url: "not-a-url" };
    expect(validateForm(form)).toContain("Target URL is not a valid URL");
  });

  it("rejects a 5-field cron", () => {
    const form = { ...toForm(config), cron: "*/5 * * * *" };
    expect(validateForm(form)).toContain("Cron must have 6 fields (including seconds)");
  });

  it("requires the mandatory selectors", () => {
    const form = { ...toForm(config), authenticated: "  ", text_input: "" };
    const errors = validateForm(form);
    expect(errors).toContain("Authenticated selector is required");
    expect(errors).toContain("Text input selector is required");
  });

  it("does not require the optional action button", () => {
    const form = { ...toForm(config), action_button: "" };
    expect(validateForm(form)).toEqual([]);
  });

  it("rejects out-of-range and non-numeric timings", () => {
    expect(validateForm({ ...toForm(config), settle_ms: "600000" })).toContain(
      "Settle must be 0–60000 ms"
    );
    expect(validateForm({ ...toForm(config), settle_ms: "abc" })).toContain(
      "Settle must be 0–60000 ms"
    );
    expect(validateForm({ ...toForm(config), typing_delay_ms: "-5" })).toContain(
      "Typing delay must be 0–2000 ms"
    );
  });

  it("accumulates every problem at once", () => {
    const form = { ...toForm(config), target_url: "nope", cron: "* * *", authenticated: "" };
    expect(validateForm(form).length).toBe(3);
  });
});
