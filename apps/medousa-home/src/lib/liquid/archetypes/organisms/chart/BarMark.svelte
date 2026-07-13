<script lang="ts">
  import { getContext } from "svelte";
  import type { LiquidChartSeries } from "$lib/markdown/liquidEmbeds";
  import { chartSeriesColor } from "./chartModel";

  type Scale = ((v: unknown) => number) & {
    bandwidth?: () => number;
    domain?: () => unknown[];
  };

  interface CakeCustom {
    series: LiquidChartSeries[];
    colors: string[];
    stacked: boolean;
    horizontal: boolean;
    showTooltip: (
      x: number,
      y: number,
      title: string,
      lines: { label: string; value: string; color?: string }[],
    ) => void;
    hideTooltip: () => void;
  }

  const {
    data,
    xScale,
    yScale,
    width,
    height,
    custom,
  } = getContext<{
    data: import("svelte/store").Readable<Record<string, string | number>[]>;
    xScale: import("svelte/store").Readable<Scale>;
    yScale: import("svelte/store").Readable<Scale>;
    width: import("svelte/store").Readable<number>;
    height: import("svelte/store").Readable<number>;
    custom: import("svelte/store").Readable<CakeCustom>;
  }>("LayerCake");

  interface BarRect {
    key: string;
    x: number;
    y: number;
    width: number;
    height: number;
    color: string;
    category: string;
    label: string;
    value: number;
  }

  const bars = $derived.by((): BarRect[] => {
    const rows = $data ?? [];
    const xS = $xScale;
    const yS = $yScale;
    const cfg = $custom;
    const h = $height;
    if (!cfg || !rows.length || !xS || !yS) return [];
    const out: BarRect[] = [];
    const series = cfg.series;
    const n = Math.max(series.length, 1);

    for (const row of rows) {
      const category = String(row.category ?? "");
      if (cfg.horizontal) {
        const y = yS(category) ?? 0;
        const band = yS.bandwidth?.() ?? 12;
        if (cfg.stacked) {
          let cursor = 0;
          series.forEach((s, si) => {
            const value = Number(row[s.key] ?? 0);
            const x0 = xS(cursor) ?? 0;
            const x1 = xS(cursor + value) ?? 0;
            out.push({
              key: `${category}-${s.key}`,
              x: Math.min(x0, x1),
              y,
              width: Math.abs(x1 - x0),
              height: band,
              color: chartSeriesColor(si, cfg.colors),
              category,
              label: s.label,
              value,
            });
            cursor += value;
          });
        } else {
          const slot = band / n;
          series.forEach((s, si) => {
            const value = Number(row[s.key] ?? 0);
            const x0 = xS(0) ?? 0;
            const x1 = xS(value) ?? 0;
            out.push({
              key: `${category}-${s.key}`,
              x: Math.min(x0, x1),
              y: y + si * slot,
              width: Math.abs(x1 - x0),
              height: Math.max(slot * 0.9, 2),
              color: chartSeriesColor(si, cfg.colors),
              category,
              label: s.label,
              value,
            });
          });
        }
      } else {
        const x = xS(category) ?? 0;
        const band = xS.bandwidth?.() ?? 12;
        if (cfg.stacked) {
          let cursor = 0;
          series.forEach((s, si) => {
            const value = Number(row[s.key] ?? 0);
            const y0 = yS(cursor) ?? 0;
            const y1 = yS(cursor + value) ?? 0;
            out.push({
              key: `${category}-${s.key}`,
              x,
              y: Math.min(y0, y1),
              width: band,
              height: Math.abs(y1 - y0),
              color: chartSeriesColor(si, cfg.colors),
              category,
              label: s.label,
              value,
            });
            cursor += value;
          });
        } else {
          const slot = band / n;
          series.forEach((s, si) => {
            const value = Number(row[s.key] ?? 0);
            const y0 = yS(0) ?? h;
            const y1 = yS(value) ?? 0;
            out.push({
              key: `${category}-${s.key}`,
              x: x + si * slot,
              y: Math.min(y0, y1),
              width: Math.max(slot * 0.9, 2),
              height: Math.abs(y1 - y0),
              color: chartSeriesColor(si, cfg.colors),
              category,
              label: s.label,
              value,
            });
          });
        }
      }
    }
    return out;
  });

  const axis = $derived.by((): {
    w: number;
    h: number;
    grid: { x?: number; y?: number }[];
    xLabels: { label: string; x: number }[];
    yLabels: { label: string; y: number }[];
  } => {
    const rows = $data ?? [];
    const xS = $xScale;
    const yS = $yScale;
    const cfg = $custom;
    const w = $width;
    const h = $height;
    if (!cfg || !xS || !yS) {
      return { grid: [], xLabels: [], yLabels: [], w: 0, h: 0 };
    }
    if (cfg.horizontal) {
      const domain = (xS.domain?.() as number[]) ?? [0, 1];
      const max = Number(domain[1] ?? 1);
      return {
        w,
        h,
        grid: [0.25, 0.5, 0.75, 1].map((t) => ({ x: xS(max * t) ?? 0 })),
        xLabels: [0, 0.5, 1].map((t) => ({
          label: String(Math.round(max * t)),
          x: xS(max * t) ?? 0,
        })),
        yLabels: rows.map((row) => {
          const category = String(row.category ?? "");
          return {
            label: category,
            y: (yS(category) ?? 0) + ((yS.bandwidth?.() ?? 0) / 2),
          };
        }),
      };
    }
    const domain = (yS.domain?.() as number[]) ?? [0, 1];
    const max = Number(domain[1] ?? 1);
    return {
      w,
      h,
      grid: [0.25, 0.5, 0.75, 1].map((t) => ({ y: yS(max * t) ?? 0 })),
      xLabels: rows.map((row) => {
        const category = String(row.category ?? "");
        return {
          label: category,
          x: (xS(category) ?? 0) + ((xS.bandwidth?.() ?? 0) / 2),
        };
      }),
      yLabels: [0, 0.5, 1].map((t) => ({
        label: String(Math.round(max * t)),
        y: yS(max * t) ?? 0,
      })),
    };
  });

  function onEnter(bar: BarRect, event: MouseEvent) {
    const cfg = $custom;
    if (!cfg) return;
    const target = event.currentTarget as SVGRectElement;
    const box = target.getBoundingClientRect();
    const parent = target.ownerSVGElement?.parentElement?.getBoundingClientRect();
    if (!parent) return;
    cfg.showTooltip(
      box.left - parent.left + box.width / 2,
      box.top - parent.top,
      bar.category,
      [{ label: bar.label, value: String(bar.value), color: bar.color }],
    );
  }
</script>

{#if axis.w > 0 && axis.h > 0}
  <g class="liquid-chart-bars">
    {#if $custom?.horizontal}
      {#each axis.grid as g, i (i)}
        <line class="liquid-chart-grid" x1={g.x ?? 0} x2={g.x ?? 0} y1="0" y2={axis.h} />
      {/each}
    {:else}
      {#each axis.grid as g, i (i)}
        <line class="liquid-chart-grid" x1="0" x2={axis.w} y1={g.y ?? 0} y2={g.y ?? 0} />
      {/each}
    {/if}

    {#each axis.yLabels as tick (tick.label + tick.y)}
      <text class="liquid-chart-axis" x={-6} y={tick.y} text-anchor="end" dominant-baseline="middle"
        >{tick.label}</text
      >
    {/each}
    {#each axis.xLabels as tick (tick.label + tick.x)}
      <text class="liquid-chart-axis" x={tick.x} y={axis.h + 14} text-anchor="middle">{tick.label}</text>
    {/each}

    {#each bars as bar (bar.key)}
      <rect
        class="liquid-chart-bar"
        role="img"
        aria-label={`${bar.category}: ${bar.label} ${bar.value}`}
        x={bar.x}
        y={bar.y}
        width={bar.width}
        height={bar.height}
        fill={bar.color}
        rx="3"
        ry="3"
        onmouseenter={(event) => onEnter(bar, event)}
        onmouseleave={() => $custom?.hideTooltip()}
      />
    {/each}
  </g>
{/if}

<style>
  .liquid-chart-grid {
    stroke: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    stroke-width: 1;
  }

  .liquid-chart-axis {
    fill: rgb(var(--color-surface-400));
    font-size: 0.62rem;
  }

  .liquid-chart-bar {
    cursor: default;
    transition: opacity 120ms ease;
  }

  .liquid-chart-bar:hover {
    opacity: 0.88;
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-chart-bar {
      transition: none;
    }
  }
</style>
