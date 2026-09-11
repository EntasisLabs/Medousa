<script lang="ts">
  import Eraser from "@lucide/svelte/icons/eraser";
  import Hand from "@lucide/svelte/icons/hand";
  import MousePointer2 from "@lucide/svelte/icons/mouse-pointer-2";
  import PenLine from "@lucide/svelte/icons/pen-line";
  import Redo2 from "@lucide/svelte/icons/redo-2";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import DrawOptionsSheet from "$lib/components/draw/DrawOptionsSheet.svelte";
  import {
    cloneDrawDocument,
    createDrawBrush,
    encodeDrawDocument,
    type DrawBrushKind,
    type DrawDocument,
    type DrawPoint,
    type DrawStroke,
  } from "$lib/draw/drawDocument";
  import {
    createDrawCamera,
    fitDrawCamera,
    panDrawCamera,
    resizeDrawCamera,
    viewToScene,
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
  import { onDestroy, onMount, untrack } from "svelte";

  interface Props {
    document: DrawDocument;
    editable?: boolean;
    variant?: "embedded" | "full";
    onchange?: (document: DrawDocument) => void;
    oninteractionchange?: (active: boolean) => void;
  }

  let {
    document,
    editable = false,
    variant = "embedded",
    onchange = () => undefined,
    oninteractionchange = () => undefined,
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
  let gesture = $state.raw<DrawGesture | null>(null);
  let pinch = $state.raw<PinchGesture | null>(null);
  let selectedIds = $state.raw<string[]>([]);
  let cursorScene = $state.raw<DrawVector | null>(null);
  let undoStack = $state.raw<DrawPatch[]>([]);
  let redoStack = $state.raw<DrawPatch[]>([]);
  let svgEl = $state<SVGSVGElement | null>(null);
  let stageEl = $state<HTMLDivElement | null>(null);

  const touchPointers = new Map<number, DrawVector>();
  const penPointers = new Set<number>();
  let pendingInkPoints: DrawPoint[] = [];
  let inkFrame = 0;
  let lastPenActivity = Number.NEGATIVE_INFINITY;
  let pendingScene: DrawDocument | null = null;
  let changeTimer: ReturnType<typeof setTimeout> | null = null;
  let persistenceHeld = false;

  const selectedStrokes = $derived(scene.strokes.filter((stroke) => selectedIds.includes(stroke.id)));
  const selectionBounds = $derived(combinedDrawBounds(selectedStrokes));
  const activeStroke = $derived(gesture?.kind === "ink" ? gesture.stroke : null);
  const lassoPath = $derived(gesture?.kind === "lasso" ? gesture.path : []);
  const zoomLabel = $derived(`${Math.round(camera.zoom * 100)}%`);

  $effect(() => {
    const fingerprint = encodeDrawDocument(document);
    if (fingerprint === observedExternalFingerprint) return;
    if (fingerprint === syncedFingerprint) {
      observedExternalFingerprint = fingerprint;
      return;
    }
    if (gesture != null || pinch != null || pendingScene != null) return;
    observedExternalFingerprint = fingerprint;
    scene = cloneDrawDocument(document);
    syncedFingerprint = fingerprint;
    selectedIds = [];
    undoStack = [];
    redoStack = [];
    camera = fitDrawCamera(combinedDrawBounds(scene.strokes), viewport);
  });

  onDestroy(() => {
    if (inkFrame) cancelAnimationFrame(inkFrame);
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

  function setPersistenceHold(active: boolean) {
    if (persistenceHeld === active) return;
    persistenceHeld = active;
    oninteractionchange(active);
  }

  function releasePersistenceIfIdle() {
    if (gesture == null && pinch == null && pendingScene == null) setPersistenceHold(false);
  }

  function flushSceneEmission(force = false) {
    if (changeTimer) {
      clearTimeout(changeTimer);
      changeTimer = null;
    }
    if (!force && (gesture != null || pinch != null)) {
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
    return eraseDrawDocumentByPath(base, path, radius, eraserMode);
  }

  function startPinch() {
    const entries = [...touchPointers.entries()].slice(0, 2);
    if (entries.length < 2) return;
    if (gesture?.pointerType === "touch") {
      if (gesture.kind === "erase" || gesture.kind === "move") scene = gesture.base;
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
    setPersistenceHold(true);

    if (event.pointerType === "pen") {
      penPointers.add(event.pointerId);
      lastPenActivity = event.timeStamp;
    }
    if (event.pointerType === "touch") {
      if (penPointers.size > 0 || event.timeStamp - lastPenActivity < 650) {
        releasePointer(event.pointerId);
        releasePersistenceIfIdle();
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
          base: scene,
          start: scenePoint,
        };
      } else {
        selectedIds = [];
        gesture = { kind: "lasso", pointerId: event.pointerId, pointerType: event.pointerType, path: [scenePoint] };
      }
      return;
    }

    if (tool === "eraser") {
      const base = scene;
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
      releasePersistenceIfIdle();
      return;
    }
    const point = scenePointFromEvent(event, event.timeStamp);
    if (!point) {
      releasePersistenceIfIdle();
      return;
    }
    gesture = {
      kind: "ink",
      pointerId: event.pointerId,
      pointerType: event.pointerType,
      startedAt: event.timeStamp,
      stroke: {
        id: randomUuid(),
        color,
        input: drawInputKind(event.pointerType),
        brush: createDrawBrush(brushKind, size / camera.zoom),
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
      const previous = gesture.path[gesture.path.length - 1];
      const points = coalescedPointerSamples(event).flatMap((sample) => {
        const sampleView = viewPointFromClient(sample.clientX, sample.clientY);
        return sampleView ? [viewToScene(camera, sampleView)] : [];
      });
      const segment = [previous, ...points];
      gesture = { ...gesture, path: [...gesture.path, ...points] };
      scene = erasePreview(scene, segment);
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
        ...gesture.base,
        strokes: gesture.base.strokes.map((stroke) =>
          selectedIds.includes(stroke.id) ? moveDrawStroke(stroke, delta) : stroke,
        ),
      };
    }
  }

  function finishGesture(event: PointerEvent) {
    if (event.pointerType === "touch") touchPointers.delete(event.pointerId);
    if (event.pointerType === "pen") penPointers.delete(event.pointerId);
    if (pinch) {
      if (pinch.ids.includes(event.pointerId) && touchPointers.size === 0) pinch = null;
      releasePointer(event.pointerId);
      releasePersistenceIfIdle();
      return;
    }
    if (!gesture || gesture.pointerId !== event.pointerId) {
      releasePointer(event.pointerId);
      releasePersistenceIfIdle();
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
      const base = scene;
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
    releasePersistenceIfIdle();
  }

  function cancelGesture(event: PointerEvent) {
    if (event.pointerType === "touch") touchPointers.delete(event.pointerId);
    if (event.pointerType === "pen") penPointers.delete(event.pointerId);
    if (pinch?.ids.includes(event.pointerId) && touchPointers.size === 0) pinch = null;
    if (!gesture || gesture.pointerId !== event.pointerId) {
      releasePersistenceIfIdle();
      return;
    }
    if (gesture.kind === "erase" || gesture.kind === "move") scene = gesture.base;
    gesture = null;
    pendingInkPoints = [];
    if (inkFrame) cancelAnimationFrame(inkFrame);
    inkFrame = 0;
    releasePersistenceIfIdle();
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
    if (fingerprint === syncedFingerprint || gesture != null || pinch != null || pendingScene != null) return;
    scene = cloneDrawDocument(next);
    syncedFingerprint = fingerprint;
    observedExternalFingerprint = fingerprint;
    selectedIds = [];
    undoStack = [];
    redoStack = [];
    camera = fitDrawCamera(combinedDrawBounds(scene.strokes), viewport);
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
    <div class="medousa-draw-toolbar" role="toolbar" aria-label="Drawing tools">
      <div class="medousa-draw-tools" role="group" aria-label="Active tool">
        <button type="button" class:active={tool === "ink"} aria-pressed={tool === "ink"} onclick={() => chooseTool("ink")}>
          <PenLine size={18} strokeWidth={2} aria-hidden="true" />
          <span>Draw</span>
        </button>
        <button type="button" class:active={tool === "eraser"} aria-pressed={tool === "eraser"} onclick={() => chooseTool("eraser")}>
          <Eraser size={18} strokeWidth={2} aria-hidden="true" />
          <span>Erase</span>
        </button>
        <button type="button" class:active={tool === "select"} aria-pressed={tool === "select"} onclick={() => chooseTool("select")}>
          <MousePointer2 size={18} strokeWidth={2} aria-hidden="true" />
          <span>Select</span>
        </button>
        <button type="button" class:active={tool === "hand"} aria-pressed={tool === "hand"} onclick={() => chooseTool("hand")}>
          <Hand size={18} strokeWidth={2} aria-hidden="true" />
          <span>Move</span>
        </button>
      </div>
      <div class="medousa-draw-quick-actions" role="group" aria-label="Drawing actions">
        <button
          type="button"
          class:active={optionsOpen}
          aria-label="Drawing options"
          aria-expanded={optionsOpen}
          onclick={() => (optionsOpen = !optionsOpen)}
        >
          <SlidersHorizontal size={19} strokeWidth={2} aria-hidden="true" />
        </button>
        <button type="button" aria-label="Undo" disabled={undoStack.length === 0} onclick={undo}>
          <Undo2 size={19} strokeWidth={2} aria-hidden="true" />
        </button>
        <button type="button" aria-label="Redo" disabled={redoStack.length === 0} onclick={redo}>
          <Redo2 size={19} strokeWidth={2} aria-hidden="true" />
        </button>
      </div>
    </div>
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
    onpointerdown={beginGesture}
    onpointermove={continueGesture}
    onpointerup={finishGesture}
    onpointercancel={cancelGesture}
    onlostpointercapture={cancelGesture}
    onpointerleave={() => (cursorScene = null)}
    onwheel={handleWheel}
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

  .medousa-draw-toolbar {
    z-index: 10;
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: 3.5rem;
    padding: 0.4rem max(0.45rem, env(safe-area-inset-left, 0px));
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.22);
    background: rgb(var(--color-surface-900) / 0.94);
    backdrop-filter: blur(18px);
  }

  .medousa-draw-tools,
  .medousa-draw-quick-actions {
    display: flex;
    align-items: center;
    gap: 0.2rem;
  }

  .medousa-draw-tools {
    min-width: 0;
  }

  .medousa-draw-toolbar button {
    display: inline-flex;
    min-width: 2.75rem;
    height: 2.65rem;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    padding: 0 0.65rem;
    border-radius: 0.75rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.72rem;
    font-weight: 600;
    white-space: nowrap;
  }

  .medousa-draw-toolbar button:hover:not(:disabled),
  .medousa-draw-toolbar button.active {
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-surface-500) / 0.28);
  }

  .medousa-draw-toolbar button.active {
    box-shadow: inset 0 0 0 1px rgb(var(--color-primary-400) / 0.42);
  }

  .medousa-draw-toolbar button:disabled {
    opacity: 0.3;
  }

  .medousa-draw-quick-actions {
    flex: 0 0 auto;
    padding-left: 0.35rem;
    border-left: 1px solid rgb(var(--color-surface-500) / 0.2);
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

  @container (max-width: 640px) {
    .medousa-draw-toolbar {
      gap: 0.25rem;
      padding-right: max(0.35rem, env(safe-area-inset-right, 0px));
      padding-left: max(0.35rem, env(safe-area-inset-left, 0px));
    }

    .medousa-draw-tools { flex: 1 1 auto; justify-content: space-between; }
    .medousa-draw-tools button { min-width: 2.55rem; padding: 0 0.5rem; }
    .medousa-draw-tools button span { display: none; }
    .medousa-draw-quick-actions { gap: 0; padding-left: 0.2rem; }
    .medousa-draw-quick-actions button { min-width: 2.5rem; padding: 0; }
  }
</style>
