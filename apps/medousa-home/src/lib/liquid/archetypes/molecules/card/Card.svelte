<script lang="ts">
  /**
   * `card` molecule — single-entity summary.
   * Structured detail (meta/summary/chips/points) opens a chat sheet when
   * `onOpenCardDetail` is provided; otherwise `slots.detail` expands in place.
   */
  import { ChevronDown } from "@lucide/svelte";
  import Slot from "$lib/liquid/render/Slot.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import {
    cardHasDetail,
    type CardDetailPayload,
    type LiquidCardPoint,
  } from "$lib/markdown/liquidEmbeds";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const body = $derived(typeof node.props.body === "string" ? node.props.body : "");
  const emoji = $derived(typeof node.props.emoji === "string" ? node.props.emoji : "");
  const image = $derived(typeof node.props.image === "string" ? node.props.image : "");
  const meta = $derived(typeof node.props.meta === "string" ? node.props.meta : "");
  const summary = $derived(typeof node.props.summary === "string" ? node.props.summary : "");
  const chips = $derived(
    Array.isArray(node.props.chips)
      ? (node.props.chips as unknown[]).filter((c): c is string => typeof c === "string" && c.trim())
      : [],
  );
  const points = $derived.by((): LiquidCardPoint[] => {
    if (!Array.isArray(node.props.points)) return [];
    return node.props.points
      .map((item) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        const pointBody = typeof row.body === "string" ? row.body.trim() : "";
        if (!label || !pointBody) return null;
        const point: LiquidCardPoint = { label, body: pointBody };
        if (typeof row.emoji === "string" && row.emoji.trim()) point.emoji = row.emoji.trim();
        return point;
      })
      .filter((p): p is LiquidCardPoint => p !== null);
  });
  const badges = $derived(
    Array.isArray(node.props.badges)
      ? (node.props.badges as unknown[]).filter((b): b is string => typeof b === "string")
      : [],
  );
  const slotDetail = $derived(node.slots?.detail ?? []);
  const hasStructuredDetail = $derived(
    cardHasDetail({ meta, summary, chips, points }),
  );
  const hasSlotDetail = $derived(slotDetail.length > 0);
  const sheetHosted = $derived(Boolean(ctx.onOpenCardDetail) && hasStructuredDetail);
  const accordionDetail = $derived(!sheetHosted && hasSlotDetail);

  let expanded = $state(false);

  function buildPayload(): CardDetailPayload {
    const payload: CardDetailPayload = { id: node.id, title };
    if (subtitle) payload.subtitle = subtitle;
    if (emoji) payload.emoji = emoji;
    if (image) payload.image = image;
    if (meta) payload.meta = meta;
    if (summary) payload.summary = summary;
    else if (body) payload.summary = body;
    if (chips.length) payload.chips = chips;
    if (points.length) payload.points = points;
    if (badges.length) payload.badges = badges;
    return payload;
  }

  function activate() {
    ctx.sink?.emit(createSceneEvent(node.id, "select", { id: node.id }));

    if (sheetHosted) {
      const payload = buildPayload();
      ctx.sink?.emit(createSceneEvent(node.id, "expand", payload));
      ctx.onOpenCardDetail?.(payload);
      return;
    }

    if (accordionDetail) {
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
    aria-expanded={accordionDetail ? expanded : sheetHosted ? false : undefined}
    aria-haspopup={sheetHosted ? "dialog" : undefined}
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
    {#if accordionDetail}
      <ChevronDown class="liquid-card-chevron" size={16} aria-hidden="true" />
    {/if}
  </button>

  {#if accordionDetail && expanded}
    <div class="liquid-card-detail">
      <Slot nodes={slotDetail} />
    </div>
  {/if}
</div>

<style>
  .liquid-card {
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 50%, transparent);
    box-shadow:
      inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 5%, transparent),
      0 1px 0 color-mix(in srgb, var(--color-surface-50) 3%, transparent);
    overflow: hidden;
  }

  .liquid-card-main {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    width: 100%;
    padding: 0.85rem 0.95rem;
    text-align: left;
    cursor: pointer;
    background: transparent;
    border: 0;
    color: inherit;
  }

  .liquid-card-main:hover {
    background: color-mix(in srgb, var(--color-surface-700) 28%, transparent);
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
    font-size: 0.9rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-card-subtitle {
    font-size: 0.78rem;
    color: rgb(var(--color-surface-300));
  }

  .liquid-card-body {
    font-size: 0.78rem;
    line-height: 1.5;
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
