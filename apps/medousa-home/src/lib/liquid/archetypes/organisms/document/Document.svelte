<script lang="ts">
  /** `document` organism — MDX-on-the-fly: an ordered narrative flow of blocks. */
  import Slot from "$lib/liquid/render/Slot.svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const flow = $derived(node.slots?.flow ?? []);
</script>

<div class="liquid-document">
  <Slot nodes={flow} />
</div>

<style>
  .liquid-document {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    min-width: 0;
  }
</style>
