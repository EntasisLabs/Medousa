<script lang="ts">
  import { getContext } from "svelte";
  import { line as d3Line, area as d3Area, curveMonotoneX, curveLinear, curveStepAfter } from "d3-shape";
  import type { LiquidChartCurve, LiquidChartSeries } from "$lib/markdown/liquidEmbeds";
  import { chartSeriesColor } from "./chartModel";

  interface Props {
    mode?: "line" | "area";
  }

  let { mode = "line" }: Props = $props();

  type Scale = ((v: unknown) => number) & { bandwidth?: () => number; domain?: () => unknown[] };

  interface CakeCustom {
    series: LiquidChartSeries[];
    colors: string[];
    curve: LiquidChartCurve;
    showTooltip: (
      x: number,
      y: number,
      title: string,
      lines: { label: string; value: string; color?: string }[],
    ) => void;
    hideTooltip: () => void;
  }

  const { data, xScale, yScale, width, height, custom } = getContext<{
    data: import("svelte/store").Readable<Record<string, string | number>[]>;
    xScale: import("svelte/store").Readable<Scale>;
    yScale: import("svelte/store").Readable<Scale>;
    width: import("svelte/store").Readable<number>;
    height: import("svelte/store").Readable<number>;
    custom: import("svelte/store").Readable<CakeCustom>;
  }>("LayerCake");

  function curveFactory(curve: LiquidChartCurve) {
    if (curve === "linear") return curveLinear;
    if (curve === "step") return curveStepAfter;
    return curveMonotoneX;
  }

  const paths = $derived.by(() => {
    const rows = $data ?? [];
    const xS = $xScale;
    const yS = $yScale;
    const cfg = $custom;
    if (!cfg || !rows.length || !xS || !yS) return [];

    const curve = curveFactory(cfg.curve ?? "smooth");
    return cfg.series.map((s, si) => {
      const points = rows.map((row) => {
        const category = String(row.category ?? "");
        const x = (xS(category) ?? 0) + ((xS.bandwidth?.() ?? 0) / 2);
        const y = yS(Number(row[s.key] ?? 0)) ?? 0;
        return { category, x, y, value: Number(row[s.key] ?? 0) };
      });

      const lineGen = d3Line<(typeof points)[number]>()
        .x((d) => d.x)
        .y((d) => d.y)
        .curve(curve);

      const areaGen = d3Area<(typeof points)[number]>()
        .x((d) => d.x)
        .y0(yS(0) ?? $height)
        .y1((d) => d.y)
        .curve(curve);

      return {
        key: s.key,
        label: s.label,
        color: chartSeriesColor(si, cfg.colors),
        line: lineGen(points) ?? "",
        area: areaGen(points) ?? "",
        points,
      };
    });
  });

  const axis = $derived.by(() => {
    const rows = $data ?? [];
    const xS = $xScale;
    const yS = $yScale;
    const w = $width;
    const h = $height;
    if (!xS || !yS) return { w: 0, h: 0, grid: [] as number[], xLabels: [] as { label: string; x: number }[], yLabels: [] as { label: string; y: number }[] };
    const domain = (yS.domain?.() as number[]) ?? [0, 1];
    const max = Number(domain[1] ?? 1);
    return {
      w,
      h,
      grid: [0.25, 0.5, 0.75, 1].map((t) => yS(max * t) ?? 0),
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

  function onPoint(
    event: MouseEvent,
    category: string,
    lines: { label: string; value: string; color?: string }[],
  ) {
    const cfg = $custom;
    if (!cfg) return;
    const target = event.currentTarget as SVGCircleElement;
    const box = target.getBoundingClientRect();
    const parent = target.ownerSVGElement?.parentElement?.getBoundingClientRect();
    if (!parent) return;
    cfg.showTooltip(box.left - parent.left, box.top - parent.top, category, lines);
  }
</script>

{#if axis.w > 0 && axis.h > 0}
  <g class="liquid-chart-line">
    {#each axis.grid as y, i (i)}
      <line class="liquid-chart-grid" x1="0" x2={axis.w} y1={y} y2={y} />
    {/each}
    {#each axis.yLabels as tick (tick.label + tick.y)}
      <text class="liquid-chart-axis" x={-6} y={tick.y} text-anchor="end" dominant-baseline="middle"
        >{tick.label}</text
      >
    {/each}
    {#each axis.xLabels as tick (tick.label + tick.x)}
      <text class="liquid-chart-axis" x={tick.x} y={axis.h + 14} text-anchor="middle">{tick.label}</text>
    {/each}

    {#each paths as series (series.key)}
      {#if mode === "area"}
        <path class="liquid-chart-area" d={series.area} fill={series.color} />
      {/if}
      <path class="liquid-chart-stroke" d={series.line} stroke={series.color} fill="none" />
      {#each series.points as pt, pi (series.key + pi)}
        <circle
          class="liquid-chart-dot"
          role="img"
          aria-label={`${pt.category}: ${series.label} ${pt.value}`}
          cx={pt.x}
          cy={pt.y}
          r="3.25"
          fill={series.color}
          onmouseenter={(event) =>
            onPoint(event, pt.category, [
              { label: series.label, value: String(pt.value), color: series.color },
            ])}
          onmouseleave={() => $custom?.hideTooltip()}
        />
      {/each}
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

  .liquid-chart-stroke {
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .liquid-chart-area {
    opacity: 0.22;
  }

  .liquid-chart-dot {
    stroke: color-mix(in srgb, var(--color-surface-950) 55%, transparent);
    stroke-width: 1;
  }
</style>
