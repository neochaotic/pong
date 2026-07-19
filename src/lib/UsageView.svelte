<script lang="ts">
  import { formatHumanDuration } from "./format";
  import { barColor, isCritical } from "./usageColor";
  import type { MetricSnapshot, UsageLogEntry, UsageSnapshot } from "./types";

  let {
    usage,
    lastUsageLog,
    configured,
    refreshing,
    dashboardVisible,
    onRefresh,
    onToggleLogin,
  }: {
    usage: UsageSnapshot | null;
    /** Newest usage-check attempt, win or lose — surfaces a failed refresh
     * even when `usage` itself is still whatever the last success left it. */
    lastUsageLog: UsageLogEntry | null;
    /** Whether `usage_url` is set at all — distinct from "set, but no
     * successful check yet". */
    configured: boolean;
    refreshing: boolean;
    dashboardVisible: boolean;
    onRefresh: () => void;
    onToggleLogin: () => void;
  } = $props();

  // Countdowns tick locally off the last-fetched reset timestamp, same as the
  // monitor's "next check" clock — estimated, not re-fetched every second.
  let nowSec = $state(Math.floor(Date.now() / 1000));

  $effect(() => {
    const timer = setInterval(() => (nowSec = Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(timer);
  });

  function secondsUntil(iso: string | null | undefined): number | null {
    if (!iso) return null;
    const target = Math.floor(new Date(iso).getTime() / 1000);
    return Number.isFinite(target) ? Math.max(0, target - nowSec) : null;
  }
</script>

{#snippet metricRow(label: string, metric: MetricSnapshot | null, testid: string)}
  <div class="flex flex-col gap-1" data-testid={testid}>
    <div class="flex items-center justify-between font-mono text-[10px] text-fog">
      <span>{label}</span>
      {#if !metric}
        <span data-testid="{testid}-unavailable">unavailable</span>
      {:else if metric.reset_at}
        <span>{metric.percent}% · resets in {formatHumanDuration(secondsUntil(metric.reset_at))}</span>
      {:else}
        <span title={metric.reset_note ?? undefined} data-testid="{testid}-reset-unknown">
          {metric.percent}% · reset time unknown
        </span>
      {/if}
    </div>
    {#if metric}
      <div
        class="h-1.5 w-full overflow-hidden rounded-full bg-ink-800 transition-shadow
               {isCritical(metric.percent)
          ? 'animate-pulse ring-2 ring-danger ring-offset-1 ring-offset-ink-950'
          : ''}"
      >
        <div
          class="h-full rounded-full transition-all"
          style="width: {Math.min(100, metric.percent)}%; background-color: {barColor(
            metric.percent
          )}"
        ></div>
      </div>
    {/if}
  </div>
{/snippet}

<div class="flex min-h-0 flex-1 flex-col gap-4" data-testid="usage-view">
  <div class="flex items-center justify-between">
    <span class="font-mono text-[10px] tracking-[0.16em] text-fog">USAGE</span>
    <button
      class="flex size-6 items-center justify-center rounded-md text-fog transition
             hover:bg-ink-800 hover:text-chalk disabled:opacity-50"
      data-testid="usage-refresh"
      onclick={onRefresh}
      disabled={refreshing}
      aria-label="Refresh usage"
      title="Refresh usage"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="13"
        height="13"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
        class={refreshing ? "animate-spin" : ""}
      >
        <path d="M21 12a9 9 0 1 1-2.64-6.36" />
        <path d="M21 3v6h-6" />
      </svg>
    </button>
  </div>

  {#if !configured}
    <p class="py-6 text-center font-mono text-[10px] text-fog" data-testid="usage-unconfigured">
      Set a usage page URL in Settings to enable this.
    </p>
  {:else if !usage}
    <p class="py-6 text-center font-mono text-[10px] text-fog" data-testid="usage-empty">
      {refreshing
        ? "Fetching usage…"
        : lastUsageLog && !lastUsageLog.ok
          ? `Last check failed: ${lastUsageLog.detail}`
          : "No usage data yet."}
    </p>
  {:else}
    <div class="flex flex-col gap-3">
      {#if lastUsageLog && !lastUsageLog.ok}
        <p
          class="font-mono text-[9px] leading-snug text-warn"
          data-testid="usage-last-failure"
          title={lastUsageLog.detail}
        >
          Last check failed: {lastUsageLog.detail} — showing the last known numbers.
        </p>
      {/if}
      {@render metricRow("SESSION", usage.session, "usage-session")}
      {@render metricRow("WEEKLY", usage.weekly, "usage-weekly")}
    </div>
  {/if}
</div>

<button
  class="rounded-lg border border-line bg-ink-800 px-3 py-2 text-[12px]
         text-fog transition hover:bg-ink-700 hover:text-chalk"
  data-testid="toggle-login"
  onclick={onToggleLogin}
  title="Show or hide the dashboard window to sign in"
>
  {dashboardVisible ? "Hide login" : "Show login"}
</button>
