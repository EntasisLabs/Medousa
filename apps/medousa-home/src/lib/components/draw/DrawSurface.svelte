<script lang="ts">
  import {
    cloneDrawDocument,
    drawStrokePath,
    encodeDrawDocument,
    type DrawDocument,
    type DrawPoint,
    type DrawStroke,
  } from "$lib/draw/drawDocument";
  import { randomUuid } from "$lib/utils/randomUuid";
  import { untrack } from "svelte";

  interface Props {
    document: DrawDocument;
    editable?: boolean;
    variant?: "embedded" | "full";
    onchange?: (document: DrawDocument) => void;
  }

  let {
    document,
    editable = false,
    variant = "embedded",
    onchange = () => undefined,
  }: Props = $props();

  type Tool = "pen" | "eraser";

  const COLORS = ["#e7e5e4", "#f87171", "#fb923c", "#facc15", "#4ade80", "#38bdf8", "#a78bfa"];
  const WIDTHS = [3, 6, 12];

  let scene = $state<DrawDocument>(untrack(() => cloneDrawDocument(document)));
  let syncedFingerprint = $state(untrack(() => encodeDrawDocument(document)));
  let tool = $state<Tool>("pen");
  let color = $state(COLORS[0]);
  let width = $state(WIDTHS[1]);
  let activeStroke = $state<DrawStroke | null>(null);
  let activePointer = $state<number | null>(null);
  let eraseSnapshot = $state<DrawDocument | null>(null);
  let undoStack = $state<DrawDocument[]>([]);
  let redoStack = $state<DrawDocument[]>([]);
  let svgEl = $state<SVGSVGElement | null>(null);
  let stageEl = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    const fingerprint = encodeDrawDocument(document);
    if (fingerprint === syncedFingerprint || activePointer != null) return;
    scene = cloneDrawDocument(document);
    syncedFingerprint = fingerprint;
    undoStack = [];
    redoStack = [];
  });

  function pointFromEvent(event: PointerEvent): DrawPoint | null {
    if (!svgEl) return null;
    const matrix = svgEl.getScreenCTM();
    if (!matrix) return null;
    const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse());
    return {
      x: Math.round(point.x * 10) / 10,
      y: Math.round(point.y * 10) / 10,
      ...(event.pressure > 0 ? { pressure: Math.round(event.pressure * 100) / 100 } : {}),
    };
  }

  function emitScene(next: DrawDocument) {
    scene = cloneDrawDocument(next);
    syncedFingerprint = encodeDrawDocument(scene);
    onchange(cloneDrawDocument(scene));
  }

  function remember(snapshot = scene) {
    undoStack = [...undoStack.slice(-49), cloneDrawDocument(snapshot)];
    redoStack = [];
  }

  function beginStroke(event: PointerEvent) {
    if (
      !editable ||
      event.button !== 0 ||
      event.isPrimary === false ||
      activePointer != null
    ) return;
    const point = pointFromEvent(event);
    if (!point) return;
    event.preventDefault();
    event.stopPropagation();
    try {
      stageEl?.focus({ preventScroll: true });
    } catch {
      stageEl?.focus();
    }
    try {
      stageEl?.setPointerCapture(event.pointerId);
    } catch {
      // Older mobile webviews can reject capture while still delivering the gesture.
    }
    activePointer = event.pointerId;

    if (tool === "eraser") {
      eraseSnapshot = cloneDrawDocument(scene);
      eraseAt(point);
      return;
    }

    activeStroke = {
      id: randomUuid(),
      color,
      width,
      points: [point],
    };
  }

  function continueStroke(event: PointerEvent) {
    if (!editable || activePointer !== event.pointerId) return;
    const point = pointFromEvent(event);
    if (!point) return;
    event.preventDefault();
    event.stopPropagation();
    if (tool === "eraser") {
      eraseAt(point);
      return;
    }
    if (!activeStroke) return;
    const last = activeStroke.points[activeStroke.points.length - 1];
    if (Math.hypot(point.x - last.x, point.y - last.y) < 1.5) return;
    activeStroke = { ...activeStroke, points: [...activeStroke.points, point] };
  }

  function eraseAt(point: DrawPoint) {
    const radius = Math.max(18, width * 2.5);
    const strokes = scene.strokes.filter(
      (stroke) => !stroke.points.some((candidate) => Math.hypot(candidate.x - point.x, candidate.y - point.y) <= radius),
    );
    if (strokes.length !== scene.strokes.length) scene = { ...scene, strokes };
  }

  function finishStroke(event: PointerEvent) {
    if (activePointer !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    activePointer = null;
    try {
      if (stageEl?.hasPointerCapture?.(event.pointerId)) {
        stageEl.releasePointerCapture(event.pointerId);
      }
    } catch {
      // The system can release capture first when a touch leaves the webview.
    }

    if (tool === "eraser") {
      const snapshot = eraseSnapshot;
      eraseSnapshot = null;
      if (snapshot && snapshot.strokes.length !== scene.strokes.length) {
        remember(snapshot);
        emitScene(scene);
      }
      return;
    }

    const stroke = activeStroke;
    activeStroke = null;
    if (!stroke) return;
    remember();
    emitScene({ ...scene, strokes: [...scene.strokes, stroke] });
  }

  function cancelStroke(event: PointerEvent) {
    if (activePointer !== event.pointerId) return;
    activePointer = null;
    activeStroke = null;
    if (eraseSnapshot) scene = eraseSnapshot;
    eraseSnapshot = null;
  }

  function undo() {
    if (!editable || undoStack.length === 0) return;
    const previous = undoStack[undoStack.length - 1];
    undoStack = undoStack.slice(0, -1);
    redoStack = [...redoStack.slice(-49), cloneDrawDocument(scene)];
    emitScene(previous);
  }

  function redo() {
    if (!editable || redoStack.length === 0) return;
    const next = redoStack[redoStack.length - 1];
    redoStack = redoStack.slice(0, -1);
    undoStack = [...undoStack.slice(-49), cloneDrawDocument(scene)];
    emitScene(next);
  }

  function clear() {
    if (!editable || scene.strokes.length === 0) return;
    remember();
    emitScene({ ...scene, strokes: [] });
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!editable || !(event.metaKey || event.ctrlKey)) return;
    if (event.key.toLowerCase() !== "z") return;
    event.preventDefault();
    event.stopPropagation();
    event.shiftKey ? redo() : undo();
  }

  export function applyDocument(next: DrawDocument) {
    const fingerprint = encodeDrawDocument(next);
    if (fingerprint === syncedFingerprint || activePointer != null) return;
    scene = cloneDrawDocument(next);
    syncedFingerprint = fingerprint;
    undoStack = [];
    redoStack = [];
  }
</script>

<div
  class="medousa-draw-surface"
  class:medousa-draw-surface--full={variant === "full"}
  class:medousa-draw-surface--editable={editable}
  data-draw-surface=""
>
  {#if editable}
    <div class="medousa-draw-toolbar" role="toolbar" tabindex="-1" aria-label="Drawing tools">
      <div class="medousa-draw-tool-group" role="group" aria-label="Tool">
        <button type="button" class:active={tool === "pen"} aria-pressed={tool === "pen"} onclick={() => (tool = "pen")}>Pen</button>
        <button type="button" class:active={tool === "eraser"} aria-pressed={tool === "eraser"} onclick={() => (tool = "eraser")}>Eraser</button>
      </div>
      <div class="medousa-draw-tool-group medousa-draw-colors" role="group" aria-label="Color">
        {#each COLORS as swatch (swatch)}
          <button
            type="button"
            class="medousa-draw-color"
            class:active={color === swatch}
            style={`--draw-color: ${swatch}`}
            aria-label={`Use ${swatch}`}
            aria-pressed={color === swatch}
            onclick={() => { color = swatch; tool = "pen"; }}
          ></button>
        {/each}
      </div>
      <div class="medousa-draw-tool-group" role="group" aria-label="Stroke width">
        {#each WIDTHS as strokeWidth (strokeWidth)}
          <button type="button" class:active={width === strokeWidth} aria-pressed={width === strokeWidth} onclick={() => (width = strokeWidth)}>{strokeWidth}</button>
        {/each}
      </div>
      <div class="medousa-draw-tool-group medousa-draw-history">
        <button type="button" disabled={undoStack.length === 0} onclick={undo}>Undo</button>
        <button type="button" disabled={redoStack.length === 0} onclick={redo}>Redo</button>
        <button type="button" disabled={scene.strokes.length === 0} onclick={clear}>Clear</button>
      </div>
    </div>
  {/if}

  <button
    type="button"
    bind:this={stageEl}
    class="medousa-draw-stage"
    aria-label={editable ? "Editable drawing canvas" : "Drawing"}
    tabindex={editable ? 0 : -1}
    onpointerdown={beginStroke}
    onpointermove={continueStroke}
    onpointerup={finishStroke}
    onpointercancel={cancelStroke}
    onlostpointercapture={cancelStroke}
    onkeydown={handleKeydown}
  >
    <svg
      bind:this={svgEl}
      viewBox={`0 0 ${scene.width} ${scene.height}`}
      preserveAspectRatio="xMidYMid meet"
      role="img"
      aria-hidden="true"
    >
      <rect class="medousa-draw-paper" width={scene.width} height={scene.height} fill={scene.background === "transparent" ? "transparent" : scene.background}></rect>
      <g class="medousa-draw-grid" aria-hidden="true">
        {#each Array.from({ length: Math.ceil(scene.width / 40) - 1 }, (_, index) => (index + 1) * 40) as x (x)}
          <line x1={x} y1="0" x2={x} y2={scene.height}></line>
        {/each}
        {#each Array.from({ length: Math.ceil(scene.height / 40) - 1 }, (_, index) => (index + 1) * 40) as y (y)}
          <line x1="0" y1={y} x2={scene.width} y2={y}></line>
        {/each}
      </g>
      {#each scene.strokes as stroke (stroke.id)}
        <path class="medousa-draw-stroke" d={drawStrokePath(stroke.points)} stroke={stroke.color} stroke-width={stroke.width}></path>
      {/each}
      {#if activeStroke}
        <path class="medousa-draw-stroke" d={drawStrokePath(activeStroke.points)} stroke={activeStroke.color} stroke-width={activeStroke.width}></path>
      {/if}
    </svg>
  </button>
</div>

<style>
  .medousa-draw-surface { display: flex; min-width: 0; flex-direction: column; overflow: hidden; border: 1px solid rgb(var(--color-surface-500) / .35); border-radius: .8rem; background: rgb(var(--color-surface-900) / .72); }
  .medousa-draw-surface--full { min-height: 0; flex: 1; border: 0; border-radius: 0; background: rgb(var(--color-surface-950)); }
  .medousa-draw-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: .5rem; padding: .55rem .65rem; border-bottom: 1px solid rgb(var(--color-surface-500) / .3); background: rgb(var(--color-surface-900) / .9); }
  .medousa-draw-tool-group { display: flex; align-items: center; gap: .2rem; padding-right: .5rem; border-right: 1px solid rgb(var(--color-surface-500) / .25); }
  .medousa-draw-history { margin-left: auto; padding-right: 0; border-right: 0; }
  .medousa-draw-toolbar button { min-width: 2rem; height: 1.85rem; padding: 0 .55rem; border-radius: .45rem; color: rgb(var(--theme-text-secondary)); font-size: .72rem; }
  .medousa-draw-toolbar button:hover:not(:disabled), .medousa-draw-toolbar button.active { color: rgb(var(--color-surface-50)); background: rgb(var(--color-surface-500) / .3); }
  .medousa-draw-toolbar button:disabled { opacity: .35; }
  .medousa-draw-color { min-width: 1.45rem; width: 1.45rem; height: 1.45rem; padding: 0; border: 2px solid transparent; border-radius: 999px; background: var(--draw-color); box-shadow: inset 0 0 0 1px rgb(0 0 0 / .25); }
  .medousa-draw-color.active { border-color: rgb(var(--color-primary-400)); outline: 1px solid rgb(var(--color-surface-950)); }
  .medousa-draw-stage { display: block; width: 100%; min-height: 0; flex: 1; padding: .65rem; border: 0; border-radius: 0; background: transparent; color: inherit; text-align: initial; }
  svg { display: block; width: 100%; height: auto; max-height: 100%; aspect-ratio: 5 / 3; border-radius: .45rem; background: rgb(var(--color-surface-950)); box-shadow: inset 0 0 0 1px rgb(var(--color-surface-500) / .28); }
  .medousa-draw-surface--full svg { height: 100%; min-height: 16rem; }
  .medousa-draw-surface--editable .medousa-draw-stage { cursor: crosshair; touch-action: none; overscroll-behavior: contain; user-select: none; -webkit-user-select: none; -webkit-touch-callout: none; }
  .medousa-draw-paper { pointer-events: all; }
  .medousa-draw-grid { stroke: rgb(var(--color-surface-500) / .12); stroke-width: 1; pointer-events: none; }
  .medousa-draw-stroke { fill: none; stroke-linecap: round; stroke-linejoin: round; pointer-events: none; }
  @media (max-width: 640px) {
    .medousa-draw-toolbar { flex-wrap: nowrap; gap: .35rem; overflow-x: auto; overscroll-behavior-x: contain; -webkit-overflow-scrolling: touch; scrollbar-width: none; }
    .medousa-draw-toolbar::-webkit-scrollbar { display: none; }
    .medousa-draw-tool-group { flex: 0 0 auto; }
    .medousa-draw-history { margin-left: 0; }
    .medousa-draw-stage { padding: .35rem; }
  }
</style>
