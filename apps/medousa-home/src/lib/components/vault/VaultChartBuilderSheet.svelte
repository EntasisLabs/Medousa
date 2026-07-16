<script lang="ts">
  import { X } from "@lucide/svelte";
  import {
    CHART_FENCE_TYPE_OPTIONS,
    type ChartFenceType,
  } from "$lib/utils/liquidFenceTemplates";
  import type { ChartFenceKv } from "$lib/utils/vaultChartFence";
  import {
    MARKDOWN_COLOR_HEX,
    MARKDOWN_COLOR_OPTIONS,
    isMarkdownColorId,
    normalizeMarkdownHexColor,
    resolveMarkdownColorCss,
  } from "$lib/utils/vaultMarkdownColors";

  interface Props {
    open: boolean;
    initialKv?: ChartFenceKv | null;
    onSave: (kv: ChartFenceKv) => void;
    onClose: () => void;
  }

  let { open, initialKv = null, onSave, onClose }: Props = $props();

  type EditKey =
    | "kind"
    | "width"
    | "height"
    | "color"
    | "legend"
    | "labels"
    | "surface"
    | null;

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

  const WIDTH_OPTIONS = [
    { id: "", label: "Full" },
    { id: "sm", label: "S" },
    { id: "md", label: "M" },
    { id: "lg", label: "L" },
  ] as const;

  const HEIGHT_OPTIONS = [
    { id: "", label: "Auto" },
    { id: "sm", label: "S" },
    { id: "md", label: "M" },
    { id: "lg", label: "L" },
    { id: "xl", label: "XL" },
  ] as const;

  let type = $state<ChartFenceType>("bar");
  let title = $state("");
  let description = $state("");
  let legend = $state("");
  let labels = $state("");
  let surface = $state("");
  let colors = $state("");
  let seriesMarks = $state("");
  let width = $state("");
  let height = $state("");
  let showDescription = $state(false);
  let editing = $state<EditKey>(null);
  let widthDraft = $state("");
  let heightDraft = $state("");
  let hexPickerOpen = $state(false);
  let hexDraft = $state("#60A5FA");

  const showSeriesMarks = $derived(type === "combo");
  const colorTokens = $derived(
    colors
      .split(/[,|]/)
      .map((c) => c.trim())
      .filter(Boolean),
  );

  const kindLabel = $derived(
    CHART_FENCE_TYPE_OPTIONS.find((o) => o.id === type)?.label ?? type,
  );
  const widthLabel = $derived(labelForSize(width, WIDTH_OPTIONS, "Full"));
  const heightLabel = $derived(labelForSize(height, HEIGHT_OPTIONS, "Auto"));
  const legendLabel = $derived(labelForEnum(legend, LEGEND_OPTIONS));
  const labelsLabel = $derived(labelForEnum(labels, LABELS_OPTIONS));
  const surfaceLabel = $derived(labelForEnum(surface, SURFACE_OPTIONS));

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
    width = seed?.width ?? "";
    height = seed?.height ?? "";
    showDescription = Boolean(seed?.description?.trim());
    editing = null;
    widthDraft = seed?.width ?? "";
    heightDraft = seed?.height ?? "";
    hexPickerOpen = false;
  });

  function labelForSize(
    value: string,
    options: ReadonlyArray<{ id: string; label: string }>,
    emptyLabel: string,
  ): string {
    const trimmed = value.trim();
    if (!trimmed) return emptyLabel;
    const hit = options.find((o) => o.id === trimmed.toLowerCase());
    return hit?.label ?? trimmed;
  }

  function labelForEnum(
    value: string,
    options: ReadonlyArray<{ id: string; label: string }>,
  ): string {
    const hit = options.find((o) => o.id === value);
    return hit?.label ?? (value.trim() || "Auto");
  }

  function tokenSwatch(token: string): string {
    const css = resolveMarkdownColorCss(token);
    if (css) return css;
    if (isMarkdownColorId(token)) return MARKDOWN_COLOR_HEX[token.toLowerCase() as keyof typeof MARKDOWN_COLOR_HEX];
    return token;
  }

  function beginEdit(key: EditKey) {
    if (editing === key) return;
    if (editing === "width") width = widthDraft.trim();
    if (editing === "height") height = heightDraft.trim();
    editing = key;
    hexPickerOpen = false;
    if (key === "width") widthDraft = width;
    if (key === "height") heightDraft = height;
  }

  function closeEdit() {
    editing = null;
    hexPickerOpen = false;
  }

  function pickEnum(
    key: "legend" | "labels" | "surface",
    id: string,
  ) {
    if (key === "legend") legend = id;
    if (key === "labels") labels = id;
    if (key === "surface") surface = id;
    closeEdit();
  }

  function pickKind(next: ChartFenceType) {
    type = next;
    if (next === "combo" && !seriesMarks.trim()) seriesMarks = "bar, line";
    closeEdit();
  }

  function pickWidthPreset(id: string) {
    width = id;
    widthDraft = id;
    closeEdit();
  }

  function pickHeightPreset(id: string) {
    height = id;
    heightDraft = id;
    closeEdit();
  }

  function commitWidthDraft() {
    width = widthDraft.trim();
    closeEdit();
  }

  function commitHeightDraft() {
    height = heightDraft.trim();
    closeEdit();
  }

  function toggleColor(token: string) {
    const parts = colorTokens.slice();
    const idx = parts.findIndex((c) => c.toLowerCase() === token.toLowerCase());
    if (idx >= 0) parts.splice(idx, 1);
    else parts.push(token);
    colors = parts.join(", ");
  }

  function colorActive(token: string): boolean {
    return colorTokens.some((c) => c.toLowerCase() === token.toLowerCase());
  }

  function openHexPicker() {
    hexDraft = "#60A5FA";
    hexPickerOpen = true;
  }

  function cancelHexPicker() {
    hexPickerOpen = false;
  }

  function applyHexPicker() {
    const hex = normalizeMarkdownHexColor(hexDraft);
    if (!hex) return;
    if (!colorActive(hex)) toggleColor(hex);
    hexPickerOpen = false;
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
      width,
      height,
    });
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (hexPickerOpen) {
        cancelHexPicker();
        return;
      }
      if (editing) {
        closeEdit();
        return;
      }
      onClose();
    }
  }

  function onFormClick(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (target.closest("[data-chart-fact-row]") || target.closest(".vault-color-hex")) {
      return;
    }
    if (editing === "width") {
      commitWidthDraft();
      return;
    }
    if (editing === "height") {
      commitHeightDraft();
      return;
    }
    if (editing) closeEdit();
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
      onclick={onFormClick}
      onsubmit={(event) => {
        event.preventDefault();
        commit();
      }}
    >
      <header class="vault-chart-builder-header">
        <div class="vault-chart-builder-identity min-w-0">
          <input
            id="chart-builder-title"
            class="vault-chart-builder-title-input"
            type="text"
            placeholder="Untitled chart"
            aria-label="Chart title"
            bind:value={title}
          />
          {#if showDescription || description.trim()}
            <input
              class="vault-chart-builder-desc-input"
              type="text"
              placeholder="A line about what this shows"
              aria-label="Description"
              bind:value={description}
              onblur={() => {
                if (!description.trim()) showDescription = false;
              }}
            />
          {:else}
            <button
              type="button"
              class="vault-chart-builder-add-desc"
              onclick={() => (showDescription = true)}
            >
              Add a note
            </button>
          {/if}
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

      <div class="vault-chart-facts">
        <!-- Kind — full type set (Live only has quick chips) -->
        <div
          class="vault-chart-fact"
          class:vault-chart-fact--open={editing === "kind"}
          data-chart-fact-row
        >
          <div class="vault-chart-fact__row">
            <span class="vault-chart-fact__label">Kind</span>
            {#if editing !== "kind"}
              <button
                type="button"
                class="vault-chart-fact__value"
                onclick={() => beginEdit("kind")}
              >
                {kindLabel}
              </button>
            {/if}
          </div>
          {#if editing === "kind"}
            <div class="vault-chart-fact__editor">
              <div class="vault-chart-builder-kinds" role="listbox" aria-label="Chart type">
                {#each CHART_FENCE_TYPE_OPTIONS as option (option.id)}
                  <button
                    type="button"
                    class="vault-chart-builder-kind"
                    class:vault-chart-builder-kind--on={type === option.id}
                    role="option"
                    aria-selected={type === option.id}
                    onclick={() => pickKind(option.id)}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        <!-- Width -->
        <div
          class="vault-chart-fact"
          class:vault-chart-fact--open={editing === "width"}
          data-chart-fact-row
        >
          <div class="vault-chart-fact__row">
            <span class="vault-chart-fact__label">Width</span>
            {#if editing !== "width"}
              <button
                type="button"
                class="vault-chart-fact__value"
                onclick={() => beginEdit("width")}
              >
                {widthLabel}
              </button>
            {/if}
          </div>
          {#if editing === "width"}
            <div class="vault-chart-fact__editor">
              <div class="vault-chart-builder-seg" role="listbox" aria-label="Width presets">
                {#each WIDTH_OPTIONS as option (option.id)}
                  <button
                    type="button"
                    class="vault-chart-builder-seg__btn"
                    class:vault-chart-builder-seg__btn--on={width === option.id}
                    role="option"
                    aria-selected={width === option.id}
                    onclick={() => pickWidthPreset(option.id)}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
              <input
                class="vault-chart-fact__free"
                type="text"
                spellcheck="false"
                placeholder="18rem, 70%, 420…"
                aria-label="Custom width"
                bind:value={widthDraft}
                onkeydown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    commitWidthDraft();
                  }
                }}
              />
            </div>
          {/if}
        </div>

        <!-- Height -->
        <div
          class="vault-chart-fact"
          class:vault-chart-fact--open={editing === "height"}
          data-chart-fact-row
        >
          <div class="vault-chart-fact__row">
            <span class="vault-chart-fact__label">Height</span>
            {#if editing !== "height"}
              <button
                type="button"
                class="vault-chart-fact__value"
                onclick={() => beginEdit("height")}
              >
                {heightLabel}
              </button>
            {/if}
          </div>
          {#if editing === "height"}
            <div class="vault-chart-fact__editor">
              <div class="vault-chart-builder-seg" role="listbox" aria-label="Height presets">
                {#each HEIGHT_OPTIONS as option (option.id)}
                  <button
                    type="button"
                    class="vault-chart-builder-seg__btn"
                    class:vault-chart-builder-seg__btn--on={height === option.id}
                    role="option"
                    aria-selected={height === option.id}
                    onclick={() => pickHeightPreset(option.id)}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
              <input
                class="vault-chart-fact__free"
                type="text"
                spellcheck="false"
                placeholder="14rem, 320, auto…"
                aria-label="Custom height"
                bind:value={heightDraft}
                onkeydown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    commitHeightDraft();
                  }
                }}
              />
            </div>
          {/if}
        </div>

        <!-- Color -->
        <div
          class="vault-chart-fact"
          class:vault-chart-fact--open={editing === "color"}
          data-chart-fact-row
        >
          <div class="vault-chart-fact__row">
            <span class="vault-chart-fact__label">Color</span>
            {#if editing !== "color"}
              <button
                type="button"
                class="vault-chart-fact__value vault-chart-fact__value--swash"
                onclick={() => beginEdit("color")}
                aria-label="Edit colors"
              >
                {#if colorTokens.length === 0}
                  <span class="vault-chart-fact__muted">Theme default</span>
                {:else}
                  <span class="vault-chart-fact__swash" aria-hidden="true">
                    {#each colorTokens as token (token)}
                      <span
                        class="vault-chart-fact__dot"
                        style:background-color={tokenSwatch(token)}
                      ></span>
                    {/each}
                  </span>
                {/if}
              </button>
            {/if}
          </div>
          {#if editing === "color"}
            <div class="vault-chart-fact__editor vault-chart-fact__editor--color">
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
                {#each colorTokens.filter((t) => !isMarkdownColorId(t)) as custom (custom)}
                  <button
                    type="button"
                    class="vault-color-swatch vault-chart-builder-swatch--on"
                    title={custom}
                    aria-label={`Remove color ${custom}`}
                    aria-pressed="true"
                    style:background-color={tokenSwatch(custom)}
                    onclick={() => toggleColor(custom)}
                  ></button>
                {/each}
                <div class="vault-color-hex">
                  <button
                    type="button"
                    class="vault-color-swatch vault-color-swatch--custom"
                    title="Custom hex color"
                    aria-label="Custom hex color"
                    aria-expanded={hexPickerOpen}
                    style:background={hexPickerOpen ? hexDraft : undefined}
                    onclick={() => {
                      if (hexPickerOpen) cancelHexPicker();
                      else openHexPicker();
                    }}
                  ></button>
                  {#if hexPickerOpen}
                    <div
                      class="vault-color-hex-popover"
                      role="dialog"
                      aria-label="Pick a custom color"
                    >
                      <input
                        type="color"
                        class="vault-color-hex-wheel"
                        bind:value={hexDraft}
                      />
                      <input
                        class="vault-color-hex-field"
                        type="text"
                        spellcheck="false"
                        maxlength="9"
                        aria-label="Hex value"
                        bind:value={hexDraft}
                        onkeydown={(event) => {
                          if (event.key === "Enter") {
                            event.preventDefault();
                            applyHexPicker();
                          }
                          if (event.key === "Escape") {
                            event.preventDefault();
                            cancelHexPicker();
                          }
                        }}
                      />
                      <button
                        type="button"
                        class="vault-color-hex-apply"
                        onclick={applyHexPicker}
                      >
                        Add
                      </button>
                    </div>
                  {/if}
                </div>
              </div>
            </div>
          {/if}
        </div>

        <!-- Legend -->
        <div
          class="vault-chart-fact"
          class:vault-chart-fact--open={editing === "legend"}
          data-chart-fact-row
        >
          <div class="vault-chart-fact__row">
            <span class="vault-chart-fact__label">Legend</span>
            {#if editing !== "legend"}
              <button
                type="button"
                class="vault-chart-fact__value"
                onclick={() => beginEdit("legend")}
              >
                {legendLabel}
              </button>
            {/if}
          </div>
          {#if editing === "legend"}
            <div class="vault-chart-fact__editor">
              <div class="vault-chart-builder-seg" role="listbox" aria-label="Legend">
                {#each LEGEND_OPTIONS as option (option.id)}
                  <button
                    type="button"
                    class="vault-chart-builder-seg__btn"
                    class:vault-chart-builder-seg__btn--on={legend === option.id}
                    role="option"
                    aria-selected={legend === option.id}
                    onclick={() => pickEnum("legend", option.id)}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        <!-- Labels -->
        <div
          class="vault-chart-fact"
          class:vault-chart-fact--open={editing === "labels"}
          data-chart-fact-row
        >
          <div class="vault-chart-fact__row">
            <span class="vault-chart-fact__label">Labels</span>
            {#if editing !== "labels"}
              <button
                type="button"
                class="vault-chart-fact__value"
                onclick={() => beginEdit("labels")}
              >
                {labelsLabel}
              </button>
            {/if}
          </div>
          {#if editing === "labels"}
            <div class="vault-chart-fact__editor">
              <div class="vault-chart-builder-seg" role="listbox" aria-label="Labels">
                {#each LABELS_OPTIONS as option (option.id)}
                  <button
                    type="button"
                    class="vault-chart-builder-seg__btn"
                    class:vault-chart-builder-seg__btn--on={labels === option.id}
                    role="option"
                    aria-selected={labels === option.id}
                    onclick={() => pickEnum("labels", option.id)}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        <!-- Surface -->
        <div
          class="vault-chart-fact"
          class:vault-chart-fact--open={editing === "surface"}
          data-chart-fact-row
        >
          <div class="vault-chart-fact__row">
            <span class="vault-chart-fact__label">Surface</span>
            {#if editing !== "surface"}
              <button
                type="button"
                class="vault-chart-fact__value"
                onclick={() => beginEdit("surface")}
              >
                {surfaceLabel}
              </button>
            {/if}
          </div>
          {#if editing === "surface"}
            <div class="vault-chart-fact__editor">
              <div class="vault-chart-builder-seg" role="listbox" aria-label="Surface">
                {#each SURFACE_OPTIONS as option (option.id)}
                  <button
                    type="button"
                    class="vault-chart-builder-seg__btn"
                    class:vault-chart-builder-seg__btn--on={surface === option.id}
                    role="option"
                    aria-selected={surface === option.id}
                    onclick={() => pickEnum("surface", option.id)}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        {#if showSeriesMarks}
          <div class="vault-chart-fact" data-chart-fact-row>
            <div class="vault-chart-fact__row vault-chart-fact__row--marks">
              <span class="vault-chart-fact__label">Marks</span>
              <input
                class="vault-chart-builder-marks"
                type="text"
                placeholder="bar, line"
                aria-label="Series marks"
                bind:value={seriesMarks}
              />
            </div>
          </div>
        {/if}
      </div>

      <footer class="vault-chart-builder-footer">
        <button type="button" class="vault-chart-builder-cancel" onclick={onClose}>
          Cancel
        </button>
        <button type="submit" class="vault-chart-builder-done">Done</button>
      </footer>
    </form>
  </div>
{/if}
