<script lang="ts">
  import { pie as d3Pie, arc as d3Arc } from "d3-shape";
  import type { ChartViewModel } from "./chartModel";
  import { chartSeriesColor } from "./chartModel";
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

  const size = 220;
  const cx = size / 2;
  const cy = size / 2;
  const outer = 84;
  const inner = $derived(donut ? 48 : 0);
  const arcGen = $derived(
    d3Arc<(typeof slices)[number]>()
      .innerRadius(inner)
      .outerRadius(outer)
      .padAngle(model.separator === false ? 0 : 0.02)
      .cornerRadius(2),
  );

  function onEnter(event: MouseEvent, category: string, value: number, color: string) {
    if (!model.tooltip) return;
    const host = (event.currentTarget as SVGPathElement).ownerSVGElement?.parentElement;
    if (!host) return;
    const box = host.getBoundingClientRect();
    tipVisible = true;
    tipX = event.clientX - box.left;
    tipY = event.clientY - box.top;
    tipTitle = category;
    tipLines = [{ label: model.series[0]?.label ?? "Value", value: String(value), color }];
  }

  function labelFor(slice: (typeof slices)[number]): string {
    if (model.labels === "none") return "";
    if (model.labels === "category") return slice.data.category;
    if (model.labels === "both") return `${slice.data.category} ${slice.data.value}`;
    return String(slice.data.value);
  }
</script>

<div class="liquid-chart-pie-wrap">
  <svg class="liquid-chart-pie" viewBox={`0 0 ${size} ${size}`} role="presentation">
    <g transform={`translate(${cx}, ${cy})`}>
      {#each slices as slice, i (slice.data.category + i)}
        <path
          class="liquid-chart-slice"
          role="img"
          aria-label={`${slice.data.category}: ${slice.data.value}`}
          d={arcGen(slice) ?? ""}
          fill={slice.data.color}
          onmouseenter={(event) => onEnter(event, slice.data.category, slice.data.value, slice.data.color)}
          onmouseleave={() => (tipVisible = false)}
        />
        {#if model.labels !== "none"}
          {@const [lx, ly] = arcGen.centroid(slice)}
          <text class="liquid-chart-pie-label" x={lx} y={ly} text-anchor="middle" dominant-baseline="middle"
            >{labelFor(slice)}</text
          >
        {/if}
      {/each}
      {#if donut && (model.centerValue || model.centerLabel)}
        {#if model.centerValue}
          <text class="liquid-chart-center-value" text-anchor="middle" y="-2">{model.centerValue}</text>
        {:else}
          <text class="liquid-chart-center-value" text-anchor="middle" y="-2">{total}</text>
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
    min-height: 13rem;
  }

  .liquid-chart-pie {
    width: min(100%, 15rem);
    height: auto;
  }

  .liquid-chart-slice {
    transition: opacity 120ms ease;
  }

  .liquid-chart-slice:hover {
    opacity: 0.9;
  }

  .liquid-chart-pie-label {
    fill: rgb(var(--color-surface-50));
    font-size: 0.58rem;
    font-weight: 600;
    pointer-events: none;
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
    .liquid-chart-slice {
      transition: none;
    }
  }
</style>
