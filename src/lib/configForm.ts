import type { Config } from "./types";

/**
 * Flat, all-string mirror of `Config` for binding to inputs.
 *
 * Numbers live as strings while editing so a half-typed value ("30") does not
 * get coerced or rejected mid-keystroke.
 */
export interface FormState {
  target_url: string;
  cron: string;
  cron_enabled: boolean;
  payload: string;
  settle_ms: string;
  typing_delay_ms: string;
  element_timeout_ms: string;
  authenticated: string;
  login_indicator: string;
  action_button: string;
  text_input: string;
  submit_button: string;
  response: string;
  cleanup_menu_button: string;
  cleanup_delete_option: string;
  cleanup_confirm_button: string;
  usage_url: string;
  notifications_enabled: boolean;
  autostart_enabled: boolean;
  probe_only: boolean;
}

/** Ceilings mirrored from `config.rs`, so the UI rejects before the backend does. */
export const MAX_SETTLE_MS = 60_000;
export const MAX_TYPING_DELAY_MS = 2_000;
export const MAX_ELEMENT_TIMEOUT_MS = 120_000;

/** Mirrors `config::default_cron` — 5am, Monday through Friday. */
export const DEFAULT_CRON = "0 0 5 * * Mon-Fri";

/**
 * Mirrors `Config::default()` in `config.rs` field-for-field — a real login
 * page (claude.ai's) and `probe_only`, so "Restore defaults" hands back
 * something a fresh install would actually ship with, not an empty form.
 */
export const DEFAULT_CONFIG: Config = {
  target_url: "https://claude.ai/new",
  cron: DEFAULT_CRON,
  cron_enabled: false,
  selectors: {
    authenticated: 'meta[name="user-login"]',
    login_indicator: "input[type=password]",
    action_button: null,
    text_input: 'textarea, div[contenteditable="true"]',
    submit_button: null,
    response: null,
  },
  cleanup: { menu_button: null, delete_option: null, confirm_button: null },
  payload: "ping",
  settle_ms: 3_000,
  typing_delay_ms: 60,
  element_timeout_ms: 10_000,
  notifications_enabled: true,
  autostart_enabled: true,
  interaction: "probe_only",
  usage_url: null,
};

export function toForm(config: Config): FormState {
  return {
    target_url: config.target_url,
    cron: config.cron,
    cron_enabled: config.cron_enabled,
    payload: config.payload,
    settle_ms: String(config.settle_ms),
    typing_delay_ms: String(config.typing_delay_ms),
    element_timeout_ms: String(config.element_timeout_ms),
    authenticated: config.selectors.authenticated,
    login_indicator: config.selectors.login_indicator,
    action_button: config.selectors.action_button ?? "",
    text_input: config.selectors.text_input,
    submit_button: config.selectors.submit_button ?? "",
    response: config.selectors.response ?? "",
    cleanup_menu_button: config.cleanup.menu_button ?? "",
    cleanup_delete_option: config.cleanup.delete_option ?? "",
    cleanup_confirm_button: config.cleanup.confirm_button ?? "",
    usage_url: config.usage_url ?? "",
    notifications_enabled: config.notifications_enabled,
    autostart_enabled: config.autostart_enabled,
    probe_only: config.interaction === "probe_only",
  };
}

/** The form state "Restore defaults" hands back — `toForm(DEFAULT_CONFIG)`,
 * kept as its own export so callers don't need to know that's how it's built. */
export function defaultForm(): FormState {
  return toForm(DEFAULT_CONFIG);
}

export function toConfig(form: FormState): Config {
  return {
    target_url: form.target_url.trim(),
    cron: form.cron.trim(),
    cron_enabled: form.cron_enabled,
    payload: form.payload,
    settle_ms: Number(form.settle_ms),
    typing_delay_ms: Number(form.typing_delay_ms),
    element_timeout_ms: Number(form.element_timeout_ms),
    selectors: {
      authenticated: form.authenticated.trim(),
      login_indicator: form.login_indicator.trim(),
      // An empty box means "no button to click", which the backend models as null.
      action_button: form.action_button.trim() === "" ? null : form.action_button.trim(),
      text_input: form.text_input.trim(),
      submit_button: form.submit_button.trim() === "" ? null : form.submit_button.trim(),
      response: form.response.trim() === "" ? null : form.response.trim(),
    },
    cleanup: {
      menu_button: form.cleanup_menu_button.trim() === "" ? null : form.cleanup_menu_button.trim(),
      delete_option:
        form.cleanup_delete_option.trim() === "" ? null : form.cleanup_delete_option.trim(),
      confirm_button:
        form.cleanup_confirm_button.trim() === "" ? null : form.cleanup_confirm_button.trim(),
    },
    usage_url: form.usage_url.trim() === "" ? null : form.usage_url.trim(),
    notifications_enabled: form.notifications_enabled,
    autostart_enabled: form.autostart_enabled,
    interaction: form.probe_only ? "probe_only" : "full",
  };
}

/** A cron expression must carry 6 or 7 whitespace-separated fields. */
export function isCronShaped(cron: string): boolean {
  const fields = cron.trim().split(/\s+/).filter(Boolean);
  return fields.length === 6 || fields.length === 7;
}

/**
 * A classic 5-field cron (no seconds — `min hour dom month dow`) is a common
 * slip for anyone used to standard cron syntax, not typo-grade garbage —
 * worth fixing by prepending `0` for seconds rather than discarding what
 * they typed. Returns `null` when the input isn't shaped like a 5-field cron.
 */
export function expandFiveFieldCron(cron: string): string | null {
  const fields = cron.trim().split(/\s+/).filter(Boolean);
  return fields.length === 5 ? `0 ${fields.join(" ")}` : null;
}

function isIntegerInRange(raw: string, max: number): boolean {
  if (!/^\d+$/.test(raw.trim())) return false;
  const value = Number(raw);
  return Number.isInteger(value) && value >= 0 && value <= max;
}

/**
 * Pre-flight validation, mirroring `Config::validate` so the user gets an
 * inline message instead of a round trip. The backend remains authoritative.
 */
export function validateForm(form: FormState): string[] {
  const errors: string[] = [];

  let url: URL | null = null;
  try {
    url = new URL(form.target_url.trim());
  } catch {
    errors.push("Target URL is not a valid URL");
  }
  if (url && url.protocol !== "http:" && url.protocol !== "https:") {
    errors.push("Target URL must use http or https");
  }

  if (!isCronShaped(form.cron)) {
    errors.push("Cron must have 6 fields (including seconds)");
  }

  for (const [label, value] of [
    ["Authenticated selector", form.authenticated],
    ["Login selector", form.login_indicator],
    ["Text input selector", form.text_input],
  ] as const) {
    if (value.trim() === "") errors.push(`${label} is required`);
  }

  if (!isIntegerInRange(form.settle_ms, MAX_SETTLE_MS)) {
    errors.push(`Settle must be 0–${MAX_SETTLE_MS} ms`);
  }
  if (!isIntegerInRange(form.typing_delay_ms, MAX_TYPING_DELAY_MS)) {
    errors.push(`Typing delay must be 0–${MAX_TYPING_DELAY_MS} ms`);
  }
  if (!isIntegerInRange(form.element_timeout_ms, MAX_ELEMENT_TIMEOUT_MS)) {
    errors.push(`Element timeout must be 0–${MAX_ELEMENT_TIMEOUT_MS} ms`);
  }

  return errors;
}
