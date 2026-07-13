<script lang="ts">
  import { getContext } from "svelte";
  import { formatChartNumber } from "./chartModel";

  type Scale = ((v: unknown) => number) & { domain?: () => unknown[] };

  const { xScale, yScale, width, height } = getContext<{
    xScale: import("svelte/store").Readable<Scale>;
    yScale: import("svelte/store").Readable<Scale>;
    width: import("svelte/store").Readable<number>;
    height: import("svelte/store").Readable<number>;
  }>("LayerCake");

  const axis = $derived.by(() => {
    const xS = $xScale;
    const yS = $yScale;
    const w = $width;
    const h = $height;
    if (!xS || !yS || !w || !h) {
      return {
        w: 0,
        h: 0,
        xTicks: [] as number[],
        yGrid: [] as number[],
        xLabels: [] as { label: string; x: number }[],
        yLabels: [] as { label: string; y: number }[],
      };
    }
    const xDomain = (xS.domain?.() as number[]) ?? [0, 1];
    const yDomain = (yS.domain?.() as number[]) ?? [0, 1];
    const x0 = Number(xDomain[0] ?? 0);
    const x1 = Number(xDomain[1] ?? 1);
    const y0 = Number(yDomain[0] ?? 0);
    const y1 = Number(yDomain[1] ?? 1);
    const xSpan = x1 - x0 || 1;
    const ySpan = y1 - y0 || 1;

    return {
      w,
      h,
      xGrid: [0.25, 0.5, 0.75].map((t) => xS(x0 + xSpan * t) ?? 0),
      yGrid: [0.25, 0.5, 0.75].map((t) => yS(y0 + ySpan * t) ?? 0),
      xLabels: [0, 0.5, 1].map((t) => {
        const v = x0 + xSpan * t;
        return { label: formatChartNumber(v), x: xS(v) ?? 0 };
      }),
      yLabels: [0, 0.5, 1].map((t) => {
        const v = y0 + ySpan * t;
        return { label: formatChartNumber(v), y: yS(v) ?? 0 };
      }),
    };
  });
</script>

{#if axis.w > 0 && axis.h > 0}
  <g class="liquid-chart-scatter-axes" aria-hidden="true">
    {#each axis.yGrid as y, i (i)}
      <line class="liquid-chart-grid" x1="0" x2={axis.w} y1={y} y2={y} />
    {/each}
    {#each axis.xGrid as x, i (i)}
      <line class="liquid-chart-grid" x1={x} x2={x} y1="0" y2={axis.h} />
    {/each}
    {#each axis.yLabels as tick (tick.label + tick.y)}
      <text class="liquid-chart-axis" x={-6} y={tick.y} text-anchor="end" dominant-baseline="middle"
        >{tick.label}</text
      >
    {/each}
    {#each axis.xLabels as tick (tick.label + tick.x)}
      <text class="liquid-chart-axis" x={tick.x} y={axis.h + 14} text-anchor="middle">{tick.label}</text>
    {/each}
  </g>
{/if}

<style>
  .liquid-chart-grid {
    stroke: var(--chart-grid, color-mix(in srgb, var(--color-surface-500) 22%, transparent));
    stroke-width: 1;
  }

  .liquid-chart-axis {
    fill: rgb(var(--chart-fg-secondary));
    font-size: 0.7rem;
    font-weight: 550;
  }
</style>
