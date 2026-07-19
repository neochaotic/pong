<script lang="ts">
  import {
    DEFAULT_CRON,
    defaultForm,
    expandFiveFieldCron,
    toConfig,
    toForm,
    validateForm,
    type FormState,
  } from "./configForm";
  import Toggle from "./Toggle.svelte";
  import type { Config } from "./types";

  let {
    config,
    onSave,
    onClose,
    onClearSession,
  }: {
    config: Config;
    /** Resolves to a backend error message, or null when the save succeeded. */
    onSave: (config: Config) => Promise<string | null>;
    onClose: () => void;
    /** Resolves to a backend error message, or null when the wipe succeeded. */
    onClearSession: () => Promise<string | null>;
  } = $props();

  // Snapshot the incoming config exactly once: from here the form owns an
  // independent draft. Tracking `config` reactively would wipe the user's edits
  // every time the backend pushed a new snapshot. The parent remounts this
  // component with fresh data each time the panel opens.
  // svelte-ignore state_referenced_locally
  let form = $state<FormState>(toForm(config));
  let errors = $state<string[]>([]);
  let saving = $state(false);
  // Two-step: wiping the session signs the user out of the dashboard, and
  // there is no undo.
  let confirmingWipe = $state(false);
  let wiping = $state(false);
  // Two-step for the same reason: overwrites every field in the form with
  // no undo. Unlike wiping the session, this never touches the backend by
  // itself — it only fills the form. Nothing is persisted until Save.
  let confirmingRestore = $state(false);

  async function clearSession() {
    if (!confirmingWipe) {
      confirmingWipe = true;
      return;
    }
    wiping = true;
    const failure = await onClearSession();
    wiping = false;
    confirmingWipe = false;
    if (failure) errors = [failure];
  }

  function restoreDefaults() {
    if (!confirmingRestore) {
      confirmingRestore = true;
      return;
    }
    form = defaultForm();
    errors = [];
    confirmingRestore = false;
  }

  /** A cron-shaped complaint from either validator, lower-cased for matching. */
  function hasCronError(messages: string[]): boolean {
    return messages.some((m) => m.toLowerCase().includes("cron"));
  }

  async function save() {
    // A classic 5-field cron (crontab.guru style, no seconds) is valid intent,
    // not a typo — complete it with seconds=0 before validating.
    const expanded = expandFiveFieldCron(form.cron);
    if (expanded) form.cron = expanded;

    // Validate locally first so typos never cost a round trip.
    errors = validateForm(form);
    if (errors.length > 0) {
      // A cron the parser can't schedule is worse than useless left in the
      // box — reset it to a known-good default so the very next click on
      // Save succeeds, instead of the user re-typing it by hand.
      if (hasCronError(errors)) form.cron = DEFAULT_CRON;
      return;
    }

    saving = true;
    const failure = await onSave(toConfig(form));
    saving = false;

    if (failure) {
      errors = [failure];
      if (hasCronError(errors)) form.cron = DEFAULT_CRON;
    } else {
      onClose();
    }
  }

  const field =
    "w-full rounded-md border border-line bg-ink-900 px-2 py-1.5 font-mono text-[11px] " +
    "text-chalk outline-none transition focus:border-signal";
  const label = "font-mono text-[9px] tracking-[0.14em] text-fog";
  /** Group header — bolder than a field label, so the eye can skip to a
   * section instead of reading every field name in a 20-field list. */
  const sectionTitle = "font-mono text-[9px] font-bold tracking-[0.16em] text-chalk";
</script>

<div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-1">
  <span class={sectionTitle}>TARGET &amp; SCHEDULE</span>

  <div class="flex flex-col gap-1">
    <span class={label}>TARGET URL</span>
    <input class={field} data-testid="field-target_url" bind:value={form.target_url} spellcheck="false" />
  </div>

  <div class="flex flex-col gap-2">
    <Toggle
      testid="field-cron_enabled"
      checked={form.cron_enabled}
      onChange={(v) => (form.cron_enabled = v)}
      label="RUN ON SCHEDULE"
      hint="Off by default. Nothing runs until you turn this on."
    />
    <div class="flex flex-col gap-1">
      <span class={label}>CRON (6 FIELDS)</span>
      <input class={field} data-testid="field-cron" bind:value={form.cron} spellcheck="false" />
      <span class="font-mono text-[9px] leading-snug text-fog">
        Default: 5am, Monday–Friday. An invalid cron resets to this on save.
      </span>
    </div>
  </div>

  <span class="{sectionTitle} border-t border-line pt-3">SELECTORS</span>

  <div class="flex flex-col gap-1">
    <span class={label}>AUTHENTICATED SELECTOR</span>
    <input class={field} data-testid="field-authenticated" bind:value={form.authenticated} spellcheck="false" />
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>LOGIN SELECTOR</span>
    <input class={field} data-testid="field-login_indicator" bind:value={form.login_indicator} spellcheck="false" />
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>TEXT INPUT SELECTOR</span>
    <input class={field} data-testid="field-text_input" bind:value={form.text_input} spellcheck="false" />
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>ACTION BUTTON (OPTIONAL)</span>
    <input class={field} data-testid="field-action_button" bind:value={form.action_button} spellcheck="false" />
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>SUBMIT BUTTON (OPTIONAL)</span>
    <input class={field} data-testid="field-submit_button" bind:value={form.submit_button} spellcheck="false" />
    <span class="font-mono text-[9px] leading-snug text-fog">
      Waits for it to become enabled, then clicks. Leave empty to press Enter.
    </span>
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>RESPONSE SELECTOR (OPTIONAL)</span>
    <input class={field} data-testid="field-response" bind:value={form.response} spellcheck="false" />
    <span class="font-mono text-[9px] leading-snug text-fog">
      Matches each reply bubble. The last one's text becomes the check's detail once it
      stops changing. Leave empty to just report "dashboard responded".
    </span>
  </div>

  <div class="flex flex-col gap-1 border-t border-line pt-3">
    <span class={sectionTitle}>CLEANUP (OPTIONAL)</span>
    <span class="font-mono text-[9px] leading-snug text-fog">
      Deletes what the check just created, run only after a successful interaction. Each
      step runs only if set — leave a step empty to skip it.
    </span>
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>CLEANUP: MENU BUTTON</span>
    <input
      class={field}
      data-testid="field-cleanup_menu_button"
      bind:value={form.cleanup_menu_button}
      spellcheck="false"
    />
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>CLEANUP: DELETE OPTION</span>
    <input
      class={field}
      data-testid="field-cleanup_delete_option"
      bind:value={form.cleanup_delete_option}
      spellcheck="false"
    />
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>CLEANUP: CONFIRM BUTTON</span>
    <input
      class={field}
      data-testid="field-cleanup_confirm_button"
      bind:value={form.cleanup_confirm_button}
      spellcheck="false"
    />
  </div>

  <div class="flex flex-col gap-1 border-t border-line pt-3">
    <span class={sectionTitle}>USAGE DASHBOARD (OPTIONAL)</span>
    <span class={label}>USAGE PAGE URL</span>
    <input class={field} data-testid="field-usage_url" bind:value={form.usage_url} spellcheck="false" />
    <span class="font-mono text-[9px] leading-snug text-fog">
      claude.ai's usage-limits page, e.g. https://claude.ai/settings/usage. Leave empty to
      hide the dash's usage numbers.
    </span>
  </div>

  <span class="{sectionTitle} border-t border-line pt-3">MESSAGE &amp; TIMING</span>

  <div class="flex flex-col gap-1">
    <span class={label}>PAYLOAD</span>
    <input class={field} data-testid="field-payload" bind:value={form.payload} spellcheck="false" />
  </div>

  <div class="flex gap-2">
    <div class="flex flex-1 flex-col gap-1">
      <span class={label}>SETTLE (MS)</span>
      <input class={field} data-testid="field-settle_ms" bind:value={form.settle_ms} inputmode="numeric" />
    </div>
    <div class="flex flex-1 flex-col gap-1">
      <span class={label}>TYPING (MS)</span>
      <input class={field} data-testid="field-typing_delay_ms" bind:value={form.typing_delay_ms} inputmode="numeric" />
    </div>
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>ELEMENT TIMEOUT (MS)</span>
    <input class={field} data-testid="field-element_timeout_ms" bind:value={form.element_timeout_ms} inputmode="numeric" />
    <span class="font-mono text-[9px] leading-snug text-fog">
      How long to wait for a single-page app to mount an element.
    </span>
  </div>

  <span class="{sectionTitle} border-t border-line pt-3">PREFERENCES</span>

  <Toggle
    testid="field-autostart_enabled"
    checked={form.autostart_enabled}
    onChange={(v) => (form.autostart_enabled = v)}
    label="LAUNCH AT LOGIN"
    hint="A tray monitor that doesn't come back on its own isn't much of a monitor."
  />

  <Toggle
    testid="field-notifications"
    checked={form.notifications_enabled}
    onChange={(v) => (form.notifications_enabled = v)}
    label="NOTIFY ON SESSION EXPIRY"
  />

  <Toggle
    testid="field-probe_only"
    checked={form.probe_only}
    onChange={(v) => (form.probe_only = v)}
    label="PROBE ONLY"
    hint="Check the session without clicking or typing."
  />

  <div class="flex flex-col gap-1 border-t border-line pt-3">
    <span class={sectionTitle}>CONFIGURATION</span>
    <button
      data-testid="restore-defaults"
      class="rounded-md border px-2 py-1.5 text-[11px] transition
             {confirmingRestore
        ? 'border-danger bg-danger/10 text-danger'
        : 'border-line bg-ink-900 text-fog hover:text-chalk'}"
      onclick={restoreDefaults}
    >
      {confirmingRestore ? "Confirm — discards every field below" : "Restore defaults"}
    </button>
    {#if confirmingRestore}
      <button
        class="self-start font-mono text-[9px] text-fog underline"
        onclick={() => (confirmingRestore = false)}
      >
        cancel
      </button>
    {/if}
    <span class="font-mono text-[9px] leading-snug text-fog">
      Fills the form with defaults — nothing is saved until you click Save.
    </span>
  </div>

  <div class="flex flex-col gap-1 border-t border-line pt-3">
    <span class={sectionTitle}>SESSION</span>
    <button
      data-testid="clear-session"
      class="rounded-md border px-2 py-1.5 text-[11px] transition
             {confirmingWipe
        ? 'border-danger bg-danger/10 text-danger'
        : 'border-line bg-ink-900 text-fog hover:text-chalk'}"
      onclick={clearSession}
      disabled={wiping}
    >
      {wiping
        ? "Clearing…"
        : confirmingWipe
          ? "Confirm — this signs you out"
          : "Clear session data"}
    </button>
    {#if confirmingWipe && !wiping}
      <button
        class="self-start font-mono text-[9px] text-fog underline"
        onclick={() => (confirmingWipe = false)}
      >
        cancel
      </button>
    {/if}
  </div>

  <p class="text-center font-mono text-[9px] text-fog/70" data-testid="app-version">
    Pong v{__APP_VERSION__}
  </p>

  {#if errors.length > 0}
    <ul class="flex flex-col gap-0.5" data-testid="form-errors">
      {#each errors as error}
        <li class="font-mono text-[10px] leading-snug text-danger">{error}</li>
      {/each}
    </ul>
  {/if}
</div>

<div class="flex gap-2 border-t border-line pt-3">
  <button
    class="flex-1 rounded-lg border border-line bg-ink-800 px-3 py-2 text-[12px]
           text-chalk transition hover:bg-ink-700"
    onclick={onClose}
  >
    Cancel
  </button>
  <button
    class="flex-1 rounded-lg bg-signal px-3 py-2 text-[12px] font-medium text-chalk
           transition hover:brightness-110 disabled:opacity-50"
    onclick={save}
    disabled={saving}
  >
    {saving ? "Saving…" : "Save"}
  </button>
</div>
