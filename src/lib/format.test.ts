import { describe, expect, it } from "vitest";
import {
  badgeLabel,
  badgeTone,
  describeReport,
  formatCountdown,
  formatHumanDuration,
  formatLatency,
  shortenUrl,
} from "./format";
import type { HealthReport } from "./types";

const report = (over: Partial<HealthReport> = {}): HealthReport => ({
  code: 200,
  verdict: "healthy",
  detail: "dashboard responded",
  latency_ms: 812,
  at: "2026-07-18T10:15:00Z",
  ...over,
});

describe("formatCountdown", () => {
  it("pads minutes and seconds", () => {
    expect(formatCountdown(65)).toBe("01:05");
    expect(formatCountdown(9)).toBe("00:09");
  });

  it("adds an hours segment past 3600s", () => {
    expect(formatCountdown(3 * 3600 + 4 * 60 + 5)).toBe("3:04:05");
  });

  it("clamps negatives to zero", () => {
    expect(formatCountdown(-30)).toBe("00:00");
  });

  it("renders a placeholder when the schedule is unknown", () => {
    expect(formatCountdown(null)).toBe("--:--");
    expect(formatCountdown(Number.NaN)).toBe("--:--");
  });
});

describe("formatHumanDuration", () => {
  it("shows minutes only under an hour", () => {
    expect(formatHumanDuration(45 * 60)).toBe("45min");
    expect(formatHumanDuration(5 * 60 + 10)).toBe("5min");
  });

  it("shows hours, with minutes only when there's a remainder", () => {
    expect(formatHumanDuration(3 * 3600)).toBe("3h");
    expect(formatHumanDuration(3 * 3600 + 20 * 60)).toBe("3h 20min");
  });

  it("shows days, with hours only when there's a remainder", () => {
    expect(formatHumanDuration(2 * 86400)).toBe("2d");
    expect(formatHumanDuration(2 * 86400 + 5 * 3600)).toBe("2d 5h");
  });

  it("floors under a minute to a friendly placeholder", () => {
    expect(formatHumanDuration(30)).toBe("< 1 min");
    expect(formatHumanDuration(0)).toBe("< 1 min");
  });

  it("clamps negatives to zero", () => {
    expect(formatHumanDuration(-30)).toBe("< 1 min");
  });

  it("renders a placeholder when unknown", () => {
    expect(formatHumanDuration(null)).toBe("unknown");
    expect(formatHumanDuration(Number.NaN)).toBe("unknown");
  });
});

describe("badgeLabel", () => {
  it("prioritises the in-flight state", () => {
    expect(badgeLabel("PINGING", "healthy")).toBe("PINGING");
    expect(badgeLabel("PINGING", "unauthorized")).toBe("PINGING");
  });

  it("calls out an expired session explicitly", () => {
    expect(badgeLabel("ERROR", "unauthorized")).toBe("UNAUTHORIZED");
  });

  it("falls back to ERROR for other failures", () => {
    expect(badgeLabel("ERROR", "degraded")).toBe("ERROR");
    expect(badgeLabel("ERROR", "unreachable")).toBe("ERROR");
  });

  it("reads READY when idle", () => {
    expect(badgeLabel("READY", "healthy")).toBe("READY");
    expect(badgeLabel("READY", null)).toBe("READY");
  });
});

describe("badgeTone", () => {
  it("maps each state to its tone", () => {
    expect(badgeTone("PINGING", null)).toBe("warn");
    expect(badgeTone("ERROR", "unauthorized")).toBe("danger");
    expect(badgeTone("READY", "healthy")).toBe("ok");
    expect(badgeTone("READY", null)).toBe("idle");
  });
});

describe("formatLatency", () => {
  it("uses milliseconds below a second", () => {
    expect(formatLatency(812)).toBe("812ms");
  });

  it("switches to seconds above a second", () => {
    expect(formatLatency(1420)).toBe("1.4s");
  });

  it("guards against invalid input", () => {
    expect(formatLatency(-1)).toBe("--");
    expect(formatLatency(Number.NaN)).toBe("--");
  });
});

describe("describeReport", () => {
  it("summarises a report on one line", () => {
    expect(describeReport(report())).toBe("200 · dashboard responded · 812ms");
  });

  it("handles the empty history", () => {
    expect(describeReport(null)).toBe("No checks yet");
  });
});

describe("shortenUrl", () => {
  it("drops the scheme and trailing slash", () => {
    expect(shortenUrl("https://example.com/")).toBe("example.com");
  });

  it("truncates long URLs with an ellipsis", () => {
    const out = shortenUrl("https://very-long-dashboard.internal/deep/path/here", 20);
    expect(out).toHaveLength(20);
    expect(out.endsWith("…")).toBe(true);
  });

  it("leaves short URLs untouched", () => {
    expect(shortenUrl("http://localhost:3000")).toBe("localhost:3000");
  });
});
