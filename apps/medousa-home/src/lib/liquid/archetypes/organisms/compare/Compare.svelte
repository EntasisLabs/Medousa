<script lang="ts">
  /**
   * `compare` organism — judgment matrix (entities as columns, axes as rows).
   * First sacred-seven response block. Paste-first from ```compare markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  interface CompareAxis {
    id: string;
    label: string;
  }

  interface CompareEntity {
    id: string;
    label: string;
    values: Record<string, string>;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const recommendation = $derived(
    typeof node.props.recommendation === "string" ? node.props.recommendation.trim() : "",
  );
  const axes = $derived.by((): CompareAxis[] => {
    const raw = node.props.axes;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label : "";
        if (!label) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `axis-${i}`;
        return { id, label };
      })
      .filter((a): a is CompareAxis => a !== null);
  });
  const entities = $derived.by((): CompareEntity[] => {
    const raw = node.props.entities;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const label = typeof row.label === "string" ? row.label : "";
        if (!label) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `entity-${i}`;
        const values: Record<string, string> = {};
        if (row.values && typeof row.values === "object" && !Array.isArray(row.values)) {
          for (const [k, v] of Object.entries(row.values as Record<string, unknown>)) {
            if (typeof v === "string") values[k] = v;
            else if (v != null) values[k] = String(v);
          }
        }
        return { id, label, values };
      })
      .filter((e): e is CompareEntity => e !== null);
  });

  function isRecommended(entity: CompareEntity): boolean {
    if (!recommendation) return false;
    return entity.label.trim().toLowerCase() === recommendation.toLowerCase();
  }

  function selectEntity(entity: CompareEntity) {
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { entityId: entity.id, label: entity.label }),
    );
  }
</script>

{#if entities.length >= 2 && axes.length >= 1}
  <div class="liquid-compare" role="group" aria-label={title || "Comparison"}>
    {#if title || subtitle}
      <header class="liquid-compare-header">
        {#if title}
          <h3 class="liquid-compare-title">{title}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-compare-subtitle">{subtitle}</p>
        {/if}
      </header>
    {/if}

    <div class="liquid-compare-scroll" data-no-tab-swipe>
      <table class="liquid-compare-table">
        <thead>
          <tr>
            <th class="liquid-compare-corner" scope="col">
              <span class="sr-only">Axis</span>
            </th>
            {#each entities as entity (entity.id)}
              <th
                class="liquid-compare-entity"
                class:liquid-compare-entity-rec={isRecommended(entity)}
                scope="col"
              >
                <button
                  type="button"
                  class="liquid-compare-entity-btn"
                  onclick={() => selectEntity(entity)}
                >
                  <span class="liquid-compare-entity-label">{entity.label}</span>
                  {#if isRecommended(entity)}
                    <span class="liquid-compare-rec-whisper">Recommended</span>
                  {/if}
                </button>
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each axes as axis (axis.id)}
            <tr>
              <th class="liquid-compare-axis" scope="row">{axis.label}</th>
              {#each entities as entity (entity.id)}
                <td
                  class="liquid-compare-cell"
                  class:liquid-compare-cell-rec={isRecommended(entity)}
                >
                  {entity.values[axis.id] ?? ""}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
{/if}

<style>
  .liquid-compare {
    margin: 0;
    padding: 0.85rem 0.9rem 0.95rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    min-width: 0;
  }

  .liquid-compare-header {
    margin-bottom: 0.7rem;
  }

  .liquid-compare-title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 650;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-compare-subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.75rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-400));
  }

  .liquid-compare-scroll {
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x pinch-zoom;
    overscroll-behavior-x: contain;
    margin: 0 -0.15rem;
    padding: 0 0.15rem;
  }

  .liquid-compare-table {
    width: max-content;
    min-width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    font-size: 0.78rem;
  }

  .liquid-compare-corner,
  .liquid-compare-axis {
    position: sticky;
    left: 0;
    z-index: 1;
    background: color-mix(in srgb, var(--color-surface-900) 92%, transparent);
    backdrop-filter: blur(6px);
  }

  .liquid-compare-corner {
    min-width: 5.5rem;
    width: 5.5rem;
    padding: 0.35rem 0.55rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
  }

  .liquid-compare-axis {
    padding: 0.55rem 0.55rem;
    text-align: left;
    font-weight: 550;
    color: rgb(var(--color-surface-300));
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 18%, transparent);
    white-space: nowrap;
  }

  .liquid-compare-entity {
    padding: 0.25rem 0.35rem 0.45rem;
    text-align: left;
    vertical-align: bottom;
    min-width: 7.5rem;
    max-width: 11rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
  }

  .liquid-compare-entity-rec {
    background: color-mix(in srgb, var(--color-primary-500) 8%, transparent);
  }

  .liquid-compare-entity-btn {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.15rem;
    width: 100%;
    padding: 0.2rem 0.35rem;
    margin: 0;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }

  .liquid-compare-entity-btn:hover {
    background: color-mix(in srgb, var(--color-surface-50) 5%, transparent);
  }

  .liquid-compare-entity-label {
    font-weight: 650;
    color: rgb(var(--color-surface-100));
    line-height: 1.25;
  }

  .liquid-compare-rec-whisper {
    font-size: 0.6rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-primary-300));
  }

  .liquid-compare-cell {
    padding: 0.55rem 0.7rem;
    color: rgb(var(--color-surface-200));
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 18%, transparent);
    vertical-align: top;
    line-height: 1.4;
  }

  .liquid-compare-cell-rec {
    background: color-mix(in srgb, var(--color-primary-500) 6%, transparent);
  }

  tbody tr:last-child .liquid-compare-axis,
  tbody tr:last-child .liquid-compare-cell {
    border-bottom: 0;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
