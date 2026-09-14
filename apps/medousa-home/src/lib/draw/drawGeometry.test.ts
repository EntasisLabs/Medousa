import { describe, expect, it } from "vitest";
import { createDrawBrush, createEmptyDrawDocument, type DrawStroke } from "./drawDocument";
import {
  combinedDrawBounds,
  drawStrokeOutlinePath,
  drawStrokeRadius,
  eraseDrawDocumentByPath,
  eraseDrawStrokeByPath,
  hitTestDrawStroke,
  moveDrawStroke,
  selectDrawStrokesInLasso,
  simplifyDrawPoints,
} from "./drawGeometry";

function stroke(points: DrawStroke["points"]): DrawStroke {
  return {
    id: "stroke",
    color: "#fff",
    input: "pen",
    brush: createDrawBrush("pen", 10),
    points,
  };
}

describe("draw geometry", () => {
  it("turns pressure into visible width", () => {
    const ink = stroke([
      { x: 0, y: 0, pressure: 0.1 },
      { x: 20, y: 0, pressure: 0.9 },
    ]);
    expect(drawStrokeRadius(ink, 1)).toBeGreaterThan(drawStrokeRadius(ink, 0) * 2);
    expect(drawStrokeOutlinePath(ink)).toMatch(/^M .* Z$/);
  });

  it("uses velocity when pressure is missing", () => {
    const slow = stroke([
      { x: 0, y: 0, elapsedMs: 0 },
      { x: 2, y: 0, elapsedMs: 20 },
    ]);
    const fast = stroke([
      { x: 0, y: 0, elapsedMs: 0 },
      { x: 20, y: 0, elapsedMs: 4 },
    ]);
    expect(drawStrokeRadius(slow, 1)).toBeGreaterThan(drawStrokeRadius(fast, 1));
  });

  it("simplifies dense points while preserving pressure changes", () => {
    const simplified = simplifyDrawPoints([
      { x: 0, y: 0, pressure: 0.5 },
      { x: 0.1, y: 0.1, pressure: 0.51 },
      { x: 0.2, y: 0.2, pressure: 0.8 },
      { x: 8, y: 8, pressure: 0.8 },
    ]);
    expect(simplified).toHaveLength(3);
    expect(simplified[1].pressure).toBe(0.8);
  });

  it("hit tests, lassos, moves, and bounds strokes", () => {
    const ink = stroke([{ x: 10, y: 10 }, { x: 30, y: 30 }]);
    expect(hitTestDrawStroke(ink, { x: 20, y: 20 })).toBe(true);
    expect(selectDrawStrokesInLasso([ink], [
      { x: 0, y: 0 }, { x: 40, y: 0 }, { x: 40, y: 40 }, { x: 0, y: 40 },
    ])).toEqual(["stroke"]);
    const moved = moveDrawStroke(ink, { x: 100, y: 50 });
    expect(moved.points[0]).toMatchObject({ x: 110, y: 60 });
    expect(combinedDrawBounds([moved])?.x).toBeGreaterThan(100);
  });

  it("splits a stroke for partial erasing", () => {
    const ink = stroke(Array.from({ length: 11 }, (_, index) => ({ x: index * 10, y: 0 })));
    const parts = eraseDrawStrokeByPath(ink, [{ x: 50, y: -10 }, { x: 50, y: 10 }], 3);
    expect(parts).toHaveLength(2);
    expect(parts[0].points.at(-1)?.x).toBeLessThan(50);
    expect(parts[1].points[0].x).toBeGreaterThan(50);
  });

  it("selects a segment that crosses the lasso without a point inside", () => {
    const crossing = stroke([{ x: -20, y: 20 }, { x: 60, y: 20 }]);
    expect(selectDrawStrokesInLasso([crossing], [
      { x: 0, y: 0 }, { x: 40, y: 0 }, { x: 40, y: 40 }, { x: 0, y: 40 },
    ])).toEqual(["stroke"]);
  });

  it("splits sparse strokes when an eraser crosses between retained samples", () => {
    const sparse = stroke([{ x: 0, y: 0 }, { x: 100, y: 0 }]);
    const parts = eraseDrawStrokeByPath(sparse, [{ x: 50, y: -10 }, { x: 50, y: 10 }], 3);
    expect(parts).toHaveLength(2);
    expect(parts[0].points.at(-1)?.x).toBeLessThan(50);
    expect(parts[1].points[0].x).toBeGreaterThan(50);
  });

  it("skips stroke cloning when an eraser segment is outside its bounds", () => {
    const ink = stroke([{ x: 0, y: 0 }, { x: 100, y: 0 }]);
    const parts = eraseDrawStrokeByPath(ink, [{ x: 400, y: 400 }, { x: 420, y: 420 }], 8);
    expect(parts).toEqual([ink]);
    expect(parts[0]).toBe(ink);
  });

  it("structurally shares an unchanged document during incremental erasing", () => {
    const document = createEmptyDrawDocument();
    document.strokes.push(stroke([{ x: 0, y: 0 }, { x: 100, y: 0 }]));
    const unchanged = eraseDrawDocumentByPath(
      document,
      [{ x: 400, y: 400 }, { x: 420, y: 420 }],
      8,
      "partial",
    );
    expect(unchanged).toBe(document);
    expect(eraseDrawDocumentByPath(document, [{ x: 50, y: 0 }], 8, "partial")).not.toBe(document);
  });

  it("does not mutate source strokes", () => {
    const document = createEmptyDrawDocument();
    const ink = stroke([{ x: 1, y: 1 }]);
    document.strokes.push(ink);
    moveDrawStroke(ink, { x: 4, y: 4 });
    expect(document.strokes[0].points[0]).toEqual({ x: 1, y: 1 });
  });
});
