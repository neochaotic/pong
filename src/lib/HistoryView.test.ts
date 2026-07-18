import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import HistoryView from "./HistoryView.svelte";
import type { HealthReport } from "./types";

const report = (over: Partial<HealthReport> = {}): HealthReport => ({
  code: 200,
  verdict: "healthy",
  detail: "dashboard responded",
  latency_ms: 812,
  at: new Date().toISOString(),
  ...over,
});

describe("HistoryView", () => {
  it("says so plainly when nothing has run yet", () => {
    render(HistoryView, { history: [], onClose: vi.fn() });

    expect(screen.getByText("No checks recorded yet.")).toBeInTheDocument();
    expect(screen.queryAllByTestId("history-row")).toHaveLength(0);
  });

  it("lists one row per check", () => {
    render(HistoryView, {
      history: [report(), report({ code: 401, verdict: "unauthorized" })],
      onClose: vi.fn(),
    });

    expect(screen.getAllByTestId("history-row")).toHaveLength(2);
  });

  it("summarises how many checks were healthy", () => {
    render(HistoryView, {
      history: [
        report(),
        report({ code: 401, verdict: "unauthorized" }),
        report({ code: 503, verdict: "degraded" }),
      ],
      onClose: vi.fn(),
    });

    expect(screen.getByText("1/3 HEALTHY")).toBeInTheDocument();
  });

  it("shows the code, detail and latency of a check", () => {
    render(HistoryView, {
      history: [report({ code: 503, detail: "no marker", latency_ms: 1500 })],
      onClose: vi.fn(),
    });

    const row = screen.getByTestId("history-row");
    expect(row).toHaveTextContent("503");
    expect(row).toHaveTextContent("no marker");
    expect(row).toHaveTextContent("1.5s");
  });

  it("survives a malformed timestamp instead of rendering NaN", () => {
    render(HistoryView, { history: [report({ at: "not-a-date" })], onClose: vi.fn() });

    expect(screen.getByTestId("history-row")).toHaveTextContent("--:--");
  });

  it("closes when Back is pressed", async () => {
    const onClose = vi.fn();
    render(HistoryView, { history: [report()], onClose });

    await userEvent.setup().click(screen.getByRole("button", { name: "Back" }));

    expect(onClose).toHaveBeenCalledOnce();
  });
});
