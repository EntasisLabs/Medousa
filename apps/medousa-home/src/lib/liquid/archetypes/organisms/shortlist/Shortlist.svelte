<script lang="ts">
  /**
   * `shortlist` organism — ranked find-me / options list.
   * Paste-first from ```shortlist markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import LiquidGlyph from "$lib/liquid/icons/LiquidGlyph.svelte";
  import { isHttpUrl } from "$lib/utils/openInBrowser";

  interface ShortlistItem {
    id: string;
    label: string;
    summary?: string;
    score?: string;
    meta?: string;
    emoji?: string;
    icon?: string;
    image?: string;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const criteria = $derived(typeof node.props.criteria === "string" ? node.props.criteria.trim() : "");
  const density = $derived(
    typeof node.props.density === "string" && node.props.density.trim().toLowerCase() === "compact"
      ? "compact"
      : "comfortable",
  );

  const items = $derived.by((): ShortlistItem[] => {
    const raw = node.props.items;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        if (!label) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `item-${i}`;
        const out: ShortlistItem = { id, label };
        if (typeof row.summary === "string" && row.summary.trim()) out.summary = row.summary.trim();
        if (typeof row.score === "string" && row.score.trim()) out.score = row.score.trim();
        if (typeof row.meta === "string" && row.meta.trim()) out.meta = row.meta.trim();
        if (typeof row.emoji === "string" && row.emoji.trim()) out.emoji = row.emoji.trim();
        if (typeof row.icon === "string" && row.icon.trim()) out.icon = row.icon.trim();
        if (typeof row.image === "string" && row.image.trim()) out.image = row.image.trim();
        return out;
      })
      .filter((x): x is ShortlistItem => x !== null);
  });

  const criteriaParts = $derived(
    criteria
      ? criteria
          .split(/[·|,/]/)
          .map((p) => p.trim())
          .filter(Boolean)
      : [],
  );

  function rankLabel(index: number): string {
    return String(index + 1).padStart(2, "0");
  }

  function selectItem(item: ShortlistItem, rank: number) {
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { itemId: item.id, label: item.label, rank }),
    );
  }
</script>

{#if items.length >= 2}
  <div
    class="liquid-shortlist"
    class:liquid-shortlist-compact={density === "compact"}
    role="list"
    aria-label={title || "Shortlist"}
  >
    {#if title || subtitle || criteria}
      <header class="liquid-shortlist-header">
        {#if title}
          <h3 class="liquid-shortlist-title">{title}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-shortlist-subtitle">{subtitle}</p>
        {/if}
        {#if criteriaParts.length}
          <div class="liquid-shortlist-criteria">
            {#each criteriaParts as part, i (i)}
              <span class="liquid-shortlist-criterion">{part}</span>
            {/each}
          </div>
        {:else if criteria}
          <p class="liquid-shortlist-criteria-plain">{criteria}</p>
        {/if}
      </header>
    {/if}

    <ol class="liquid-shortlist-rows">
      {#each items as item, i (item.id)}
        <li class="liquid-shortlist-row" role="listitem">
          <button
            type="button"
            class="liquid-shortlist-btn"
            onclick={() => selectItem(item, i + 1)}
          >
            <span class="liquid-shortlist-rank" aria-hidden="true">{rankLabel(i)}</span>
            {#if item.image && isHttpUrl(item.image)}
              <img class="liquid-shortlist-thumb" src={item.image} alt="" loading="lazy" />
            {:else if item.emoji || item.icon}
              <span class="liquid-shortlist-emoji" aria-hidden="true">
                <LiquidGlyph icon={item.icon} emoji={item.emoji} size={16} />
              </span>
            {/if}
            <span class="liquid-shortlist-main">
              <span class="liquid-shortlist-label">{item.label}</span>
              {#if item.summary}
                <span class="liquid-shortlist-summary">{item.summary}</span>
              {/if}
              {#if item.meta}
                <span class="liquid-shortlist-meta">{item.meta}</span>
              {/if}
            </span>
            {#if item.score}
              <span class="liquid-shortlist-score">{item.score}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ol>
  </div>
{/if}

<style>
  .liquid-shortlist {
    margin: 0;
    padding: 0.85rem 0.9rem 0.95rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    min-width: 0;
  }

  .liquid-shortlist-header {
    margin-bottom: 0.75rem;
  }

  .liquid-shortlist-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-shortlist-subtitle {
    margin: 0.35rem 0 0;
    font-size: 0.8rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .liquid-shortlist-criteria {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-top: 0.5rem;
  }

  .liquid-shortlist-criterion {
    font-size: 0.6rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    color: rgb(var(--color-surface-300));
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-surface-800) 55%, transparent);
  }

  .liquid-shortlist-criteria-plain {
    margin: 0.45rem 0 0;
    font-size: 0.68rem;
    color: rgb(var(--color-surface-500));
  }

  .liquid-shortlist-rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .liquid-shortlist-compact .liquid-shortlist-rows {
    gap: 0.25rem;
  }

  .liquid-shortlist-btn {
    display: grid;
    grid-template-columns: 1.75rem auto minmax(0, 1fr) auto;
    align-items: start;
    gap: 0.55rem;
    width: 100%;
    margin: 0;
    padding: 0.55rem 0.5rem;
    border: 0;
    border-radius: 0.65rem;
    background: color-mix(in srgb, var(--color-surface-950) 35%, transparent);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .liquid-shortlist-compact .liquid-shortlist-btn {
    padding: 0.4rem 0.4rem;
    gap: 0.4rem;
  }

  .liquid-shortlist-btn:hover {
    background: color-mix(in srgb, var(--color-surface-50) 5%, transparent);
  }

  .liquid-shortlist-rank {
    font-size: 0.72rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-surface-500));
    padding-top: 0.15rem;
  }

  .liquid-shortlist-thumb {
    width: 2.4rem;
    height: 2.4rem;
    border-radius: 0.45rem;
    object-fit: cover;
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
  }

  .liquid-shortlist-compact .liquid-shortlist-thumb {
    width: 1.9rem;
    height: 1.9rem;
  }

  .liquid-shortlist-emoji {
    font-size: 1.15rem;
    line-height: 1;
    padding-top: 0.1rem;
  }

  .liquid-shortlist-main {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
  }

  .liquid-shortlist-label {
    font-size: 0.9rem;
    font-weight: 650;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-50));
    line-height: 1.25;
  }

  .liquid-shortlist-summary {
    font-size: 0.75rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-300));
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .liquid-shortlist-compact .liquid-shortlist-summary {
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .liquid-shortlist-meta {
    font-size: 0.65rem;
    color: rgb(var(--color-surface-500));
  }

  .liquid-shortlist-score {
    font-size: 0.85rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-primary-300));
    padding-top: 0.1rem;
    white-space: nowrap;
  }

  /* When no thumb/emoji, collapse the auto column */
  .liquid-shortlist-btn:not(:has(.liquid-shortlist-thumb)):not(:has(.liquid-shortlist-emoji)) {
    grid-template-columns: 1.75rem minmax(0, 1fr) auto;
  }
</style>
