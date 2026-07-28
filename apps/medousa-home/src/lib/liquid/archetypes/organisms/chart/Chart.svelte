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
  import {
    TrendingUp,
    TrendingDown,
    Minus,
    Download,
    FileImage,
    FileCode,
    FileSpreadsheet,
  } from "@lucide/svelte";
  import {
    chartSupportsSvgExport,
    exportChartCsv,
    exportChartPng,
    exportChartSvg,
  } from "$lib/utils/chartExport";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();
  const showExportMenu = $derived(!ctx.exportPaper);

  let rootEl = $state<HTMLElement | null>(null);
  let menuOpen = $state(false);
  let exportBusy = $state(false);
  let exportNote = $state<string | null>(null);
  let noteTimer: ReturnType<typeof setTimeout> | null = null;

  function flashNote(message: string) {
    exportNote = message;
    if (noteTimer) clearTimeout(noteTimer);
    noteTimer = setTimeout(() => {
      exportNote = null;
    }, 2400);
  }

  async function runExport(format: "png" | "svg" | "csv") {
    if (exportBusy || !model) return;
    menuOpen = false;
    exportBusy = true;
    try {
      if (format === "csv") {
        if (exportChartCsv(model)) flashNote("CSV saved");
      } else if (format === "svg") {
        if (!rootEl) return;
        const ok = await exportChartSvg(rootEl, model);
        flashNote(ok ? "SVG saved" : "SVG export supports pie / donut / radar / radial — try PNG");
      } else {
        if (!rootEl) return;
        const ok = await exportChartPng(rootEl, model);
        flashNote(ok ? "PNG saved" : "Couldn’t capture PNG");
      }
    } finally {
      exportBusy = false;
    }
  }

  function toggleMenu() {
    menuOpen = !menuOpen;
  }

  function closeMenu(event: MouseEvent) {
    if (!menuOpen) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest(".liquid-chart-export")) return;
    menuOpen = false;
  }

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

<svelte:window onclick={closeMenu} />

{#if model}
  <div
    class="liquid-chart"
    class:liquid-chart--sized={Boolean(model.width)}
    role="img"
    aria-label={aria}
    bind:this={rootEl}
    style:width={model.width || undefined}
    style:--liquid-chart-height={model.height || undefined}
    style:--chart-plot={model.surface || undefined}
  >
    {#if model.title || model.description || showExportMenu}
      <header class="liquid-chart-header">
        <div class="liquid-chart-header-text">
          {#if model.title}
            <p class="liquid-chart-title" role="heading" aria-level="3">{model.title}</p>
          {/if}
          {#if model.description}
            <p class="liquid-chart-description">{model.description}</p>
          {/if}
        </div>
        {#if showExportMenu}
          <div class="liquid-chart-export">
            <button
              type="button"
              class="liquid-chart-export-btn"
              title="Export chart"
              aria-label="Export chart"
              aria-expanded={menuOpen}
              aria-busy={exportBusy}
              onclick={(event) => {
                event.stopPropagation();
                toggleMenu();
              }}
            >
              <Download size={14} strokeWidth={2} />
            </button>
            {#if menuOpen}
              <div class="liquid-chart-export-menu" role="menu">
                <button type="button" role="menuitem" onclick={() => runExport("png")}>
                  <FileImage size={13} strokeWidth={2} /> PNG
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onclick={() => runExport("svg")}
                  title={chartSupportsSvgExport(model)
                    ? "Vector export"
                    : "Vector export available for pie, donut, radar, radial"}
                >
                  <FileCode size={13} strokeWidth={2} /> SVG
                </button>
                <button type="button" role="menuitem" onclick={() => runExport("csv")}>
                  <FileSpreadsheet size={13} strokeWidth={2} /> CSV
                </button>
              </div>
            {/if}
            {#if exportNote}
              <p class="liquid-chart-export-note" role="status">{exportNote}</p>
            {/if}
          </div>
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
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    /* Dark-canvas default — vault + chat are ink-first; paper only in light shell outside vault. */
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    width: 100%;
    max-width: 100%;
    box-sizing: border-box;
  }

  .liquid-chart--sized {
    max-width: 100%;
  }

  :global(html:not(.dark)) .liquid-chart {
    background: color-mix(in srgb, var(--color-surface-50) 55%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 22%, transparent);
    box-shadow:
      0 1px 0 color-mix(in srgb, var(--color-surface-50) 70%, transparent) inset,
      0 8px 24px rgb(0 0 0 / 0.04);
  }

  /* Vault note surface stays ink-glass even when the shell is in light mode. */
  :global(html:not(.dark) .vault-editor) .liquid-chart {
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
  }

  .liquid-chart-header {
    margin-bottom: 0.45rem;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .liquid-chart-header-text {
    min-width: 0;
  }

  .liquid-chart-export {
    position: relative;
    flex-shrink: 0;
  }

  .liquid-chart-export-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 30%, transparent);
    background: transparent;
    color: rgb(var(--chart-fg-muted));
    cursor: pointer;
    opacity: 0.65;
    transition: opacity 120ms ease, background 120ms ease;
  }

  .liquid-chart:hover .liquid-chart-export-btn,
  .liquid-chart-export-btn[aria-expanded="true"] {
    opacity: 1;
    background: color-mix(in srgb, var(--color-surface-500) 12%, transparent);
  }

  .liquid-chart-export-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 0.25rem);
    z-index: 30;
    display: flex;
    flex-direction: column;
    min-width: 7.5rem;
    padding: 0.25rem;
    border-radius: 0.6rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 35%, transparent);
    background: rgb(var(--color-surface-800));
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.28);
  }

  .liquid-chart-export-menu button {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.35rem 0.55rem;
    border: none;
    border-radius: 0.4rem;
    background: transparent;
    color: rgb(var(--chart-fg));
    font-size: 0.8125rem;
    text-align: left;
    cursor: pointer;
  }

  .liquid-chart-export-menu button:hover {
    background: color-mix(in srgb, var(--color-surface-500) 16%, transparent);
  }

  .liquid-chart-export-note {
    position: absolute;
    right: 0;
    top: calc(100% + 0.35rem);
    z-index: 31;
    margin: 0;
    padding: 0.3rem 0.55rem;
    white-space: nowrap;
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-800));
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 35%, transparent);
    color: rgb(var(--chart-fg-muted));
    font-size: 0.75rem;
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.25);
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
