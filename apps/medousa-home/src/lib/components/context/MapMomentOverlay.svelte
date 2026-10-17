<script lang="ts">
  import ContextSavedMoment from "$lib/components/context/ContextSavedMoment.svelte";
  import type { LocusNodeDetailResponse } from "$lib/types/locus";
  import { X } from "@lucide/svelte";
  let { detail, sessionLabels = {}, onClear }: {
    detail: LocusNodeDetailResponse;
    sessionLabels?: Record<string, string>;
    onClear?: () => void;
  } = $props();
</script>

<article class="relative max-h-[65vh] overflow-y-auto rounded-xl border border-surface-600/40 bg-surface-900 p-5 shadow-xl" aria-label="Saved moment" onpointerdown={(event) => event.stopPropagation()}>
  {#if onClear}
    <button type="button" class="absolute right-3 top-3 text-content-quiet hover:text-surface-100" aria-label="Clear focus" onclick={onClear}><X size={14} /></button>
  {/if}
  <ContextSavedMoment compact={true} {detail} {sessionLabels} />
</article>
