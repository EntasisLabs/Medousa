<script lang="ts">
  import { LayerCake, Svg, Html } from "layercake";
  import { scaleLinear } from "d3-scale";
  import type { ChartViewModel } from "./chartModel";
  import ChartTooltip from "./ChartTooltip.svelte";
  import ScatterDots from "./ScatterDots.svelte";
  import ScatterAxes from "./ScatterAxes.svelte";

  interface Props {
    model: ChartViewModel;
  }

  let { model }: Props = $props();

  let tipVisible = $state(false);
  let tipX = $state(0);
  let tipY = $state(0);
  let tipTitle = $state("");
  let tipLines = $state<{ label: string; value: string; color?: string }[]>([]);

  const groups = $derived.by(() => {
    const names = [
      ...new Set(model.points.map((p) => p.group).filter((g): g is string => Boolean(g))),
    ];
    if (names.length) return names;
    return ["Points"];
  });

  const domain = $derived.by(() => {
    const xs = model.points.map((p) => p.x);
    const ys = model.points.map((p) => p.y);
    const xMin = Math.min(...xs);
    const xMax = Math.max(...xs);
    const yMin = Math.min(...ys);
    const yMax = Math.max(...ys);
    const xPad = (xMax - xMin || 1) * 0.08;
    const yPad = (yMax - yMin || 1) * 0.08;
    return {
      x: [xMin - xPad, xMax + xPad] as [number, number],
      y: [Math.min(0, yMin - yPad), yMax + yPad] as [number, number],
    };
  });

  const rows = $derived(
    model.points.map((p, i) => ({
      key: `pt-${i}`,
      x: p.x,
      y: p.y,
      group: p.group || "Points",
      colorIndex: Math.max(0, groups.indexOf(p.group || "Points")),
    })),
  );

  function showTooltip(
    localX: number,
    localY: number,
    title: string,
    lines: { label: string; value: string; color?: string }[],
  ) {
    if (!model.tooltip) return;
    tipVisible = true;
    tipX = localX;
    tipY = localY;
    tipTitle = title;
    tipLines = lines;
  }

  function hideTooltip() {
    tipVisible = false;
  }

  const cakeCustom = $derived({
    colors: model.colors,
    interactive: model.interactive,
    tooltip: model.tooltip,
    showTooltip,
    hideTooltip,
  });
</script>

<div class="liquid-chart-scatter-wrap liquid-chart-mount">
  <LayerCake
    data={rows}
    x="x"
    y="y"
    xScale={scaleLinear()}
    yScale={scaleLinear()}
    xDomain={domain.x}
    yDomain={domain.y}
    padding={{ top: 12, right: 16, bottom: 32, left: 40 }}
    custom={cakeCustom}
  >
    <Svg>
      <ScatterAxes />
      <ScatterDots />
    </Svg>
    {#if model.tooltip}
      <Html>
        <ChartTooltip visible={tipVisible} x={tipX} y={tipY} title={tipTitle} lines={tipLines} />
      </Html>
    {/if}
  </LayerCake>
</div>

<style>
  .liquid-chart-scatter-wrap {
    position: relative;
    width: 100%;
    height: var(--liquid-chart-height, 14.5rem);
    min-height: 10rem;
  }

  .liquid-chart-scatter-wrap :global(.layercake-container) {
    width: 100%;
    height: 100%;
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
    .liquid-chart-mount {
      animation: none;
    }
  }
</style>
