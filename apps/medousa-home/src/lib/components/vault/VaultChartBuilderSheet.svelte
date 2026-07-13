<script lang="ts">
  import { tick } from "svelte";
  import { X } from "@lucide/svelte";
  import {
    CHART_FENCE_TYPE_OPTIONS,
    type ChartFenceType,
  } from "$lib/utils/liquidFenceTemplates";
  import type { ChartFenceKv } from "$lib/utils/vaultChartFence";
  import { MARKDOWN_COLOR_OPTIONS } from "$lib/utils/vaultMarkdownColors";

  interface Props {
    open: boolean;
    initialKv?: ChartFenceKv | null;
    onSave: (kv: ChartFenceKv) => void;
    onClose: () => void;
  }

  let { open, initialKv = null, onSave, onClose }: Props = $props();

  const LEGEND_OPTIONS = [
    { id: "", label: "Auto" },
    { id: "bottom", label: "Bottom" },
    { id: "top", label: "Top" },
    { id: "none", label: "Off" },
  ] as const;

  const LABELS_OPTIONS = [
    { id: "", label: "Auto" },
    { id: "none", label: "Off" },
    { id: "value", label: "Values" },
    { id: "category", label: "Categories" },
    { id: "both", label: "Both" },
  ] as const;

  const SURFACE_OPTIONS = [
    { id: "", label: "Auto" },
    { id: "soft", label: "Soft" },
    { id: "muted", label: "Muted" },
    { id: "none", label: "None" },
    { id: "gray/12", label: "Wash" },
  ] as const;

  let type = $state<ChartFenceType>("bar");
  let title = $state("");
  let description = $state("");
  let legend = $state("");
  let labels = $state("");
  let surface = $state("");
  let colors = $state("");
  let seriesMarks = $state("");

  const showSeriesMarks = $derived(type === "combo");
  const typeLabel = $derived(
    CHART_FENCE_TYPE_OPTIONS.find((row) => row.id === type)?.label ?? "Chart",
  );

  $effect(() => {
    if (!open) return;
    const seed = initialKv;
    type = seed?.type ?? "bar";
    title = seed?.title ?? "";
    description = seed?.description ?? "";
    legend = seed?.legend ?? "";
    labels = seed?.labels ?? "";
    surface = seed?.surface ?? "";
    colors = seed?.colors ?? "";
    seriesMarks = seed?.seriesMarks ?? (type === "combo" ? "bar, line" : "");
    void tick().then(() => {
      (
        document.querySelector("[data-chart-builder-title]") as HTMLInputElement | null
      )?.focus();
    });
  });

  function toggleColor(id: string) {
    const parts = colors
      .split(/[,|]/)
      .map((c) => c.trim())
      .filter(Boolean);
    const idx = parts.findIndex((c) => c.toLowerCase() === id.toLowerCase());
    if (idx >= 0) parts.splice(idx, 1);
    else parts.push(id);
    colors = parts.join(", ");
  }

  function colorActive(id: string): boolean {
    return colors
      .split(/[,|]/)
      .map((c) => c.trim().toLowerCase())
      .includes(id.toLowerCase());
  }

  function commit() {
    onSave({
      type,
      title,
      description,
      legend,
      labels,
      surface,
      colors,
      seriesMarks: showSeriesMarks ? seriesMarks : "",
    });
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="chart-builder-title"
    tabindex="-1"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <form
      class="vault-interact-sheet vault-chart-builder-sheet"
      onsubmit={(event) => {
        event.preventDefault();
        commit();
      }}
    >
      <header class="vault-chart-builder-header">
        <div class="min-w-0">
          <p class="vault-interact-kicker">Chart</p>
          <h3 id="chart-builder-title" class="vault-chart-builder-heading">
            Configure {typeLabel}
          </h3>
        </div>
        <button
          type="button"
          class="vault-interact-dismiss shrink-0"
          aria-label="Close"
          onclick={onClose}
        >
          <X size={14} strokeWidth={2} />
        </button>
      </header>

      <section class="vault-chart-builder-section" aria-label="Chart type">
        <div class="vault-chip-row" role="listbox" aria-label="Chart type">
          {#each CHART_FENCE_TYPE_OPTIONS as option (option.id)}
            <button
              type="button"
              class="vault-chip"
              class:vault-chip--active={type === option.id}
              role="option"
              aria-selected={type === option.id}
              onclick={() => (type = option.id)}
            >
              {option.label}
            </button>
          {/each}
        </div>
      </section>

      <section class="vault-chart-builder-section">
        <label class="vault-chart-builder-label" for="chart-builder-title-input">Title</label>
        <input
          id="chart-builder-title-input"
          class="vault-chart-builder-input"
          type="text"
          placeholder="Untitled chart"
          data-chart-builder-title
          bind:value={title}
        />
        <input
          class="vault-chart-builder-input vault-chart-builder-input--quiet"
          type="text"
          placeholder="Short description"
          aria-label="Description"
          bind:value={description}
        />
      </section>

      <section class="vault-chart-builder-section" aria-label="Legend">
        <p class="vault-chart-builder-label">Legend</p>
        <div class="vault-chip-row" role="listbox" aria-label="Legend position">
          {#each LEGEND_OPTIONS as option (option.id)}
            <button
              type="button"
              class="vault-chip"
              class:vault-chip--active={legend === option.id}
              role="option"
              aria-selected={legend === option.id}
              onclick={() => (legend = option.id)}
            >
              {option.label}
            </button>
          {/each}
        </div>
      </section>

      <section class="vault-chart-builder-section" aria-label="Labels">
        <p class="vault-chart-builder-label">Labels</p>
        <div class="vault-chip-row" role="listbox" aria-label="Data labels">
          {#each LABELS_OPTIONS as option (option.id)}
            <button
              type="button"
              class="vault-chip"
              class:vault-chip--active={labels === option.id}
              role="option"
              aria-selected={labels === option.id}
              onclick={() => (labels = option.id)}
            >
              {option.label}
            </button>
          {/each}
        </div>
      </section>

      <section class="vault-chart-builder-section" aria-label="Surface">
        <p class="vault-chart-builder-label">Surface</p>
        <div class="vault-chip-row" role="listbox" aria-label="Plot surface">
          {#each SURFACE_OPTIONS as option (option.id)}
            <button
              type="button"
              class="vault-chip"
              class:vault-chip--active={surface === option.id}
              role="option"
              aria-selected={surface === option.id}
              onclick={() => (surface = option.id)}
            >
              {option.label}
            </button>
          {/each}
        </div>
      </section>

      {#if showSeriesMarks}
        <section class="vault-chart-builder-section">
          <label class="vault-chart-builder-label" for="chart-builder-marks">Series marks</label>
          <input
            id="chart-builder-marks"
            class="vault-chart-builder-input"
            type="text"
            placeholder="bar, line"
            bind:value={seriesMarks}
          />
        </section>
      {/if}

      <section class="vault-chart-builder-section" aria-label="Colors">
        <p class="vault-chart-builder-label">Colors</p>
        <div class="vault-chart-builder-swatches" role="group" aria-label="Chart colors">
          {#each MARKDOWN_COLOR_OPTIONS as color (color.id)}
            <button
              type="button"
              class="vault-color-swatch"
              class:vault-chart-builder-swatch--on={colorActive(color.id)}
              title={color.label}
              aria-label={`Color: ${color.label}`}
              aria-pressed={colorActive(color.id)}
              style:background-color={color.swatch}
              onclick={() => toggleColor(color.id)}
            ></button>
          {/each}
        </div>
      </section>

      <p class="vault-chart-builder-hint">
        Data stays in the chart fence table — switch to Source to edit rows.
      </p>

      <footer class="vault-chart-builder-footer">
        <button type="button" class="vault-chart-builder-cancel" onclick={onClose}>
          Cancel
        </button>
        <button type="submit" class="vault-interact-commit">Save</button>
      </footer>
    </form>
  </div>
{/if}
