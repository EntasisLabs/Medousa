import { describe, expect, it } from "vitest";
import {
  createDrawCamera,
  panDrawCamera,
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
});
