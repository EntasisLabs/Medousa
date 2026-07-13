<script lang="ts">
  /**
   * `chart` organism — paste-first plots from ```chart markdown.
   * Marks: bar / line / area / pie / donut / radar / radial / scatter / combo / heatmap.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { chartViewModel, resolveLegend } from "./chartModel";
  import ChartFrame from "./ChartFrame.svelte";
  import ChartLegend from "./ChartLegend.svelte";
  import BarMark from "./BarMark.svelte";
  import LineMark from "./LineMark.svelte";
  import PieMark from "./PieMark.svelte";
  import RadarMark from "./RadarMark.svelte";
  import RadialMark from "./RadialMark.svelte";
  import ScatterMark from "./ScatterMark.svelte";
  import HeatmapMark from "./HeatmapMark.svelte";
  import { TrendingUp, TrendingDown, Minus } from "@lucide/svelte";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();
  void ctx;

  const model = $derived(chartViewModel(node.props as Record<string, unknown>));
  const legendPos = $derived(
    model ? resolveLegend(model.legend, model.series.length) : "none",
  );
  const legendItems = $derived(
    !model
      ? []
      : model.type === "pie" ||
          model.type === "donut" ||
          (model.type === "radial" && model.categories.length >= 3)
        ? model.categories.map((label, i) => ({
            key: `cat-${i}`,
            label,
          }))
        : model.type === "heatmap"
          ? []
          : model.series.map((s) => ({ key: s.key, label: s.label })),
  );

  const aria = $derived(
    model
      ? [model.title || "Chart", model.description].filter(Boolean).join(" — ")
      : "Chart",
  );
</script>

{#if model}
  <div
    class="liquid-chart"
    class:liquid-chart--sized={Boolean(model.width)}
    role="img"
    aria-label={aria}
    style:width={model.width || undefined}
    style:--liquid-chart-height={model.height || undefined}
    style:--chart-plot={model.surface || undefined}
  >
    {#if model.title || model.description}
      <header class="liquid-chart-header">
        {#if model.title}
          <p class="liquid-chart-title" role="heading" aria-level="3">{model.title}</p>
        {/if}
        {#if model.description}
          <p class="liquid-chart-description">{model.description}</p>
        {/if}
      </header>
    {/if}

    {#if legendPos === "top" && legendItems.length}
      <ChartLegend items={legendItems} colors={model.colors} position="top" />
    {/if}

    <div class="liquid-chart-body">
      {#if model.type === "bar"}
        <ChartFrame {model}>
          <BarMark />
        </ChartFrame>
      {:else if model.type === "line"}
        <ChartFrame {model}>
          <LineMark mode="line" />
        </ChartFrame>
      {:else if model.type === "area"}
        <ChartFrame {model}>
          <LineMark mode="area" />
        </ChartFrame>
      {:else if model.type === "combo"}
        <ChartFrame {model}>
          <BarMark />
          <LineMark mode="line" />
        </ChartFrame>
      {:else if model.type === "pie"}
        <PieMark {model} />
      {:else if model.type === "donut"}
        <PieMark {model} donut />
      {:else if model.type === "radar"}
        <RadarMark {model} />
      {:else if model.type === "radial"}
        <RadialMark {model} />
      {:else if model.type === "scatter"}
        <ScatterMark {model} />
      {:else if model.type === "heatmap"}
        <HeatmapMark {model} />
      {/if}
    </div>

    {#if legendPos === "bottom" && legendItems.length}
      <ChartLegend items={legendItems} colors={model.colors} position="bottom" />
    {/if}

    {#if model.trend || model.caption}
      <footer class="liquid-chart-footer">
        {#if model.trend}
          <p class="liquid-chart-trend">
            {#if model.trendDirection === "down"}
              <TrendingDown size={14} strokeWidth={2} aria-hidden="true" />
            {:else if model.trendDirection === "flat"}
              <Minus size={14} strokeWidth={2} aria-hidden="true" />
            {:else}
              <TrendingUp size={14} strokeWidth={2} aria-hidden="true" />
            {/if}
            <span>{model.trend}</span>
          </p>
        {/if}
        {#if model.caption}
          <p class="liquid-chart-caption">{model.caption}</p>
        {/if}
      </footer>
    {/if}
  </div>
{/if}

<style>
  .liquid-chart {
    margin: 0;
    padding: 0.75rem 0.85rem 0.8rem;
    border-radius: 0.9rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
    background: color-mix(in srgb, var(--color-surface-50) 55%, transparent);
    box-shadow:
      0 1px 0 color-mix(in srgb, var(--color-surface-50) 70%, transparent) inset,
      0 8px 24px rgb(0 0 0 / 0.04);
    width: 100%;
    max-width: 100%;
    box-sizing: border-box;
  }

  .liquid-chart--sized {
    max-width: 100%;
  }

  :global(html.dark) .liquid-chart {
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
  }

  .liquid-chart-header {
    margin-bottom: 0.45rem;
  }

  .liquid-chart-title {
    margin: 0;
    font-size: 1.125rem;
    font-weight: 700;
    line-height: 1.25;
    letter-spacing: -0.02em;
    color: rgb(var(--chart-fg));
  }

  /* Beat .markdown-content p color/size inheritance in vault + chat */
  :global(.markdown-content) .liquid-chart-title {
    margin: 0;
    color: rgb(var(--chart-fg));
    font-size: 1.125rem;
    font-weight: 700;
  }

  :global(.markdown-content) .liquid-chart-description {
    margin: 0.2rem 0 0;
    color: rgb(var(--chart-fg-muted));
  }

  :global(.markdown-content) .liquid-chart-trend {
    margin: 0;
    color: rgb(var(--chart-fg));
  }

  :global(.markdown-content) .liquid-chart-caption {
    margin: 0.25rem 0 0;
    color: rgb(var(--chart-fg-muted));
  }

  .liquid-chart-description {
    margin: 0.2rem 0 0;
    font-size: 0.8125rem;
    line-height: 1.35;
    font-weight: 450;
    color: rgb(var(--chart-fg-muted));
  }

  .liquid-chart-body {
    min-width: 0;
  }

  .liquid-chart-footer {
    margin-top: 0.55rem;
    padding-top: 0.5rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
  }

  .liquid-chart-trend {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 650;
    line-height: 1.3;
    color: rgb(var(--chart-fg));
  }

  .liquid-chart-caption {
    margin: 0.25rem 0 0;
    font-size: 0.75rem;
    line-height: 1.35;
    font-weight: 450;
    color: rgb(var(--chart-fg-muted));
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-chart :global(.liquid-chart-mount) {
      animation: none !important;
    }
  }
</style>
