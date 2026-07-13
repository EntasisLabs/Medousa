<script lang="ts">
  import { pie as d3Pie, arc as d3Arc } from "d3-shape";
  import type { ChartViewModel } from "./chartModel";
  import {
    chartSeriesColor,
    formatChartLabel,
    formatChartNumber,
    hasActiveHighlight,
    isActiveKey,
    resolveLabelPosition,
  } from "./chartModel";
  import ChartTooltip from "./ChartTooltip.svelte";

  interface Props {
    model: ChartViewModel;
    donut?: boolean;
  }

  let { model, donut = false }: Props = $props();

  let tipVisible = $state(false);
  let tipX = $state(0);
  let tipY = $state(0);
  let tipTitle = $state("");
  let tipLines = $state<{ label: string; value: string; color?: string }[]>([]);

  const EXPLODE = 6;

  const slices = $derived.by(() => {
    const values = model.series[0]?.values ?? [];
    const data = model.categories.map((category, i) => ({
      category,
      value: values[i] ?? 0,
      color: chartSeriesColor(i, model.colors),
    }));
    const layout = d3Pie<(typeof data)[number]>()
      .value((d) => d.value)
      .sort(null)(data);
    return layout;
  });

  const total = $derived(slices.reduce((sum, s) => sum + s.data.value, 0));

  const labelPos = $derived(
    resolveLabelPosition({
      type: donut ? "donut" : "pie",
      labels: model.labels,
      labelPosition: model.labelPosition,
      centerLabel: model.centerLabel,
      centerValue: model.centerValue,
    }),
  );

  const highlight = $derived(hasActiveHighlight(model.activeKey));

  const size = 260;
  const cx = size / 2;
  const cy = size / 2;
  const outer = 78;
  const labelRadius = 102;
  const elbowRadius = 90;
  const inner = $derived(donut ? 44 : 0);
  const arcGen = $derived(
    d3Arc<(typeof slices)[number]>()
      .innerRadius(inner)
      .outerRadius(outer)
      .padAngle(model.separator === false ? 0 : 0.02)
      .cornerRadius(2),
  );

  function midAngle(slice: (typeof slices)[number]): number {
    return (slice.startAngle + slice.endAngle) / 2;
  }

  function explodeOffset(slice: (typeof slices)[number]): { x: number; y: number } {
    if (!highlight) return { x: 0, y: 0 };
    if (!isActiveKey(model.activeKey, { category: slice.data.category })) {
      return { x: 0, y: 0 };
    }
    const a = midAngle(slice) - Math.PI / 2;
    return { x: Math.cos(a) * EXPLODE, y: Math.sin(a) * EXPLODE };
  }

  function isSliceActive(slice: (typeof slices)[number]): boolean {
    if (!highlight) return true;
    return isActiveKey(model.activeKey, { category: slice.data.category });
  }

  function polar(r: number, angle: number): [number, number] {
    const a = angle - Math.PI / 2;
    return [Math.cos(a) * r, Math.sin(a) * r];
  }

  function outsideLabel(slice: (typeof slices)[number]) {
    const mid = midAngle(slice);
    const [x0, y0] = polar(outer, mid);
    const [x1, y1] = polar(elbowRadius, mid);
    const side = Math.cos(mid - Math.PI / 2) >= 0 ? 1 : -1;
    const [x2, y2] = [polar(labelRadius, mid)[0] + side * 8, polar(labelRadius, mid)[1]];
    return {
      path: `M${x0},${y0}L${x1},${y1}L${x2},${y2}`,
      x: x2 + side * 4,
      y: y2,
      anchor: side >= 0 ? ("start" as const) : ("end" as const),
      text: formatChartLabel(model.labels, slice.data.category, slice.data.value),
    };
  }

  function onEnter(event: MouseEvent, category: string, value: number, color: string) {
    if (!model.tooltip) return;
    const host = (event.currentTarget as SVGPathElement).ownerSVGElement?.parentElement;
    if (!host) return;
    const box = host.getBoundingClientRect();
    tipVisible = true;
    tipX = event.clientX - box.left;
    tipY = event.clientY - box.top;
    tipTitle = category;
    tipLines = [
      {
        label: model.series[0]?.label ?? "Value",
        value: formatChartNumber(value),
        color,
      },
    ];
  }
</script>

<div class="liquid-chart-pie-wrap">
  <svg class="liquid-chart-pie" viewBox={`0 0 ${size} ${size}`} role="presentation">
    <g transform={`translate(${cx}, ${cy})`}>
      {#each slices as slice, i (slice.data.category + i)}
        {@const off = explodeOffset(slice)}
        {@const active = isSliceActive(slice)}
        <g transform={`translate(${off.x}, ${off.y})`}>
          <path
            class="liquid-chart-slice"
            class:liquid-chart-dim={!active}
            role="img"
            aria-label={`${slice.data.category}: ${slice.data.value}`}
            d={arcGen(slice) ?? ""}
            fill={slice.data.color}
            onmouseenter={(event) =>
              onEnter(event, slice.data.category, slice.data.value, slice.data.color)}
            onmouseleave={() => (tipVisible = false)}
          />
          {#if labelPos === "inside"}
            {@const [lx, ly] = arcGen.centroid(slice)}
            <text
              class="liquid-chart-pie-label liquid-chart-pie-label-inside"
              class:liquid-chart-dim={!active}
              x={lx}
              y={ly}
              text-anchor="middle"
              dominant-baseline="middle"
              >{formatChartLabel(model.labels, slice.data.category, slice.data.value)}</text
            >
          {:else if labelPos === "outside"}
            {@const lbl = outsideLabel(slice)}
            {#if lbl.text}
              <path
                class="liquid-chart-leader"
                class:liquid-chart-dim={!active}
                d={lbl.path}
                fill="none"
              />
              <text
                class="liquid-chart-pie-label liquid-chart-pie-label-outside"
                class:liquid-chart-dim={!active}
                x={lbl.x}
                y={lbl.y}
                text-anchor={lbl.anchor}
                dominant-baseline="middle"
                >{lbl.text}</text
              >
            {/if}
          {/if}
        </g>
      {/each}
      {#if donut && (model.centerValue || model.centerLabel)}
        {#if model.centerValue}
          <text class="liquid-chart-center-value" text-anchor="middle" y="-2">{model.centerValue}</text>
        {:else}
          <text class="liquid-chart-center-value" text-anchor="middle" y="-2"
            >{formatChartNumber(total)}</text
          >
        {/if}
        {#if model.centerLabel}
          <text class="liquid-chart-center-label" text-anchor="middle" y="16">{model.centerLabel}</text>
        {/if}
      {/if}
    </g>
  </svg>
  <ChartTooltip visible={tipVisible} x={tipX} y={tipY} title={tipTitle} lines={tipLines} />
</div>

<style>
  .liquid-chart-pie-wrap {
    position: relative;
    display: grid;
    place-items: center;
    width: 100%;
    min-height: 14rem;
  }

  .liquid-chart-pie {
    width: min(100%, 17rem);
    height: auto;
  }

  .liquid-chart-slice {
    transition: opacity 120ms ease;
  }

  .liquid-chart-slice:hover {
    opacity: 0.92;
  }

  .liquid-chart-dim {
    opacity: 0.35;
  }

  .liquid-chart-leader {
    stroke: color-mix(in srgb, var(--color-surface-400) 70%, transparent);
    stroke-width: 1;
    transition: opacity 120ms ease;
  }

  .liquid-chart-pie-label {
    font-size: 0.58rem;
    font-weight: 600;
    pointer-events: none;
    transition: opacity 120ms ease;
  }

  .liquid-chart-pie-label-inside {
    fill: rgb(var(--color-surface-50));
  }

  .liquid-chart-pie-label-outside {
    fill: rgb(var(--color-surface-200));
    font-variant-numeric: tabular-nums;
  }

  .liquid-chart-center-value {
    fill: rgb(var(--color-surface-50));
    font-size: 1.15rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .liquid-chart-center-label {
    fill: rgb(var(--color-surface-400));
    font-size: 0.68rem;
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-chart-slice,
    .liquid-chart-leader,
    .liquid-chart-pie-label {
      transition: none;
    }
  }
</style>
