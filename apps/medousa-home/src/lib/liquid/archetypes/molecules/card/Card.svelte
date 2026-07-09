<script lang="ts">
  /**
   * `card` molecule — a single-entity summary. If it carries a `detail` slot,
   * tapping exposes the detail in place (lazy: rendered only once expanded —
   * generate-more-than-show, client edition). Emits `select` / `expand`.
   */
  import { ChevronDown } from "@lucide/svelte";
  import Slot from "$lib/liquid/render/Slot.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const body = $derived(typeof node.props.body === "string" ? node.props.body : "");
  const emoji = $derived(typeof node.props.emoji === "string" ? node.props.emoji : "");
  const image = $derived(typeof node.props.image === "string" ? node.props.image : "");
  const badges = $derived(
    Array.isArray(node.props.badges)
      ? (node.props.badges as unknown[]).filter((b): b is string => typeof b === "string")
      : [],
  );
  const detail = $derived(node.slots?.detail ?? []);
  const hasDetail = $derived(detail.length > 0);

  let expanded = $state(false);

  function activate() {
    ctx.sink?.emit(createSceneEvent(node.id, "select", { id: node.id }));
    if (hasDetail) {
      expanded = !expanded;
      ctx.sink?.emit(createSceneEvent(node.id, expanded ? "expand" : "collapse", { id: node.id }));
    }
  }
</script>

<div class="liquid-card" class:liquid-card-expanded={expanded}>
  <button
    type="button"
    class="liquid-card-main"
    onclick={activate}
    aria-expanded={hasDetail ? expanded : undefined}
  >
    {#if image}
      <img class="liquid-card-thumb" src={image} alt="" loading="lazy" />
    {:else if emoji}
      <span class="liquid-card-emoji" aria-hidden="true">{emoji}</span>
    {/if}
    <span class="liquid-card-text">
      <span class="liquid-card-title">{title}</span>
      {#if subtitle}<span class="liquid-card-subtitle">{subtitle}</span>{/if}
      {#if body}<span class="liquid-card-body">{body}</span>{/if}
      {#if badges.length}
        <span class="liquid-card-badges">
          {#each badges as badge, index (index)}
            <span class="liquid-card-badge">{badge}</span>
          {/each}
        </span>
      {/if}
    </span>
    {#if hasDetail}
      <ChevronDown class="liquid-card-chevron" size={16} aria-hidden="true" />
    {/if}
  </button>

  {#if hasDetail && expanded}
    <div class="liquid-card-detail">
      <Slot nodes={detail} />
    </div>
  {/if}
</div>

<style>
  .liquid-card {
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 45%, transparent);
    overflow: hidden;
  }

  .liquid-card-main {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
    width: 100%;
    padding: 0.7rem 0.8rem;
    text-align: left;
    cursor: pointer;
    background: transparent;
    border: 0;
    color: inherit;
  }

  .liquid-card-main:hover {
    background: color-mix(in srgb, var(--color-surface-700) 30%, transparent);
  }

  .liquid-card-thumb {
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 0.5rem;
    object-fit: cover;
    flex-shrink: 0;
  }

  .liquid-card-emoji {
    font-size: 1.4rem;
    line-height: 1;
    flex-shrink: 0;
  }

  .liquid-card-text {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
    flex: 1 1 auto;
  }

  .liquid-card-title {
    font-size: 0.85rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .liquid-card-subtitle {
    font-size: 0.75rem;
    color: rgb(var(--color-surface-300));
  }

  .liquid-card-body {
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-200));
  }

  .liquid-card-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-top: 0.15rem;
  }

  .liquid-card-badge {
    font-size: 0.625rem;
    padding: 0.05rem 0.4rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    color: rgb(var(--color-surface-200));
  }

  .liquid-card-main :global(.liquid-card-chevron) {
    flex-shrink: 0;
    margin-top: 0.1rem;
    color: rgb(var(--color-surface-400));
    transition: transform 0.18s ease;
  }

  .liquid-card-expanded .liquid-card-main :global(.liquid-card-chevron) {
    transform: rotate(180deg);
  }

  .liquid-card-detail {
    padding: 0 0.8rem 0.8rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-600) 25%, transparent);
    padding-top: 0.6rem;
  }
</style>
