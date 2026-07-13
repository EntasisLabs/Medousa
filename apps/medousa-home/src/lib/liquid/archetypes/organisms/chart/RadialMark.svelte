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
  let hoverKey = $state<string | null>(null);

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
        .innerRadius(50)
        .outerRadius(74)
        .startAngle(0)
        .endAngle(Math.PI * 2)
        .cornerRadius(12)(datum) ?? "";
    const progress =
      d3Arc<typeof datum>()
        .innerRadius(50)
        .outerRadius(74)
        .startAngle(-Math.PI / 2)
        .endAngle(end)
        .cornerRadius(12)(datum) ?? "";
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
    const outerMax = 92;
    const thickness = 14;
    const gap = 5;
    return model.categories.map((category, i) => {
      const value = values[i] ?? 0;
      const ratio = Math.min(Math.max(value / max, 0), 1);
      const outer = outerMax - i * (thickness + gap);
      const inner = outer - thickness;
      const cap = thickness / 2;
      const end = -Math.PI / 2 + ratio * Math.PI * 2;
      const datum = { startAngle: 0, endAngle: Math.PI * 2 };
      const track =
        d3Arc<typeof datum>()
          .innerRadius(inner)
          .outerRadius(outer)
          .startAngle(0)
          .endAngle(Math.PI * 2)
          .cornerRadius(cap)(datum) ?? "";
      const progress =
        d3Arc<typeof datum>()
          .innerRadius(inner)
          .outerRadius(outer)
          .startAngle(-Math.PI / 2)
          .endAngle(end)
          .cornerRadius(cap)(datum) ?? "";
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

  function onEnter(event: MouseEvent, title: string, value: number, color: string, key: string) {
    if (model.interactive !== false) hoverKey = key;
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

  function onLeave() {
    hoverKey = null;
    tipVisible = false;
  }

  function arcDimmed(active: boolean, key: string): boolean {
    if (model.interactive !== false && hoverKey) return key !== hoverKey;
    return !active;
  }
</script>

<div class="liquid-chart-radial-wrap liquid-chart-mount">
  <svg class="liquid-chart-radial" viewBox={`0 0 ${size} ${size}`} role="presentation">
    <g transform={`translate(${cx}, ${cy})`}>
      {#if model.surface && model.surface !== "transparent"}
        <circle class="liquid-chart-plot-plate" r="98" />
      {/if}
      {#if multiArc}
        {#each arcs as arc (arc.key)}
          {@const dimmed = arcDimmed(arc.active, arc.key)}
          <path
            class="liquid-chart-radial-track"
            d={arc.track}
            fill={`color-mix(in srgb, ${arc.color} 16%, transparent)`}
          />
          <path
            class="liquid-chart-radial-progress"
            class:liquid-chart-dim={dimmed}
            class:liquid-chart-radial-hot={hoverKey === arc.key}
            role="img"
            aria-label={`${arc.category}: ${arc.value}`}
            d={arc.progress}
            fill={arc.color}
            onmouseenter={(event) => onEnter(event, arc.category, arc.value, arc.color, arc.key)}
            onmouseleave={onLeave}
          />
        {/each}
        {#if model.labels !== "none"}
          {#each arcs as arc (arc.key + "-lbl")}
            {#if arc.label}
              <text
                class="liquid-chart-radial-ring-label"
                class:liquid-chart-dim={arcDimmed(arc.active, arc.key)}
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
        <path
          class="liquid-chart-radial-track"
          d={single.track}
          fill={`color-mix(in srgb, ${single.color} 16%, transparent)`}
        />
        <path
          class="liquid-chart-radial-progress"
          class:liquid-chart-radial-hot={hoverKey === "single"}
          role="img"
          aria-label={`${series?.label ?? "Value"}: ${single.value}`}
          d={single.progress}
          fill={single.color}
          onmouseenter={(event) =>
            onEnter(event, series?.label ?? "Value", single.value, single.color, "single")}
          onmouseleave={onLeave}
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

  .liquid-chart-plot-plate {
    fill: var(--chart-plot);
    stroke: none;
  }

  .liquid-chart-radial-track {
    /* series tint set inline; fallback if missing */
    fill: var(--chart-plot);
  }

  .liquid-chart-radial-progress {
    transition:
      opacity 160ms ease,
      filter 160ms ease;
  }

  .liquid-chart-radial-hot {
    filter: brightness(1.08);
  }

  .liquid-chart-dim {
    opacity: 0.38;
  }

  .liquid-chart-radial-ring-label {
    fill: rgb(var(--chart-fg-muted));
    font-size: 0.58rem;
    font-weight: 600;
    pointer-events: none;
    opacity: 0.9;
  }

  .liquid-chart-center-value {
    fill: rgb(var(--chart-fg));
    font-size: 1.35rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .liquid-chart-center-label {
    fill: rgb(var(--chart-fg-muted));
    font-size: 0.72rem;
    font-weight: 500;
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
    .liquid-chart-radial-progress {
      transition: none;
    }

    .liquid-chart-mount {
      animation: none;
    }
  }
</style>
