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
  payload: string;
  settle_ms: string;
  typing_delay_ms: string;
  authenticated: string;
  login_indicator: string;
  action_button: string;
  text_input: string;
  notifications_enabled: boolean;
  probe_only: boolean;
}

/** Ceilings mirrored from `config.rs`, so the UI rejects before the backend does. */
export const MAX_SETTLE_MS = 60_000;
export const MAX_TYPING_DELAY_MS = 2_000;

export function toForm(config: Config): FormState {
  return {
    target_url: config.target_url,
    cron: config.cron,
    payload: config.payload,
    settle_ms: String(config.settle_ms),
    typing_delay_ms: String(config.typing_delay_ms),
    authenticated: config.selectors.authenticated,
    login_indicator: config.selectors.login_indicator,
    action_button: config.selectors.action_button ?? "",
    text_input: config.selectors.text_input,
    notifications_enabled: config.notifications_enabled,
    probe_only: config.interaction === "probe_only",
  };
}

export function toConfig(form: FormState): Config {
  return {
    target_url: form.target_url.trim(),
    cron: form.cron.trim(),
    payload: form.payload,
    settle_ms: Number(form.settle_ms),
    typing_delay_ms: Number(form.typing_delay_ms),
    selectors: {
      authenticated: form.authenticated.trim(),
      login_indicator: form.login_indicator.trim(),
      // An empty box means "no button to click", which the backend models as null.
      action_button: form.action_button.trim() === "" ? null : form.action_button.trim(),
      text_input: form.text_input.trim(),
    },
    notifications_enabled: form.notifications_enabled,
    interaction: form.probe_only ? "probe_only" : "full",
  };
}

/** A cron expression must carry 6 or 7 whitespace-separated fields. */
export function isCronShaped(cron: string): boolean {
  const fields = cron.trim().split(/\s+/).filter(Boolean);
  return fields.length === 6 || fields.length === 7;
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

  return errors;
}
