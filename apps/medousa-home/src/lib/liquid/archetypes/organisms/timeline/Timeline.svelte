<script lang="ts">
  /**
   * `timeline` organism — vertical rail (default) or horizontal snapshot carousel.
   * Distinct from `plan` (forward phases + scrubber). Paste-first from ```timeline.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import LiquidGlyph from "$lib/liquid/icons/LiquidGlyph.svelte";
  import TimelineSnapshot from "./TimelineSnapshot.svelte";

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

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const granularity = $derived(
    typeof node.props.granularity === "string" ? node.props.granularity.trim().toLowerCase() : "",
  );
  const layout = $derived(
    typeof node.props.layout === "string" && node.props.layout.trim().toLowerCase() === "snapshot"
      ? "snapshot"
      : "rail",
  );

  const events = $derived.by((): TimelineEvent[] => {
    const raw = node.props.events;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        if (!label) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `event-${i}`;
        const ev: TimelineEvent = { id, label };
        if (typeof row.ts === "string" && row.ts.trim()) ev.ts = row.ts.trim();
        if (typeof row.detail === "string" && row.detail.trim()) ev.detail = row.detail.trim();
        if (typeof row.body === "string" && row.body.trim()) ev.body = row.body.trim();
        if (typeof row.lane === "string" && row.lane.trim()) ev.lane = row.lane.trim();
        if (typeof row.emoji === "string" && row.emoji.trim()) ev.emoji = row.emoji.trim();
        if (typeof row.icon === "string" && row.icon.trim()) ev.icon = row.icon.trim();
        if (typeof row.meta === "string" && row.meta.trim()) ev.meta = row.meta.trim();
        if (typeof row.image === "string" && row.image.trim()) ev.image = row.image.trim();
        if (typeof row.media === "string" && row.media.trim()) ev.media = row.media.trim();
        return ev;
      })
      .filter((e): e is TimelineEvent => e !== null);
  });

  function selectEvent(ev: TimelineEvent) {
    ctx.sink?.emit(createSceneEvent(node.id, "select", { eventId: ev.id, label: ev.label }));
  }
</script>

{#if events.length >= 2}
  {#if layout === "snapshot"}
    <TimelineSnapshot {node} {events} {title} {subtitle} />
  {:else}
    <div class="liquid-timeline" role="list" aria-label={title || "Timeline"}>
      {#if title || subtitle || granularity}
        <header class="liquid-timeline-header">
          {#if title}
            <h3 class="liquid-timeline-title">{title}</h3>
          {/if}
          {#if subtitle}
            <p class="liquid-timeline-subtitle">{subtitle}</p>
          {/if}
          {#if granularity === "day" || granularity === "hour" || granularity === "event"}
            <p class="liquid-timeline-granularity">{granularity}</p>
          {/if}
        </header>
      {/if}

      <ol class="liquid-timeline-rail">
        {#each events as ev, i (ev.id)}
          <li class="liquid-timeline-item" class:liquid-timeline-item-last={i === events.length - 1}>
            <div class="liquid-timeline-spine" aria-hidden="true">
              <span class="liquid-timeline-dot"></span>
              {#if i < events.length - 1}
                <span class="liquid-timeline-line"></span>
              {/if}
            </div>
            <button type="button" class="liquid-timeline-card" onclick={() => selectEvent(ev)}>
              {#if ev.ts}
                <span class="liquid-timeline-ts">{ev.ts}</span>
              {/if}
              <span class="liquid-timeline-label-row">
                {#if ev.emoji || ev.icon}
                  <span class="liquid-timeline-emoji" aria-hidden="true">
                    <LiquidGlyph icon={ev.icon} emoji={ev.emoji} size={14} />
                  </span>
                {/if}
                <span class="liquid-timeline-label">{ev.label}</span>
                {#if ev.meta || ev.lane}
                  <span class="liquid-timeline-lane">{ev.meta || ev.lane}</span>
                {/if}
              </span>
              {#if ev.body || ev.detail}
                <span class="liquid-timeline-detail">{ev.body || ev.detail}</span>
              {/if}
            </button>
          </li>
        {/each}
      </ol>
    </div>
  {/if}
{/if}

<style>
  .liquid-timeline {
    margin: 0;
    padding: 0.85rem 0.9rem 1rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    min-width: 0;
  }

  .liquid-timeline-header {
    margin-bottom: 0.85rem;
  }

  .liquid-timeline-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-timeline-subtitle {
    margin: 0.35rem 0 0;
    font-size: 0.8rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .liquid-timeline-granularity {
    margin: 0.35rem 0 0;
    font-size: 0.6rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .liquid-timeline-rail {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .liquid-timeline-item {
    display: grid;
    grid-template-columns: 1.1rem minmax(0, 1fr);
    gap: 0.65rem;
    align-items: stretch;
  }

  .liquid-timeline-spine {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding-top: 0.45rem;
  }

  .liquid-timeline-dot {
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 999px;
    flex-shrink: 0;
    background: rgb(var(--color-primary-400));
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-primary-500) 18%, transparent);
  }

  .liquid-timeline-line {
    flex: 1 1 auto;
    width: 1px;
    margin-top: 0.35rem;
    min-height: 1.25rem;
    background: color-mix(in srgb, var(--color-surface-500) 45%, transparent);
  }

  .liquid-timeline-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.25rem;
    width: 100%;
    margin: 0 0 0.85rem;
    padding: 0.15rem 0.2rem 0.35rem;
    border: 0;
    border-radius: 0.5rem;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .liquid-timeline-item-last .liquid-timeline-card {
    margin-bottom: 0;
  }

  .liquid-timeline-card:hover {
    background: color-mix(in srgb, var(--color-surface-50) 4%, transparent);
  }

  .liquid-timeline-ts {
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: rgb(var(--color-surface-400));
  }

  .liquid-timeline-label-row {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.35rem 0.5rem;
  }

  .liquid-timeline-emoji {
    font-size: 0.95rem;
    line-height: 1;
  }

  .liquid-timeline-label {
    font-size: 0.9rem;
    font-weight: 650;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-50));
    line-height: 1.3;
  }

  .liquid-timeline-lane {
    font-size: 0.6rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    padding: 0.12rem 0.45rem;
    border-radius: 999px;
    color: rgb(var(--color-primary-200));
    border: 1px solid color-mix(in srgb, var(--color-primary-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
  }

  .liquid-timeline-detail {
    font-size: 0.78rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-300));
  }
</style>
