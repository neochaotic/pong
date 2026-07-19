<script lang="ts">
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import HistoryView from "./lib/HistoryView.svelte";
  import SettingsView from "./lib/SettingsView.svelte";
  import StatusBadge from "./lib/StatusBadge.svelte";
  import Toggle from "./lib/Toggle.svelte";
  import UsageView from "./lib/UsageView.svelte";
  import {
    closeRelogin,
    forceCheck,
    forceUsageCheck,
    getConfig,
    getHistory,
    getSnapshot,
    getUsage,
    getUsageHistory,
    hidePopover,
    onSnapshot,
    openRelogin,
    toggleDashboard,
    clearSession,
    saveConfig,
  } from "./lib/api";
  import { describeReport, formatCountdown, shortenUrl } from "./lib/format";
  import type {
    Config,
    HealthReport,
    MonitorSnapshot,
    UsageLogEntry,
    UsageSnapshot,
  } from "./lib/types";

  /** Re-fetch usage while the popover is on screen; skipped while hidden. */
  const USAGE_REFRESH_INTERVAL_MS = 5 * 60 * 1000;

  /** The dashboard is primary; the monitor's countdown is one tab over. */
  type Tab = "dash" | "monitor";

  let snapshot = $state<MonitorSnapshot | null>(null);
  let config = $state<Config | null>(null);
  let tab = $state<Tab>("dash");
  let view = $state<"main" | "settings" | "history">("main");
  /** Which tab's log the open History view is showing. */
  let historySource = $state<Tab>("dash");
  let history = $state<HealthReport[]>([]);
  let usage = $state<UsageSnapshot | null>(null);
  let usageHistory = $state<UsageLogEntry[]>([]);
  /** Newest usage-check attempt, shown on the dash even when it failed and
   * left `usage` stale — otherwise a failed refresh is invisible. */
  let lastUsageLog = $state<UsageLogEntry | null>(null);
  let usageRefreshing = $state(false);
  let nowSec = $state(Math.floor(Date.now() / 1000));
  let busy = $state(false);
  let reconnecting = $state(false);
  let confirmingReconnect = $state(false);
  /** Set when a heartbeat genuinely says "still not signed in" — phrased as
   * a status, not an error, since that's the expected state for as long as
   * someone is mid sign-in. A same-instant guard collision (another check
   * already running) is a wholly different, transient case and never
   * reaches here — see `finishReconnect`. */
  let reconnectStatus = $state<string | null>(null);

  // The backend pushes an absolute timestamp; the countdown ticks locally so the
  // display stays smooth without a message every second.
  const remaining = $derived.by(() => {
    const next = snapshot?.next_run_unix;
    return next == null ? null : Math.max(0, next - nowSec);
  });

  const phase = $derived(snapshot?.phase ?? "READY");
  const verdict = $derived(snapshot?.last_report?.verdict ?? null);

  async function refreshUsage() {
    usageRefreshing = true;
    try {
      await forceUsageCheck();
      usage = await getUsage();
      const log = await getUsageHistory();
      lastUsageLog = log[0] ?? null;
    } finally {
      usageRefreshing = false;
    }
  }

  onMount(() => {
    getSnapshot()
      .then((s) => (snapshot = s))
      .catch(() => {});
    getUsage()
      .then((u) => (usage = u))
      .catch(() => {});
    // Needed up front (not just when Settings opens) so the dash tab's
    // footer can show usage_url instead of the health check's target_url.
    getConfig()
      .then((c) => (config = c))
      .catch(() => {});

    const unlisten = onSnapshot((s) => {
      snapshot = s;
      if (!s.needs_relogin) {
        reconnecting = false;
        reconnectStatus = null;
      }
    });
    const timer = setInterval(() => (nowSec = Math.floor(Date.now() / 1000)), 1000);

    // The popover window is shown/hidden, not remounted, so onMount alone
    // cannot tell "just opened" from "opened an hour ago". Page visibility
    // does: a hidden native window reports document.hidden, so this doubles
    // as both "refresh on open" and "only tick every 5min while visible" —
    // a background popover has no screen to update anyway.
    let usageTimer: ReturnType<typeof setInterval> | null = null;
    function onVisibilityChange() {
      if (document.visibilityState !== "visible") {
        if (usageTimer) clearInterval(usageTimer);
        usageTimer = null;
        return;
      }
      refreshUsage();
      if (usageTimer) clearInterval(usageTimer);
      usageTimer = setInterval(refreshUsage, USAGE_REFRESH_INTERVAL_MS);
    }
    document.addEventListener("visibilitychange", onVisibilityChange);
    if (document.visibilityState === "visible") onVisibilityChange();

    return () => {
      clearInterval(timer);
      if (usageTimer) clearInterval(usageTimer);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlisten.then((stop) => stop()).catch(() => {});
    };
  });

  async function openSettings() {
    // Load fresh: the file may have been edited by hand since launch.
    config = await getConfig();
    view = "settings";
  }

  async function openHistory() {
    historySource = tab;
    history = tab === "monitor" ? await getHistory() : [];
    usageHistory = tab === "dash" ? await getUsageHistory() : [];
    view = "history";
  }

  function closeSettings() {
    view = "main";
  }

  async function persist(next: Config): Promise<string | null> {
    try {
      snapshot = await saveConfig(next);
      return null;
    } catch (error) {
      // The backend's validator is authoritative; surface its message verbatim.
      return String(error);
    }
  }

  async function wipeSession(): Promise<string | null> {
    try {
      snapshot = await clearSession();
      return null;
    } catch (error) {
      return String(error);
    }
  }

  async function toggleLoginWindow() {
    await toggleDashboard();
  }

  async function runCheck() {
    busy = true;
    try {
      await forceCheck();
    } finally {
      busy = false;
    }
  }

  /** Quick on/off from the monitor tab — the full cron expression still lives
   * in Settings, this only flips whether it runs. */
  async function toggleCronQuick(next: boolean) {
    const current = await getConfig();
    snapshot = await saveConfig({ ...current, cron_enabled: next });
  }

  async function reconnect() {
    reconnecting = true;
    reconnectStatus = null;
    await openRelogin();
  }

  /** A same-instant collision with a background check (its own guard is
   * shared with every scheduled check, and now with the automatic relogin
   * poll too) is common and meaningless to the user — it says nothing about
   * whether they're actually signed in. Retrying once, silently, turns
   * "here's a scary technical error" into what it actually is: bad timing. */
  function isTransientCollision(message: string): boolean {
    return message.includes("check is currently running");
  }

  async function finishReconnect(isRetry = false) {
    confirmingReconnect = true;
    if (!isRetry) reconnectStatus = null;
    try {
      await closeRelogin();
      reconnecting = false;
      reconnectStatus = null;
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      if (isTransientCollision(message) && !isRetry) {
        await new Promise((r) => setTimeout(r, 1200));
        confirmingReconnect = false;
        return finishReconnect(true);
      }
      // A genuine "still not signed in" — expected while someone is mid
      // sign-in, not a failure, so it reads as a status rather than an
      // error. The dashboard window stays open either way.
      reconnectStatus = isTransientCollision(message)
        ? "Still checking…"
        : "Not signed in yet.";
    } finally {
      confirmingReconnect = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    if (view === "main") hidePopover();
    else closeSettings();
  }

  /** Recasts a usage-scrape log into HistoryView's shape — same list UI,
   * different source. */
  function usageHistoryAsReports(entries: UsageLogEntry[]): HealthReport[] {
    return entries.map((entry) => ({
      code: entry.ok ? 200 : 503,
      verdict: entry.ok ? "healthy" : "degraded",
      detail: entry.detail,
      latency_ms: entry.latency_ms,
      at: entry.at,
    }));
  }
</script>

<svelte:window on:keydown={onKeydown} />

<main
  class="flex h-screen w-screen flex-col justify-between overflow-hidden rounded-xl
         border border-line bg-ink-950 p-4 text-chalk"
>
  <header class="flex items-center justify-between pb-3">
    {#if view === "main"}
      <div class="relative flex items-center rounded-lg bg-ink-900 p-0.5" role="tablist">
        <!-- Sliding thumb behind the label, not a background swap on click:
             the two tabs read as one control with a moving state, not two
             independent buttons. -->
        <div
          class="absolute inset-y-0.5 w-16 rounded-md bg-ink-700 transition-transform
                 duration-200 ease-out"
          style="transform: translateX({tab === 'dash' ? '0' : '4rem'})"
        ></div>
        <button
          role="tab"
          aria-selected={tab === "dash"}
          class="relative z-10 w-16 py-1 text-center font-mono text-[9px] tracking-[0.12em]
                 transition-colors {tab === 'dash' ? 'text-chalk' : 'text-fog hover:text-chalk'}"
          data-testid="tab-dash"
          onclick={() => (tab = "dash")}
        >
          DASH
        </button>
        <button
          role="tab"
          aria-selected={tab === "monitor"}
          class="relative z-10 w-16 py-1 text-center font-mono text-[9px] tracking-[0.12em]
                 transition-colors {tab === 'monitor' ? 'text-chalk' : 'text-fog hover:text-chalk'}"
          data-testid="tab-monitor"
          onclick={() => (tab = "monitor")}
        >
          PONG
        </button>
      </div>
      <div class="flex items-center gap-2">
        {#if tab === "monitor"}
          <StatusBadge {phase} {verdict} />
        {/if}
        <!-- 28px hit area around a 16px glyph: an emoji at 10px was both hard
             to see and hard to click. -->
        <button
          class="flex size-7 items-center justify-center rounded-md text-fog
                 transition hover:bg-ink-800 hover:text-chalk"
          onclick={openHistory}
          aria-label="History"
          title="Check history"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M3 3v5h5" />
            <path d="M3.05 13A9 9 0 1 0 6 5.3L3 8" />
            <path d="M12 7v5l4 2" />
          </svg>
        </button>
        <button
          class="flex size-7 items-center justify-center rounded-md text-fog
                 transition hover:bg-ink-800 hover:text-chalk"
          onclick={openSettings}
          aria-label="Settings"
          title="Settings"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <circle cx="12" cy="12" r="3" />
            <path
              d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
            />
          </svg>
        </button>
      </div>
    {:else}
      <h1 class="font-mono text-[11px] font-semibold tracking-[0.18em] text-fog">PONG</h1>
      <span class="font-mono text-[10px] tracking-[0.14em] text-fog">
        {view === "history" ? (historySource === "dash" ? "USAGE HISTORY" : "HISTORY") : "SETTINGS"}
      </span>
    {/if}
  </header>

  {#if view === "history"}
    <div class="flex min-h-0 flex-1 flex-col" in:fly={{ x: 24, duration: 180 }}>
      <HistoryView
        history={historySource === "dash" ? usageHistoryAsReports(usageHistory) : history}
        onClose={closeSettings}
      />
    </div>
  {:else if view === "settings" && config}
    <div class="flex min-h-0 flex-1 flex-col" in:fly={{ x: 24, duration: 180 }}>
      <SettingsView {config} onSave={persist} onClose={closeSettings} onClearSession={wipeSession} />
    </div>
  {:else if snapshot?.needs_relogin}
    <!-- Recovery path takes over the body: nothing else matters until it clears. -->
    <section class="flex flex-col gap-2">
      <p class="text-[13px] leading-snug text-chalk">Dashboard session expired.</p>
      <p class="text-[11px] leading-snug text-fog">
        {reconnecting
          ? "Log in on the dashboard window — Pong checks automatically every few seconds."
          : "Reopen the dashboard to sign in again."}
      </p>
      {#if reconnecting && !reconnectStatus}
        <p class="flex items-center gap-1.5 text-[10px] leading-snug text-fog">
          <span class="size-1.5 animate-pulse rounded-full bg-signal"></span>
          Watching for sign-in…
        </p>
      {/if}
      {#if reconnectStatus}
        <p class="text-[11px] leading-snug text-fog" data-testid="reconnect-status">
          {reconnectStatus}
        </p>
      {/if}
      <button
        class="rounded-lg bg-signal px-3 py-2 text-[12px] font-medium text-chalk
               transition hover:brightness-110 active:brightness-95 disabled:opacity-50"
        onclick={() => (reconnecting ? finishReconnect() : reconnect())}
        disabled={confirmingReconnect}
      >
        {confirmingReconnect
          ? "Checking…"
          : reconnecting
            ? "Check now"
            : "Reconnect dashboard"}
      </button>
    </section>

    <footer class="flex flex-col gap-2 border-t border-line pt-3">
      <div class="flex items-center justify-between">
        <span class="truncate font-mono text-[10px] text-fog">
          {snapshot ? shortenUrl(snapshot.target_url) : "—"}
        </span>
        <button class="font-mono text-[10px] text-fog transition hover:text-chalk" onclick={hidePopover}>
          CLOSE
        </button>
      </div>
    </footer>
  {:else}
    <!-- Both tabs occupy the exact same box (absolute + relative) so the
         crossfade overlays them instead of stacking two panels in flow,
         which read as a jump rather than a smooth cut. -->
    <div class="relative min-h-0 flex-1">
      {#if tab === "dash"}
        <div
          class="absolute inset-0 flex flex-col justify-between"
          transition:fade={{ duration: 180 }}
        >
          <UsageView
            {usage}
            {lastUsageLog}
            configured={config?.usage_url != null}
            refreshing={usageRefreshing}
            dashboardVisible={snapshot?.dashboard_visible ?? false}
            onRefresh={refreshUsage}
            onToggleLogin={toggleLoginWindow}
          />

          <footer class="flex flex-col gap-2 border-t border-line pt-3">
            <div class="flex items-center justify-between">
              <span class="truncate font-mono text-[10px] text-fog">
                {config?.usage_url ? shortenUrl(config.usage_url) : "—"}
              </span>
              <button
                class="font-mono text-[10px] text-fog transition hover:text-chalk"
                onclick={hidePopover}
              >
                CLOSE
              </button>
            </div>
          </footer>
        </div>
      {:else}
        <div
          class="absolute inset-0 flex flex-col justify-between"
          transition:fade={{ duration: 180 }}
        >
          <section class="flex flex-col gap-1">
            <div class="flex items-center justify-between">
              <span class="font-mono text-[10px] tracking-[0.16em] text-fog">
                {snapshot?.cron_enabled ? "NEXT CHECK" : "SCHEDULE"}
              </span>
              <Toggle
                testid="quick-cron-toggle"
                checked={snapshot?.cron_enabled ?? false}
                onChange={toggleCronQuick}
                label="Run on schedule"
                showLabel={false}
              />
            </div>
            {#if snapshot?.cron_enabled}
              <span class="tabular font-mono text-[38px] leading-none font-light text-chalk">
                {formatCountdown(remaining)}
              </span>
            {:else}
              <span class="font-mono text-[13px] leading-snug text-fog">
                Disabled — flip the switch above, or use Ping Now below.
              </span>
            {/if}
          </section>

          <button
            class="rounded-lg border border-line bg-ink-800 px-3 py-2 text-[12px]
                   font-medium text-chalk transition hover:bg-ink-700 disabled:opacity-50"
            onclick={runCheck}
            disabled={busy || phase === "PINGING"}
          >
            {busy || phase === "PINGING" ? "Checking…" : "Ping Now"}
          </button>

          <footer class="flex flex-col gap-2 border-t border-line pt-3">
            <p
              class="truncate font-mono text-[10px] text-fog"
              title={snapshot?.last_report?.detail ?? ""}
            >
              {describeReport(snapshot?.last_report ?? null)}
            </p>
            <div class="flex items-center justify-between">
              <span class="truncate font-mono text-[10px] text-fog">
                {snapshot ? shortenUrl(snapshot.target_url) : "—"}
              </span>
              <button
                class="font-mono text-[10px] text-fog transition hover:text-chalk"
                onclick={hidePopover}
              >
                CLOSE
              </button>
            </div>
          </footer>
        </div>
      {/if}
    </div>
  {/if}
</main>
