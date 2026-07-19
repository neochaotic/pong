<script lang="ts">
  import { formatCountdown } from "./format";
  import type { UsageLogEntry, UsageSnapshot } from "./types";

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

  function secondsUntil(iso: string | undefined): number | null {
    if (!iso) return null;
    const target = Math.floor(new Date(iso).getTime() / 1000);
    return Number.isFinite(target) ? Math.max(0, target - nowSec) : null;
  }

  const sessionRemaining = $derived(secondsUntil(usage?.session_reset_at));
  const weeklyRemaining = $derived(secondsUntil(usage?.weekly_reset_at));

  // Same red/yellow/green semantics as the rest of the app (StatusBadge),
  // not the brand accent — this is a health signal, not a decoration.
  function barTone(percent: number): string {
    if (percent >= 90) return "bg-danger";
    if (percent >= 70) return "bg-warn";
    return "bg-ok";
  }
</script>

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
      <div class="flex flex-col gap-1" data-testid="usage-session">
        <div class="flex items-center justify-between font-mono text-[10px] text-fog">
          <span>SESSION</span>
          <span>{usage.session_percent}% · resets in {formatCountdown(sessionRemaining)}</span>
        </div>
        <div class="h-1.5 w-full overflow-hidden rounded-full bg-ink-800">
          <div
            class="h-full rounded-full transition-all {barTone(usage.session_percent)}"
            style="width: {Math.min(100, usage.session_percent)}%"
          ></div>
        </div>
      </div>

      <div class="flex flex-col gap-1" data-testid="usage-weekly">
        <div class="flex items-center justify-between font-mono text-[10px] text-fog">
          <span>WEEKLY</span>
          <span>{usage.weekly_percent}% · resets in {formatCountdown(weeklyRemaining)}</span>
        </div>
        <div class="h-1.5 w-full overflow-hidden rounded-full bg-ink-800">
          <div
            class="h-full rounded-full transition-all {barTone(usage.weekly_percent)}"
            style="width: {Math.min(100, usage.weekly_percent)}%"
          ></div>
        </div>
      </div>
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
