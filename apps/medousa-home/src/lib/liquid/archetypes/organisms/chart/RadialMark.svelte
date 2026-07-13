<script lang="ts">
  import { arc as d3Arc } from "d3-shape";
  import type { ChartViewModel } from "./chartModel";
  import {
    chartSeriesColor,
    formatChartLabel,
    formatChartNumber,
    hasActiveHighlight,
    isActiveKey,
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

  const size = 240;
  const cx = size / 2;
  const cy = size / 2;

  /** Multi-arc when ≥3 categories; otherwise single progress arc. */
  const multiArc = $derived(model.categories.length >= 3);

  const series = $derived(model.series[0]);
  const highlight = $derived(hasActiveHighlight(model.activeKey));

  const single = $derived.by(() => {
    const values = series?.values ?? [];
    const first = values[0] ?? 0;
    const max = Math.max(...values, 1);
    const ratio = Math.min(Math.max(first / max, 0), 1);
    const end = -Math.PI / 2 + ratio * Math.PI * 2;
    const datum = { startAngle: 0, endAngle: Math.PI * 2 };
    const track =
      d3Arc<typeof datum>()
        .innerRadius(54)
        .outerRadius(72)
        .startAngle(0)
        .endAngle(Math.PI * 2)
        .cornerRadius(6)(datum) ?? "";
    const progress =
      d3Arc<typeof datum>()
        .innerRadius(54)
        .outerRadius(72)
        .startAngle(-Math.PI / 2)
        .endAngle(end)
        .cornerRadius(6)(datum) ?? "";
    return {
      track,
      progress,
      value: first,
      max,
      ratio,
      centerValue: model.centerValue || formatChartNumber(first),
      centerLabel: model.centerLabel || series?.label || "",
      color: chartSeriesColor(0, model.colors),
    };
  });

  const arcs = $derived.by(() => {
    if (!multiArc || !series) return [];
    const values = series.values;
    const max = Math.max(...values, 1);
    const n = model.categories.length;
    const outerMax = 88;
    const thickness = 10;
    const gap = 5;
    return model.categories.map((category, i) => {
      const value = values[i] ?? 0;
      const ratio = Math.min(Math.max(value / max, 0), 1);
      const outer = outerMax - i * (thickness + gap);
      const inner = outer - thickness;
      const end = -Math.PI / 2 + ratio * Math.PI * 2;
      const datum = { startAngle: 0, endAngle: Math.PI * 2 };
      const track =
        d3Arc<typeof datum>()
          .innerRadius(inner)
          .outerRadius(outer)
          .startAngle(0)
          .endAngle(Math.PI * 2)
          .cornerRadius(4)(datum) ?? "";
      const progress =
        d3Arc<typeof datum>()
          .innerRadius(inner)
          .outerRadius(outer)
          .startAngle(-Math.PI / 2)
          .endAngle(end)
          .cornerRadius(4)(datum) ?? "";
      const active =
        !highlight || isActiveKey(model.activeKey, { category, key: series.key, label: series.label });
      return {
        key: category,
        category,
        value,
        color: chartSeriesColor(i, model.colors),
        track,
        progress,
        active,
        label: formatChartLabel(model.labels, category, value),
        labelY: -outer + thickness / 2,
      };
    });
  });

  function onEnter(event: MouseEvent, title: string, value: number, color: string) {
    if (!model.tooltip) return;
    const host = (event.currentTarget as SVGPathElement).ownerSVGElement?.parentElement;
    if (!host) return;
    const box = host.getBoundingClientRect();
    tipVisible = true;
    tipX = event.clientX - box.left;
    tipY = event.clientY - box.top;
    tipTitle = title;
    tipLines = [
      {
        label: series?.label ?? "Value",
        value: formatChartNumber(value),
        color,
      },
    ];
  }
</script>

<div class="liquid-chart-radial-wrap">
  <svg class="liquid-chart-radial" viewBox={`0 0 ${size} ${size}`} role="presentation">
    <g transform={`translate(${cx}, ${cy})`}>
      {#if multiArc}
        {#each arcs as arc (arc.key)}
          <path class="liquid-chart-radial-track" d={arc.track} />
          <path
            class="liquid-chart-radial-progress"
            class:liquid-chart-dim={!arc.active}
            role="img"
            aria-label={`${arc.category}: ${arc.value}`}
            d={arc.progress}
            fill={arc.color}
            onmouseenter={(event) => onEnter(event, arc.category, arc.value, arc.color)}
            onmouseleave={() => (tipVisible = false)}
          />
        {/each}
        {#if model.labels !== "none"}
          {#each arcs as arc (arc.key + "-lbl")}
            {#if arc.label}
              <text
                class="liquid-chart-radial-ring-label"
                class:liquid-chart-dim={!arc.active}
                x="0"
                y={arc.labelY}
                text-anchor="middle"
                dominant-baseline="middle">{arc.label}</text
              >
            {/if}
          {/each}
        {/if}
        {#if model.centerValue || model.centerLabel}
          {#if model.centerValue}
            <text class="liquid-chart-center-value" text-anchor="middle" y="-2">{model.centerValue}</text>
          {/if}
          {#if model.centerLabel}
            <text class="liquid-chart-center-label" text-anchor="middle" y={model.centerValue ? 16 : 0}
              >{model.centerLabel}</text
            >
          {/if}
        {/if}
      {:else}
        <path class="liquid-chart-radial-track" d={single.track} />
        <path
          class="liquid-chart-radial-progress"
          role="img"
          aria-label={`${series?.label ?? "Value"}: ${single.value}`}
          d={single.progress}
          fill={single.color}
          onmouseenter={(event) =>
            onEnter(event, series?.label ?? "Value", single.value, single.color)}
          onmouseleave={() => (tipVisible = false)}
        />
        <text class="liquid-chart-center-value" text-anchor="middle" y="-2">{single.centerValue}</text>
        {#if single.centerLabel}
          <text class="liquid-chart-center-label" text-anchor="middle" y="16">{single.centerLabel}</text>
        {/if}
      {/if}
    </g>
  </svg>
  <ChartTooltip visible={tipVisible} x={tipX} y={tipY} title={tipTitle} lines={tipLines} />
</div>

<style>
  .liquid-chart-radial-wrap {
    position: relative;
    display: grid;
    place-items: center;
    width: 100%;
    min-height: 14rem;
  }

  .liquid-chart-radial {
    width: min(100%, 15rem);
    height: auto;
  }

  .liquid-chart-radial-track {
    fill: color-mix(in srgb, var(--color-surface-500) 22%, transparent);
  }

  .liquid-chart-radial-progress {
    transition: opacity 120ms ease;
  }

  .liquid-chart-dim {
    opacity: 0.35;
  }

  .liquid-chart-radial-ring-label {
    fill: rgb(var(--color-surface-200));
    font-size: 0.52rem;
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
    .liquid-chart-radial-progress {
      transition: none;
    }
  }
</style>
