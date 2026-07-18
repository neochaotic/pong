<script lang="ts">
  import { toConfig, toForm, validateForm, type FormState } from "./configForm";
  import type { Config } from "./types";

  let {
    config,
    onSave,
    onClose,
  }: {
    config: Config;
    /** Resolves to a backend error message, or null when the save succeeded. */
    onSave: (config: Config) => Promise<string | null>;
    onClose: () => void;
  } = $props();

  // Snapshot the incoming config exactly once: from here the form owns an
  // independent draft. Tracking `config` reactively would wipe the user's edits
  // every time the backend pushed a new snapshot. The parent remounts this
  // component with fresh data each time the panel opens.
  // svelte-ignore state_referenced_locally
  let form = $state<FormState>(toForm(config));
  let errors = $state<string[]>([]);
  let saving = $state(false);

  async function save() {
    // Validate locally first so typos never cost a round trip.
    errors = validateForm(form);
    if (errors.length > 0) return;

    saving = true;
    const failure = await onSave(toConfig(form));
    saving = false;

    if (failure) errors = [failure];
    else onClose();
  }

  const field =
    "w-full rounded-md border border-line bg-ink-900 px-2 py-1.5 font-mono text-[11px] " +
    "text-chalk outline-none transition focus:border-signal";
  const label = "font-mono text-[9px] tracking-[0.14em] text-fog";
</script>

<div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-1">
  <div class="flex flex-col gap-1">
    <span class={label}>TARGET URL</span>
    <input class={field} data-testid="field-target_url" bind:value={form.target_url} spellcheck="false" />
  </div>

  <div class="flex flex-col gap-1">
    <span class={label}>CRON (6 FIELDS)</span>
    <input class={field} data-testid="field-cron" bind:value={form.cron} spellcheck="false" />
  </div>

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

  <label class="flex items-center gap-2">
    <input
      type="checkbox"
      data-testid="field-notifications" bind:checked={form.notifications_enabled}
      class="size-3 accent-signal"
    />
    <span class="font-mono text-[10px] text-fog">Notify on session expiry</span>
  </label>

  <label class="flex items-start gap-2">
    <input
      type="checkbox"
      data-testid="field-probe_only"
      bind:checked={form.probe_only}
      class="mt-0.5 size-3 accent-signal"
    />
    <span class="font-mono text-[10px] leading-snug text-fog">
      Probe only — check the session without clicking or typing
    </span>
  </label>

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
