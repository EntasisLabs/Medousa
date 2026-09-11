<script lang="ts">
  import {
    cloneDrawDocument,
    cloneDrawStroke,
    createDrawBrush,
    encodeDrawDocument,
    type DrawBrushKind,
    type DrawDocument,
    type DrawPoint,
    type DrawStroke,
  } from "$lib/draw/drawDocument";
  import {
    createDrawCamera,
    panDrawCamera,
    viewToScene,
    zoomDrawCameraAt,
    type DrawCamera,
    type DrawVector,
  } from "$lib/draw/drawCamera";
  import {
    combinedDrawBounds,
    drawStrokeOutlinePath,
    eraseDrawStrokeByPath,
    hitTestDrawStroke,
    moveDrawStroke,
    selectDrawStrokesInLasso,
    simplifyDrawPoints,
    topDrawStrokeAt,
  } from "$lib/draw/drawGeometry";
  import { applyDrawPatch, createDrawPatch, type DrawPatch } from "$lib/draw/drawHistory";
  import {
    coalescedPointerSamples,
    drawInputKind,
    drawPointFromPointer,
    shouldDrawWithPointer,
  } from "$lib/draw/drawInput";
  import { randomUuid } from "$lib/utils/randomUuid";
  import { onDestroy, untrack } from "svelte";

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

  type Tool = "ink" | "eraser" | "select" | "hand";
  type EraserMode = "partial" | "stroke";

  type InkGesture = {
    kind: "ink";
    pointerId: number;
    pointerType: string;
    startedAt: number;
    stroke: DrawStroke;
  };
  type EraseGesture = {
    kind: "erase";
    pointerId: number;
    pointerType: string;
    base: DrawDocument;
    path: DrawVector[];
  };
  type LassoGesture = {
    kind: "lasso";
    pointerId: number;
    pointerType: string;
    path: DrawVector[];
  };
  type MoveGesture = {
    kind: "move";
    pointerId: number;
    pointerType: string;
    base: DrawDocument;
    start: DrawVector;
  };
  type PanGesture = {
    kind: "pan";
    pointerId: number;
    pointerType: string;
    lastView: DrawVector;
  };
  type DrawGesture = InkGesture | EraseGesture | LassoGesture | MoveGesture | PanGesture;

  type PinchGesture = {
    ids: [number, number];
    startCamera: DrawCamera;
    startCenter: DrawVector;
    startDistance: number;
  };

  const COLORS = ["#e7e5e4", "#f87171", "#fb923c", "#facc15", "#4ade80", "#38bdf8", "#a78bfa"];
  const SIZES = [3, 6, 12, 24];
  const BRUSHES: { kind: DrawBrushKind; label: string }[] = [
    { kind: "pen", label: "Pen" },
    { kind: "pencil", label: "Pencil" },
    { kind: "marker", label: "Marker" },
    { kind: "highlighter", label: "Highlighter" },
  ];

  let scene = $state<DrawDocument>(untrack(() => cloneDrawDocument(document)));
  let syncedFingerprint = $state(untrack(() => encodeDrawDocument(document)));
  let observedExternalFingerprint = $state(untrack(() => encodeDrawDocument(document)));
  let tool = $state<Tool>("ink");
  let brushKind = $state<DrawBrushKind>("pen");
  let color = $state(COLORS[0]);
  let size = $state(6);
  let eraserMode = $state<EraserMode>("partial");
  let fingerDraw = $state(false);
  let camera = $state<DrawCamera>(createDrawCamera());
  let gesture = $state<DrawGesture | null>(null);
  let pinch = $state<PinchGesture | null>(null);
  let selectedIds = $state<string[]>([]);
  let cursorScene = $state<DrawVector | null>(null);
  let undoStack = $state<DrawPatch[]>([]);
  let redoStack = $state<DrawPatch[]>([]);
  let svgEl = $state<SVGSVGElement | null>(null);
  let stageEl = $state<HTMLButtonElement | null>(null);

  const touchPointers = new Map<number, DrawVector>();
  const penPointers = new Set<number>();
  let pendingInkPoints: DrawPoint[] = [];
  let inkFrame = 0;
  let lastPenActivity = Number.NEGATIVE_INFINITY;

  const selectedStrokes = $derived(scene.strokes.filter((stroke) => selectedIds.includes(stroke.id)));
  const selectionBounds = $derived(combinedDrawBounds(selectedStrokes));
  const activeStroke = $derived(gesture?.kind === "ink" ? gesture.stroke : null);
  const lassoPath = $derived(gesture?.kind === "lasso" ? gesture.path : []);
  const zoomLabel = $derived(`${Math.round(camera.zoom * 100)}%`);

  $effect(() => {
    const fingerprint = encodeDrawDocument(document);
    if (fingerprint === observedExternalFingerprint) return;
    observedExternalFingerprint = fingerprint;
    if (fingerprint === syncedFingerprint || gesture != null || pinch != null) return;
    scene = cloneDrawDocument(document);
    syncedFingerprint = fingerprint;
    selectedIds = [];
    undoStack = [];
    redoStack = [];
  });

  onDestroy(() => {
    if (inkFrame) cancelAnimationFrame(inkFrame);
  });

  function viewPointFromClient(clientX: number, clientY: number): DrawVector | null {
    if (!svgEl) return null;
    const matrix = svgEl.getScreenCTM();
    if (!matrix) return null;
    const point = new DOMPoint(clientX, clientY).matrixTransform(matrix.inverse());
    return { x: point.x, y: point.y };
  }

  function scenePointFromEvent(event: PointerEvent, startedAt: number): DrawPoint | null {
    return drawPointFromPointer(
      event,
      (clientX, clientY) => {
        const view = viewPointFromClient(clientX, clientY);
        return view ? viewToScene(camera, view) : null;
      },
      startedAt,
    );
  }

  function capturePointer(event: PointerEvent) {
    try {
      stageEl?.setPointerCapture(event.pointerId);
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

  function emitScene(next: DrawDocument) {
    scene = cloneDrawDocument(next);
    syncedFingerprint = encodeDrawDocument(scene);
    onchange(cloneDrawDocument(scene));
  }

  function commitScene(base: DrawDocument, next: DrawDocument, label: string) {
    const patch = createDrawPatch(base, next, label);
    if (!patch) {
      scene = cloneDrawDocument(base);
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

  function appendInkPoints(points: DrawPoint[]) {
    if (gesture?.kind !== "ink" || points.length === 0) return;
    const current = gesture.stroke.points;
    const next = [...current];
    for (const point of points) {
      const last = next[next.length - 1];
      if (!last || Math.hypot(point.x - last.x, point.y - last.y) >= 0.35 / camera.zoom) {
        next.push(point);
      }
    }
    gesture = { ...gesture, stroke: { ...gesture.stroke, points: next } };
  }

  function flushInkPoints() {
    if (inkFrame) cancelAnimationFrame(inkFrame);
    inkFrame = 0;
    const points = pendingInkPoints;
    pendingInkPoints = [];
    appendInkPoints(points);
  }

  function queueInkPoints(points: DrawPoint[]) {
    pendingInkPoints.push(...points);
    if (inkFrame) return;
    inkFrame = requestAnimationFrame(() => {
      inkFrame = 0;
      const pending = pendingInkPoints;
      pendingInkPoints = [];
      appendInkPoints(pending);
    });
  }

  function erasePreview(base: DrawDocument, path: DrawVector[]): DrawDocument {
    const radius = Math.max(8, size * 1.1) / camera.zoom;
    if (eraserMode === "stroke") {
      return {
        ...cloneDrawDocument(base),
        strokes: base.strokes
          .filter((stroke) => !path.some((point) => hitTestDrawStroke(stroke, point, radius)))
          .map(cloneDrawStroke),
      };
    }
    return {
      ...cloneDrawDocument(base),
      strokes: base.strokes.flatMap((stroke) => eraseDrawStrokeByPath(stroke, path, radius)),
    };
  }

  function startPinch() {
    const entries = [...touchPointers.entries()].slice(0, 2);
    if (entries.length < 2) return;
    if (gesture?.pointerType === "touch") {
      if (gesture.kind === "erase" || gesture.kind === "move") scene = cloneDrawDocument(gesture.base);
      gesture = null;
      pendingInkPoints = [];
    }
    const [first, second] = entries;
    pinch = {
      ids: [first[0], second[0]],
      startCamera: { ...camera },
      startCenter: { x: (first[1].x + second[1].x) / 2, y: (first[1].y + second[1].y) / 2 },
      startDistance: Math.max(1, Math.hypot(second[1].x - first[1].x, second[1].y - first[1].y)),
    };
  }

  function updatePinch() {
    if (!pinch) return;
    const first = touchPointers.get(pinch.ids[0]);
    const second = touchPointers.get(pinch.ids[1]);
    if (!first || !second) return;
    const center = { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
    const currentDistance = Math.max(1, Math.hypot(second.x - first.x, second.y - first.y));
    const zoomed = zoomDrawCameraAt(
      pinch.startCamera,
      pinch.startCenter,
      pinch.startCamera.zoom * (currentDistance / pinch.startDistance),
    );
    camera = panDrawCamera(zoomed, {
      x: center.x - pinch.startCenter.x,
      y: center.y - pinch.startCenter.y,
    });
  }

  function beginGesture(event: PointerEvent) {
    if (!editable || event.button !== 0) return;
    const view = viewPointFromClient(event.clientX, event.clientY);
    if (!view) return;
    event.preventDefault();
    event.stopPropagation();
    focusStage();
    capturePointer(event);

    if (event.pointerType === "pen") {
      penPointers.add(event.pointerId);
      lastPenActivity = event.timeStamp;
    }
    if (event.pointerType === "touch") {
      if (penPointers.size > 0 || event.timeStamp - lastPenActivity < 650) {
        releasePointer(event.pointerId);
        return;
      }
      touchPointers.set(event.pointerId, view);
      if (touchPointers.size >= 2) {
        startPinch();
        return;
      }
    }
    if (pinch) return;

    const scenePoint = viewToScene(camera, view);
    if (tool === "hand" || (event.pointerType === "touch" && !fingerDraw)) {
      gesture = { kind: "pan", pointerId: event.pointerId, pointerType: event.pointerType, lastView: view };
      return;
    }

    if (tool === "select") {
      const hit = topDrawStrokeAt(scene.strokes, scenePoint, 8 / camera.zoom);
      if (hit) {
        if (!selectedIds.includes(hit.id)) selectedIds = [hit.id];
        gesture = {
          kind: "move",
          pointerId: event.pointerId,
          pointerType: event.pointerType,
          base: cloneDrawDocument(scene),
          start: scenePoint,
        };
      } else {
        selectedIds = [];
        gesture = { kind: "lasso", pointerId: event.pointerId, pointerType: event.pointerType, path: [scenePoint] };
      }
      return;
    }

    if (tool === "eraser") {
      const base = cloneDrawDocument(scene);
      gesture = {
        kind: "erase",
        pointerId: event.pointerId,
        pointerType: event.pointerType,
        base,
        path: [scenePoint],
      };
      scene = erasePreview(base, [scenePoint]);
      return;
    }

    if (!shouldDrawWithPointer(event.pointerType, fingerDraw, penPointers.size > 0 && event.pointerType !== "pen")) {
      return;
    }
    const point = scenePointFromEvent(event, event.timeStamp);
    if (!point) return;
    gesture = {
      kind: "ink",
      pointerId: event.pointerId,
      pointerType: event.pointerType,
      startedAt: event.timeStamp,
      stroke: {
        id: randomUuid(),
        color,
        input: drawInputKind(event.pointerType),
        brush: createDrawBrush(brushKind, size),
        points: [point],
      },
    };
  }

  function continueGesture(event: PointerEvent) {
    const view = viewPointFromClient(event.clientX, event.clientY);
    if (!view) return;
    cursorScene = viewToScene(camera, view);
    if (event.pointerType === "pen") lastPenActivity = event.timeStamp;
    if (event.pointerType === "touch") {
      touchPointers.set(event.pointerId, view);
      if (pinch) {
        event.preventDefault();
        event.stopPropagation();
        updatePinch();
        return;
      }
    }
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();

    if (gesture.kind === "pan") {
      camera = panDrawCamera(camera, { x: view.x - gesture.lastView.x, y: view.y - gesture.lastView.y });
      gesture = { ...gesture, lastView: view };
      return;
    }

    if (gesture.kind === "ink") {
      const points = coalescedPointerSamples(event)
        .map((sample) => scenePointFromEvent(sample, gesture!.kind === "ink" ? gesture!.startedAt : event.timeStamp))
        .filter((point): point is DrawPoint => point != null);
      queueInkPoints(points);
      return;
    }

    const scenePoint = viewToScene(camera, view);
    if (gesture.kind === "erase") {
      const path = [...gesture.path, scenePoint];
      gesture = { ...gesture, path };
      scene = erasePreview(gesture.base, path);
      return;
    }
    if (gesture.kind === "lasso") {
      const last = gesture.path[gesture.path.length - 1];
      if (Math.hypot(scenePoint.x - last.x, scenePoint.y - last.y) >= 2 / camera.zoom) {
        gesture = { ...gesture, path: [...gesture.path, scenePoint] };
      }
      return;
    }
    if (gesture.kind === "move") {
      const delta = { x: scenePoint.x - gesture.start.x, y: scenePoint.y - gesture.start.y };
      scene = {
        ...cloneDrawDocument(gesture.base),
        strokes: gesture.base.strokes.map((stroke) =>
          selectedIds.includes(stroke.id) ? moveDrawStroke(stroke, delta) : cloneDrawStroke(stroke),
        ),
      };
    }
  }

  function finishGesture(event: PointerEvent) {
    if (event.pointerType === "touch") touchPointers.delete(event.pointerId);
    if (event.pointerType === "pen") penPointers.delete(event.pointerId);
    if (pinch) {
      if (pinch.ids.includes(event.pointerId)) pinch = null;
      releasePointer(event.pointerId);
      return;
    }
    if (!gesture || gesture.pointerId !== event.pointerId) {
      releasePointer(event.pointerId);
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const completed = gesture;

    if (completed.kind === "ink") {
      flushInkPoints();
      const finalStroke = gesture?.kind === "ink" ? gesture.stroke : completed.stroke;
      gesture = null;
      const simplified = { ...finalStroke, points: simplifyDrawPoints(finalStroke.points) };
      const base = cloneDrawDocument(scene);
      commitScene(base, { ...base, strokes: [...base.strokes, simplified] }, "Draw stroke");
      releasePointer(event.pointerId);
      return;
    }

    gesture = null;
    if (completed.kind === "erase") {
      commitScene(completed.base, scene, eraserMode === "partial" ? "Erase ink" : "Erase strokes");
    } else if (completed.kind === "move") {
      commitScene(completed.base, scene, "Move selection");
    } else if (completed.kind === "lasso") {
      selectedIds = selectDrawStrokesInLasso(scene.strokes, completed.path);
    }
    releasePointer(event.pointerId);
  }

  function cancelGesture(event: PointerEvent) {
    if (event.pointerType === "touch") touchPointers.delete(event.pointerId);
    if (event.pointerType === "pen") penPointers.delete(event.pointerId);
    if (pinch?.ids.includes(event.pointerId)) pinch = null;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    if (gesture.kind === "erase" || gesture.kind === "move") scene = cloneDrawDocument(gesture.base);
    gesture = null;
    pendingInkPoints = [];
    if (inkFrame) cancelAnimationFrame(inkFrame);
    inkFrame = 0;
  }

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
    const base = cloneDrawDocument(scene);
    const next = { ...base, strokes: base.strokes.filter((stroke) => !selectedIds.includes(stroke.id)) };
    selectedIds = [];
    commitScene(base, next, "Delete selection");
  }

  function duplicateSelection() {
    if (!editable || selectedIds.length === 0) return;
    const base = cloneDrawDocument(scene);
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
    const base = cloneDrawDocument(scene);
    selectedIds = [];
    commitScene(base, { ...base, strokes: [] }, "Clear drawing");
  }

  function resetCamera() {
    camera = createDrawCamera();
  }

  function zoomBy(factor: number, anchor: DrawVector = { x: scene.width / 2, y: scene.height / 2 }) {
    camera = zoomDrawCameraAt(camera, anchor, camera.zoom * factor);
  }

  function handleWheel(event: WheelEvent) {
    if (!editable) return;
    const anchor = viewPointFromClient(event.clientX, event.clientY);
    if (!anchor) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.ctrlKey || event.metaKey) {
      camera = zoomDrawCameraAt(camera, anchor, camera.zoom * Math.exp(-event.deltaY * 0.008));
    } else {
      const translated = viewPointFromClient(
        event.clientX - event.deltaX,
        event.clientY - event.deltaY,
      );
      camera = panDrawCamera(camera, translated
        ? { x: translated.x - anchor.x, y: translated.y - anchor.y }
        : { x: -event.deltaX, y: -event.deltaY });
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
      gesture = null;
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
    if (fingerprint === syncedFingerprint || gesture != null || pinch != null) return;
    scene = cloneDrawDocument(next);
    syncedFingerprint = fingerprint;
    observedExternalFingerprint = fingerprint;
    selectedIds = [];
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
        <button type="button" class:active={tool === "ink"} aria-pressed={tool === "ink"} onclick={() => (tool = "ink")}>Draw</button>
        <button type="button" class:active={tool === "eraser"} aria-pressed={tool === "eraser"} onclick={() => (tool = "eraser")}>Erase</button>
        <button type="button" class:active={tool === "select"} aria-pressed={tool === "select"} onclick={() => (tool = "select")}>Select</button>
        <button type="button" class:active={tool === "hand"} aria-pressed={tool === "hand"} onclick={() => (tool = "hand")}>Hand</button>
      </div>

      {#if tool === "ink"}
        <div class="medousa-draw-tool-group" role="group" aria-label="Brush">
          {#each BRUSHES as brush (brush.kind)}
            <button type="button" class:active={brushKind === brush.kind} aria-pressed={brushKind === brush.kind} onclick={() => updateBrush(brush.kind)}>{brush.label}</button>
          {/each}
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
              onclick={() => { color = swatch; tool = "ink"; }}
            ></button>
          {/each}
        </div>
        <div class="medousa-draw-tool-group" role="group" aria-label="Brush size">
          {#each SIZES as brushSize (brushSize)}
            <button type="button" class:active={size === brushSize} aria-pressed={size === brushSize} onclick={() => (size = brushSize)}>{brushSize}</button>
          {/each}
        </div>
      {:else if tool === "eraser"}
        <div class="medousa-draw-tool-group" role="group" aria-label="Eraser mode">
          <button type="button" class:active={eraserMode === "partial"} aria-pressed={eraserMode === "partial"} onclick={() => (eraserMode = "partial")}>Partial</button>
          <button type="button" class:active={eraserMode === "stroke"} aria-pressed={eraserMode === "stroke"} onclick={() => (eraserMode = "stroke")}>Stroke</button>
        </div>
      {:else if tool === "select" && selectedIds.length > 0}
        <div class="medousa-draw-tool-group" role="group" aria-label="Selection actions">
          <button type="button" onclick={duplicateSelection}>Duplicate</button>
          <button type="button" onclick={deleteSelection}>Delete</button>
        </div>
      {/if}

      <div class="medousa-draw-tool-group" role="group" aria-label="Touch input">
        <button type="button" class:active={fingerDraw} aria-pressed={fingerDraw} onclick={() => (fingerDraw = !fingerDraw)}>Finger draws</button>
      </div>
      <div class="medousa-draw-tool-group" role="group" aria-label="View">
        <button type="button" aria-label="Zoom out" onclick={() => zoomBy(1 / 1.2)}>−</button>
        <button type="button" class="medousa-draw-zoom" title="Reset view" onclick={resetCamera}>{zoomLabel}</button>
        <button type="button" aria-label="Zoom in" onclick={() => zoomBy(1.2)}>+</button>
      </div>
      <div class="medousa-draw-tool-group medousa-draw-history" role="group" aria-label="History">
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
    class:tool-eraser={tool === "eraser"}
    class:tool-select={tool === "select"}
    class:tool-hand={tool === "hand"}
    aria-label={editable ? "Editable drawing canvas" : "Drawing"}
    tabindex={editable ? 0 : -1}
    onpointerdown={beginGesture}
    onpointermove={continueGesture}
    onpointerup={finishGesture}
    onpointercancel={cancelGesture}
    onlostpointercapture={cancelGesture}
    onpointerleave={() => (cursorScene = null)}
    onwheel={handleWheel}
    onkeydown={handleKeydown}
  >
    <svg
      bind:this={svgEl}
      viewBox={`0 0 ${scene.width} ${scene.height}`}
      preserveAspectRatio="xMidYMid meet"
      role="img"
      aria-hidden="true"
    >
      <defs>
        <pattern id="draw-grid" width="40" height="40" patternUnits="userSpaceOnUse">
          <path d="M 40 0 L 0 0 0 40" fill="none" stroke="currentColor"></path>
        </pattern>
      </defs>
      <rect class="medousa-draw-viewport" width={scene.width} height={scene.height}></rect>
      <g transform={`translate(${camera.panX} ${camera.panY}) scale(${camera.zoom})`}>
        <rect class="medousa-draw-paper" width={scene.width} height={scene.height} fill={scene.background === "transparent" ? "transparent" : scene.background}></rect>
        <rect class="medousa-draw-grid" width={scene.width} height={scene.height} fill="url(#draw-grid)"></rect>
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
  </button>
</div>

<style>
  .medousa-draw-surface { display: flex; min-width: 0; flex-direction: column; overflow: hidden; border: 1px solid rgb(var(--color-surface-500) / .35); border-radius: .8rem; background: rgb(var(--color-surface-900) / .72); }
  .medousa-draw-surface--full { min-height: 0; flex: 1; border: 0; border-radius: 0; background: rgb(var(--color-surface-950)); }
  .medousa-draw-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: .5rem; padding: .55rem .65rem; border-bottom: 1px solid rgb(var(--color-surface-500) / .3); background: rgb(var(--color-surface-900) / .94); }
  .medousa-draw-tool-group { display: flex; align-items: center; gap: .2rem; padding-right: .5rem; border-right: 1px solid rgb(var(--color-surface-500) / .25); }
  .medousa-draw-history { margin-left: auto; padding-right: 0; border-right: 0; }
  .medousa-draw-toolbar button { min-width: 2rem; height: 1.9rem; padding: 0 .55rem; border-radius: .45rem; color: rgb(var(--theme-text-secondary)); font-size: .72rem; white-space: nowrap; }
  .medousa-draw-toolbar button:hover:not(:disabled), .medousa-draw-toolbar button.active { color: rgb(var(--color-surface-50)); background: rgb(var(--color-surface-500) / .3); }
  .medousa-draw-toolbar button:disabled { opacity: .35; }
  .medousa-draw-toolbar .medousa-draw-zoom { min-width: 3.4rem; font-variant-numeric: tabular-nums; }
  .medousa-draw-color { min-width: 1.45rem; width: 1.45rem; height: 1.45rem; padding: 0; border: 2px solid transparent; border-radius: 999px; background: var(--draw-color); box-shadow: inset 0 0 0 1px rgb(0 0 0 / .25); }
  .medousa-draw-color.active { border-color: rgb(var(--color-primary-400)); outline: 1px solid rgb(var(--color-surface-950)); }
  .medousa-draw-stage { display: block; width: 100%; min-height: 0; flex: 1; padding: .65rem; border: 0; border-radius: 0; background: transparent; color: inherit; text-align: initial; }
  svg { display: block; width: 100%; height: auto; max-height: 100%; aspect-ratio: 5 / 3; overflow: hidden; border-radius: .45rem; background: rgb(var(--color-surface-950)); box-shadow: inset 0 0 0 1px rgb(var(--color-surface-500) / .28); }
  .medousa-draw-surface--full svg { height: 100%; min-height: 16rem; }
  .medousa-draw-surface--editable .medousa-draw-stage { cursor: crosshair; touch-action: none; overscroll-behavior: contain; user-select: none; -webkit-user-select: none; -webkit-touch-callout: none; }
  .medousa-draw-surface--editable .medousa-draw-stage.tool-eraser { cursor: none; }
  .medousa-draw-surface--editable .medousa-draw-stage.tool-select { cursor: default; }
  .medousa-draw-surface--editable .medousa-draw-stage.tool-hand { cursor: grab; }
  .medousa-draw-surface--editable .medousa-draw-stage.tool-hand:active { cursor: grabbing; }
  .medousa-draw-viewport { fill: rgb(var(--color-surface-950)); pointer-events: all; }
  .medousa-draw-paper { pointer-events: all; }
  .medousa-draw-grid { color: rgb(var(--color-surface-500) / .12); stroke: currentColor; stroke-width: 1; pointer-events: none; }
  .medousa-draw-stroke { pointer-events: none; }
  .medousa-draw-stroke.highlighter { mix-blend-mode: normal; }
  .medousa-draw-stroke.selected { filter: drop-shadow(0 0 2px rgb(var(--color-primary-400) / .85)); }
  .medousa-draw-lasso { fill: rgb(var(--color-primary-400) / .08); stroke: rgb(var(--color-primary-300) / .9); stroke-width: 1.5; stroke-dasharray: 7 5; vector-effect: non-scaling-stroke; pointer-events: none; }
  .medousa-draw-selection { fill: none; stroke: rgb(var(--color-primary-300) / .95); stroke-dasharray: 7 4; pointer-events: none; }
  .medousa-draw-eraser-cursor { fill: rgb(var(--color-surface-50) / .08); stroke: rgb(var(--color-surface-50) / .72); pointer-events: none; }
  @media (max-width: 640px) {
    .medousa-draw-toolbar { flex-wrap: nowrap; gap: .35rem; overflow-x: auto; overscroll-behavior-x: contain; -webkit-overflow-scrolling: touch; scrollbar-width: none; }
    .medousa-draw-toolbar::-webkit-scrollbar { display: none; }
    .medousa-draw-tool-group { flex: 0 0 auto; }
    .medousa-draw-history { margin-left: 0; }
    .medousa-draw-stage { padding: .35rem; }
  }
</style>
