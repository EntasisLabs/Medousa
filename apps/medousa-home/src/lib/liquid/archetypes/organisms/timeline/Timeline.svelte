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

  const hasTimeGutter = $derived(events.some((ev) => Boolean(ev.ts)));

  function selectEvent(ev: TimelineEvent) {
    ctx.sink?.emit(createSceneEvent(node.id, "select", { eventId: ev.id, label: ev.label }));
  }

  function hasGlyph(ev: TimelineEvent): boolean {
    return Boolean(ev.icon?.trim() || ev.emoji?.trim());
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

      <ol
        class="liquid-timeline-rail"
        class:liquid-timeline-rail-with-ts={hasTimeGutter}
      >
        {#each events as ev, i (ev.id)}
          <li
            class="liquid-timeline-item"
            class:liquid-timeline-item-last={i === events.length - 1}
          >
            {#if hasTimeGutter}
              <span class="liquid-timeline-ts">{ev.ts ?? ""}</span>
            {/if}
            <div class="liquid-timeline-spine" aria-hidden="true">
              <span
                class="liquid-timeline-dot"
                class:liquid-timeline-dot-glyph={hasGlyph(ev)}
              >
                {#if hasGlyph(ev)}
                  <LiquidGlyph icon={ev.icon} emoji={ev.emoji} fallback="•" size={12} />
                {/if}
              </span>
            </div>
            <button type="button" class="liquid-timeline-card" onclick={() => selectEvent(ev)}>
              <span class="liquid-timeline-label-row">
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
    --timeline-spine-col: 1.5rem;
    --timeline-gap: 0.55rem;
    --timeline-node: 1.15rem;
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--timeline-gap);
  }

  .liquid-timeline-item {
    display: grid;
    grid-template-columns: var(--timeline-spine-col) minmax(0, 1fr);
    gap: 0.7rem;
    align-items: stretch;
    min-width: 0;
  }

  .liquid-timeline-rail-with-ts .liquid-timeline-item {
    grid-template-columns: minmax(2.6rem, auto) var(--timeline-spine-col) minmax(0, 1fr);
  }

  .liquid-timeline-ts {
    display: flex;
    align-items: flex-start;
    justify-content: flex-end;
    padding-top: 0.55rem;
    font-size: 0.68rem;
    font-weight: 650;
    letter-spacing: 0.02em;
    font-variant-numeric: tabular-nums;
    text-align: right;
    line-height: 1.2;
    color: rgb(var(--color-surface-400));
  }

  .liquid-timeline-spine {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    width: var(--timeline-spine-col);
    padding-top: 0.4rem;
  }

  /* Connector from this node through the gap to the next item's node. */
  .liquid-timeline-item:not(.liquid-timeline-item-last) .liquid-timeline-spine::after {
    content: "";
    position: absolute;
    top: calc(0.4rem + var(--timeline-node));
    bottom: calc(-1 * var(--timeline-gap));
    left: 50%;
    width: 2px;
    margin-left: -1px;
    z-index: 0;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--color-primary-400) 55%, transparent) 0%,
      color-mix(in srgb, var(--color-primary-500) 28%, var(--color-surface-500)) 55%,
      color-mix(in srgb, var(--color-surface-500) 50%, transparent) 100%
    );
  }

  .liquid-timeline-dot {
    position: relative;
    z-index: 1;
    display: grid;
    place-items: center;
    width: var(--timeline-node);
    height: var(--timeline-node);
    border-radius: 999px;
    flex-shrink: 0;
    background: rgb(var(--color-primary-400));
    border: 1px solid color-mix(in srgb, var(--color-primary-300) 45%, transparent);
    box-shadow:
      0 0 0 3px color-mix(in srgb, var(--color-primary-500) 22%, transparent),
      0 1px 4px rgb(0 0 0 / 0.2);
    color: rgb(var(--color-surface-50));
  }

  .liquid-timeline-dot-glyph {
    background: color-mix(in srgb, var(--color-primary-500) 28%, var(--color-surface-900));
    border-color: color-mix(in srgb, var(--color-primary-400) 50%, transparent);
  }

  .liquid-timeline-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.3rem;
    width: 100%;
    margin: 0;
    padding: 0.55rem 0.7rem 0.6rem;
    border-radius: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 26%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 42%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 3%, transparent);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .liquid-timeline-card:hover {
    background: color-mix(in srgb, var(--color-surface-700) 22%, transparent);
    border-color: color-mix(in srgb, var(--color-primary-500) 28%, var(--color-surface-500));
  }

  .liquid-timeline-label-row {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.35rem 0.5rem;
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
