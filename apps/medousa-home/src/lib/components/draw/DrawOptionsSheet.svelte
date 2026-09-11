<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import type { DrawBrushKind } from "$lib/draw/drawDocument";

  type DrawTool = "ink" | "eraser" | "select" | "hand";
  type EraserMode = "partial" | "stroke";

  interface Props {
    open: boolean;
    tool: DrawTool;
    brushKind: DrawBrushKind;
    color: string;
    size: number;
    eraserMode: EraserMode;
    fingerDraw: boolean;
    zoomLabel: string;
    canUndo: boolean;
    canRedo: boolean;
    hasSelection: boolean;
    hasInk: boolean;
    onClose: () => void;
    onBrush: (brush: DrawBrushKind) => void;
    onColor: (color: string) => void;
    onSize: (size: number) => void;
    onEraserMode: (mode: EraserMode) => void;
    onFingerDraw: () => void;
    onZoomOut: () => void;
    onZoomIn: () => void;
    onFit: () => void;
    onUndo: () => void;
    onRedo: () => void;
    onDuplicate: () => void;
    onDelete: () => void;
    onClear: () => void;
  }

  let {
    open,
    tool,
    brushKind,
    color,
    size,
    eraserMode,
    fingerDraw,
    zoomLabel,
    canUndo,
    canRedo,
    hasSelection,
    hasInk,
    onClose,
    onBrush,
    onColor,
    onSize,
    onEraserMode,
    onFingerDraw,
    onZoomOut,
    onZoomIn,
    onFit,
    onUndo,
    onRedo,
    onDuplicate,
    onDelete,
    onClear,
  }: Props = $props();

  const COLORS = ["#e7e5e4", "#f87171", "#fb923c", "#facc15", "#4ade80", "#38bdf8", "#a78bfa"];
  const SIZES = [3, 6, 12, 24];
  const BRUSHES: { kind: DrawBrushKind; label: string }[] = [
    { kind: "pen", label: "Pen" },
    { kind: "pencil", label: "Pencil" },
    { kind: "marker", label: "Marker" },
    { kind: "highlighter", label: "Highlighter" },
  ];

  const summary = $derived(
    tool === "ink"
      ? `${BRUSHES.find((brush) => brush.kind === brushKind)?.label ?? "Pen"} · ${size}px`
      : tool === "eraser"
        ? `${eraserMode === "partial" ? "Partial" : "Whole-stroke"} eraser`
        : tool === "select"
          ? "Select and move ink"
          : "Pan the canvas",
  );
</script>

{#if open}
  <BodyPortal>
    <button
      type="button"
      class="draw-sheet-backdrop"
      aria-label="Close drawing options"
      onclick={onClose}
    ></button>
    <div class="draw-sheet" role="dialog" aria-modal="true" aria-labelledby="draw-options-title">
    <div class="draw-sheet-handle" aria-hidden="true"></div>
    <header class="draw-sheet-header">
      <div>
        <h2 id="draw-options-title">Drawing options</h2>
        <p>{summary}</p>
      </div>
      <button type="button" aria-label="Close drawing options" onclick={onClose}>
        <X size={19} strokeWidth={2} aria-hidden="true" />
      </button>
    </header>

    <div class="draw-sheet-body">
      {#if tool === "ink"}
        <section>
          <h3>Brush</h3>
          <div class="draw-option-grid" role="group" aria-label="Brush">
            {#each BRUSHES as brush (brush.kind)}
              <button type="button" class:active={brushKind === brush.kind} aria-pressed={brushKind === brush.kind} onclick={() => onBrush(brush.kind)}>{brush.label}</button>
            {/each}
          </div>
        </section>
        <section>
          <h3>Color</h3>
          <div class="draw-sheet-colors" role="group" aria-label="Color">
            {#each COLORS as swatch (swatch)}
              <button
                type="button"
                class:active={color === swatch}
                style={`--draw-color: ${swatch}`}
                aria-label={`Use ${swatch}`}
                aria-pressed={color === swatch}
                onclick={() => onColor(swatch)}
              ></button>
            {/each}
          </div>
        </section>
        <section>
          <h3>Size</h3>
          <div class="draw-size-options" role="group" aria-label="Brush size">
            {#each SIZES as brushSize (brushSize)}
              <button type="button" class:active={size === brushSize} aria-pressed={size === brushSize} onclick={() => onSize(brushSize)}>
                <span class="draw-size-dot" style={`--draw-size: ${Math.max(3, Math.sqrt(brushSize) * 3)}px`}></span>
                <span>{brushSize}</span>
              </button>
            {/each}
          </div>
        </section>
      {:else if tool === "eraser"}
        <section>
          <h3>Erase mode</h3>
          <div class="draw-option-grid" role="group" aria-label="Eraser mode">
            <button type="button" class:active={eraserMode === "partial"} aria-pressed={eraserMode === "partial"} onclick={() => onEraserMode("partial")}>Partial ink</button>
            <button type="button" class:active={eraserMode === "stroke"} aria-pressed={eraserMode === "stroke"} onclick={() => onEraserMode("stroke")}>Whole stroke</button>
          </div>
        </section>
      {:else if tool === "select"}
        <section>
          <h3>Selection</h3>
          <div class="draw-option-grid" role="group" aria-label="Selection actions">
            <button type="button" disabled={!hasSelection} onclick={onDuplicate}>Duplicate</button>
            <button type="button" disabled={!hasSelection} onclick={onDelete}>Delete</button>
          </div>
        </section>
      {/if}

      <section class="draw-sheet-row">
        <div>
          <h3>Finger draws</h3>
          <p>Off keeps one finger for moving the canvas.</p>
        </div>
        <button type="button" class="draw-toggle" class:active={fingerDraw} aria-pressed={fingerDraw} onclick={onFingerDraw}>{fingerDraw ? "On" : "Off"}</button>
      </section>

      <section>
        <h3>View</h3>
        <div class="draw-view-options" role="group" aria-label="View">
          <button type="button" aria-label="Zoom out" onclick={onZoomOut}>−</button>
          <button type="button" onclick={onFit}>Fit drawing</button>
          <span aria-live="polite">{zoomLabel}</span>
          <button type="button" aria-label="Zoom in" onclick={onZoomIn}>+</button>
        </div>
      </section>

      <section>
        <h3>Document</h3>
        <div class="draw-option-grid" role="group" aria-label="Document actions">
          <button type="button" disabled={!canUndo} onclick={onUndo}>Undo</button>
          <button type="button" disabled={!canRedo} onclick={onRedo}>Redo</button>
          <button type="button" class="danger" disabled={!hasInk} onclick={onClear}>Clear drawing</button>
        </div>
      </section>
    </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .draw-sheet-backdrop {
    position: fixed;
    inset: 0;
    z-index: 130;
    width: 100%;
    height: 100%;
    border: 0;
    border-radius: 0;
    background: rgb(0 0 0 / 0.42);
    backdrop-filter: blur(2px);
    touch-action: none;
  }

  .draw-sheet {
    position: fixed;
    right: 0.65rem;
    bottom: 0.65rem;
    z-index: 131;
    display: flex;
    width: min(30rem, calc(100% - 1.3rem));
    max-height: min(42rem, calc(100% - 1.3rem));
    flex-direction: column;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 1.15rem;
    background: rgb(var(--color-surface-900));
    color: rgb(var(--color-surface-50));
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.42);
  }

  .draw-sheet-handle {
    display: none;
    width: 2.5rem;
    height: 0.28rem;
    margin: 0.55rem auto 0;
    border-radius: 999px;
    background: rgb(var(--color-surface-400) / 0.55);
  }

  .draw-sheet-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem 1rem 0.8rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.2);
  }

  .draw-sheet-header h2,
  .draw-sheet-header p,
  .draw-sheet h3,
  .draw-sheet-body p { margin: 0; }
  .draw-sheet-header h2 { font-size: 1rem; font-weight: 700; }
  .draw-sheet-header p { margin-top: 0.18rem; color: rgb(var(--theme-text-secondary)); font-size: 0.76rem; }

  .draw-sheet-header > button {
    display: inline-flex;
    width: 2.4rem;
    height: 2.4rem;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    background: rgb(var(--color-surface-500) / 0.22);
  }

  .draw-sheet-body {
    display: grid;
    gap: 1.1rem;
    overflow-y: auto;
    padding: 1rem max(1rem, env(safe-area-inset-right, 0px)) max(1rem, env(safe-area-inset-bottom, 0px));
    overscroll-behavior: contain;
  }

  .draw-sheet section { display: grid; gap: 0.5rem; }
  .draw-sheet h3 { color: rgb(var(--theme-text-secondary)); font-size: 0.72rem; font-weight: 650; letter-spacing: 0.04em; text-transform: uppercase; }

  .draw-option-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.45rem;
  }

  .draw-option-grid button,
  .draw-size-options button,
  .draw-view-options button,
  .draw-toggle {
    min-height: 2.75rem;
    padding: 0.55rem 0.7rem;
    border-radius: 0.7rem;
    background: rgb(var(--color-surface-500) / 0.16);
    color: rgb(var(--color-surface-100));
    font-size: 0.78rem;
    font-weight: 600;
  }

  button:disabled { opacity: 0.3; }
  .draw-option-grid button.active,
  .draw-size-options button.active,
  .draw-toggle.active { background: rgb(var(--color-primary-500) / 0.2); color: rgb(var(--color-primary-200)); box-shadow: inset 0 0 0 1px rgb(var(--color-primary-400) / 0.48); }
  .draw-option-grid button.danger { color: rgb(248 113 113); }

  .draw-sheet-colors { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); align-items: center; gap: 0.45rem; }
  .draw-sheet-colors button { width: 100%; max-width: 2.45rem; aspect-ratio: 1; justify-self: center; border: 3px solid transparent; border-radius: 999px; background: var(--draw-color); box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.28); }
  .draw-sheet-colors button.active { border-color: rgb(var(--color-surface-900)); outline: 2px solid rgb(var(--color-primary-400)); }

  .draw-size-options { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 0.45rem; }
  .draw-size-options button { display: flex; align-items: center; justify-content: center; gap: 0.45rem; }
  .draw-size-dot { width: var(--draw-size); height: var(--draw-size); border-radius: 999px; background: currentColor; }
  .draw-sheet-row { grid-template-columns: minmax(0, 1fr) auto; align-items: center; }
  .draw-sheet-row p { margin-top: 0.25rem; color: rgb(var(--theme-text-secondary)); font-size: 0.76rem; }
  .draw-toggle { min-width: 4rem; }

  .draw-view-options { display: grid; grid-template-columns: 2.75rem minmax(6rem, 1fr) 4rem 2.75rem; align-items: center; gap: 0.4rem; }
  .draw-view-options span { text-align: center; color: rgb(var(--theme-text-secondary)); font-size: 0.78rem; font-variant-numeric: tabular-nums; }

  @media (max-width: 640px) {
    .draw-sheet { right: 0; bottom: 0; width: 100%; max-height: min(86dvh, 46rem); border-right: 0; border-bottom: 0; border-left: 0; border-radius: 1.25rem 1.25rem 0 0; }
    .draw-sheet-handle { display: block; }
    .draw-sheet-header { padding-top: 0.65rem; }
    .draw-option-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
</style>
