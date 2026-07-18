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
}

/** Mirrors `config::Config`. */
export interface Config {
  target_url: string;
  cron: string;
  selectors: Selectors;
  payload: string;
  settle_ms: number;
  typing_delay_ms: number;
  notifications_enabled: boolean;
}

/** Mirrors `state::MonitorSnapshot`. */
export interface MonitorSnapshot {
  phase: Phase;
  target_url: string;
  cron: string;
  next_run_unix: number | null;
  seconds_until_next: number | null;
  last_report: HealthReport | null;
  needs_relogin: boolean;
}
