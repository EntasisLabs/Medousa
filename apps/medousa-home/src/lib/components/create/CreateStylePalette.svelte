<script lang="ts">
  import CreateTextControls from "./CreateTextControls.svelte";
  import type { TextStylePatch } from "$lib/create/creativeText";
  import {
    DRAW_BRUSHES,
    DRAW_BRUSH_SIZES,
    type DrawBrushKind,
  } from "$lib/draw/drawDocument";
  import type { DrawEraserMode } from "$lib/draw/drawInteraction";
  import type { CreativeTool } from "$lib/create/creativeScene";
  import { X, Square } from "@lucide/svelte";
  import {
    canvasColors,
    type CanvasFill,
    type CanvasSize,
  } from "$lib/create/creativeInteraction";
  let {
    tool = "select",
    brushKind = "pen",
    drawSize = 6,
    eraserMode = "partial",
    fingerDraw = false,
    selectionMode = "box",
    onSelectionMode = () => {},
    onDrawOptions = () => {},
    inkColor,
    color,
    size,
    fill,
    showFill,
    showPaint = true,
    showSize = true,
    textStyle,
    onTextStyle = () => {},
    onTextLeave = () => {},
    context,
    disabled,
    onchange,
    onclose,
  }: {
    tool?: CreativeTool;
    brushKind?: DrawBrushKind;
    drawSize?: number;
    eraserMode?: DrawEraserMode;
    fingerDraw?: boolean;
    selectionMode?: "box" | "lasso";
    onSelectionMode?: (mode: "box" | "lasso") => void;
    onDrawOptions?: (patch: {
      brush?: DrawBrushKind;
      size?: number;
      eraser?: DrawEraserMode;
      finger?: boolean;
    }) => void;
    inkColor: string;
    color: string;
    size: CanvasSize;
    fill: CanvasFill;
    showFill: boolean;
    showPaint?: boolean;
    showSize?: boolean;
    textStyle?: TextStylePatch;
    onTextStyle?: (patch: TextStylePatch) => void;
    onTextLeave?: () => void;
    context: string;
    disabled: boolean;
    onchange: (patch: {
      color?: string;
      size?: CanvasSize;
      fill?: CanvasFill;
    }) => void;
    onclose: () => void;
  } = $props();
</script>

<section
  class="create-style-palette"
  aria-label="Style palette"
  onpointerdown={(e) => { if (!(e.target instanceof Element && e.target.closest("input, select"))) e.preventDefault(); }}
>
  <header>
    <span>{context}</span><button
      type="button"
      aria-label="Hide style palette"
      onclick={onclose}><X size={13} /></button
    >
  </header>
  {#if tool === "select"}
    <div class="create-brushes" role="group" aria-label="Selection mode">
      <button type="button" {disabled} aria-pressed={selectionMode === "box"}
        onclick={() => onSelectionMode("box")}>Box</button>
      <button type="button" {disabled} aria-pressed={selectionMode === "lasso"}
        onclick={() => onSelectionMode("lasso")}>Lasso</button>
    </div>
  {:else if tool === "ink"}
    <div class="create-brushes" role="group" aria-label="Brush">
      {#each DRAW_BRUSHES as brush}<button
          type="button"
          {disabled}
          aria-pressed={brushKind === brush.kind}
          onclick={() => onDrawOptions({ brush: brush.kind })}
          >{brush.label}</button
        >{/each}
    </div>
  {:else if tool === "erase"}
    <div
      class="create-brushes eraser-modes"
      role="group"
      aria-label="Eraser mode"
    >
      <button
        type="button"
        {disabled}
        aria-pressed={eraserMode === "partial"}
        onclick={() => onDrawOptions({ eraser: "partial" })}>Partial ink</button
      >
      <button
        type="button"
        {disabled}
        aria-pressed={eraserMode === "stroke"}
        onclick={() => onDrawOptions({ eraser: "stroke" })}>Whole object</button
      >
    </div>
  {/if}
  {#if showPaint}<div class="create-swatches" role="group" aria-label="Color">
    {#each canvasColors.map( (c) => (c.name === "Ink" ? { ...c, value: inkColor } : c) ) as swatch}
      <button
        type="button"
        aria-label={swatch.name}
        aria-pressed={color.toLowerCase() === swatch.value}
        {disabled}
        style:--swatch={swatch.value}
        onclick={() => onchange({ color: swatch.value })}><span></span></button
      >
    {/each}
  </div>
  {/if}
  {#if textStyle}<div class="create-style-section">
    <CreateTextControls value={textStyle} {disabled} onchange={onTextStyle} onleave={onTextLeave}/>
  </div>{/if}
  {#if showSize && !textStyle}<div class="create-style-section">
    <span
      >{tool === "ink" || tool === "erase"
        ? `Size · ${drawSize}px`
        : "Size"}</span
    >
    <div
      class="create-sizes"
      role="group"
      aria-label={tool === "ink" || tool === "erase" ? "Brush size" : "Size"}
    >
      {#if tool === "ink" || tool === "erase"}
        {#each DRAW_BRUSH_SIZES as value}<button
            type="button"
            {disabled}
            aria-pressed={drawSize === value}
            onclick={() => onDrawOptions({ size: value })}>{value}</button
          >{/each}
      {:else}
        {#each ["s", "m", "l", "xl"] as value}<button
            type="button"
            {disabled}
            aria-pressed={size === value}
            onclick={() => onchange({ size: value as CanvasSize })}
            >{value.toUpperCase()}</button
          >{/each}
      {/if}
    </div>
  </div>
  {/if}
  {#if tool === "ink" || tool === "erase"}
    <div class="create-style-section">
      <button
        type="button"
        class="finger-toggle"
        {disabled}
        aria-pressed={fingerDraw}
        onclick={() => onDrawOptions({ finger: !fingerDraw })}
        >Finger draws <span>{fingerDraw ? "On" : "Off"}</span></button
      >
    </div>
  {/if}
  {#if showFill}<div class="create-style-section">
      <span>Fill</span>
      <div class="create-fills" role="group" aria-label="Shape fill">
        <button
          type="button"
          aria-label="No fill"
          aria-pressed={fill === "none"}
          {disabled}
          onclick={() => onchange({ fill: "none" })}
          ><Square size={15} /></button
        >
        <button
          type="button"
          aria-label="Soft fill"
          aria-pressed={fill === "soft"}
          {disabled}
          onclick={() => onchange({ fill: "soft" })}
          ><span class="soft"></span></button
        >
        <button
          type="button"
          aria-label="Solid fill"
          aria-pressed={fill === "solid"}
          {disabled}
          onclick={() => onchange({ fill: "solid" })}
          ><span class="solid"></span></button
        >
      </div>
    </div>{/if}
</section>

<style>
  .create-style-palette {
    width: 154px;
    padding: 11px;
    background: var(--create-raised);
    border: 1px solid var(--create-line);
    border-radius: 11px;
    box-shadow: 0 3px 12px #0001;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 5px;
    margin-bottom: 8px;
    font-size: 11px;
    color: var(--create-dim);
  }
  header span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  button {
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
  }
  header button {
    padding: 4px;
  }
  button:hover,
  button[aria-pressed="true"] {
    background: var(--create-soft);
  }
  button:disabled {
    opacity: 0.4;
  }
  .create-swatches {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 5px;
  }
  .create-swatches button {
    width: 34px;
    height: 34px;
    margin: auto;
  }
  .create-swatches span {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--swatch);
    border: 1px solid var(--create-line);
  }
  .create-swatches [aria-pressed="true"] {
    box-shadow: inset 0 0 0 1px var(--create-line);
  }
  .create-swatches [aria-pressed="true"] span {
    box-shadow:
      inset 0 0 0 5px var(--swatch),
      inset 0 0 0 8px var(--create-raised);
  }
  .create-style-section {
    margin-top: 11px;
    padding-top: 10px;
    border-top: 1px solid var(--create-line);
  }
  .create-style-section > span {
    display: block;
    font-size: 11px;
    color: var(--create-dim);
    margin-bottom: 6px;
  }
  .create-sizes,
  .create-fills {
    display: flex;
    gap: 3px;
  }
  .create-sizes button,
  .create-fills button {
    flex: 1;
    height: 30px;
    font-size: 11px;
  }
  .create-fills span {
    height: 14px;
    width: 14px;
    border: 1px solid var(--create-dim);
    border-radius: 2px;
  }
  .soft {
    background: var(--create-soft);
  }
  .solid {
    background: var(--create-dim);
  }
  @media (pointer: coarse) {
    .create-swatches button {
      min-height: 40px;
    }
    .create-sizes button,
    .create-fills button {
      min-height: 38px;
    }
  }
  .create-brushes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 3px;
    margin-bottom: 10px;
  }
  .create-brushes button {
    padding: 7px 2px;
    font-size: 11px;
    min-height: 30px;
  }
  .eraser-modes {
    grid-template-columns: 1fr;
  }
  .finger-toggle {
    width: 100%;
    display: flex;
    justify-content: space-between;
    padding: 5px 0;
    font-size: 11px;
  }
  .finger-toggle span {
    color: var(--create-dim);
  }
</style>
