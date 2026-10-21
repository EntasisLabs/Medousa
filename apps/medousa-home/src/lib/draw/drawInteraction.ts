import {
  createDrawBrush,
  type DrawBrushKind,
  type DrawPoint,
  type DrawStroke,
} from "./drawDocument";
import {
  panDrawCamera,
  viewToScene,
  zoomDrawCameraAt,
  type DrawCamera,
  type DrawVector,
} from "./drawCamera";
import {
  coalescedPointerSamples,
  drawInputKind,
  drawPointFromPointer,
  shouldDrawWithPointer,
} from "./drawInput";
import { simplifyDrawPoints } from "./drawGeometry";
import { randomUuid } from "$lib/utils/randomUuid";

export type DrawTool = "ink" | "eraser" | "select" | "hand";
export type DrawEraserMode = "partial" | "stroke";
export type DrawInteractionState = {
  activeStroke: DrawStroke | null;
  lassoPath: DrawVector[];
  cursor: DrawVector | null;
  active: boolean;
  pinching: boolean;
};
type Gesture<S> = {
  kind: "ink" | "erase" | "lasso" | "move" | "pan";
  pointerId: number;
  pointerType: string;
  base: S;
  start: DrawVector;
  lastView: DrawVector;
  startedAt: number;
  path: DrawVector[];
  stroke?: DrawStroke;
  hit?: string;
  initial: string[];
};
export interface DrawInteractionOptions<S> {
  editable: () => boolean;
  tool: () => DrawTool | null;
  camera: () => DrawCamera;
  setCamera: (camera: DrawCamera) => void;
  viewPoint: (x: number, y: number) => DrawVector | null;
  scene: () => S;
  preview: (scene: S) => void;
  commit: (base: S, next: S, label: string) => void;
  addStroke: (base: S, stroke: DrawStroke) => S;
  erase: (
    base: S,
    path: DrawVector[],
    radius: number,
    mode: DrawEraserMode,
  ) => S;
  move: (base: S, ids: string[], delta: DrawVector) => S;
  hit: (scene: S, point: DrawVector, padding: number) => string | null;
  lasso: (scene: S, path: DrawVector[]) => string[];
  selected: () => string[];
  select: (ids: string[]) => void;
  brush: (point: DrawVector) => {
    kind: DrawBrushKind;
    size: number;
    color: string;
  };
  size: () => number;
  eraserMode: () => DrawEraserMode;
  fingerDraw: () => boolean;
  capture: (id: number) => void;
  release: (id: number) => void;
  focus: () => void;
  update: (state: DrawInteractionState) => void;
  activate?: () => void;
  idle?: () => void;
  tap?: (
    event: PointerEvent,
    hit: string | undefined,
    point: DrawVector,
  ) => void;
  zoomAt?: typeof zoomDrawCameraAt;
  multiselect?: boolean;
  middlePan?: boolean;
  maxPoints?: number;
  maxBrushSize?: number;
}

/** Notes' pointer lifecycle, sample batching, lasso and touch navigation. Hosts
 * adapt document edits, rendering, and persistence without duplicating gestures. */
export function createDrawInteraction<S>(o: DrawInteractionOptions<S>) {
  let gesture: Gesture<S> | null = null;
  let pinch: {
    ids: [number, number];
    camera: DrawCamera;
    center: DrawVector;
    distance: number;
  } | null = null;
  const touches = new Map<number, DrawVector>(),
    pens = new Set<number>();
  let lastPenActivity = Number.NEGATIVE_INFINITY;
  let pending: DrawPoint[] = [],
    frame = 0,
    cursor: DrawVector | null = null;
  const zoomAt = o.zoomAt ?? zoomDrawCameraAt;
  const state = (): DrawInteractionState => ({
    activeStroke: gesture?.kind === "ink" ? (gesture.stroke ?? null) : null,
    lassoPath: gesture?.kind === "lasso" ? gesture.path : [],
    cursor,
    active: gesture !== null,
    pinching: pinch !== null,
  });
  function publish() {
    o.update(state());
  }
  function idle() {
    if (!gesture && !pinch) o.idle?.();
  }
  function scenePoint(event: PointerEvent, startedAt: number) {
    return drawPointFromPointer(
      event,
      (x, y) => {
        const v = o.viewPoint(x, y);
        return v ? viewToScene(o.camera(), v) : null;
      },
      startedAt,
    );
  }
  function append(points: DrawPoint[]) {
    if (gesture?.kind !== "ink" || !gesture.stroke || !points.length) return;
    const next = [...gesture.stroke.points];
    for (const point of points) {
      const last = next[next.length - 1];
      if (
        next.length < (o.maxPoints ?? Infinity) &&
        (!last ||
          Math.hypot(point.x - last.x, point.y - last.y) >=
            0.35 / o.camera().zoom)
      )
        next.push(point);
    }
    gesture = { ...gesture, stroke: { ...gesture.stroke, points: next } };
    publish();
  }
  function flush() {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    const points = pending;
    pending = [];
    append(points);
  }
  function discardSamples() {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    pending = [];
  }
  function queue(points: DrawPoint[]) {
    pending.push(...points);
    if (!frame)
      frame = requestAnimationFrame(() => {
        frame = 0;
        const points = pending;
        pending = [];
        append(points);
      });
  }
  function startPinch() {
    const entries = [...touches.entries()].slice(0, 2);
    if (entries.length < 2) return;
    if (gesture?.pointerType === "touch") {
      if (gesture.kind === "erase" || gesture.kind === "move")
        o.preview(gesture.base);
      gesture = null;
      discardSamples();
    }
    const [a, b] = entries;
    pinch = {
      ids: [a[0], b[0]],
      camera: { ...o.camera() },
      center: { x: (a[1].x + b[1].x) / 2, y: (a[1].y + b[1].y) / 2 },
      distance: Math.max(1, Math.hypot(b[1].x - a[1].x, b[1].y - a[1].y)),
    };
    publish();
  }
  function updatePinch() {
    if (!pinch) return;
    const a = touches.get(pinch.ids[0]),
      b = touches.get(pinch.ids[1]);
    if (!a || !b) return;
    const center = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    o.setCamera(
      panDrawCamera(
        zoomAt(
          pinch.camera,
          pinch.center,
          (pinch.camera.zoom * Math.max(1, Math.hypot(b.x - a.x, b.y - a.y))) /
            pinch.distance,
        ),
        { x: center.x - pinch.center.x, y: center.y - pinch.center.y },
      ),
    );
  }
  function begin(event: PointerEvent): boolean {
    const tool = o.tool();
    if (
      !o.editable() ||
      (!tool && event.pointerType !== "touch") ||
      (event.button !== 0 && !(o.middlePan && event.button === 1))
    )
      return false;
    const v = o.viewPoint(event.clientX, event.clientY);
    if (!v) return false;
    event.preventDefault();
    event.stopPropagation();
    o.focus();
    o.capture(event.pointerId);
    o.activate?.();
    if (event.pointerType === "pen") {
      pens.add(event.pointerId);
      lastPenActivity = event.timeStamp;
    }
    if (event.pointerType === "touch") {
      if (pens.size || event.timeStamp - lastPenActivity < 650) {
        o.release(event.pointerId);
        idle();
        return true;
      }
      touches.set(event.pointerId, v);
      if (touches.size >= 2) {
        startPinch();
        return true;
      }
    }
    if (
      event.pointerType === "pen" &&
      (pinch || gesture?.pointerType === "touch")
    ) {
      if (gesture && (gesture.kind === "erase" || gesture.kind === "move"))
        o.preview(gesture.base);
      if (gesture) o.release(gesture.pointerId);
      gesture = null;
      pinch = null;
      touches.clear();
      discardSamples();
    }
    if (pinch || gesture) return true;
    const p = viewToScene(o.camera(), v),
      base = o.scene();
    const common = {
      pointerId: event.pointerId,
      pointerType: event.pointerType,
      base,
      start: p,
      lastView: v,
      startedAt: event.timeStamp,
      path: [p],
      initial: [...o.selected()],
    };
    if (
      tool === "hand" ||
      event.button === 1 ||
      (event.pointerType === "touch" && !o.fingerDraw())
    ) {
      gesture = { ...common, kind: "pan" };
      publish();
      return true;
    }
    if (!tool) {
      o.release(event.pointerId);
      idle();
      return false;
    }
    if (tool === "select") {
      const hit = o.hit(base, p, 8 / o.camera().zoom);
      if (hit) {
        if (o.multiselect && event.shiftKey) {
          o.select(
            o.selected().includes(hit)
              ? o.selected().filter((id) => id !== hit)
              : [...o.selected(), hit],
          );
          o.release(event.pointerId);
          idle();
          return true;
        }
        if (!o.selected().includes(hit)) o.select([hit]);
        gesture = { ...common, kind: "move", hit };
      } else {
        if (!(o.multiselect && event.shiftKey)) o.select([]);
        gesture = {
          ...common,
          kind: "lasso",
          initial: o.multiselect && event.shiftKey ? common.initial : [],
        };
      }
      publish();
      return true;
    }
    if (tool === "eraser") {
      gesture = { ...common, kind: "erase" };
      o.preview(
        o.erase(
          base,
          [p],
          Math.max(8, o.size() * 1.1) / o.camera().zoom,
          o.eraserMode(),
        ),
      );
      publish();
      return true;
    }
    if (
      !shouldDrawWithPointer(
        event.pointerType,
        o.fingerDraw(),
        pens.size > 0 && event.pointerType !== "pen",
      )
    ) {
      o.release(event.pointerId);
      idle();
      return true;
    }
    const point = scenePoint(event, event.timeStamp);
    if (!point) {
      o.release(event.pointerId);
      idle();
      return true;
    }
    const brush = o.brush(p);
    gesture = {
      ...common,
      kind: "ink",
      stroke: {
        id: randomUuid(),
        input: drawInputKind(event.pointerType),
        color: brush.color,
        brush: createDrawBrush(
          brush.kind,
          Math.min(o.maxBrushSize ?? Infinity, brush.size / o.camera().zoom),
        ),
        points: [point],
      },
    };
    publish();
    return true;
  }
  function move(event: PointerEvent): boolean {
    const v = o.viewPoint(event.clientX, event.clientY);
    if (!v) return false;
    cursor = viewToScene(o.camera(), v);
    if (event.pointerType === "pen") lastPenActivity = event.timeStamp;
    if (event.pointerType === "touch") {
      touches.set(event.pointerId, v);
      if (pinch) {
        event.preventDefault();
        event.stopPropagation();
        updatePinch();
        publish();
        return true;
      }
    }
    const g = gesture;
    if (!g || g.pointerId !== event.pointerId) {
      publish();
      return false;
    }
    event.preventDefault();
    event.stopPropagation();
    if (g.kind === "pan") {
      o.setCamera(
        panDrawCamera(o.camera(), {
          x: v.x - g.lastView.x,
          y: v.y - g.lastView.y,
        }),
      );
      gesture = { ...g, lastView: v };
      publish();
      return true;
    }
    if (g.kind === "ink") {
      queue(
        coalescedPointerSamples(event)
          .map((e) => scenePoint(e, g.startedAt))
          .filter((p): p is DrawPoint => p !== null),
      );
      return true;
    }
    const p = cursor;
    if (g.kind === "erase") {
      const points = coalescedPointerSamples(event).flatMap((e) => {
        const view = o.viewPoint(e.clientX, e.clientY);
        return view ? [viewToScene(o.camera(), view)] : [];
      });
      gesture = { ...g, path: [...g.path, ...points] };
      o.preview(
        o.erase(
          o.scene(),
          [g.path[g.path.length - 1], ...points],
          Math.max(8, o.size() * 1.1) / o.camera().zoom,
          o.eraserMode(),
        ),
      );
    } else if (g.kind === "lasso") {
      const last = g.path[g.path.length - 1];
      if (Math.hypot(p.x - last.x, p.y - last.y) >= 2 / o.camera().zoom)
        gesture = { ...g, path: [...g.path, p] };
    } else if (g.kind === "move")
      o.preview(
        o.move(g.base, o.selected(), {
          x: p.x - g.start.x,
          y: p.y - g.start.y,
        }),
      );
    publish();
    return true;
  }
  function finish(event: PointerEvent): boolean {
    if (event.pointerType === "touch") touches.delete(event.pointerId);
    if (event.pointerType === "pen") pens.delete(event.pointerId);
    if (pinch) {
      if (pinch.ids.includes(event.pointerId) && !touches.size) pinch = null;
      o.release(event.pointerId);
      publish();
      idle();
      return true;
    }
    const g = gesture;
    if (!g || g.pointerId !== event.pointerId) {
      o.release(event.pointerId);
      idle();
      return false;
    }
    event.preventDefault();
    event.stopPropagation();
    if (g.kind === "ink") {
      flush();
      const stroke = gesture?.stroke ?? g.stroke!;
      gesture = null;
      o.commit(
        g.base,
        o.addStroke(g.base, {
          ...stroke,
          points: simplifyDrawPoints(stroke.points),
        }),
        "Draw stroke",
      );
    } else {
      gesture = null;
      if (g.kind === "erase" || g.kind === "move")
        o.commit(
          g.base,
          o.scene(),
          g.kind === "move"
            ? "Move selection"
            : o.eraserMode() === "partial"
              ? "Erase ink"
              : "Erase strokes",
        );
      else if (g.kind === "lasso")
        o.select([...new Set([...g.initial, ...o.lasso(o.scene(), g.path)])]);
    }
    o.release(event.pointerId);
    publish();
    idle();
    const v = o.viewPoint(event.clientX, event.clientY);
    if (
      v &&
      (g.kind === "move" || (g.kind === "lasso" && g.path.length === 1)) &&
      Math.hypot(v.x - g.lastView.x, v.y - g.lastView.y) < 3 &&
      !event.shiftKey
    )
      o.tap?.(event, g.hit, viewToScene(o.camera(), v));
    return true;
  }
  function cancel(event?: PointerEvent) {
    if (event) {
      if (event.pointerType === "touch") touches.delete(event.pointerId);
      if (event.pointerType === "pen") pens.delete(event.pointerId);
      if (pinch?.ids.includes(event.pointerId) && !touches.size) pinch = null;
      if (!gesture || gesture.pointerId !== event.pointerId) {
        publish();
        idle();
        return;
      }
    }
    const g = gesture;
    if (g && (g.kind === "erase" || g.kind === "move")) o.preview(g.base);
    gesture = null;
    discardSamples();
    if (!event) {
      touches.clear();
      pens.clear();
      pinch = null;
    }
    if (g) o.release(g.pointerId);
    publish();
    idle();
  }
  function wheel(event: WheelEvent) {
    if (!o.editable()) return;
    const anchor = o.viewPoint(event.clientX, event.clientY);
    if (!anchor) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.ctrlKey || event.metaKey)
      o.setCamera(
        zoomAt(
          o.camera(),
          anchor,
          o.camera().zoom * Math.exp(-event.deltaY * 0.008),
        ),
      );
    else {
      const translated = o.viewPoint(
        event.clientX - event.deltaX,
        event.clientY - event.deltaY,
      );
      o.setCamera(
        panDrawCamera(
          o.camera(),
          translated
            ? { x: translated.x - anchor.x, y: translated.y - anchor.y }
            : { x: -event.deltaX, y: -event.deltaY },
        ),
      );
    }
  }
  return {
    begin,
    move,
    finish,
    cancel,
    wheel,
    flush,
    get active() {
      return !!gesture || !!pinch;
    },
    get drawing() {
      return gesture?.kind === "ink";
    },
    get state() {
      return state();
    },
    leave() {
      cursor = null;
      publish();
    },
    destroy() {
      discardSamples();
      touches.clear();
      pens.clear();
    },
  };
}
