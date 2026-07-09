<script lang="ts">
  /** `chip_group` molecule — a wrapping row of chips (filters/tags). */
  import Slot from "$lib/liquid/render/Slot.svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const chips = $derived(node.slots?.chips ?? []);
</script>

{#if chips.length}
  <div class="liquid-chip-group">
    <Slot nodes={chips} />
  </div>
{/if}

<style>
  .liquid-chip-group {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    min-width: 0;
  }
</style>
