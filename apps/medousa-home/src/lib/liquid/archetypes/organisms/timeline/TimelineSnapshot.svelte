<script lang="ts">
  /** Horizontal date strip + peek carousel for `layout: snapshot` timelines. */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import LiquidGlyph from "$lib/liquid/icons/LiquidGlyph.svelte";
  import { isHttpUrl } from "$lib/utils/openInBrowser";

  interface TimelineEvent {
    id: string;
    label: string;
    ts?: string;
    detail?: string;
    lane?: string;
    emoji?: string;
    icon?: string;
    meta?: string;
    body?: string;
    image?: string;
    media?: string;
  }

  let { node, events, title, subtitle }: ArchetypeProps & {
    events: TimelineEvent[];
    title: string;
    subtitle: string;
  } = $props();

  const ctx = getLiquidContext();

  let activeIndex = $state(0);
  let trackEl = $state<HTMLElement | null>(null);
  let carouselEl = $state<HTMLElement | null>(null);
  let syncingFromTrack = false;

  function eventBody(ev: TimelineEvent): string {
    return ev.body?.trim() || ev.detail?.trim() || "";
  }

  function eventMeta(ev: TimelineEvent): string {
    return ev.meta?.trim() || ev.lane?.trim() || "";
  }

  function eventImage(ev: TimelineEvent): string | undefined {
    const src = ev.image?.trim() || ev.media?.trim();
    return src && isHttpUrl(src) ? src : undefined;
  }

  function selectEvent(ev: TimelineEvent, index: number) {
    activeIndex = index;
    ctx.sink?.emit(createSceneEvent(node.id, "select", { eventId: ev.id, label: ev.label, index }));
  }

  function focusIndex(index: number, behavior: ScrollBehavior = "smooth") {
    if (index < 0 || index >= events.length) return;
    activeIndex = index;
    const card = carouselEl?.children[index] as HTMLElement | undefined;
    card?.scrollIntoView({ behavior, inline: "center", block: "nearest" });
    const nodeBtn = trackEl?.children[index] as HTMLElement | undefined;
    nodeBtn?.scrollIntoView({ behavior, inline: "center", block: "nearest" });
  }

  function onTrackSelect(index: number) {
    syncingFromTrack = true;
    focusIndex(index);
    queueMicrotask(() => {
      syncingFromTrack = false;
    });
  }

  function onCarouselScroll() {
    if (syncingFromTrack || !carouselEl) return;
    const center = carouselEl.scrollLeft + carouselEl.clientWidth / 2;
    let best = 0;
    let bestDist = Infinity;
    for (let i = 0; i < carouselEl.children.length; i++) {
      const child = carouselEl.children[i] as HTMLElement;
      const childCenter = child.offsetLeft + child.offsetWidth / 2;
      const dist = Math.abs(center - childCenter);
      if (dist < bestDist) {
        bestDist = dist;
        best = i;
      }
    }
    if (best !== activeIndex) {
      activeIndex = best;
      const nodeBtn = trackEl?.children[best] as HTMLElement | undefined;
      nodeBtn?.scrollIntoView({ behavior: "smooth", inline: "center", block: "nearest" });
    }
  }
</script>

<div class="liquid-timeline-snapshot" aria-label={title || "Timeline snapshot"}>
  {#if title || subtitle}
    <header class="liquid-timeline-snapshot-header">
      {#if title}
        <h3 class="liquid-timeline-snapshot-title">{title}</h3>
      {/if}
      {#if subtitle}
        <p class="liquid-timeline-snapshot-subtitle">{subtitle}</p>
      {/if}
    </header>
  {/if}

  <div class="liquid-timeline-snapshot-track-wrap">
    <div
      class="liquid-timeline-snapshot-track"
      bind:this={trackEl}
      role="tablist"
      aria-label="Timeline dates"
    >
      <span class="liquid-timeline-snapshot-rail" aria-hidden="true"></span>
      {#each events as ev, i (ev.id)}
        <button
          type="button"
          class="liquid-timeline-snapshot-node"
          class:liquid-timeline-snapshot-node-active={i === activeIndex}
          role="tab"
          aria-selected={i === activeIndex}
          aria-controls={`timeline-snapshot-card-${ev.id}`}
          onclick={() => onTrackSelect(i)}
        >
          {#if ev.ts}
            <span class="liquid-timeline-snapshot-node-ts">{ev.ts}</span>
          {/if}
          <span class="liquid-timeline-snapshot-node-dot" aria-hidden="true">
            <span class="liquid-timeline-snapshot-node-glyph">
              <LiquidGlyph icon={ev.icon} emoji={ev.emoji} fallback="•" size={12} />
            </span>
          </span>
        </button>
      {/each}
    </div>
  </div>

  <div
    class="liquid-timeline-snapshot-carousel"
    class:liquid-timeline-snapshot-carousel--export={ctx.exportPaper}
    bind:this={carouselEl}
    data-no-tab-swipe
    onscroll={onCarouselScroll}
  >
    {#each events as ev, i (ev.id)}
      {@const image = eventImage(ev)}
      <div
        id={`timeline-snapshot-card-${ev.id}`}
        class="liquid-timeline-snapshot-card"
        class:liquid-timeline-snapshot-card-active={i === activeIndex}
        role="tabpanel"
        aria-hidden={i !== activeIndex}
      >
        <button
          type="button"
          class="liquid-timeline-snapshot-card-btn"
          onclick={() => {
            focusIndex(i);
            selectEvent(ev, i);
          }}
        >
          {#if image}
            <img class="liquid-timeline-snapshot-card-image" src={image} alt="" loading="lazy" />
          {/if}
          <div class="liquid-timeline-snapshot-card-body">
            <div class="liquid-timeline-snapshot-card-head">
              <span class="liquid-timeline-snapshot-card-glyph" aria-hidden="true">
                <LiquidGlyph icon={ev.icon} emoji={ev.emoji} fallback="•" size={16} />
              </span>
              <div class="liquid-timeline-snapshot-card-titles">
                {#if ev.ts}
                  <span class="liquid-timeline-snapshot-card-ts">{ev.ts}</span>
                {/if}
                <span class="liquid-timeline-snapshot-card-title">{ev.label}</span>
              </div>
            </div>
            {#if eventMeta(ev)}
              <span class="liquid-timeline-snapshot-card-meta">{eventMeta(ev)}</span>
            {/if}
            {#if eventBody(ev)}
              <p class="liquid-timeline-snapshot-card-copy">{eventBody(ev)}</p>
            {/if}
          </div>
        </button>
      </div>
    {/each}
  </div>
</div>

<style>
  .liquid-timeline-snapshot {
    margin: 0;
    padding: 0.85rem 0.75rem 1rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow:
      inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent),
      0 10px 28px rgb(0 0 0 / 0.12);
    min-width: 0;
  }

  .liquid-timeline-snapshot-header {
    margin-bottom: 0.75rem;
    padding: 0 0.15rem;
  }

  .liquid-timeline-snapshot-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-timeline-snapshot-subtitle {
    margin: 0.35rem 0 0;
    font-size: 0.8rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .liquid-timeline-snapshot-track-wrap {
    position: relative;
    margin-bottom: 0.85rem;
    padding: 0 0.15rem;
  }

  .liquid-timeline-snapshot-track {
    position: relative;
    display: flex;
    gap: 0.35rem;
    overflow-x: auto;
    padding: 0.35rem 0.2rem 0.15rem;
    scroll-snap-type: x proximity;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x pinch-zoom;
    overscroll-behavior-x: contain;
    scrollbar-width: none;
  }

  .liquid-timeline-snapshot-track::-webkit-scrollbar {
    display: none;
  }

  .liquid-timeline-snapshot-rail {
    position: absolute;
    left: 0.75rem;
    right: 0.75rem;
    top: calc(100% - 0.55rem);
    height: 2px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-surface-500) 42%, transparent);
    pointer-events: none;
  }

  .liquid-timeline-snapshot-node {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.45rem;
    min-width: 3.4rem;
    padding: 0.15rem 0.25rem 0.55rem;
    border: 0;
    border-radius: 0.65rem;
    background: transparent;
    color: inherit;
    cursor: pointer;
    scroll-snap-align: center;
    transition:
      transform 120ms ease,
      background 120ms ease;
  }

  .liquid-timeline-snapshot-node:hover {
    background: color-mix(in srgb, var(--color-surface-50) 4%, transparent);
  }

  .liquid-timeline-snapshot-node-active {
    transform: translateY(-1px);
  }

  .liquid-timeline-snapshot-node-ts {
    font-size: 0.62rem;
    font-weight: 650;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-400));
    white-space: nowrap;
  }

  .liquid-timeline-snapshot-node-active .liquid-timeline-snapshot-node-ts {
    color: rgb(var(--color-primary-200));
  }

  .liquid-timeline-snapshot-node-dot {
    display: grid;
    place-items: center;
    width: 2rem;
    height: 2rem;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 72%, transparent);
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.16);
    transition:
      border-color 120ms ease,
      box-shadow 120ms ease,
      background 120ms ease;
  }

  .liquid-timeline-snapshot-node-active .liquid-timeline-snapshot-node-dot {
    border-color: color-mix(in srgb, var(--color-primary-400) 55%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 16%, transparent);
    box-shadow:
      0 0 0 3px color-mix(in srgb, var(--color-primary-500) 14%, transparent),
      0 6px 16px rgb(0 0 0 / 0.18);
  }

  .liquid-timeline-snapshot-node-glyph {
    font-size: 0.95rem;
    line-height: 1;
  }

  .liquid-timeline-snapshot-carousel {
    display: flex;
    gap: 0.65rem;
    overflow-x: auto;
    padding: 0.15rem 0.1rem 0.45rem;
    scroll-snap-type: x mandatory;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x pinch-zoom;
    overscroll-behavior-x: contain;
    mask-image: linear-gradient(
      to right,
      transparent 0,
      #000 0.5rem,
      #000 calc(100% - 1.25rem),
      transparent 100%
    );
  }

  .liquid-timeline-snapshot-carousel--export {
    flex-wrap: wrap;
    overflow: visible;
    scroll-snap-type: none;
    touch-action: auto;
    overscroll-behavior: auto;
    mask-image: none;
    -webkit-mask-image: none;
  }

  .liquid-timeline-snapshot-card {
    flex: 0 0 auto;
    width: min(17rem, 84%);
    scroll-snap-align: center;
    transition: transform 140ms ease;
  }

  .liquid-timeline-snapshot-carousel--export .liquid-timeline-snapshot-card {
    flex: 1 1 calc(50% - 0.35rem);
    width: auto;
    min-width: min(14rem, 100%);
    scroll-snap-align: none;
  }

  .liquid-timeline-snapshot-card-active {
    transform: scale(1.01);
  }

  .liquid-timeline-snapshot-card-btn {
    display: flex;
    flex-direction: column;
    width: 100%;
    min-height: 100%;
    margin: 0;
    padding: 0;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 32%, transparent);
    border-radius: 0.8rem;
    background: color-mix(in srgb, var(--color-surface-900) 62%, transparent);
    box-shadow: 0 8px 22px rgb(0 0 0 / 0.14);
    color: inherit;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    transition:
      border-color 140ms ease,
      box-shadow 140ms ease,
      background 140ms ease;
  }

  .liquid-timeline-snapshot-card-active .liquid-timeline-snapshot-card-btn {
    border-color: color-mix(in srgb, var(--color-primary-400) 42%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 78%, transparent);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--color-primary-500) 18%, transparent),
      0 12px 28px rgb(0 0 0 / 0.2);
  }

  .liquid-timeline-snapshot-card-btn:hover {
    border-color: color-mix(in srgb, var(--color-surface-400) 40%, transparent);
  }

  .liquid-timeline-snapshot-card-image {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    display: block;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 24%, transparent);
  }

  .liquid-timeline-snapshot-card-body {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    padding: 0.75rem 0.8rem 0.85rem;
  }

  .liquid-timeline-snapshot-card-head {
    display: flex;
    align-items: flex-start;
    gap: 0.55rem;
  }

  .liquid-timeline-snapshot-card-glyph {
    font-size: 1.05rem;
    line-height: 1;
    flex-shrink: 0;
  }

  .liquid-timeline-snapshot-card-titles {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }

  .liquid-timeline-snapshot-card-ts {
    font-size: 0.62rem;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-400));
  }

  .liquid-timeline-snapshot-card-title {
    font-size: 0.92rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    line-height: 1.3;
    color: rgb(var(--color-surface-50));
  }

  .liquid-timeline-snapshot-card-meta {
    font-size: 0.62rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-primary-200));
  }

  .liquid-timeline-snapshot-card-copy {
    margin: 0;
    font-size: 0.78rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-300));
  }
</style>
