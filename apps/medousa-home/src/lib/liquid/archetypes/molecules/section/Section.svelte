<script lang="ts">
  /** `section` molecule — the narrative connector: heading + optional sub + content. */
  import Slot from "$lib/liquid/render/Slot.svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const content = $derived(node.slots?.content ?? []);
</script>

<section class="liquid-section">
  {#if title}
    <h3 class="liquid-section-title">{title}</h3>
  {/if}
  {#if subtitle}
    <p class="liquid-section-sub">{subtitle}</p>
  {/if}
  {#if content.length}
    <div class="liquid-section-content">
      <Slot nodes={content} />
    </div>
  {/if}
</section>

<style>
  .liquid-section {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    min-width: 0;
  }

  .liquid-section-title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .liquid-section-sub {
    margin: 0;
    font-size: 0.8rem;
    color: rgb(var(--color-surface-300));
  }

  .liquid-section-content {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    margin-top: 0.15rem;
  }
</style>
