import type { HealthReport, Phase, Verdict } from "./types";

/** Visual tone used to colour badges and the status dot. */
export type Tone = "ok" | "warn" | "danger" | "idle";

/**
 * Format a countdown as `mm:ss`, or `h:mm:ss` past the hour.
 * Negative and non-finite inputs clamp to zero so the UI never shows garbage.
 */
export function formatCountdown(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return "--:--";

  const total = Math.max(0, Math.floor(seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");

  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

/**
 * Human-friendly duration for something hours or days out — "3h 20min",
 * "2d 5h", "45min" — rounded to the coarsest two units that matter. Unlike
 * `formatCountdown` (a ticking clock built for short, frequent countdowns
 * like the next health check), a usage-limit reset can be a week away, and a
 * second-precision `HH:MM:SS` reads as noise at that distance.
 */
export function formatHumanDuration(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return "unknown";

  const total = Math.max(0, Math.floor(seconds));
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);

  if (days > 0) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (hours > 0) return minutes > 0 ? `${hours}h ${minutes}min` : `${hours}h`;
  if (minutes > 0) return `${minutes}min`;
  return "< 1 min";
}

/** The badge label shown next to the status dot. */
export function badgeLabel(phase: Phase, verdict: Verdict | null): string {
  if (phase === "PINGING") return "PINGING";
  if (verdict === "unauthorized") return "UNAUTHORIZED";
  if (phase === "ERROR") return "ERROR";
  return "READY";
}

/** Colour tone derived from the current phase and last verdict. */
export function badgeTone(phase: Phase, verdict: Verdict | null): Tone {
  if (phase === "PINGING") return "warn";
  if (phase === "ERROR") return "danger";
  return verdict === "healthy" ? "ok" : "idle";
}

/** Compact latency readout, e.g. `812ms` or `1.4s`. */
export function formatLatency(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "--";
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`;
}

/** One-line summary of the last check for the popover footer. */
export function describeReport(report: HealthReport | null): string {
  if (!report) return "No checks yet";
  return `${report.code} · ${report.detail} · ${formatLatency(report.latency_ms)}`;
}

/** Strip the scheme and trailing slash so a URL fits the 320px popover. */
export function shortenUrl(url: string, max = 34): string {
  const trimmed = url.replace(/^https?:\/\//, "").replace(/\/$/, "");
  return trimmed.length <= max ? trimmed : `${trimmed.slice(0, max - 1)}…`;
}
