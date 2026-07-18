<script lang="ts">
  import { onMount } from "svelte";
  import SettingsView from "./lib/SettingsView.svelte";
  import StatusBadge from "./lib/StatusBadge.svelte";
  import {
    closeRelogin,
    forceCheck,
    getConfig,
    getSnapshot,
    hidePopover,
    onSnapshot,
    openRelogin,
    quitApp,
    resizePopover,
    saveConfig,
  } from "./lib/api";
  import { describeReport, formatCountdown, shortenUrl } from "./lib/format";
  import type { Config, MonitorSnapshot } from "./lib/types";

  const MONITOR_HEIGHT = 260;
  const SETTINGS_HEIGHT = 470;

  let snapshot = $state<MonitorSnapshot | null>(null);
  let config = $state<Config | null>(null);
  let view = $state<"monitor" | "settings">("monitor");
  let nowSec = $state(Math.floor(Date.now() / 1000));
  let busy = $state(false);
  let reconnecting = $state(false);

  // The backend pushes an absolute timestamp; the countdown ticks locally so the
  // display stays smooth without a message every second.
  const remaining = $derived.by(() => {
    const next = snapshot?.next_run_unix;
    return next == null ? null : Math.max(0, next - nowSec);
  });

  const phase = $derived(snapshot?.phase ?? "READY");
  const verdict = $derived(snapshot?.last_report?.verdict ?? null);

  onMount(() => {
    getSnapshot()
      .then((s) => (snapshot = s))
      .catch(() => {});

    const unlisten = onSnapshot((s) => {
      snapshot = s;
      if (!s.needs_relogin) reconnecting = false;
    });
    const timer = setInterval(() => (nowSec = Math.floor(Date.now() / 1000)), 1000);

    return () => {
      clearInterval(timer);
      unlisten.then((stop) => stop()).catch(() => {});
    };
  });

  async function openSettings() {
    // Load fresh: the file may have been edited by hand since launch.
    config = await getConfig();
    view = "settings";
    await resizePopover(SETTINGS_HEIGHT);
  }

  async function closeSettings() {
    view = "monitor";
    await resizePopover(MONITOR_HEIGHT);
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

  async function runCheck() {
    busy = true;
    try {
      await forceCheck();
    } finally {
      busy = false;
    }
  }

  async function reconnect() {
    reconnecting = true;
    await openRelogin();
  }

  async function finishReconnect() {
    await closeRelogin();
    reconnecting = false;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    if (view === "settings") closeSettings();
    else hidePopover();
  }
</script>

<svelte:window on:keydown={onKeydown} />

<main
  class="flex h-screen w-screen flex-col justify-between overflow-hidden rounded-xl
         border border-line bg-ink-950 p-4 text-chalk"
>
  <header class="flex items-center justify-between pb-3">
    <h1 class="font-mono text-[11px] font-semibold tracking-[0.18em] text-fog">PONG</h1>
    {#if view === "monitor"}
      <div class="flex items-center gap-2">
        <StatusBadge {phase} {verdict} />
        <!-- 28px hit area around a 16px glyph: an emoji at 10px was both hard
             to see and hard to click. -->
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
      <span class="font-mono text-[10px] tracking-[0.14em] text-fog">SETTINGS</span>
    {/if}
  </header>

  {#if view === "settings" && config}
    <SettingsView {config} onSave={persist} onClose={closeSettings} />
  {:else if snapshot?.needs_relogin}
    <!-- Recovery path takes over the body: nothing else matters until it clears. -->
    <section class="flex flex-col gap-2">
      <p class="text-[13px] leading-snug text-chalk">Dashboard session expired.</p>
      <p class="text-[11px] leading-snug text-fog">
        {reconnecting
          ? "Log in on the dashboard window, then confirm below."
          : "Reopen the dashboard to sign in again."}
      </p>
      <button
        class="rounded-lg bg-signal px-3 py-2 text-[12px] font-medium text-chalk
               transition hover:brightness-110 active:brightness-95"
        onclick={reconnecting ? finishReconnect : reconnect}
      >
        {reconnecting ? "I'm signed in — resume monitoring" : "Reconnect dashboard"}
      </button>
    </section>

    <footer class="flex flex-col gap-2 border-t border-line pt-3">
      <div class="flex items-center justify-between">
        <span class="truncate font-mono text-[10px] text-fog">
          {snapshot ? shortenUrl(snapshot.target_url) : "—"}
        </span>
        <button class="font-mono text-[10px] text-fog transition hover:text-danger" onclick={quitApp}>
          QUIT
        </button>
      </div>
    </footer>
  {:else}
    <section class="flex flex-col gap-1">
      <span class="font-mono text-[10px] tracking-[0.16em] text-fog">NEXT CHECK</span>
      <span class="tabular font-mono text-[38px] leading-none font-light text-chalk">
        {formatCountdown(remaining)}
      </span>
    </section>

    <button
      class="rounded-lg border border-line bg-ink-800 px-3 py-2 text-[12px] font-medium
             text-chalk transition hover:bg-ink-700 disabled:opacity-50"
      onclick={runCheck}
      disabled={busy || phase === "PINGING"}
    >
      {busy || phase === "PINGING" ? "Checking…" : "Force Check"}
    </button>

    <footer class="flex flex-col gap-2 border-t border-line pt-3">
      <p class="truncate font-mono text-[10px] text-fog" title={snapshot?.last_report?.detail ?? ""}>
        {describeReport(snapshot?.last_report ?? null)}
      </p>
      <div class="flex items-center justify-between">
        <span class="truncate font-mono text-[10px] text-fog">
          {snapshot ? shortenUrl(snapshot.target_url) : "—"}
        </span>
        <button class="font-mono text-[10px] text-fog transition hover:text-danger" onclick={quitApp}>
          QUIT
        </button>
      </div>
    </footer>
  {/if}
</main>
