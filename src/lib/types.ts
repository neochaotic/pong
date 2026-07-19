/** Mirrors `health::Verdict` in the Rust backend. */
export type Verdict = "healthy" | "unauthorized" | "degraded" | "unreachable";

/** Mirrors `health::Phase`. */
export type Phase = "READY" | "PINGING" | "ERROR";

/** Mirrors `health::HealthReport`. */
export interface HealthReport {
  code: number;
  verdict: Verdict;
  detail: string;
  latency_ms: number;
  at: string;
}

/** Mirrors `config::Selectors`. */
export interface Selectors {
  authenticated: string;
  login_indicator: string;
  action_button: string | null;
  text_input: string;
  submit_button: string | null;
  response: string | null;
}

/** Mirrors `config::Cleanup`. */
export interface Cleanup {
  menu_button: string | null;
  delete_option: string | null;
  confirm_button: string | null;
}

/** Mirrors `config::Config`. */
export type Interaction = "full" | "probe_only";

export interface Config {
  target_url: string;
  cron: string;
  cron_enabled: boolean;
  selectors: Selectors;
  cleanup: Cleanup;
  payload: string;
  settle_ms: number;
  typing_delay_ms: number;
  element_timeout_ms: number;
  notifications_enabled: boolean;
  autostart_enabled: boolean;
  interaction: Interaction;
  usage_url: string | null;
}

/** Mirrors `usage::MetricSnapshot`. */
export interface MetricSnapshot {
  percent: number;
  /** `null` when the percent was scraped but the reset text couldn't be parsed. */
  reset_at: string | null;
  /** Raw reset text, set only when `reset_at` is null because parsing failed. */
  reset_note: string | null;
}

/**
 * Mirrors `usage::UsageSnapshot`. Session and weekly are independent — one
 * metric missing or lacking a reset time never blanks the other.
 */
export interface UsageSnapshot {
  session: MetricSnapshot | null;
  weekly: MetricSnapshot | null;
  fetched_at: string;
}

/** Mirrors `usage::UsageLogEntry`. */
export interface UsageLogEntry {
  ok: boolean;
  detail: string;
  latency_ms: number;
  at: string;
}

/** Mirrors `state::MonitorSnapshot`. */
export interface MonitorSnapshot {
  phase: Phase;
  target_url: string;
  cron: string;
  cron_enabled: boolean;
  next_run_unix: number | null;
  seconds_until_next: number | null;
  last_report: HealthReport | null;
  needs_relogin: boolean;
  dashboard_visible: boolean;
}
