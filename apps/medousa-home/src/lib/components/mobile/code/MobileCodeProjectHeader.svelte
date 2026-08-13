<script lang="ts">
  import {
    humanExecutorLabel,
    humanPhaseLabel,
  } from "$lib/forge";
  import { undertakings } from "$lib/stores/undertakings.svelte";

  const title = $derived(undertakings.detail?.title ?? "Project");
  const phase = $derived(
    undertakings.detail ? humanPhaseLabel(undertakings.detail.human_phase) : "",
  );
  const executor = $derived(
    humanExecutorLabel(undertakings.active?.executorKind) ?? "",
  );
  const meta = $derived([phase, executor].filter(Boolean).join(" · "));
</script>

<header class="flex h-10 shrink-0 items-center gap-2 px-3">
  <p class="min-w-0 truncate text-[13px] font-medium text-content-secondary">{title}</p>
  {#if meta}
    <p class="shrink-0 truncate text-[11px] text-content-quiet">{meta}</p>
  {/if}
</header>
