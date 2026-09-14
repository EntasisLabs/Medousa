import { describe, expect, it } from "vitest";
import {
  createDrawCamera,
  fitDrawCamera,
  panDrawCamera,
  resizeDrawCamera,
  sceneToView,
  viewToScene,
  zoomDrawCameraAt,
} from "./drawCamera";

describe("draw camera", () => {
  it.each([
    { panX: 0, panY: 0, zoom: 0.25 },
    { panX: 72, panY: -31, zoom: 2.75 },
    { panX: -640, panY: 480, zoom: 8 },
  ])("round-trips scene and view coordinates at $zoom×", (camera) => {
    const point = { x: 144, y: 88 };
    const roundTrip = viewToScene(camera, sceneToView(camera, point));
    expect(roundTrip.x).toBeCloseTo(point.x);
    expect(roundTrip.y).toBeCloseTo(point.y);
  });

  it("keeps the zoom anchor fixed and clamps zoom", () => {
    const anchor = { x: 400, y: 240 };
    const camera = panDrawCamera(createDrawCamera(), { x: 20, y: 15 });
    const sceneAnchor = viewToScene(camera, anchor);
    const zoomed = zoomDrawCameraAt(camera, anchor, 99);
    expect(zoomed.zoom).toBe(8);
    expect(sceneToView(zoomed, sceneAnchor)).toEqual(anchor);
  });

  it("fits content into a real viewport", () => {
    const fitted = fitDrawCamera(
      { x: 100, y: 200, width: 800, height: 400 },
      { width: 400, height: 700 },
      20,
    );
    expect(fitted.zoom).toBe(0.45);
    expect(sceneToView(fitted, { x: 100, y: 200 })).toEqual({ x: 20, y: 260 });
    expect(sceneToView(fitted, { x: 900, y: 600 })).toEqual({ x: 380, y: 440 });
  });

  it("keeps the viewed scene center through a viewport resize", () => {
    const camera = { panX: -50, panY: 80, zoom: 1.5 };
    const previous = { width: 400, height: 700 };
    const center = viewToScene(camera, { x: previous.width / 2, y: previous.height / 2 });
    const next = { width: 900, height: 500 };
    const resized = resizeDrawCamera(camera, previous, next);
    expect(sceneToView(resized, center)).toEqual({ x: 450, y: 250 });
  });
});
