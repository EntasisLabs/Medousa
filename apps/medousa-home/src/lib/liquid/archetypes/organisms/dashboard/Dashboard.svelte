<script lang="ts">
  /**
   * `dashboard` organism — paste-first metric/status tile grid.
   * Live feed/work bindings deferred. From ```dashboard markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  type TileTone = "default" | "accent" | "success" | "warn" | "error";
  const TONES: TileTone[] = ["default", "accent", "success", "warn", "error"];

  interface DashboardTile {
    id: string;
    label: string;
    value: string;
    delta?: string;
    tone: TileTone;
    emoji?: string;
    hint?: string;
    unit?: string;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const columns = $derived.by(() => {
    const raw = typeof node.props.columns === "string" ? node.props.columns.trim() : "";
    if (raw === "3" || raw === "4") return raw;
    return "2";
  });

  const tiles = $derived.by((): DashboardTile[] => {
    const raw = node.props.tiles;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label.trim() : "";
        const value = typeof row.value === "string" ? row.value.trim() : "";
        if (!label || !value) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `tile-${i}`;
        const toneRaw = typeof row.tone === "string" ? row.tone.trim().toLowerCase() : "default";
        const tone: TileTone = TONES.includes(toneRaw as TileTone) ? (toneRaw as TileTone) : "default";
        const tile: DashboardTile = { id, label, value, tone };
        if (typeof row.delta === "string" && row.delta.trim()) tile.delta = row.delta.trim();
        if (typeof row.emoji === "string" && row.emoji.trim()) tile.emoji = row.emoji.trim();
        if (typeof row.hint === "string" && row.hint.trim()) tile.hint = row.hint.trim();
        if (typeof row.unit === "string" && row.unit.trim()) tile.unit = row.unit.trim();
        return tile;
      })
      .filter((t): t is DashboardTile => t !== null);
  });

  function selectTile(tile: DashboardTile) {
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { tileId: tile.id, label: tile.label }),
    );
  }
</script>

{#if tiles.length >= 2}
  <div
    class="liquid-dashboard"
    role="list"
    aria-label={title || "Dashboard"}
    style={`--dash-cols: ${columns}`}
  >
    {#if title || subtitle}
      <header class="liquid-dashboard-header">
        {#if title}
          <h3 class="liquid-dashboard-title">{title}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-dashboard-subtitle">{subtitle}</p>
        {/if}
      </header>
    {/if}

    <div class="liquid-dashboard-grid">
      {#each tiles as tile (tile.id)}
        <div class="liquid-dashboard-tile-wrap" role="listitem">
          <button
            type="button"
            class="liquid-dashboard-tile"
            data-tone={tile.tone}
            onclick={() => selectTile(tile)}
          >
            <span class="liquid-dashboard-tile-top">
              {#if tile.emoji}
                <span class="liquid-dashboard-emoji" aria-hidden="true">{tile.emoji}</span>
              {/if}
              <span class="liquid-dashboard-label">{tile.label}</span>
            </span>
            <span class="liquid-dashboard-value-row">
              <span class="liquid-dashboard-value">{tile.value}</span>
              {#if tile.unit}
                <span class="liquid-dashboard-unit">{tile.unit}</span>
              {/if}
            </span>
            {#if tile.delta}
              <span class="liquid-dashboard-delta">{tile.delta}</span>
            {/if}
            {#if tile.hint}
              <span class="liquid-dashboard-hint">{tile.hint}</span>
            {/if}
          </button>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .liquid-dashboard {
    margin: 0;
    padding: 0.85rem 0.9rem 0.95rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    min-width: 0;
  }

  .liquid-dashboard-header {
    margin-bottom: 0.75rem;
  }

  .liquid-dashboard-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-dashboard-subtitle {
    margin: 0.35rem 0 0;
    font-size: 0.8rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .liquid-dashboard-grid {
    display: grid;
    grid-template-columns: repeat(var(--dash-cols, 2), minmax(0, 1fr));
    gap: 0.55rem;
  }

  @media (max-width: 420px) {
    .liquid-dashboard-grid {
      grid-template-columns: repeat(min(2, var(--dash-cols, 2)), minmax(0, 1fr));
    }
  }

  @media (max-width: 320px) {
    .liquid-dashboard-grid {
      grid-template-columns: 1fr;
    }
  }

  .liquid-dashboard-tile-wrap {
    min-width: 0;
  }

  .liquid-dashboard-tile {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.25rem;
    margin: 0;
    padding: 0.7rem 0.75rem;
    width: 100%;
    border-radius: 0.7rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 42%, transparent);
    color: inherit;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }

  .liquid-dashboard-tile:hover {
    background: color-mix(in srgb, var(--color-surface-50) 5%, transparent);
  }

  .liquid-dashboard-tile[data-tone="accent"] {
    border-color: color-mix(in srgb, var(--color-primary-500) 40%, transparent);
  }

  .liquid-dashboard-tile[data-tone="success"] {
    border-color: color-mix(in srgb, var(--color-success-500) 40%, transparent);
  }

  .liquid-dashboard-tile[data-tone="warn"] {
    border-color: color-mix(in srgb, var(--color-warning-500) 40%, transparent);
  }

  .liquid-dashboard-tile[data-tone="error"] {
    border-color: color-mix(in srgb, var(--color-error-500) 40%, transparent);
  }

  .liquid-dashboard-tile-top {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
    width: 100%;
  }

  .liquid-dashboard-emoji {
    font-size: 0.9rem;
    line-height: 1;
    flex-shrink: 0;
  }

  .liquid-dashboard-label {
    font-size: 0.68rem;
    font-weight: 550;
    color: rgb(var(--color-surface-400));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .liquid-dashboard-value-row {
    display: flex;
    align-items: baseline;
    gap: 0.25rem;
    min-width: 0;
  }

  .liquid-dashboard-value {
    font-size: 1.35rem;
    font-weight: 700;
    letter-spacing: -0.03em;
    line-height: 1.15;
    color: rgb(var(--color-surface-50));
    font-variant-numeric: tabular-nums;
  }

  .liquid-dashboard-tile[data-tone="accent"] .liquid-dashboard-value {
    color: rgb(var(--color-primary-200));
  }

  .liquid-dashboard-tile[data-tone="success"] .liquid-dashboard-value {
    color: rgb(var(--color-success-200));
  }

  .liquid-dashboard-tile[data-tone="warn"] .liquid-dashboard-value {
    color: rgb(var(--color-warning-200));
  }

  .liquid-dashboard-tile[data-tone="error"] .liquid-dashboard-value {
    color: rgb(var(--color-error-200));
  }

  .liquid-dashboard-unit {
    font-size: 0.72rem;
    font-weight: 550;
    color: rgb(var(--color-surface-400));
  }

  .liquid-dashboard-delta {
    font-size: 0.68rem;
    font-weight: 550;
    color: rgb(var(--color-surface-300));
  }

  .liquid-dashboard-tile[data-tone="success"] .liquid-dashboard-delta {
    color: rgb(var(--color-success-300));
  }

  .liquid-dashboard-tile[data-tone="warn"] .liquid-dashboard-delta {
    color: rgb(var(--color-warning-300));
  }

  .liquid-dashboard-tile[data-tone="error"] .liquid-dashboard-delta {
    color: rgb(var(--color-error-300));
  }

  .liquid-dashboard-tile[data-tone="accent"] .liquid-dashboard-delta {
    color: rgb(var(--color-primary-300));
  }

  .liquid-dashboard-hint {
    margin-top: 0.15rem;
    font-size: 0.65rem;
    line-height: 1.35;
    color: rgb(var(--color-surface-500));
  }
</style>
