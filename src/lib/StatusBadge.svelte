<script lang="ts">
  import { badgeLabel, badgeTone } from "./format";
  import type { Phase, Verdict } from "./types";

  let { phase, verdict = null }: { phase: Phase; verdict?: Verdict | null } = $props();

  const label = $derived(badgeLabel(phase, verdict));
  const tone = $derived(badgeTone(phase, verdict));

  // Full class strings, so Tailwind's scanner can see them.
  const dotClass: Record<string, string> = {
    ok: "bg-ok",
    warn: "bg-warn",
    danger: "bg-danger",
    idle: "bg-fog",
  };
  const textClass: Record<string, string> = {
    ok: "text-ok",
    warn: "text-warn",
    danger: "text-danger",
    idle: "text-fog",
  };
</script>

<span
  data-testid="status-badge"
  data-tone={tone}
  class="inline-flex items-center gap-1.5 rounded-full border border-line bg-ink-800 px-2.5 py-1
         font-mono text-[10px] font-medium tracking-widest {textClass[tone]}"
>
  <span
    class="size-1.5 rounded-full {dotClass[tone]}"
    class:animate-pulse={phase === "PINGING"}
  ></span>
  {label}
</span>
