<script lang="ts">
  /** `chip_group` molecule — horizontal chip strip (filters / day tabs). */
  import Slot from "$lib/liquid/render/Slot.svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const chips = $derived(node.slots?.chips ?? []);
</script>

{#if chips.length}
  <div class="liquid-chip-group" data-no-tab-swipe>
    <Slot nodes={chips} />
  </div>
{/if}

<style>
  .liquid-chip-group {
    display: flex;
    flex-wrap: nowrap;
    gap: 0.4rem;
    min-width: 0;
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x pinch-zoom;
    overscroll-behavior-x: contain;
    padding-bottom: 0.15rem;
  }

  .liquid-chip-group :global(.liquid-chip) {
    flex: 0 0 auto;
  }
</style>
