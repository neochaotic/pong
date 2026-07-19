<script lang="ts">
  let {
    checked,
    onChange,
    label,
    hint,
    testid,
    showLabel = true,
  }: {
    checked: boolean;
    onChange: (checked: boolean) => void;
    /** Always used as the accessible name, even when showLabel is false. */
    label: string;
    hint?: string;
    testid?: string;
    /** Set false for a bare switch (e.g. inline next to its own heading) —
     * `label` still becomes the aria-label. */
    showLabel?: boolean;
  } = $props();
</script>

<div class="flex items-start justify-between gap-3">
  {#if showLabel}
    <span class="flex flex-col gap-0.5">
      <span class="font-mono text-[10px] text-chalk">{label}</span>
      {#if hint}
        <span class="font-mono text-[9px] leading-snug text-fog">{hint}</span>
      {/if}
    </span>
  {/if}

  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    data-testid={testid}
    class="relative mt-0.5 h-4 w-7 shrink-0 rounded-full transition-colors
           {checked ? 'bg-signal' : 'bg-ink-700'}"
    onclick={() => onChange(!checked)}
  >
    <!-- Anchored at left-0.5 (not the ambiguous "static position" transform
         alone would use); translateX then moves a known, fixed distance. -->
    <span
      class="absolute left-0.5 top-0.5 size-3 rounded-full bg-chalk transition-transform
             duration-150 ease-out"
      style="transform: translateX({checked ? '12px' : '0px'})"
    ></span>
  </button>
</div>
