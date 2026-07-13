<script lang="ts">
  import type { ChartViewModel } from "./chartModel";
  import { formatChartNumber, resolveChartColor } from "./chartModel";
  import ChartTooltip from "./ChartTooltip.svelte";

  interface Props {
    model: ChartViewModel;
  }

  let { model }: Props = $props();

  let tipVisible = $state(false);
  let tipX = $state(0);
  let tipY = $state(0);
  let tipTitle = $state("");
  let tipLines = $state<{ label: string; value: string; color?: string }[]>([]);
  let hoverKey = $state<string | null>(null);

  const matrix = $derived(model.matrix);
  const baseColor = $derived(resolveChartColor(model.colors[0] ?? "blue", 0));

  const extent = $derived.by(() => {
    if (!matrix) return { min: 0, max: 1 };
    let min = Infinity;
    let max = -Infinity;
    for (const row of matrix.values) {
      for (const v of row) {
        if (v < min) min = v;
        if (v > max) max = v;
      }
    }
    if (!Number.isFinite(min) || !Number.isFinite(max)) return { min: 0, max: 1 };
    if (min === max) return { min, max: max + 1 };
    return { min, max };
  });

  const cells = $derived.by(() => {
    if (!matrix) return [];
    const out: {
      key: string;
      row: string;
      col: string;
      value: number;
      ri: number;
      ci: number;
      alpha: number;
    }[] = [];
    const span = extent.max - extent.min || 1;
    for (let ri = 0; ri < matrix.rows.length; ri++) {
      for (let ci = 0; ci < matrix.cols.length; ci++) {
        const value = matrix.values[ri]?.[ci] ?? 0;
        const t = (value - extent.min) / span;
        out.push({
          key: `${ri}-${ci}`,
          row: matrix.rows[ri],
          col: matrix.cols[ci],
          value,
          ri,
          ci,
          alpha: 0.18 + t * 0.78,
        });
      }
    }
    return out;
  });

  const colCount = $derived(matrix?.cols.length ?? 1);
  const rowCount = $derived(matrix?.rows.length ?? 1);

  function onEnter(
    event: MouseEvent,
    cell: (typeof cells)[number],
  ) {
    if (model.interactive !== false) hoverKey = cell.key;
    if (!model.tooltip) return;
    const host = (event.currentTarget as HTMLElement).closest(".liquid-chart-heatmap-wrap");
    if (!host) return;
    const box = host.getBoundingClientRect();
    tipVisible = true;
    tipX = event.clientX - box.left;
    tipY = event.clientY - box.top;
    tipTitle = `${cell.row} · ${cell.col}`;
    tipLines = [{ label: "Value", value: formatChartNumber(cell.value), color: baseColor }];
  }

  function onLeave() {
    hoverKey = null;
    tipVisible = false;
  }
</script>

{#if matrix}
  <div
    class="liquid-chart-heatmap-wrap liquid-chart-mount"
    class:liquid-chart-heatmap-surface={Boolean(model.surface)}
    style:--heatmap-cols={colCount}
    style:--heatmap-rows={rowCount}
    style:--heatmap-base={baseColor}
    style:--chart-plot={model.surface || undefined}
  >
    <div class="liquid-chart-heatmap-grid" role="table" aria-label={model.title || "Heatmap"}>
      <div class="liquid-chart-heatmap-corner" aria-hidden="true"></div>
      {#each matrix.cols as col, ci (col + ci)}
        <div class="liquid-chart-heatmap-col-label" role="columnheader">{col}</div>
      {/each}
      {#each matrix.rows as row, ri (row + ri)}
        <div class="liquid-chart-heatmap-row-label" role="rowheader">{row}</div>
        {#each matrix.cols as col, ci (col + ci)}
          {@const cell = cells.find((c) => c.ri === ri && c.ci === ci)}
          {#if cell}
            <button
              type="button"
              class="liquid-chart-heatmap-cell"
              class:liquid-chart-heatmap-hot={hoverKey === cell.key}
              style:--cell-alpha={cell.alpha}
              aria-label={`${cell.row}, ${cell.col}: ${cell.value}`}
              onmouseenter={(event) => onEnter(event, cell)}
              onmouseleave={onLeave}
              onfocus={(event) => onEnter(event as unknown as MouseEvent, cell)}
              onblur={onLeave}
            ></button>
          {/if}
        {/each}
      {/each}
    </div>
    {#if model.tooltip}
      <ChartTooltip visible={tipVisible} x={tipX} y={tipY} title={tipTitle} lines={tipLines} />
    {/if}
  </div>
{/if}

<style>
  .liquid-chart-heatmap-wrap {
    position: relative;
    width: 100%;
    min-height: 8rem;
    padding: 0.15rem 0.1rem 0.25rem;
  }

  .liquid-chart-heatmap-surface {
    border-radius: 0.65rem;
    background: var(--chart-plot, transparent);
    padding: 0.45rem 0.5rem 0.55rem;
  }

  .liquid-chart-heatmap-grid {
    display: grid;
    grid-template-columns: minmax(3.5rem, max-content) repeat(var(--heatmap-cols, 1), minmax(0, 1fr));
    gap: 0.28rem;
    align-items: stretch;
  }

  .liquid-chart-heatmap-corner {
    min-height: 1rem;
  }

  .liquid-chart-heatmap-col-label,
  .liquid-chart-heatmap-row-label {
    font-size: 0.6875rem;
    font-weight: 600;
    line-height: 1.2;
    color: rgb(var(--chart-fg-muted));
  }

  .liquid-chart-heatmap-col-label {
    text-align: center;
    padding: 0 0.15rem 0.2rem;
  }

  .liquid-chart-heatmap-row-label {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding-right: 0.35rem;
    text-align: right;
  }

  .liquid-chart-heatmap-cell {
    appearance: none;
    border: none;
    margin: 0;
    min-height: 1.85rem;
    border-radius: 0.45rem;
    background: color-mix(
      in srgb,
      var(--heatmap-base) calc(var(--cell-alpha) * 100%),
      transparent
    );
    cursor: default;
    transition:
      filter 160ms ease,
      transform 160ms ease;
  }

  .liquid-chart-heatmap-hot {
    filter: brightness(1.08);
    transform: scale(1.02);
  }

  .liquid-chart-mount {
    animation: liquid-chart-mount 260ms ease-out both;
  }

  @keyframes liquid-chart-mount {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-chart-heatmap-cell {
      transition: none;
    }

    .liquid-chart-mount {
      animation: none;
    }
  }
</style>
