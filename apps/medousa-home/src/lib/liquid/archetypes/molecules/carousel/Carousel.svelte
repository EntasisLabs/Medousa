<script lang="ts">
  /** `carousel` molecule — horizontal scroller of item nodes (usually cards). */
  import Slot from "$lib/liquid/render/Slot.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const items = $derived(node.slots?.items ?? []);
  let reachedEnd = false;

  function onScroll(event: Event) {
    const el = event.currentTarget as HTMLElement;
    const atEnd = el.scrollLeft + el.clientWidth >= el.scrollWidth - 24;
    if (atEnd && !reachedEnd) {
      reachedEnd = true;
      ctx.sink?.emit(createSceneEvent(node.id, "scroll_end", {}));
    } else if (!atEnd) {
      reachedEnd = false;
    }
  }
</script>

{#if items.length}
  <div class="liquid-carousel" onscroll={onScroll}>
    {#each items as item (item.id)}
      <div class="liquid-carousel-item">
        <Slot nodes={[item]} />
      </div>
    {/each}
  </div>
{/if}

<style>
  .liquid-carousel {
    display: flex;
    gap: 0.75rem;
    overflow-x: auto;
    padding: 0.15rem 0.1rem 0.45rem;
    scroll-snap-type: x proximity;
    -webkit-overflow-scrolling: touch;
    mask-image: linear-gradient(
      to right,
      transparent 0,
      #000 0.6rem,
      #000 calc(100% - 1.4rem),
      transparent 100%
    );
  }

  .liquid-carousel-item {
    flex: 0 0 auto;
    width: min(16.5rem, 82%);
    scroll-snap-align: start;
  }
</style>
