/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { createDrawInteraction, type DrawTool } from "./drawInteraction";
import { createDrawCamera, type DrawCamera } from "./drawCamera";
import { createEmptyDrawDocument, type DrawBrushKind } from "./drawDocument";
import {
  eraseDrawDocumentByPath,
  moveDrawStroke,
  selectDrawStrokesInLasso,
  topDrawStrokeAt,
} from "./drawGeometry";
function setup() {
  let scene = createEmptyDrawDocument(),
    camera: DrawCamera = createDrawCamera(),
    selected: string[] = [],
    tool: DrawTool = "ink",
    brush: DrawBrushKind = "pen",
    finger = false;
  const commit = vi.fn((base, next) => (scene = next)),
    update = vi.fn(),
    release = vi.fn();
  const core = createDrawInteraction({
    editable: () => true,
    tool: () => tool,
    camera: () => camera,
    setCamera: (c) => (camera = c),
    viewPoint: (x, y) => ({ x, y }),
    scene: () => scene,
    preview: (next) => (scene = next),
    commit,
    addStroke: (base, stroke) => ({
      ...base,
      strokes: [...base.strokes, stroke],
    }),
    erase: eraseDrawDocumentByPath,
    move: (base, ids, delta) => ({
      ...base,
      strokes: base.strokes.map((s) =>
        ids.includes(s.id) ? moveDrawStroke(s, delta) : s,
      ),
    }),
    hit: (base, p, padding) =>
      topDrawStrokeAt(base.strokes, p, padding)?.id ?? null,
    lasso: (base, path) => selectDrawStrokesInLasso(base.strokes, path),
    selected: () => selected,
    select: (ids) => (selected = ids),
    brush: () => ({ kind: brush, size: 6, color: "#123456" }),
    size: () => 6,
    eraserMode: () => "partial",
    fingerDraw: () => finger,
    capture: () => {},
    release,
    focus: () => {},
    update,
  });
  return {
    core,
    commit,
    update,
    release,
    get scene() {
      return scene;
    },
    get camera() {
      return camera;
    },
    get selected() {
      return selected;
    },
    setTool: (v: DrawTool) => (tool = v),
    setCamera: (v: DrawCamera) => (camera = v),
    setBrush: (v: DrawBrushKind) => (brush = v),
    setFinger: (v: boolean) => (finger = v),
  };
}
function event(
  type: string,
  x: number,
  y: number,
  pointerType = "mouse",
  pointerId = 1,
  time = 1000,
) {
  const e = new PointerEvent(type, {
    clientX: x,
    clientY: y,
    pointerId,
    pointerType,
    button: 0,
    pressure: 0.5,
    tiltX: 20,
    tiltY: -5,
  });
  Object.defineProperty(e, "timeStamp", { value: time });
  return e;
}
afterEach(() => vi.useRealTimers());
it("batches high-frequency samples on animation frames and flushes on lift", () => {
  vi.useFakeTimers();
  const h = setup();
  h.core.begin(event("pointerdown", 10, 10, "pen"));
  h.core.move(event("pointermove", 15, 10, "pen"));
  h.core.move(event("pointermove", 20, 10, "pen"));
  expect(h.core.state.activeStroke?.points).toHaveLength(1);
  h.core.finish(event("pointerup", 20, 10, "pen"));
  expect(h.commit).toHaveBeenCalledTimes(1);
  expect(h.scene.strokes[0].points.length).toBeGreaterThanOrEqual(2);
  expect(h.scene.strokes[0].points[0]).toMatchObject({
    tiltX: 20,
    tiltY: -5,
    pressure: expect.any(Number),
  });
  vi.runAllTimers();
  expect(h.commit).toHaveBeenCalledTimes(1);
});
it("uses each Notes brush preset with the same screen-space width at zoom", () => {
  for (const brush of ["pen", "pencil", "marker", "highlighter"] as const) {
    const h = setup();
    h.setBrush(brush);
    h.setCamera({ zoom: 2, panX: 0, panY: 0 });
    h.core.begin(event("pointerdown", 20, 20));
    h.core.finish(event("pointerup", 20, 20));
    expect(h.scene.strokes[0].brush).toMatchObject({ kind: brush, size: 3 });
    expect(h.scene.strokes[0].points[0]).toMatchObject({ x: 10, y: 10 });
  }
});
it("touch pans by default, pinches around its anchor, then releases cleanly", () => {
  const h = setup();
  h.core.begin(event("pointerdown", 10, 10, "touch", 1));
  h.core.move(event("pointermove", 20, 10, "touch", 1));
  expect(h.camera.panX).toBe(10);
  expect(h.scene.strokes).toEqual([]);
  h.core.begin(event("pointerdown", 40, 10, "touch", 2));
  h.core.move(event("pointermove", 60, 10, "touch", 2));
  expect(h.camera.zoom).toBeCloseTo(2);
  h.core.finish(event("pointerup", 20, 10, "touch", 1));
  h.core.cancel(event("pointercancel", 60, 10, "touch", 2));
  expect(h.core.state.pinching).toBe(false);
  expect(h.core.active).toBe(false);
});
it("pen takes precedence over a resting touch and rejects palm input", () => {
  const h = setup();
  h.core.begin(event("pointerdown", 0, 0, "touch", 2, 100));
  h.core.begin(event("pointerdown", 10, 10, "pen", 1, 200));
  expect(h.core.drawing).toBe(true);
  h.core.begin(event("pointerdown", 40, 40, "touch", 3, 210));
  expect(h.core.drawing).toBe(true);
  h.core.finish(event("pointerup", 10, 10, "pen", 1, 220));
  expect(h.scene.strokes).toHaveLength(1);
  h.core.begin(event("pointerdown", 40, 40, "touch", 4, 300));
  expect(h.core.active).toBe(false);
});
it("uses stroke-aware picking and freehand lasso, with reversible movement previews", () => {
  const h = setup();
  h.core.begin(event("pointerdown", 10, 10));
  h.core.move(event("pointermove", 100, 10));
  h.core.finish(event("pointerup", 100, 10));
  h.setTool("select");
  h.core.begin(event("pointerdown", 5, 0));
  for (const p of [
    [110, 0],
    [110, 20],
    [5, 20],
    [5, 0],
  ])
    h.core.move(event("pointermove", p[0], p[1]));
  h.core.finish(event("pointerup", 5, 0));
  expect(h.selected).toEqual([h.scene.strokes[0].id]);
  const before = structuredClone(h.scene);
  h.core.begin(event("pointerdown", 20, 10));
  h.core.move(event("pointermove", 40, 30));
  expect(h.scene.strokes[0].points[0]).toMatchObject({ x: 30, y: 30 });
  h.core.cancel(event("pointercancel", 40, 30));
  expect(h.scene).toEqual(before);
});
