<script lang="ts">
  /**
   * `compare` organism — table-driven judgment: matrix or 2-up face-off.
   * Paste-first from ```compare markdown.
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
  const mode = $derived.by((): "matrix" | "faceoff" => {
    const raw = typeof node.props.mode === "string" ? node.props.mode.trim().toLowerCase() : "";
    if (raw === "faceoff" || raw === "face-off") return "faceoff";
    return "matrix";
  });
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

  const presentation = $derived(
    mode === "faceoff" && entities.length === 2 ? "faceoff" : "matrix",
  );

  function isRecommended(entity: CompareEntity): boolean {
    if (!recommendation) return false;
    const label = entity.label.trim().toLowerCase();
    const rec = recommendation.toLowerCase();
    // Exact or containment so "Sonnet 5" matches "Claude Sonnet 5".
    return label === rec || label.includes(rec) || rec.includes(label);
  }

  function selectEntity(entity: CompareEntity) {
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { entityId: entity.id, label: entity.label }),
    );
  }
</script>

{#if entities.length >= 2 && axes.length >= 1}
  <div
    class="liquid-compare"
    class:liquid-compare--faceoff={presentation === "faceoff"}
    role="group"
    aria-label={title || "Comparison"}
  >
    {#if title || subtitle || recommendation}
      <header class="liquid-compare-header">
        {#if title}
          <h3 class="liquid-compare-title">{title}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-compare-subtitle">{subtitle}</p>
        {/if}
        {#if recommendation}
          <p class="liquid-compare-rec-banner">
            Recommended · <strong>{recommendation}</strong>
          </p>
        {/if}
      </header>
    {/if}

    {#if presentation === "faceoff"}
      <div class="liquid-compare-faceoff">
        {#each entities as entity (entity.id)}
          {@const rec = isRecommended(entity)}
          <button
            type="button"
            class="liquid-compare-card"
            class:liquid-compare-card--rec={rec}
            onclick={() => selectEntity(entity)}
          >
            <div class="liquid-compare-card__head">
              <span class="liquid-compare-card__label">{entity.label}</span>
              {#if rec}
                <span class="liquid-compare-card__badge">Recommended</span>
              {/if}
            </div>
            <ul class="liquid-compare-card__points">
              {#each axes as axis (axis.id)}
                <li class="liquid-compare-card__point">
                  <span class="liquid-compare-card__axis">{axis.label}</span>
                  <span class="liquid-compare-card__value"
                    >{entity.values[axis.id] ?? "—"}</span
                  >
                </li>
              {/each}
            </ul>
            {#if rec}
              <div class="liquid-compare-card__footer">Recommended</div>
            {/if}
          </button>
        {/each}
      </div>
    {:else}
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
    {/if}
  </div>
{/if}

<style>
  /* Tokens are RGB channels — use rgb(var(--token) / a), not bare color-mix. */
  .liquid-compare {
    margin: 0;
    padding: 1rem 1.05rem 1.1rem;
    border-radius: 0.95rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.34);
    background:
      linear-gradient(
        165deg,
        rgb(var(--color-surface-800) / 0.72) 0%,
        rgb(var(--color-surface-900) / 0.88) 55%,
        rgb(var(--color-surface-950) / 0.7) 100%
      );
    box-shadow:
      inset 0 1px 0 rgb(var(--color-surface-50) / 0.07),
      0 12px 32px rgb(0 0 0 / 0.28);
    min-width: 0;
  }

  .liquid-compare-header {
    margin-bottom: 0.9rem;
  }

  .liquid-compare-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 650;
    letter-spacing: -0.015em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-compare-subtitle {
    margin: 0.3rem 0 0;
    font-size: 0.78rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-400));
  }

  .liquid-compare-rec-banner {
    display: inline-block;
    margin: 0.6rem 0 0;
    padding: 0.22rem 0.55rem;
    border-radius: 999px;
    font-size: 0.68rem;
    letter-spacing: 0.02em;
    color: rgb(var(--color-primary-100));
    background: rgb(var(--color-primary-500) / 0.16);
    border: 1px solid rgb(var(--color-primary-400) / 0.35);
  }

  .liquid-compare-rec-banner strong {
    color: rgb(var(--color-primary-50, var(--color-surface-50)));
    font-weight: 650;
  }

  /* —— Matrix —— */
  .liquid-compare-scroll {
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x pinch-zoom;
    overscroll-behavior-x: contain;
    margin: 0;
    padding: 0;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    background: rgb(var(--color-surface-950) / 0.45);
  }

  .liquid-compare-table {
    width: max-content;
    min-width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    font-size: 0.8rem;
  }

  .liquid-compare-corner,
  .liquid-compare-axis {
    position: sticky;
    left: 0;
    z-index: 2;
    background: rgb(var(--color-surface-900) / 0.96);
    backdrop-filter: blur(8px);
  }

  .liquid-compare-corner {
    min-width: 5.75rem;
    width: 5.75rem;
    padding: 0.6rem 0.7rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-800) / 0.95);
  }

  .liquid-compare-axis {
    padding: 0.7rem 0.7rem;
    text-align: left;
    font-size: 0.68rem;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-400));
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.14);
    border-right: 1px solid rgb(var(--color-surface-500) / 0.12);
    white-space: nowrap;
  }

  .liquid-compare-entity {
    padding: 0.5rem 0.45rem 0.55rem;
    text-align: left;
    vertical-align: bottom;
    min-width: 8rem;
    max-width: 12rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-800) / 0.72);
  }

  .liquid-compare-entity-rec {
    background: rgb(var(--color-primary-500) / 0.2);
    box-shadow: inset 0 -2px 0 rgb(var(--color-primary-400) / 0.75);
  }

  .liquid-compare-entity-btn {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.2rem;
    width: 100%;
    padding: 0.3rem 0.4rem;
    margin: 0;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }

  .liquid-compare-entity-btn:hover {
    background: rgb(var(--color-surface-50) / 0.06);
  }

  .liquid-compare-entity-label {
    font-weight: 650;
    font-size: 0.84rem;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-50));
    line-height: 1.25;
  }

  .liquid-compare-rec-whisper {
    font-size: 0.58rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(var(--color-primary-200));
  }

  .liquid-compare-cell {
    padding: 0.7rem 0.8rem;
    color: rgb(var(--color-surface-200));
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.12);
    vertical-align: top;
    line-height: 1.45;
  }

  tbody tr:nth-child(even) .liquid-compare-cell:not(.liquid-compare-cell-rec) {
    background: rgb(var(--color-surface-800) / 0.22);
  }

  .liquid-compare-cell-rec {
    background: rgb(var(--color-primary-500) / 0.1);
  }

  tbody tr:nth-child(even) .liquid-compare-cell-rec {
    background: rgb(var(--color-primary-500) / 0.14);
  }

  tbody tr:last-child .liquid-compare-axis,
  tbody tr:last-child .liquid-compare-cell {
    border-bottom: 0;
  }

  /* —— Face-off —— */
  .liquid-compare-faceoff {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.8rem;
  }

  .liquid-compare-card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.8rem;
    background:
      linear-gradient(
        180deg,
        rgb(var(--color-surface-800) / 0.55) 0%,
        rgb(var(--color-surface-950) / 0.65) 100%
      );
    box-shadow: inset 0 1px 0 rgb(var(--color-surface-50) / 0.05);
    color: inherit;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    transition:
      border-color 140ms ease,
      background-color 140ms ease,
      transform 140ms ease;
  }

  .liquid-compare-card:hover {
    border-color: rgb(var(--color-surface-400) / 0.42);
    background:
      linear-gradient(
        180deg,
        rgb(var(--color-surface-700) / 0.45) 0%,
        rgb(var(--color-surface-900) / 0.7) 100%
      );
  }

  .liquid-compare-card--rec {
    border-color: rgb(var(--color-primary-400) / 0.5);
    background:
      linear-gradient(
        180deg,
        rgb(var(--color-primary-500) / 0.18) 0%,
        rgb(var(--color-surface-950) / 0.7) 100%
      );
    box-shadow:
      inset 0 1px 0 rgb(var(--color-primary-200) / 0.12),
      0 0 0 1px rgb(var(--color-primary-500) / 0.12);
  }

  .liquid-compare-card__head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.4rem 0.55rem;
    padding: 0.9rem 0.95rem 0.7rem;
  }

  .liquid-compare-card__label {
    font-size: 0.95rem;
    font-weight: 650;
    letter-spacing: -0.015em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-compare-card__badge {
    font-size: 0.58rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 0.14rem 0.4rem;
    border-radius: 999px;
    color: rgb(var(--color-primary-100));
    background: rgb(var(--color-primary-500) / 0.22);
    border: 1px solid rgb(var(--color-primary-400) / 0.35);
  }

  .liquid-compare-card__points {
    list-style: none;
    margin: 0;
    padding: 0.1rem 0.7rem 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    flex: 1;
  }

  .liquid-compare-card__point {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.5rem 0.6rem;
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-950) / 0.45);
    border: 1px solid rgb(var(--color-surface-500) / 0.16);
  }

  .liquid-compare-card__axis {
    font-size: 0.62rem;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-400));
  }

  .liquid-compare-card__value {
    font-size: 0.82rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-100));
  }

  .liquid-compare-card__footer {
    margin-top: auto;
    padding: 0.55rem 0.9rem;
    font-size: 0.68rem;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    text-align: center;
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-primary-500) / 0.55);
    border-top: 1px solid rgb(var(--color-primary-300) / 0.25);
  }

  @media (max-width: 640px) {
    .liquid-compare-faceoff {
      grid-template-columns: 1fr;
    }
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
