<script lang="ts">
  /**
   * `plan` organism — Monogram-style trip flow: segment scrubber + media-forward
   * phase cards. Paste-first from ```plan markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { isHttpUrl } from "$lib/utils/openInBrowser";

  interface PlanSegment {
    id: string;
    label: string;
    time?: string;
    emoji?: string;
    image?: string;
    subtitle?: string;
    body?: string;
    badge?: string;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const grouping = $derived(
    typeof node.props.grouping === "string" ? node.props.grouping.trim().toLowerCase() : "",
  );

  const segments = $derived.by((): PlanSegment[] => {
    const raw = node.props.segments;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        if (!label) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `segment-${i}`;
        const seg: PlanSegment = { id, label };
        if (typeof row.time === "string" && row.time.trim()) seg.time = row.time.trim();
        if (typeof row.emoji === "string" && row.emoji.trim()) seg.emoji = row.emoji.trim();
        if (typeof row.image === "string" && row.image.trim()) seg.image = row.image.trim();
        if (typeof row.subtitle === "string" && row.subtitle.trim()) seg.subtitle = row.subtitle.trim();
        if (typeof row.body === "string" && row.body.trim()) seg.body = row.body.trim();
        if (typeof row.badge === "string" && row.badge.trim()) seg.badge = row.badge.trim();
        return seg;
      })
      .filter((s): s is PlanSegment => s !== null);
  });

  let activeIndex = $state(0);
  let stripEl = $state<HTMLElement | null>(null);

  function scrubberLabel(seg: PlanSegment): string {
    return seg.time?.trim() || seg.label;
  }

  function selectSegment(index: number, scroll = true) {
    const seg = segments[index];
    if (!seg) return;
    activeIndex = index;
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { segmentId: seg.id, label: seg.label }),
    );
    if (scroll && stripEl) {
      const el = stripEl.querySelector<HTMLElement>(`[data-plan-card="${index}"]`);
      el?.scrollIntoView({ behavior: "smooth", inline: "start", block: "nearest" });
    }
  }

  function onStripScroll() {
    const strip = stripEl;
    if (!strip || segments.length === 0) return;
    const cards = strip.querySelectorAll<HTMLElement>("[data-plan-card]");
    let best = 0;
    let bestDist = Infinity;
    for (const el of cards) {
      const i = Number(el.dataset.planCard);
      if (Number.isNaN(i)) continue;
      const dist = Math.abs(el.offsetLeft - strip.scrollLeft);
      if (dist < bestDist) {
        bestDist = dist;
        best = i;
      }
    }
    if (best !== activeIndex) activeIndex = best;
  }
</script>

{#if segments.length >= 2}
  <div class="liquid-plan" role="group" aria-label={title || "Plan"}>
    {#if title || subtitle || grouping}
      <header class="liquid-plan-header">
        {#if title}
          <h3 class="liquid-plan-title">{title}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-plan-subtitle">{subtitle}</p>
        {/if}
        {#if grouping === "day" || grouping === "phase" || grouping === "milestone"}
          <p class="liquid-plan-grouping">{grouping}</p>
        {/if}
      </header>
    {/if}

    <div class="liquid-plan-scrubber" data-no-tab-swipe role="tablist" aria-label="Phases">
      {#each segments as seg, i (seg.id)}
        <button
          type="button"
          class="liquid-plan-scrub"
          class:liquid-plan-scrub-active={i === activeIndex}
          role="tab"
          aria-selected={i === activeIndex}
          onclick={() => selectSegment(i)}
        >
          {#if seg.emoji}
            <span class="liquid-plan-scrub-emoji" aria-hidden="true">{seg.emoji}</span>
          {/if}
          <span class="liquid-plan-scrub-label">{scrubberLabel(seg)}</span>
        </button>
      {/each}
    </div>

    <div
      class="liquid-plan-strip"
      data-no-tab-swipe
      bind:this={stripEl}
      onscroll={onStripScroll}
    >
      {#each segments as seg, i (seg.id)}
        <article
          class="liquid-plan-card"
          class:liquid-plan-card-active={i === activeIndex}
          data-plan-card={i}
        >
          <button type="button" class="liquid-plan-card-btn" onclick={() => selectSegment(i, false)}>
            {#if seg.image && isHttpUrl(seg.image)}
              <span class="liquid-plan-card-media">
                <img src={seg.image} alt="" loading="lazy" />
                {#if seg.badge}
                  <span class="liquid-plan-card-badge">{seg.badge}</span>
                {/if}
              </span>
            {:else if seg.emoji}
              <span class="liquid-plan-card-emoji-hero" aria-hidden="true">{seg.emoji}</span>
            {/if}
            <span class="liquid-plan-card-text">
              <span class="liquid-plan-card-title">{seg.label}</span>
              {#if seg.subtitle}
                <span class="liquid-plan-card-sub">{seg.subtitle}</span>
              {/if}
              {#if seg.body}
                <span class="liquid-plan-card-body">{seg.body}</span>
              {/if}
              {#if seg.time}
                <span class="liquid-plan-card-time">{seg.time}</span>
              {/if}
            </span>
          </button>
        </article>
      {/each}
    </div>
  </div>
{/if}

<style>
  .liquid-plan {
    margin: 0;
    padding: 0.85rem 0.9rem 1rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    min-width: 0;
  }

  .liquid-plan-header {
    margin-bottom: 0.75rem;
  }

  .liquid-plan-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-plan-subtitle {
    margin: 0.35rem 0 0;
    font-size: 0.8rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .liquid-plan-grouping {
    margin: 0.35rem 0 0;
    font-size: 0.6rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .liquid-plan-scrubber {
    display: flex;
    gap: 0.35rem;
    overflow-x: auto;
    padding: 0.1rem 0 0.65rem;
    margin-bottom: 0.15rem;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x pinch-zoom;
    overscroll-behavior-x: contain;
  }

  .liquid-plan-scrub {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    min-width: 3.6rem;
    padding: 0.25rem 0.45rem 0.4rem;
    border: 0;
    border-radius: 0.5rem;
    background: transparent;
    color: rgb(var(--color-surface-500));
    cursor: pointer;
    position: relative;
  }

  .liquid-plan-scrub-emoji {
    font-size: 1.05rem;
    line-height: 1;
  }

  .liquid-plan-scrub-label {
    font-size: 0.65rem;
    font-weight: 550;
    white-space: nowrap;
    max-width: 5.5rem;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .liquid-plan-scrub-active {
    color: rgb(var(--color-surface-100));
  }

  .liquid-plan-scrub-active::after {
    content: "";
    position: absolute;
    left: 20%;
    right: 20%;
    bottom: 0.1rem;
    height: 2px;
    border-radius: 999px;
    background: rgb(var(--color-surface-100));
  }

  .liquid-plan-strip {
    display: flex;
    gap: 0.75rem;
    overflow-x: auto;
    padding: 0.1rem 0.05rem 0.35rem;
    scroll-snap-type: x proximity;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x pinch-zoom;
    overscroll-behavior-x: contain;
    mask-image: linear-gradient(
      to right,
      transparent 0,
      #000 0.5rem,
      #000 calc(100% - 1.2rem),
      transparent 100%
    );
  }

  .liquid-plan-card {
    flex: 0 0 auto;
    width: min(16rem, 78%);
    scroll-snap-align: start;
    border-radius: 1rem;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 55%, transparent);
  }

  .liquid-plan-card-active {
    border-color: color-mix(in srgb, var(--color-primary-400) 40%, transparent);
  }

  .liquid-plan-card-btn {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .liquid-plan-card-media {
    position: relative;
    display: block;
    aspect-ratio: 4 / 3;
    overflow: hidden;
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
  }

  .liquid-plan-card-media img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .liquid-plan-card-badge {
    position: absolute;
    top: 0.55rem;
    left: 0.55rem;
    padding: 0.2rem 0.55rem;
    border-radius: 999px;
    font-size: 0.65rem;
    font-weight: 600;
    color: rgb(var(--color-surface-900));
    background: color-mix(in srgb, var(--color-surface-50) 92%, transparent);
  }

  .liquid-plan-card-emoji-hero {
    display: flex;
    align-items: center;
    justify-content: center;
    aspect-ratio: 4 / 3;
    font-size: 2.5rem;
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
  }

  .liquid-plan-card-text {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.7rem 0.75rem 0.8rem;
  }

  .liquid-plan-card-title {
    font-size: 0.9rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-50));
    line-height: 1.25;
  }

  .liquid-plan-card-sub {
    font-size: 0.72rem;
    color: rgb(var(--color-surface-400));
  }

  .liquid-plan-card-body {
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-300));
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .liquid-plan-card-time {
    margin-top: 0.2rem;
    font-size: 0.72rem;
    font-weight: 650;
    color: rgb(var(--color-surface-200));
  }
</style>
