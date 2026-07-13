<script lang="ts">
  import { getContext } from "svelte";
  import { chartSeriesColor, formatChartNumber } from "./chartModel";

  type Scale = (v: unknown) => number;

  interface ScatterRow {
    key: string;
    x: number;
    y: number;
    group: string;
    colorIndex: number;
  }

  interface CakeCustom {
    colors: string[];
    interactive: boolean;
    tooltip: boolean;
    showTooltip: (
      x: number,
      y: number,
      title: string,
      lines: { label: string; value: string; color?: string }[],
    ) => void;
    hideTooltip: () => void;
  }

  const { data, xScale, yScale, custom } = getContext<{
    data: import("svelte/store").Readable<ScatterRow[]>;
    xScale: import("svelte/store").Readable<Scale>;
    yScale: import("svelte/store").Readable<Scale>;
    custom: import("svelte/store").Readable<CakeCustom>;
  }>("LayerCake");

  let hoverKey = $state<string | null>(null);

  const dots = $derived.by(() => {
    const rows = $data ?? [];
    const xS = $xScale;
    const yS = $yScale;
    const cfg = $custom;
    if (!cfg || !rows.length || !xS || !yS) return [];
    return rows.map((row) => ({
      ...row,
      cx: xS(row.x) ?? 0,
      cy: yS(row.y) ?? 0,
      color: chartSeriesColor(row.colorIndex, cfg.colors),
    }));
  });

  function onEnter(event: MouseEvent, pt: (typeof dots)[number]) {
    const cfg = $custom;
    if (!cfg) return;
    if (cfg.interactive !== false) hoverKey = pt.key;
    if (!cfg.tooltip) return;
    const host = (event.currentTarget as SVGCircleElement).ownerSVGElement?.parentElement;
    if (!host) return;
    const box = host.getBoundingClientRect();
    cfg.showTooltip(event.clientX - box.left, event.clientY - box.top, pt.group, [
      { label: "X", value: formatChartNumber(pt.x), color: pt.color },
      { label: "Y", value: formatChartNumber(pt.y), color: pt.color },
    ]);
  }

  function onLeave() {
    hoverKey = null;
    $custom?.hideTooltip();
  }
</script>

{#each dots as pt (pt.key)}
  {@const hot = hoverKey === pt.key}
  <circle
    class="liquid-chart-scatter-dot"
    class:liquid-chart-scatter-hot={hot}
    role="img"
    aria-label={`${pt.group}: ${pt.x}, ${pt.y}`}
    cx={pt.cx}
    cy={pt.cy}
    r={hot ? 5.5 : 4}
    fill={pt.color}
    onmouseenter={(event) => onEnter(event, pt)}
    onmouseleave={onLeave}
  />
{/each}

<style>
  .liquid-chart-scatter-dot {
    stroke: color-mix(in srgb, var(--color-surface-50) 55%, transparent);
    stroke-width: 1;
    transition:
      r 160ms ease,
      opacity 160ms ease;
    cursor: default;
  }

  .liquid-chart-scatter-hot {
    filter: brightness(1.08);
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-chart-scatter-dot {
      transition: none;
    }
  }
</style>
