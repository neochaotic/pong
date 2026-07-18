<script lang="ts">
  import { formatLatency } from "./format";
  import type { HealthReport } from "./types";

  let { history, onClose }: { history: HealthReport[]; onClose: () => void } = $props();

  // Full class strings so Tailwind's scanner can see them.
  const dotClass: Record<string, string> = {
    healthy: "bg-ok",
    unauthorized: "bg-danger",
    degraded: "bg-warn",
    unreachable: "bg-fog",
  };

  /** Clock time is what matters when scanning a list of checks. */
  function timeOf(iso: string): string {
    const date = new Date(iso);
    return Number.isNaN(date.getTime())
      ? "--:--"
      : date.toLocaleTimeString(undefined, {
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
          hour12: false,
        });
  }

  /** Group by day so a long history stays readable. */
  function dayOf(iso: string): string {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return "";
    const today = new Date();
    const isToday = date.toDateString() === today.toDateString();
    return isToday
      ? "TODAY"
      : date.toLocaleDateString(undefined, { day: "2-digit", month: "short" }).toUpperCase();
  }

  const rows = $derived(
    history.map((report, i) => ({
      report,
      // Only label the first entry of each day.
      day: i === 0 || dayOf(report.at) !== dayOf(history[i - 1].at) ? dayOf(report.at) : null,
    }))
  );

  const healthy = $derived(history.filter((r) => r.verdict === "healthy").length);
</script>

<div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto pr-1">
  {#if history.length === 0}
    <p class="py-6 text-center font-mono text-[10px] text-fog">
      No checks recorded yet.
    </p>
  {:else}
    <p class="font-mono text-[9px] tracking-[0.14em] text-fog">
      {healthy}/{history.length} HEALTHY
    </p>

    {#each rows as { report, day } (report.at + report.code)}
      {#if day}
        <span class="mt-1 font-mono text-[9px] tracking-[0.14em] text-fog">{day}</span>
      {/if}
      <div
        data-testid="history-row"
        class="flex items-center gap-2 rounded-md border border-line bg-ink-900 px-2 py-1.5"
      >
        <span class="size-1.5 shrink-0 rounded-full {dotClass[report.verdict] ?? 'bg-fog'}"></span>
        <span class="tabular shrink-0 font-mono text-[10px] text-fog">{timeOf(report.at)}</span>
        <span class="tabular shrink-0 font-mono text-[10px] text-chalk">{report.code}</span>
        <span class="truncate font-mono text-[10px] text-fog" title={report.detail}>
          {report.detail}
        </span>
        <span class="tabular ml-auto shrink-0 font-mono text-[10px] text-fog">
          {formatLatency(report.latency_ms)}
        </span>
      </div>
    {/each}
  {/if}
</div>

<div class="border-t border-line pt-3">
  <button
    class="w-full rounded-lg border border-line bg-ink-800 px-3 py-2 text-[12px]
           text-chalk transition hover:bg-ink-700"
    onclick={onClose}
  >
    Back
  </button>
</div>
