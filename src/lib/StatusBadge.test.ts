import { render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import StatusBadge from "./StatusBadge.svelte";

describe("StatusBadge", () => {
  it("renders the ready state in the idle tone", () => {
    render(StatusBadge, { phase: "READY", verdict: null });

    const badge = screen.getByTestId("status-badge");
    expect(badge).toHaveTextContent("READY");
    expect(badge).toHaveAttribute("data-tone", "idle");
  });

  it("turns green once a check has succeeded", () => {
    render(StatusBadge, { phase: "READY", verdict: "healthy" });

    const badge = screen.getByTestId("status-badge");
    expect(badge).toHaveTextContent("READY");
    expect(badge).toHaveAttribute("data-tone", "ok");
  });

  it("shows PINGING while a check is in flight", () => {
    render(StatusBadge, { phase: "PINGING", verdict: "healthy" });

    const badge = screen.getByTestId("status-badge");
    expect(badge).toHaveTextContent("PINGING");
    expect(badge).toHaveAttribute("data-tone", "warn");
  });

  it("calls out an expired session distinctly from a generic error", () => {
    render(StatusBadge, { phase: "ERROR", verdict: "unauthorized" });

    const badge = screen.getByTestId("status-badge");
    expect(badge).toHaveTextContent("UNAUTHORIZED");
    expect(badge).toHaveAttribute("data-tone", "danger");
  });

  it("falls back to ERROR for unreachable dashboards", () => {
    render(StatusBadge, { phase: "ERROR", verdict: "unreachable" });

    expect(screen.getByTestId("status-badge")).toHaveTextContent("ERROR");
  });
});
