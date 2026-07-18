import { describe, expect, it } from "vitest";
import { isCronShaped, toConfig, toForm, validateForm } from "./configForm";
import type { Config } from "./types";

const config: Config = {
  target_url: "https://dash.internal/login",
  cron: "0 */15 * * * *",
  selectors: {
    authenticated: "#dashboard-main",
    login_indicator: "input[type=password]",
    action_button: "#new-chat",
    text_input: "textarea#prompt",
    submit_button: null,
  },
  payload: "ping",
  settle_ms: 3000,
  typing_delay_ms: 60,
  element_timeout_ms: 10000,
  notifications_enabled: true,
  interaction: "full",
};

describe("toForm / toConfig", () => {
  it("survives a round trip", () => {
    expect(toConfig(toForm(config))).toEqual(config);
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
