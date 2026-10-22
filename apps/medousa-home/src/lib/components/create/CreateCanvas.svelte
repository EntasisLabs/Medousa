<script lang="ts">
  import { onMount, onDestroy, untrack, tick } from "svelte";
  import CreateTextEditor from "./CreateTextEditor.svelte";
  import CreateSelection from "./CreateSelection.svelte";
  import CreateRotationLabel from "./CreateRotationLabel.svelte";
  import CreateLayoutOverlay from "./CreateLayoutOverlay.svelte";
  import { alignObjects, distributeObjects, defaultLayout, snapLayoutPoint, type AlignmentTarget, type LayoutGuide, type LayoutPreferences } from "$lib/create/creativeLayout";
  import { creativeLayoutDrag } from "$lib/create/creativeLayoutDrag";
  import { groupSelection, ungroupSelection, selectionTarget, selectionBounds, objectBounds, resizeSelectionFromHandle, arrangeSelection, descendants, selectionFrame, rotateSelection, rotateSelectionFrame, moveLineEndpoint, type Alignment, type SelectionFrame } from "$lib/create/creativeSelection";
  import CreateSceneNode from "./CreateSceneNode.svelte";
  import type {
    CreativeNode,
    CreativeOperation,
    CreativeScene,
  } from "$lib/types/generated/daemon_api";
  import {
    createDrawCamera,
    resizeDrawCamera,
    viewToScene,
    type DrawVector,
    type DrawCamera,
  } from "$lib/draw/drawCamera";
  import {
    createDrawInteraction,
    type DrawInteractionState,
    type DrawEraserMode,
  } from "$lib/draw/drawInteraction";
  import { drawStrokeOutlinePath } from "$lib/draw/drawGeometry";
  import type { DrawBrushKind } from "$lib/draw/drawDocument";
  import {
    creativeStrokeNode,
    creativeSceneChanges,
    eraseCreativeInk,
    hitCreativeScene,
    lassoCreativeScene,
  } from "$lib/create/creativeInk";
  import {
    createNode,
    editableNode,
    inverse,
    mapPoint,
    parentPoint,
    sceneBounds,
    selectionRoots,
    worldMatrix,
    multiply,
    creativeFit,
    creativeZoom,
    type CreativeTool,
  } from "$lib/create/creativeScene";
  import {
    canvasInk,
    nodeColor,
    dragRect,
    dragShape,
    newCanvasText,
    duplicateSelection,
    styleNode,
    updateTextNode,
    type CanvasStylePatch,
    type CanvasShape,
    type CanvasFill,
    type CanvasSize,
  } from "$lib/create/creativeInteraction";
  import { type TextStylePatch } from "$lib/create/creativeText";
  import { snapLinePoint, type LineStylePatch } from "$lib/create/creativeLine";
  import { randomUuid } from "$lib/utils/randomUuid";
  let {
    scene,
    images,
    selected = $bindable<string[]>([]),
    tool = $bindable<CreativeTool>("select"),
    shape = "rectangle",
    lineStyle = {},
    fill = "soft",
    textSize = "m",
    textStyle = {},
    color = "#393e33",
    automaticInk = false,
    brushSize = 6,
    brushKind = "pen",
    eraserMode = "partial",
    fingerDraw = false,
    selectionMode = "box",
    layout = defaultLayout,
    initialView = null,
    onview = () => {},
    disabled = false,
    oncommit = () => {},
    onselect = () => {},
    onzoom = () => {},
    onerror = () => {},
    onediting = () => {},
    onScope = () => {},
  }: {
    scene: CreativeScene;
    images: Record<string, string>;
    selected?: string[];
    tool?: CreativeTool;
    shape?: CanvasShape;
    lineStyle?: LineStylePatch;
    fill?: CanvasFill;
    textSize?: CanvasSize;
    textStyle?: TextStylePatch;
    color?: string;
    automaticInk?: boolean;
    brushSize?: number;
    brushKind?: DrawBrushKind;
    eraserMode?: DrawEraserMode;
    fingerDraw?: boolean;
    selectionMode?: "box" | "lasso";
    layout?: LayoutPreferences;
    initialView?: { camera: DrawCamera; viewport: { width: number; height: number } } | null;
    onview?: (view: { camera: DrawCamera; viewport: { width: number; height: number } }) => void;
    disabled?: boolean;
    oncommit?: (label: string, operations: CreativeOperation[]) => void;
    onselect?: (ids: string[]) => void;
    onzoom?: (percent: number) => void;
    onerror?: (error: unknown) => void;
    onediting?: (node: CreativeNode | null) => void;
    onScope?: (name: string | null) => void;
  } = $props();
  let svg = $state<SVGSVGElement>(),
    stage = $state<HTMLDivElement>();
  let camera = $state.raw(untrack(() => initialView?.camera ?? createDrawCamera())),
    viewport = $state(untrack(() => initialView?.viewport ?? { width: 1000, height: 600 })),
    space = $state(false);
  type Gesture = {
    id: number;
    kind: "resize" | "shape" | "frame" | "duplicate" | "endpoint" | "rotate";
    center?: DrawVector;
    angle?: number;
    rotationDelta?: number;
    baseAngle?: number;
    frame?: SelectionFrame;
    rotation?: number;
    handle?: string;
    start: DrawVector;
    view: DrawVector;
    nodes: CreativeNode[];
    base: CreativeNode[];
    started: number;
    parent: CreativeNode | null;
    initial?: string[];
    end?: DrawVector;
    hit?: string;
    layoutScene?: CreativeScene;
  };
  let gesture = $state.raw<Gesture | null>(null),
    draft = $state.raw<CreativeNode[] | null>(null);
  let editing = $state.raw<{
      node: CreativeNode;
      original: CreativeNode | null;
    } | null>(null),
    textDraft = $state(""),
    textInput = $state<HTMLTextAreaElement>();
  const editMatrix = $derived(
    editing
      ? multiply(
          [camera.zoom, 0, 0, camera.zoom, camera.panX, camera.panY],
          worldMatrix(editing.node, scene.nodes),
        )
      : null,
  );
  let groupScope = $state<string | null>(null);
  $effect(() => {
    if (groupScope && !scene.nodes.some(n => n.id === groupScope)) groupScope = null;
    onScope(groupScope ? scene.nodes.find(n => n.id === groupScope)?.name ?? "Group" : null);
  });
  let lastActivation: { point: DrawVector; time: number } | null = null;
  let lastTap: { id?: string; time: number; view: DrawVector } | null = null;
  let inkParent = $state.raw<CreativeNode | null>(null);
  let interactionState = $state.raw<DrawInteractionState>({
    activeStroke: null,
    lassoPath: [],
    cursor: null,
    active: false,
    pinching: false,
  });
  const rendered = $derived(draft ?? scene.nodes);
  const roots = $derived(rendered.filter((n) => !n.parent_id));
  const prefix = `canvas-${randomUuid()}`;
  const zoom = $derived(camera.zoom);
  let guides = $state.raw<LayoutGuide[]>([]), bypassSnap = false;
  const layoutDrag = creativeLayoutDrag(() => ({ settings: layout, zoom, bypass: bypassSnap }), value => (guides = value));
  $effect(() => onzoom(Math.round(zoom * 100)));
  $effect(() => onview({ camera, viewport: { ...viewport } }));
  export function element() {
    return svg;
  }
  export function fit() {
    camera = creativeFit(sceneBounds(scene.nodes), viewport);
  }
  export function fitSelection() { const bounds = selectionBounds(selected, scene.nodes); if (bounds) camera = creativeFit(bounds, viewport); }
  export function zoomBy(factor: number) {
    camera = creativeZoom(
      camera,
      { x: viewport.width / 2, y: viewport.height / 2 },
      camera.zoom * factor,
    );
  }
  export function zoomPercent() {
    return Math.round(camera.zoom * 100);
  }
  function pick(ids: string[]) {
    if (groupScope && ids.some(id => !descendants([groupScope!], scene.nodes).has(id))) groupScope = null;
    selected = ids;
    onselect(ids);
  }
  function viewPoint(x: number, y: number): DrawVector | null {
    if (!svg) return null;
    const matrix = svg.getScreenCTM?.();
    if (matrix) {
      const p = new DOMPoint(x, y).matrixTransform(matrix.inverse());
      return { x: p.x, y: p.y };
    }
    const r = svg.getBoundingClientRect();
    return { x: x - r.left, y: y - r.top };
  }
  function view(e: PointerEvent | WheelEvent) {
    return viewPoint(e.clientX, e.clientY)!;
  }
  function point(e: PointerEvent) {
    return viewToScene(camera, view(e));
  }
  function targetNode(e: PointerEvent | MouseEvent) {
    const id = (e.target as Element).closest<SVGGElement>("[data-node-id]")
      ?.dataset.nodeId;
    return scene.nodes.find((n) => n.id === id);
  }
  function parentFor(p: DrawVector): CreativeNode | null {
    const selectedFrame = scene.nodes.find(
      (n) =>
        selected.includes(n.id) &&
        (n.content.kind === "frame" || n.content.kind === "group") &&
        editableNode(n, scene.nodes),
    );
    if (selectedFrame) return selectedFrame;
    const scope = scene.nodes.find(n => n.id === groupScope);
    if (scope && editableNode(scope, scene.nodes)) return scope;
    return (
      [...scene.nodes].reverse().find((n) => {
        if (n.content.kind !== "frame" || !editableNode(n, scene.nodes))
          return false;
        const q = mapPoint(inverse(worldMatrix(n, scene.nodes)), p);
        return (
          q.x >= 0 &&
          q.y >= 0 &&
          q.x <= n.transform.width &&
          q.y <= n.transform.height
        );
      }) ?? null
    );
  }
  function local(p: DrawVector, parent: CreativeNode | null) {
    return parent ? mapPoint(inverse(worldMatrix(parent, scene.nodes)), p) : p;
  }
  function capture(id: number) {
    try {
      svg?.setPointerCapture(id);
    } catch {
      /* WebViews may still deliver the gesture. */
    }
  }
  function release(id: number) {
    layoutDrag.reset();
    try {
      if (svg?.hasPointerCapture(id)) svg.releasePointerCapture(id);
    } catch {
      /* Already released by the platform. */
    }
  }
  function currentScene() {
    return draft ? { ...scene, nodes: draft } : scene;
  }
  function inside(p: DrawVector, b: { x: number; y: number; width: number; height: number }) {
    return p.x >= b.x && p.y >= b.y && p.x <= b.x + b.width && p.y <= b.y + b.height;
  }
  function hitSelection(base: CreativeScene, p: DrawVector, padding: number): string | null {
    const hit = hitCreativeScene(base, p, padding);
    if (hit) return selectionTarget(hit, base.nodes, groupScope, selected);
    for (const node of [...base.nodes].reverse()) {
      if (node.content.kind === "group" && node.id !== groupScope && (!groupScope || !descendants([node.id], base.nodes).has(groupScope)) && editableNode(node, base.nodes) && inside(p, objectBounds(node, base.nodes)))
        return selectionTarget(node.id, base.nodes, groupScope, selected);
    }
    const roots = selectionRoots(selected, base.nodes), bounds = selectionBounds(selected, base.nodes);
    return roots.length > 1 && bounds && inside(p, bounds) ? roots[0].id : null;
  }
  function activate(hit: CreativeNode | undefined, p: DrawVector) {
    const target = hit && scene.nodes.find(n => n.id === selectionTarget(hit.id, scene.nodes, groupScope));
    if (target?.content.kind === "group") {
      groupScope = target.id;
      pick(hit && hit.id !== target.id ? [selectionTarget(hit.id, scene.nodes, groupScope)] : []);
      return;
    }
    const parent = parentFor(p);
    if (!hit || hit.content.kind === "text" || hit.content.kind === "frame")
      void beginText(hit?.content.kind === "text" ? hit : null, local(p, parent), parent);
  }
  export function exitGroup() {
    if (!groupScope) return;
    const id = groupScope;
    let parent = scene.nodes.find(n => n.id === scene.nodes.find(n => n.id === id)?.parent_id);
    while (parent && parent.content.kind !== "group") parent = scene.nodes.find(n => n.id === parent!.parent_id);
    groupScope = parent?.id ?? null;
    pick([id]);
  }
  let controlTouch = false;
  const interaction = createDrawInteraction<CreativeScene>({
    editable: () => !disabled,
    tool: () =>
      controlTouch ? null : space || tool === "hand"
        ? "hand"
        : tool === "erase"
          ? "eraser"
          : tool === "ink" || tool === "select"
            ? tool
            : null,
    camera: () => camera,
    setCamera: (next) => (camera = next),
    viewPoint,
    scene: currentScene,
    preview: (next) => (draft = next.nodes),
    commit: (before, after, label) => {
      const operations = creativeSceneChanges(before, after);
      if (operations.length) oncommit(label, operations);
      draft = null;
      if (label === "Draw stroke") pick([]);
      else pick(selected.filter((id) => after.nodes.some((n) => n.id === id)));
    },
    addStroke: (base, stroke) => ({
      ...base,
      nodes: [...base.nodes, creativeStrokeNode(stroke, base.nodes, inkParent)],
    }),
    erase: eraseCreativeInk,
    move: layoutDrag.move,
    hit: hitSelection,
    lasso: (base, path) => [...new Set(lassoCreativeScene(base, path).map(id => selectionTarget(id, base.nodes, groupScope)))],
    selected: () => selected,
    select: pick,
    brush: (p) => {
      inkParent = parentFor(p);
      const paint = automaticInk
        ? canvasInk(
            inkParent?.content.kind === "frame"
              ? inkParent.content.background
              : undefined,
            color,
          )
        : color;
      return { kind: brushKind, size: brushSize, color: paint };
    },
    size: () => brushSize,
    eraserMode: () => eraserMode,
    fingerDraw: () =>
      controlTouch || tool === "text" || tool === "shape" || tool === "frame"
        ? true
        : fingerDraw,
    capture,
    release,
    focus: () => stage?.focus({ preventScroll: true }),
    update: (next) => (interactionState = next),
    zoomAt: creativeZoom,
    multiselect: true,
    selectionMode: () => selectionMode,
    middlePan: true,
    maxPoints: 8000,
    maxBrushSize: 160,
    tap: (event, hit, p) => {
      const node = scene.nodes.find((n) => n.id === hit),
        parent = parentFor(p),
        id = hit ?? parent?.id;
      const v = view(event);
      if (
        lastTap &&
        lastTap.id === id &&
        event.timeStamp - lastTap.time < 450 &&
        Math.hypot(v.x - lastTap.view.x, v.y - lastTap.view.y) < 6
      ) {
        lastTap = null;
        lastActivation = { point: v, time: event.timeStamp };
        const raw = hitCreativeScene(scene, p, 8 / zoom);
        activate(scene.nodes.find(n => n.id === raw) ?? node, p);
      } else {
        lastTap = { id, time: event.timeStamp, view: v };
        if (!node && parent) pick([parent.id]);
      }
    },
  });
  function down(e: PointerEvent) {
    if (
      !svg ||
      (gesture && e.pointerType !== "touch") ||
      (e.button !== 0 && e.button !== 1)
    )
      return;
    if (editing) {
      finishText();
      if (editing) return;
    }
    const resizeHandle = (e.target as Element).closest<SVGElement>("[data-resize], [data-endpoint], [data-rotate]");
    const resize = resizeHandle?.dataset.resize;
    const control = !space && e.button === 0 && tool === "select" ? resizeHandle : null;
    if (!disabled && !space && !control && tool === "select" && e.altKey && e.button === 0) {
      const p = point(e), hit = hitSelection(scene, p, 8 / zoom);
      if (hit) {
        const initial = [...selected], ids = selected.includes(hit) ? selected : [hit];
        const copies = duplicateSelection(ids, scene.nodes, { x: 0, y: 0 });
        const roots = copies.filter(n => !copies.some(parent => parent.id === n.parent_id));
        gesture = { id: e.pointerId, kind: "duplicate", start: p, view: view(e), nodes: copies, base: scene.nodes, started: e.timeStamp, parent: null, initial, layoutScene: { ...scene, nodes: [...scene.nodes, ...copies] } };
        draft = [...scene.nodes, ...copies]; pick(roots.map(n => n.id));
        stage?.focus({ preventScroll: true }); capture(e.pointerId); e.preventDefault(); return;
      }
    }
    if (!control || e.pointerType === "touch") {
      controlTouch = Boolean(control);
      const handled = interaction.begin(e);
      controlTouch = false;
      if (handled) {
        if (interaction.state.pinching) { gesture = null; draft = null; }
        return;
      }
    }
    if (disabled) return;
    stage?.focus({ preventScroll: true });
    const p = point(e),
      v = view(e),
      hit = targetNode(e);
    if (control) {
      const nodes = selectionRoots(selected, scene.nodes);
      const frame = selectionFrame(selected, scene.nodes);
      if (!nodes.length || !frame) return;
      gesture = {
        id: e.pointerId,
        kind: control.dataset.endpoint ? "endpoint" : control.dataset.rotate ? "rotate" : "resize",
        handle: control.dataset.endpoint ?? control.dataset.handle ?? "se",
        initial: [...selected],
        center: frame.center,
        frame,
        baseAngle: frame.angle,
        angle: Math.atan2(p.y - frame.center.y, p.x - frame.center.x),
        rotationDelta: 0,
        start: p,
        view: v,
        nodes,
        base: scene.nodes,
        started: e.timeStamp,
        parent: null,
      };
    } else {
      const parent = tool === "frame" ? null : parentFor(p),
        q = local(tool === "shape" || tool === "frame" ? snapLayoutPoint(p, layout, e.ctrlKey) : p, parent);
      if (tool === "text") {
        e.preventDefault();
        void beginText(hit?.content.kind === "text" ? hit : null, q, parent);
        return;
      }
      if (tool !== "shape" && tool !== "frame") return;
      const paint = automaticInk
        ? canvasInk(
            parent?.content.kind === "frame"
              ? parent.content.background
              : undefined,
            color,
          )
        : color;
      const content: CreativeNode["content"] =
        tool === "frame"
          ? {
              kind: "frame",
              name: `Frame ${scene.nodes.filter((n) => n.content.kind === "frame").length + 1}`,
              background: "#f7f5ee",
            }
          : {
              kind: "shape",
              shape: shape === "arrow" ? "line" : shape,
              fill: "transparent",
              stroke: paint,
              stroke_width: canvasShapeWidth(),
            };
      const node = createNode(
        content,
        { x: q.x, y: q.y, width: 1, height: 1 },
        parent?.id ?? null,
      );
      gesture = {
        id: e.pointerId,
        kind: tool,
        start: q,
        view: v,
        nodes: [node],
        base: scene.nodes,
        started: e.timeStamp,
        parent,
      };
      draft = [...scene.nodes, node];
      pick([node.id]);
    }
    e.preventDefault();
    capture(e.pointerId);
  }
  function canvasShapeWidth() {
    return Math.max(1, brushSize / zoom);
  }
  // Match Notes: WebKit's own SVG selection/drag chrome must not compete with the canvas.
  function suppressNativeGesture(event: Event) {
    event.preventDefault();
  }
  function move(e: PointerEvent) {
    bypassSnap = e.ctrlKey;
    if (interaction.move(e)) return;
    const g = gesture;
    if (!g || g.id !== e.pointerId) return;
    e.preventDefault();
    const p = ["resize", "endpoint", "shape", "frame"].includes(g.kind) ? snapLayoutPoint(point(e), layout, e.ctrlKey) : point(e);
    if (g.kind === "resize") {
      draft = resizeSelectionFromHandle(g.nodes.map(n => n.id), g.base, g.handle ?? "se", p, e.shiftKey);
    } else if (g.kind === "endpoint") {
      const next = moveLineEndpoint(g.nodes[0], g.base, g.handle as "start" | "end", p, e.shiftKey);
      draft = g.base.map(n => n.id === next.id ? next : n);
    } else if (g.kind === "rotate" && g.center) {
      const angle = Math.atan2(p.y - g.center.y, p.x - g.center.x);
      const change = Math.atan2(Math.sin(angle - g.angle!), Math.cos(angle - g.angle!));
      g.rotationDelta = (g.rotationDelta ?? 0) + change * 180 / Math.PI; g.angle = angle;
      const target = (g.baseAngle ?? 0) + g.rotationDelta;
      const rotation = (e.shiftKey ? Math.round(target / 15) * 15 : target) - (g.baseAngle ?? 0);
      gesture = { ...g, rotation };
      draft = rotateSelection(g.nodes.map(n => n.id), g.base, g.center, rotation);
    } else if (g.kind === "duplicate") {
      draft = layoutDrag.move(g.layoutScene!, selected, { x: p.x - g.start.x, y: p.y - g.start.y }).nodes;
    } else {
      const n = g.nodes[0];
      const startWorld = g.parent ? mapPoint(worldMatrix(g.parent, g.base), g.start) : g.start;
      const q = local(g.kind === "shape" && (shape === "line" || shape === "arrow") ? snapLinePoint(startWorld, p, e.shiftKey) : p, g.parent);
      const next =
        g.kind === "shape"
          ? dragShape(
              n,
              shape,
              g.start,
              q,
              nodeColor(n) ?? color,
              shape === "line" || shape === "arrow" ? lineStyle.stroke_width ?? canvasShapeWidth() : canvasShapeWidth(),
              fill,
              lineStyle,
            )
          : {
              ...n,
              transform: {
                ...n.transform,
                ...dragRect(g.start, q),
                width: Math.max(4, Math.abs(q.x - g.start.x)),
                height: Math.max(4, Math.abs(q.y - g.start.y)),
              },
            };
      draft = [...g.base, next];
    }
  }
  function finish(e: PointerEvent, cancel = false) {
    if (!gesture) {
      if (cancel) interaction.cancel(e);
      else interaction.finish(e);
      return;
    }
    const g = gesture;
    if (!g || g.id !== e.pointerId) return;
    if (
      !cancel &&
      Math.hypot(view(e).x - g.view.x, view(e).y - g.view.y) >= 3
    ) {
      move(e);
      const operations = creativeSceneChanges(
        { ...scene, nodes: g.base },
        { ...scene, nodes: draft ?? g.base },
      );
      if (operations.length > 1000) onerror(new Error("This selection is too large to transform in one edit. Select fewer objects."));
      else if (operations.length)
        oncommit(
          g.kind === "rotate" ? "Rotate selection" : g.kind === "endpoint" ? "Move endpoint" : g.kind === "resize"
            ? "Resize selection"
            : g.kind === "frame"
              ? "Add frame"
              : g.kind === "duplicate" ? "Duplicate selection" : "Add shape",
          operations,
        );
      if (g.kind !== "resize") tool = "select";
    }
    gesture = null;
    draft = null;
    release(e.pointerId);
    if (cancel) interaction.cancel(e);
    else interaction.finish(e);
    if (cancel || Math.hypot(view(e).x - g.view.x, view(e).y - g.view.y) < 3)
      pick(g.initial ?? selected.filter((id) => scene.nodes.some((n) => n.id === id)));
  }
  async function beginText(
    node: CreativeNode | null,
    p: DrawVector,
    parent: CreativeNode | null = null,
  ) {
    if (disabled || (node && !editableNode(node, scene.nodes))) return;
    const paint = automaticInk
      ? canvasInk(
          parent?.content.kind === "frame"
            ? parent.content.background
            : undefined,
          color,
        )
      : color;
    const next = node ?? newCanvasText(p, paint, textSize, parent?.id ?? null, textStyle);
    if (next.content.kind !== "text") return;
    editing = { node: next, original: node };
    onediting(next);
    textDraft = next.content.text;
    gesture = null;
    draft = null;
    tool = "select";
    pick(node ? [node.id] : []);
    await tick();
    textInput?.focus();
    textInput?.select();
  }
  export function styleText(patch: CanvasStylePatch) {
    if (!editing || disabled) return false;
    const next = styleNode(updateTextNode(editing.node, { text: textDraft }), patch);
    editing = { ...editing, node: next };
    onediting(next);
    return true;
  }
  export function finishText(cancel = false) {
    if (!editing) return;
    if (cancel) {
      editing = null;
      onediting(null);
      return;
    }
    if (disabled) return;
    const { node, original } = editing;
    if (
      original &&
      JSON.stringify(scene.nodes.find((n) => n.id === original.id)) !==
        JSON.stringify(original)
    ) {
      onerror(
        new Error(
          "This text changed in another view. Copy your draft before reopening it.",
        ),
      );
      return;
    }
    if (node.content.kind !== "text") return;
    const text = textDraft;
    editing = null;
    onediting(null);
    if (!text.trim() && !original) {
      pick([]);
      return;
    }
    if (
      original?.content.kind === "text" &&
      text === original.content.text &&
      JSON.stringify(node) === JSON.stringify(original)
    )
      return;
    const next = updateTextNode(node, { text });
    oncommit(original ? "Edit text" : "Add text", [
      { op: "put_node", node: next },
    ]);
    pick([next.id]);
  }
  function doubleClick(e: MouseEvent) {
    if (disabled || editing || space || tool !== "select") return;
    const v = viewPoint(e.clientX, e.clientY)!;
    if (lastActivation && e.timeStamp - lastActivation.time < 80 && Math.hypot(v.x - lastActivation.point.x, v.y - lastActivation.point.y) < 6) return;
    const p = viewToScene(camera, v);
    activate(targetNode(e), p);
  }
  function selectionEdit(action: () => { operations: CreativeOperation[]; selected: string[] }, label: string) {
    if (disabled || gesture || interaction.state.active || editing) return;
    try {
      const result = action();
      if (!result.operations.length) return;
      oncommit(label, result.operations); pick(result.selected);
      stage?.focus({ preventScroll: true });
    } catch (error) { onerror(error); }
  }
  export function group() { selectionEdit(() => groupSelection(selected, scene.nodes), "Group selection"); }
  export function ungroup() { selectionEdit(() => ungroupSelection(selected, scene.nodes), "Ungroup selection"); }
  export function align(alignment: Alignment, target: AlignmentTarget = "selection") { selectionEdit(() => ({ operations: alignObjects(selected, scene.nodes, alignment, target), selected }), target === "frame" ? "Align to frame" : "Align selection"); }
  export function distribute(axis: "x" | "y") { selectionEdit(() => ({ operations: distributeObjects(selected, scene.nodes, axis), selected }), "Space selection evenly"); }
  export function arrange(direction: -1 | 1) { selectionEdit(() => ({ operations: arrangeSelection(selected, scene.nodes, direction), selected }), "Arrange selection"); }

  export function deleteSelection() {
    if (disabled) return;
    const ops = selectionRoots(selected, scene.nodes).map((n) => ({
      op: "remove_node" as const,
      id: n.id,
    }));
    if (ops.length) oncommit("Delete selection", ops);
    pick([]);
  }
  export function duplicate() {
    if (disabled) return;
    const nodes = duplicateSelection(selected, scene.nodes, {
      x: 20 / zoom,
      y: 20 / zoom,
    });
    if (!nodes.length) return;
    oncommit(
      "Duplicate selection",
      nodes.map((node) => ({ op: "put_node", node })),
    );
    pick(
      nodes
        .filter((n) => !nodes.some((p) => p.id === n.parent_id))
        .map((n) => n.id),
    );
  }
  function key(e: KeyboardEvent) {
    if (
      (e.target as HTMLElement).closest(
        "textarea,input,select,[contenteditable=true]",
      )
    )
      return;
    if (e.key === " ") {
      e.preventDefault();
      space = e.type === "keydown";
    }
    if (e.type !== "keydown") return;
    if (e.key === "Escape") {
      layoutDrag.reset();
      if (!gesture && !interaction.state.active && groupScope) { e.preventDefault(); e.stopPropagation(); exitGroup(); return; }
      const initial = gesture?.initial;
      const id = gesture?.id;
      interaction.cancel();
      gesture = null;
      draft = null;
      pick(initial ?? []);
      tool = "select";
      if (id !== undefined && svg?.hasPointerCapture(id))
        svg.releasePointerCapture(id);
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "g") {
      e.preventDefault(); e.stopPropagation();
      if (e.shiftKey) ungroup(); else group(); return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      tool = "select";
      pick(
        selectionRoots(
          scene.nodes
            .filter((n) => n.content.kind !== "frame" && (!groupScope || n.id !== groupScope && descendants([groupScope], scene.nodes).has(n.id)))
            .map((n) => n.id),
          scene.nodes,
        ).map((n) => n.id),
      );
    }
    if (tool === "select" && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "d") {
      e.preventDefault();
      duplicate();
    }
    if (tool === "select" && (e.key === "Delete" || e.key === "Backspace") && selected.length) {
      e.preventDefault();
      deleteSelection();
    }
    const direction = {
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    }[e.key];
    if (tool === "select" && direction && !disabled && selected.length && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      const step = e.shiftKey ? 10 : 1;
      oncommit(
        "Move selection",
        selectionRoots(selected, scene.nodes).map((n) => {
          const a = parentPoint(n, scene.nodes, { x: 0, y: 0 }),
            b = parentPoint(n, scene.nodes, {
              x: direction[0] * step,
              y: direction[1] * step,
            });
          return {
            op: "put_node",
            node: {
              ...n,
              transform: {
                ...n.transform,
                x: n.transform.x + b.x - a.x,
                y: n.transform.y + b.y - a.y,
              },
            },
          };
        }),
      );
    }
  }
  onDestroy(() => {
    interaction.destroy();
    finishText();
  });
  onMount(() => {
    let initialized = initialView !== null;
    const observer = new ResizeObserver(() => {
      if (!stage) return;
      const next = {
        width: Math.max(1, stage.clientWidth),
        height: Math.max(1, stage.clientHeight),
      };
      camera = initialized
        ? resizeDrawCamera(camera, viewport, next)
        : creativeFit(sceneBounds(untrack(() => scene.nodes)), next);
      viewport = next;
      initialized = true;
    });
    if (stage) observer.observe(stage);
    svg?.addEventListener("wheel", interaction.wheel, { passive: false });
    return () => {
      observer.disconnect();
      svg?.removeEventListener("wheel", interaction.wheel);
    };
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (The canvas is a keyboard-operated graphics application.) -->
<div
  class="create-stage"
  bind:this={stage}
  role="application"
  aria-label="Creative canvas"
  tabindex="0"
  onkeydown={key}
  onkeyup={key}
  onblur={() => (space = false)}
  data-tool={space ? "hand" : tool}
>
  <svg
    bind:this={svg}
    width="100%"
    height="100%"
    role="img"
    aria-label="Editable creative project"
    ondblclick={doubleClick}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={(e) => finish(e)}
    onpointerleave={() => interaction.leave()}
    onpointercancel={(e) => finish(e, true)}
    onlostpointercapture={(e) => finish(e, true)}
    onselectstart={suppressNativeGesture}
    ondragstart={suppressNativeGesture}
    oncontextmenu={suppressNativeGesture}
  >
    <g
      data-camera
      transform={`translate(${camera.panX} ${camera.panY}) scale(${zoom})`}
    >
      {#if scene.background !== "transparent"}<rect
          x={-100000}
          y={-100000}
          width="200000"
          height="200000"
          fill={scene.background}
        />{/if}
      {#each roots as node (node.id)}<CreateSceneNode
          {node}
          nodes={rendered}
          {images}
          selected={tool === "select" ? selected : []}
          {zoom}
          {prefix}
          editingId={editing?.node.id}
        />{/each}
      <CreateLayoutOverlay {camera} {viewport} settings={layout} {guides} {prefix}/>
      {#if interactionState.activeStroke}
        {#if inkParent?.content.kind === "frame"}
          <defs
            ><clipPath id={`${prefix}-active-ink`}
              ><rect
                width={inkParent.transform.width}
                height={inkParent.transform.height}
                transform={`matrix(${worldMatrix(inkParent, scene.nodes).join(" ")})`}
              /></clipPath
            ></defs
          >
        {/if}
        <path
          data-editor-only
          clip-path={inkParent?.content.kind === "frame"
            ? `url(#${prefix}-active-ink)`
            : undefined}
          d={drawStrokeOutlinePath(interactionState.activeStroke)}
          fill={interactionState.activeStroke.color}
          fill-opacity={interactionState.activeStroke.brush.opacity}
          pointer-events="none"
        />
      {/if}
      {#if interactionState.lassoPath.length > 1}
        <polyline
          data-editor-only
          aria-label={selectionMode === "box" ? "Box selection" : "Lasso selection"}
          points={interactionState.lassoPath
            .map((p) => `${p.x},${p.y}`)
            .join(" ")}
          fill="var(--create-selection)"
          fill-opacity=".08"
          stroke="var(--create-selection)"
          stroke-width={1.5 / zoom}
          stroke-dasharray={`${7 / zoom} ${5 / zoom}`}
          pointer-events="none"
        />
      {/if}
      {#if tool === "erase" && interactionState.cursor}
        <circle
          data-editor-only
          cx={interactionState.cursor.x}
          cy={interactionState.cursor.y}
          r={Math.max(8, brushSize * 1.1) / zoom}
          fill="var(--create-selection)"
          fill-opacity=".08"
          stroke="var(--create-text)"
          stroke-opacity=".7"
          stroke-width={1.5 / zoom}
          pointer-events="none"
        />
      {/if}
      {#if tool === "select" && !editing}<CreateSelection nodes={rendered} {selected} {zoom} {disabled} frameOverride={gesture?.kind === "rotate" && gesture.frame ? rotateSelectionFrame(gesture.frame, gesture.rotation ?? 0) : undefined} />{/if}
    </g>
  </svg>
  {#if gesture?.kind === "rotate" && gesture.frame}
    <CreateRotationLabel frame={rotateSelectionFrame(gesture.frame, gesture.rotation ?? 0)} {camera}/>
  {/if}
  {#if editing && editMatrix && editing.node.content.kind === "text"}
    <CreateTextEditor node={editing.node as CreativeNode & { content: Extract<CreativeNode["content"], { kind: "text" }> }} matrix={editMatrix} bind:value={textDraft} bind:input={textInput} {disabled}
      oninput={(value) => { textDraft = value; if (editing) onediting(updateTextNode(editing.node, { text: value })); }}
      onfinish={(focus) => { finishText(); if (focus) stage?.focus(); }}/>
  {/if}
</div>

<style>
  .create-stage {
    position: relative;
    flex: 1;
    min-height: 120px;
    min-width: 0;
    overflow: hidden;
    outline: none;
    background-color: var(--create-canvas);

    touch-action: none;
    user-select: none;
  }
  .create-stage svg {
    display: block;
    overflow: hidden;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    -webkit-user-drag: none;
  }
  .create-stage :global(svg *) {
    user-select: none;
    -webkit-user-select: none;
    -webkit-user-drag: none;
  }
  .create-stage[data-tool="hand"] {
    cursor: grab;
  }
  .create-stage[data-tool="erase"] {
    cursor: none;
  }

  .create-stage[data-tool="ink"],
  .create-stage[data-tool="shape"],
  .create-stage[data-tool="frame"] {
    cursor: crosshair;
  }
  .create-stage[data-tool="text"] {
    cursor: text;
  }

</style>
