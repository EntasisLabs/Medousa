<script lang="ts">
  /**
   * `timeline` organism — vertical dated event rail for history / what-happened.
   * Distinct from `plan` (forward phases + scrubber). Paste-first from ```timeline.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  interface TimelineEvent {
    id: string;
    label: string;
    ts?: string;
    detail?: string;
    lane?: string;
    emoji?: string;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const granularity = $derived(
    typeof node.props.granularity === "string" ? node.props.granularity.trim().toLowerCase() : "",
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
        if (typeof row.lane === "string" && row.lane.trim()) ev.lane = row.lane.trim();
        if (typeof row.emoji === "string" && row.emoji.trim()) ev.emoji = row.emoji.trim();
        return ev;
      })
      .filter((e): e is TimelineEvent => e !== null);
  });

  function selectEvent(ev: TimelineEvent) {
    ctx.sink?.emit(createSceneEvent(node.id, "select", { eventId: ev.id, label: ev.label }));
  }
</script>

{#if events.length >= 2}
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
              {#if ev.emoji}
                <span class="liquid-timeline-emoji" aria-hidden="true">{ev.emoji}</span>
              {/if}
              <span class="liquid-timeline-label">{ev.label}</span>
              {#if ev.lane}
                <span class="liquid-timeline-lane">{ev.lane}</span>
              {/if}
            </span>
            {#if ev.detail}
              <span class="liquid-timeline-detail">{ev.detail}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ol>
  </div>
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
