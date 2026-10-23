<script lang="ts">
  import DrawOptionsSheet from "$lib/components/draw/DrawOptionsSheet.svelte";
  import DrawToolbar from "$lib/components/draw/DrawToolbar.svelte";
  import {
    cloneDrawDocument,
    createDrawBrush,
    encodeDrawDocument,
    type DrawBrushKind,
    type DrawDocument,
  } from "$lib/draw/drawDocument";
  import {
    createDrawCamera,
    fitDrawCamera,
    resizeDrawCamera,
    zoomDrawCameraAt,
    type DrawCamera,
    type DrawVector,
  } from "$lib/draw/drawCamera";
  import {
    combinedDrawBounds,
    drawStrokeOutlinePath,
    eraseDrawDocumentByPath,
    moveDrawStroke,
    selectDrawStrokesInLasso,
    topDrawStrokeAt,
  } from "$lib/draw/drawGeometry";
  import { applyDrawPatch, createDrawPatch, type DrawPatch } from "$lib/draw/drawHistory";
  import { createDrawInteraction, type DrawInteractionState, type DrawTool as Tool, type DrawEraserMode as EraserMode } from "$lib/draw/drawInteraction";
  import { randomUuid } from "$lib/utils/randomUuid";
  import { onDestroy, onMount, untrack } from "svelte";

  interface Props {
    document: DrawDocument;
    editable?: boolean;
    variant?: "embedded" | "full";
    onchange?: (document: DrawDocument) => void;
    oninteractionchange?: (active: boolean) => void;
    mobileToolbar?: boolean;
  }

  let {
    document,
    editable = false,
    variant = "embedded",
    onchange = () => undefined,
    oninteractionchange = () => undefined,
    mobileToolbar = false,
  }: Props = $props();

  const DRAW_CHANGE_SETTLE_MS = 650;

  let scene = $state.raw<DrawDocument>(untrack(() => cloneDrawDocument(document)));
  let syncedFingerprint = $state(untrack(() => encodeDrawDocument(document)));
  let observedExternalFingerprint = $state(untrack(() => encodeDrawDocument(document)));
  let tool = $state<Tool>("ink");
  let brushKind = $state<DrawBrushKind>("pen");
  let color = $state("#e7e5e4");
  let size = $state(6);
  let eraserMode = $state<EraserMode>("partial");
  let fingerDraw = $state(false);
  let optionsOpen = $state(false);
  let camera = $state.raw<DrawCamera>(createDrawCamera());
  let viewport = $state.raw({ width: 1200, height: 720 });
  let interactionState = $state.raw<DrawInteractionState>({activeStroke:null, lassoPath:[], cursor:null, active:false, pinching:false});
  let selectedIds = $state.raw<string[]>([]);
  const cursorScene = $derived(interactionState.cursor);
  let undoStack = $state.raw<DrawPatch[]>([]);
  let redoStack = $state.raw<DrawPatch[]>([]);
  let svgEl = $state<SVGSVGElement | null>(null);
  let stageEl = $state<HTMLDivElement | null>(null);
  let toolbarEl = $state<ReturnType<typeof DrawToolbar> | null>(null);

  let pendingScene: DrawDocument | null = null;
  let changeTimer: ReturnType<typeof setTimeout> | null = null;
  let persistenceHeld = false;

  const selectedStrokes = $derived(scene.strokes.filter((stroke) => selectedIds.includes(stroke.id)));
  const selectionBounds = $derived(combinedDrawBounds(selectedStrokes));
  const interacting = $derived(interactionState.active || interactionState.pinching);
  const activeStroke = $derived(interactionState.activeStroke);
  const lassoPath = $derived(interactionState.lassoPath);
  const zoomLabel = $derived(`${Math.round(camera.zoom * 100)}%`);

  $effect(() => {
    const fingerprint = encodeDrawDocument(document);
    if (fingerprint === observedExternalFingerprint) return;
    if (fingerprint === syncedFingerprint) {
      observedExternalFingerprint = fingerprint;
      return;
    }
    if (interacting || pendingScene != null) return;
    observedExternalFingerprint = fingerprint;
    scene = cloneDrawDocument(document);
    syncedFingerprint = fingerprint;
    selectedIds = [];
    undoStack = [];
    redoStack = [];
    camera = fitDrawCamera(combinedDrawBounds(scene.strokes), viewport);
  });

  onDestroy(() => {
    interaction.destroy();
    flushSceneEmission(true);
    setPersistenceHold(false);
  });

  onMount(() => {
    if (!svgEl) return;
    let initialized = false;
    const updateViewport = () => {
      if (!svgEl) return;
      const bounds = svgEl.getBoundingClientRect();
      const next = {
        width: Math.max(1, Math.round(bounds.width)),
        height: Math.max(1, Math.round(bounds.height)),
      };
      if (initialized && next.width === viewport.width && next.height === viewport.height) return;
      if (!initialized) {
        viewport = next;
        camera = fitDrawCamera(combinedDrawBounds(scene.strokes), next);
        initialized = true;
        return;
      }
      camera = resizeDrawCamera(camera, viewport, next);
      viewport = next;
    };
    const observer = new ResizeObserver(updateViewport);
    observer.observe(svgEl);
    updateViewport();
    return () => observer.disconnect();
  });

  function viewPointFromClient(clientX: number, clientY: number): DrawVector | null {
    if (!svgEl) return null;
    const matrix = svgEl.getScreenCTM();
    if (!matrix) return null;
    const point = new DOMPoint(clientX, clientY).matrixTransform(matrix.inverse());
    return { x: point.x, y: point.y };
  }

  function capturePointer(pointerId: number) {
    try {
      stageEl?.setPointerCapture(pointerId);
    } catch {
      // Some mobile webviews reject capture while still delivering the gesture.
    }
  }

  function releasePointer(pointerId: number) {
    try {
      if (stageEl?.hasPointerCapture?.(pointerId)) stageEl.releasePointerCapture(pointerId);
    } catch {
      // The platform may release capture first during an interrupted gesture.
    }
  }

  function focusStage() {
    try {
      stageEl?.focus({ preventScroll: true });
    } catch {
      stageEl?.focus();
    }
  }

  function setPersistenceHold(active: boolean) {
    if (persistenceHeld === active) return;
    persistenceHeld = active;
    oninteractionchange(active);
  }

  function releasePersistenceIfIdle() {
    if (!interaction.active && pendingScene == null) setPersistenceHold(false);
  }

  function flushSceneEmission(force = false) {
    if (changeTimer) {
      clearTimeout(changeTimer);
      changeTimer = null;
    }
    if (!force && interacting) {
      changeTimer = setTimeout(() => {
        changeTimer = null;
        flushSceneEmission();
      }, DRAW_CHANGE_SETTLE_MS);
      return;
    }
    if (pendingScene) {
      const next = pendingScene;
      pendingScene = null;
      syncedFingerprint = encodeDrawDocument(next);
      onchange(cloneDrawDocument(next));
    }
    releasePersistenceIfIdle();
  }

  function emitScene(next: DrawDocument) {
    scene = next;
    pendingScene = next;
    setPersistenceHold(true);
    if (changeTimer) clearTimeout(changeTimer);
    changeTimer = setTimeout(() => {
      changeTimer = null;
      flushSceneEmission();
    }, DRAW_CHANGE_SETTLE_MS);
  }

  function commitScene(base: DrawDocument, next: DrawDocument, label: string) {
    const patch = createDrawPatch(base, next, label);
    if (!patch) {
      scene = base;
      releasePersistenceIfIdle();
      return;
    }
    undoStack = [...undoStack.slice(-99), patch];
    redoStack = [];
    emitScene(next);
  }

  function updateBrush(kind: DrawBrushKind) {
    brushKind = kind;
    size = createDrawBrush(kind).size;
    tool = "ink";
  }

  const interaction = createDrawInteraction<DrawDocument>({
    editable:()=>editable, tool:()=>tool, camera:()=>camera, setCamera:next=>camera=next,
    viewPoint:viewPointFromClient, scene:()=>scene, preview:next=>scene=next, commit:commitScene,
    addStroke:(base,stroke)=>({...base,strokes:[...base.strokes,stroke]}),
    erase:eraseDrawDocumentByPath,
    move:(base,ids,delta)=>({...base,strokes:base.strokes.map(stroke=>ids.includes(stroke.id)?moveDrawStroke(stroke,delta):stroke)}),
    hit:(base,point,padding)=>topDrawStrokeAt(base.strokes,point,padding)?.id??null,
    lasso:(base,path)=>selectDrawStrokesInLasso(base.strokes,path),
    selected:()=>selectedIds, select:ids=>selectedIds=ids,
    brush:()=>({kind:brushKind,size,color}), size:()=>size, eraserMode:()=>eraserMode, fingerDraw:()=>fingerDraw,
    capture:capturePointer, release:releasePointer, focus:focusStage,
    update:next=>interactionState=next,
    activate:()=>{toolbarEl?.activateMobileControls();setPersistenceHold(true)}, idle:releasePersistenceIfIdle,
  });
  function undo() {
    if (!editable || undoStack.length === 0) return;
    const patch = undoStack[undoStack.length - 1];
    undoStack = undoStack.slice(0, -1);
    redoStack = [...redoStack.slice(-99), patch];
    selectedIds = [];
    emitScene(applyDrawPatch(scene, patch, "undo"));
  }

  function redo() {
    if (!editable || redoStack.length === 0) return;
    const patch = redoStack[redoStack.length - 1];
    redoStack = redoStack.slice(0, -1);
    undoStack = [...undoStack.slice(-99), patch];
    selectedIds = [];
    emitScene(applyDrawPatch(scene, patch, "redo"));
  }

  function deleteSelection() {
    if (!editable || selectedIds.length === 0) return;
    const base = scene;
    const next = { ...base, strokes: base.strokes.filter((stroke) => !selectedIds.includes(stroke.id)) };
    selectedIds = [];
    commitScene(base, next, "Delete selection");
  }

  function duplicateSelection() {
    if (!editable || selectedIds.length === 0) return;
    const base = scene;
    const copies = base.strokes
      .filter((stroke) => selectedIds.includes(stroke.id))
      .map((stroke) => ({ ...moveDrawStroke(stroke, { x: 18 / camera.zoom, y: 18 / camera.zoom }), id: randomUuid() }));
    if (copies.length === 0) return;
    const next = { ...base, strokes: [...base.strokes, ...copies] };
    selectedIds = copies.map((stroke) => stroke.id);
    commitScene(base, next, "Duplicate selection");
  }

  function clear() {
    if (!editable || scene.strokes.length === 0) return;
    const base = scene;
    selectedIds = [];
    commitScene(base, { ...base, strokes: [] }, "Clear drawing");
    camera = createDrawCamera();
  }

  function resetCamera() {
    camera = fitDrawCamera(combinedDrawBounds(scene.strokes), viewport);
  }

  function zoomBy(
    factor: number,
    anchor: DrawVector = { x: viewport.width / 2, y: viewport.height / 2 },
  ) {
    camera = zoomDrawCameraAt(camera, anchor, camera.zoom * factor);
  }

  function chooseTool(next: Tool) {
    tool = next;
  }

  function suppressNativeGesture(event: Event) {
    if (editable) event.preventDefault();
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (optionsOpen && event.key === "Escape") {
      event.preventDefault();
      optionsOpen = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!editable) return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z") {
      event.preventDefault();
      event.stopPropagation();
      event.shiftKey ? redo() : undo();
      return;
    }
    if ((event.key === "Backspace" || event.key === "Delete") && selectedIds.length > 0) {
      event.preventDefault();
      deleteSelection();
    } else if (event.key === "Escape") {
      selectedIds = [];
      interaction.cancel();
    } else if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      zoomBy(1.2);
    } else if (event.key === "-") {
      event.preventDefault();
      zoomBy(1 / 1.2);
    }
  }

  export function applyDocument(next: DrawDocument) {
    const fingerprint = encodeDrawDocument(next);
    if (fingerprint === syncedFingerprint || interacting || pendingScene != null) return;
    scene = cloneDrawDocument(next);
    syncedFingerprint = fingerprint;
    observedExternalFingerprint = fingerprint;
    selectedIds = [];
    undoStack = [];
    redoStack = [];
    camera = fitDrawCamera(combinedDrawBounds(scene.strokes), viewport);
  }

  export function currentDocument(): DrawDocument {
    flushSceneEmission(true);
    return cloneDrawDocument(scene);
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<div
  class="medousa-draw-surface"
  class:medousa-draw-surface--full={variant === "full"}
  class:medousa-draw-surface--editable={editable}
  data-draw-surface=""
>
  {#if editable}
    <DrawToolbar
      bind:this={toolbarEl}
      {tool}
      {optionsOpen}
      canUndo={undoStack.length > 0}
      canRedo={redoStack.length > 0}
      onTool={chooseTool}
      onOptions={() => (optionsOpen = !optionsOpen)}
      onUndo={undo}
      onRedo={redo}
      mobileVisible={mobileToolbar}
    />
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    bind:this={stageEl}
    class="medousa-draw-stage"
    class:tool-eraser={tool === "eraser"}
    class:tool-select={tool === "select"}
    class:tool-hand={tool === "hand"}
    aria-label={editable ? "Editable drawing canvas" : "Drawing"}
    role={editable ? "application" : "img"}
    tabindex={editable ? 0 : -1}
    onpointerdown={interaction.begin}
    onpointermove={interaction.move}
    onpointerup={interaction.finish}
    onpointercancel={interaction.cancel}
    onlostpointercapture={interaction.cancel}
    onpointerleave={() => interaction.leave()}
    onwheel={interaction.wheel}
    onkeydown={handleKeydown}
    oncontextmenu={suppressNativeGesture}
    onselectstart={suppressNativeGesture}
    ondragstart={suppressNativeGesture}
  >
    <svg
      bind:this={svgEl}
      viewBox={`0 0 ${viewport.width} ${viewport.height}`}
      preserveAspectRatio="none"
      role="presentation"
      aria-hidden="true"
    >
      <rect class="medousa-draw-viewport" width={viewport.width} height={viewport.height}></rect>
      <g transform={`translate(${camera.panX} ${camera.panY}) scale(${camera.zoom})`}>
        {#each scene.strokes as stroke (stroke.id)}
          <path
            class="medousa-draw-stroke"
            class:selected={selectedIds.includes(stroke.id)}
            class:highlighter={stroke.brush.kind === "highlighter"}
            d={drawStrokeOutlinePath(stroke)}
            fill={stroke.color}
            fill-opacity={stroke.brush.opacity}
          ></path>
        {/each}
        {#if activeStroke}
          <path
            class="medousa-draw-stroke medousa-draw-stroke--active"
            class:highlighter={activeStroke.brush.kind === "highlighter"}
            d={drawStrokeOutlinePath(activeStroke)}
            fill={activeStroke.color}
            fill-opacity={activeStroke.brush.opacity}
          ></path>
        {/if}
        {#if lassoPath.length > 1}
          <polyline class="medousa-draw-lasso" points={lassoPath.map((point) => `${point.x},${point.y}`).join(" ")}></polyline>
        {/if}
        {#if selectionBounds}
          <rect
            class="medousa-draw-selection"
            x={selectionBounds.x - 6 / camera.zoom}
            y={selectionBounds.y - 6 / camera.zoom}
            width={selectionBounds.width + 12 / camera.zoom}
            height={selectionBounds.height + 12 / camera.zoom}
            rx={6 / camera.zoom}
            stroke-width={2 / camera.zoom}
          ></rect>
        {/if}
        {#if tool === "eraser" && cursorScene}
          <circle class="medousa-draw-eraser-cursor" cx={cursorScene.x} cy={cursorScene.y} r={Math.max(8, size * 1.1) / camera.zoom} stroke-width={1.5 / camera.zoom}></circle>
        {/if}
      </g>
    </svg>
  </div>

  <DrawOptionsSheet
    open={editable && optionsOpen}
    {tool}
    {brushKind}
    {color}
    {size}
    {eraserMode}
    {fingerDraw}
    {zoomLabel}
    canUndo={undoStack.length > 0}
    canRedo={redoStack.length > 0}
    hasSelection={selectedIds.length > 0}
    hasInk={scene.strokes.length > 0}
    onClose={() => (optionsOpen = false)}
    onBrush={updateBrush}
    onColor={(next) => (color = next)}
    onSize={(next) => (size = next)}
    onEraserMode={(next) => (eraserMode = next)}
    onFingerDraw={() => (fingerDraw = !fingerDraw)}
    onZoomOut={() => zoomBy(1 / 1.2)}
    onZoomIn={() => zoomBy(1.2)}
    onFit={resetCamera}
    onUndo={undo}
    onRedo={redo}
    onDuplicate={duplicateSelection}
    onDelete={deleteSelection}
    onClear={clear}
  />
</div>

<style>
  .medousa-draw-surface {
    position: relative;
    container-type: inline-size;
    display: flex;
    min-width: 0;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-500) / 0.3);
    border-radius: 0.8rem;
    background: rgb(var(--color-surface-950));
  }

  .medousa-draw-surface--full {
    min-height: 0;
    flex: 1;
    border: 0;
    border-radius: 0;
  }

  .medousa-draw-stage {
    display: block;
    width: 100%;
    min-height: 16rem;
    flex: 1;
    overflow: hidden;
    outline: none;
    background: rgb(var(--color-surface-950));
  }

  .medousa-draw-surface:not(.medousa-draw-surface--full) .medousa-draw-stage {
    aspect-ratio: 5 / 3;
  }

  svg {
    display: block;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .medousa-draw-surface--editable .medousa-draw-stage {
    cursor: crosshair;
    touch-action: none;
    overscroll-behavior: contain;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    -webkit-user-drag: none;
    -webkit-tap-highlight-color: transparent;
  }

  .medousa-draw-surface--editable .medousa-draw-stage:focus-visible {
    box-shadow: inset 0 0 0 2px rgb(var(--color-primary-400) / 0.72);
  }

  .medousa-draw-surface--editable .medousa-draw-stage.tool-eraser { cursor: none; }
  .medousa-draw-surface--editable .medousa-draw-stage.tool-select { cursor: default; }
  .medousa-draw-surface--editable .medousa-draw-stage.tool-hand { cursor: grab; }
  .medousa-draw-surface--editable .medousa-draw-stage.tool-hand:active { cursor: grabbing; }
  .medousa-draw-viewport { fill: rgb(var(--color-surface-950)); pointer-events: all; }
  .medousa-draw-stroke { pointer-events: none; }
  .medousa-draw-stroke.highlighter { mix-blend-mode: normal; }
  .medousa-draw-stroke.selected { filter: drop-shadow(0 0 2px rgb(var(--color-primary-400) / 0.85)); }
  .medousa-draw-lasso { fill: rgb(var(--color-primary-400) / 0.08); stroke: rgb(var(--color-primary-300) / 0.9); stroke-width: 1.5; stroke-dasharray: 7 5; vector-effect: non-scaling-stroke; pointer-events: none; }
  .medousa-draw-selection { fill: none; stroke: rgb(var(--color-primary-300) / 0.95); stroke-dasharray: 7 4; pointer-events: none; }
  .medousa-draw-eraser-cursor { fill: rgb(var(--color-surface-50) / 0.08); stroke: rgb(var(--color-surface-50) / 0.72); pointer-events: none; }

</style>
