<script lang="ts">
  import type { ChartViewModel } from "./chartModel";
  import {
    chartSeriesColor,
    formatChartNumber,
    hasActiveHighlight,
    isActiveKey,
    yMax,
  } from "./chartModel";
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
  let hoverSeriesKey = $state<string | null>(null);
  let hoverCategory = $state<string | null>(null);

  const size = 280;
  const cx = size / 2;
  const cy = size / 2;
  const radius = 92;
  const rings = [0.25, 0.5, 0.75, 1];

  const n = $derived(model.categories.length);
  const maxVal = $derived(yMax({ ...model, stacked: false }));
  const highlight = $derived(hasActiveHighlight(model.activeKey));
  const showDots = $derived(model.labels !== "none");

  function angleAt(i: number): number {
    return (Math.PI * 2 * i) / n - Math.PI / 2;
  }

  function point(i: number, r: number): [number, number] {
    const a = angleAt(i);
    return [cx + Math.cos(a) * r, cy + Math.sin(a) * r];
  }

  const gridPolygons = $derived(
    rings.map((t) => {
      const pts = model.categories.map((_, i) => point(i, radius * t));
      return pts.map(([x, y]) => `${x},${y}`).join(" ");
    }),
  );

  const axisLines = $derived(
    model.categories.map((_, i) => {
      const [x, y] = point(i, radius);
      return { x1: cx, y1: cy, x2: x, y2: y, label: model.categories[i] };
    }),
  );

  const axisLabels = $derived(
    model.categories.map((label, i) => {
      const [x, y] = point(i, radius + 18);
      return { label, x, y };
    }),
  );

  const polygons = $derived(
    model.series.map((s, si) => {
      const pts = model.categories.map((_, i) => {
        const v = s.values[i] ?? 0;
        const r = (v / maxVal) * radius;
        const [x, y] = point(i, r);
        return { x, y, category: model.categories[i], value: v };
      });
      const active =
        !highlight || isActiveKey(model.activeKey, { key: s.key, label: s.label });
      return {
        key: s.key,
        label: s.label,
        color: chartSeriesColor(si, model.colors),
        points: pts.map((p) => `${p.x},${p.y}`).join(" "),
        vertices: pts,
        active,
      };
    }),
  );

  function tipForVertex(category: string) {
    return model.series.map((s, si) => {
      const idx = model.categories.indexOf(category);
      return {
        label: s.label,
        value: formatChartNumber(s.values[idx] ?? 0),
        color: chartSeriesColor(si, model.colors),
      };
    });
  }

  function seriesDimmed(active: boolean, key: string): boolean {
    if (model.interactive !== false && hoverSeriesKey) return key !== hoverSeriesKey;
    return !active;
  }

  function onVertex(event: MouseEvent, category: string, seriesKey: string) {
    if (model.interactive !== false) {
      hoverSeriesKey = seriesKey;
      hoverCategory = category;
    }
    if (!model.tooltip) return;
    const host = (event.currentTarget as SVGCircleElement).ownerSVGElement?.parentElement;
    if (!host) return;
    const box = host.getBoundingClientRect();
    tipVisible = true;
    tipX = event.clientX - box.left;
    tipY = event.clientY - box.top;
    tipTitle = category;
    tipLines = tipForVertex(category);
  }

  function onVertexLeave() {
    hoverSeriesKey = null;
    hoverCategory = null;
    tipVisible = false;
  }
</script>

{#if n < 3}
  <p class="liquid-chart-stub">Radar needs at least 3 axes (categories).</p>
{:else}
  <div class="liquid-chart-radar-wrap liquid-chart-mount">
    <svg class="liquid-chart-radar" viewBox={`0 0 ${size} ${size}`} role="presentation">
      {#each gridPolygons as poly, i (i)}
        <polygon class="liquid-chart-radar-grid" points={poly} />
      {/each}
      {#each axisLines as axis (axis.label)}
        <line
          class="liquid-chart-radar-axis"
          x1={axis.x1}
          y1={axis.y1}
          x2={axis.x2}
          y2={axis.y2}
        />
      {/each}
      {#each axisLabels as lbl (lbl.label)}
        <text
          class="liquid-chart-radar-label"
          x={lbl.x}
          y={lbl.y}
          text-anchor="middle"
          dominant-baseline="middle">{lbl.label}</text
        >
      {/each}

      {#each polygons as series (series.key)}
        {@const dimmed = seriesDimmed(series.active, series.key)}
        <polygon
          class="liquid-chart-radar-fill"
          class:liquid-chart-dim={dimmed}
          points={series.points}
          fill={series.color}
          stroke={series.color}
        />
        {#each series.vertices as v, vi (series.key + vi)}
          <circle
            class="liquid-chart-radar-dot"
            class:liquid-chart-dot-soft={!showDots}
            class:liquid-chart-dim={dimmed}
            class:liquid-chart-dot-hot={hoverSeriesKey === series.key && hoverCategory === v.category}
            role="img"
            aria-label={`${v.category}: ${series.label} ${v.value}`}
            cx={v.x}
            cy={v.y}
            r={hoverSeriesKey === series.key && hoverCategory === v.category
              ? 5
              : showDots
                ? 3.25
                : 2.5}
            fill={series.color}
            onmouseenter={(event) => onVertex(event, v.category, series.key)}
            onmouseleave={onVertexLeave}
          />
        {/each}
      {/each}
    </svg>
    <ChartTooltip visible={tipVisible} x={tipX} y={tipY} title={tipTitle} lines={tipLines} />
  </div>
{/if}

<style>
  .liquid-chart-stub {
    margin: 0;
    padding: 2rem 0.75rem;
    text-align: center;
    font-size: 0.8rem;
    color: rgb(var(--color-surface-500));
  }

  .liquid-chart-radar-wrap {
    position: relative;
    display: grid;
    place-items: center;
    width: 100%;
    min-height: 16rem;
  }

  .liquid-chart-radar {
    width: min(100%, 18rem);
    height: auto;
    overflow: visible;
  }

  .liquid-chart-radar-grid {
    fill: none;
    stroke: color-mix(in srgb, var(--color-surface-500) 32%, transparent);
    stroke-width: 1;
  }

  .liquid-chart-radar-axis {
    stroke: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    stroke-width: 1;
  }

  .liquid-chart-radar-label {
    fill: rgb(var(--color-surface-500));
    font-size: 0.62rem;
  }

  .liquid-chart-radar-fill {
    fill-opacity: 0.18;
    stroke-width: 2;
    stroke-linejoin: round;
    transition: opacity 160ms ease;
  }

  .liquid-chart-radar-dot {
    stroke: color-mix(in srgb, var(--color-surface-50) 70%, transparent);
    stroke-width: 1;
    transition:
      opacity 160ms ease,
      r 160ms ease;
  }

  .liquid-chart-dot-soft {
    opacity: 0.85;
  }

  .liquid-chart-dim {
    opacity: 0.38;
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
    .liquid-chart-radar-fill,
    .liquid-chart-radar-dot {
      transition: none;
    }

    .liquid-chart-mount {
      animation: none;
    }
  }
</style>
