import { describe, expect, it } from "vitest";
import {
  createNode,
  mapPoint,
  worldMatrix,
  starterScene,
} from "./creativeScene";
import {
  dragShape,
  duplicateSelection,
  marqueeSelection,
  styleNode,
  hitCanvasNode,
  shapeFill,
  canvasInk,
} from "./creativeInteraction";
const rect = (x = 0, y = 0) =>
  createNode(
    {
      kind: "shape",
      shape: "rectangle",
      fill: "transparent",
      stroke: "#123456",
      stroke_width: 2,
    },
    { x, y, width: 60, height: 40 },
  );
describe("canvas interactions on durable scene nodes", () => {
  it("starts blank without forcing a frame but preserves framed starters", () => {
    expect(starterScene("blank").nodes).toEqual([]);
    expect(starterScene("composition").nodes[0].transform).toMatchObject({
      width: 1024,
      height: 1024,
    });
  });
  it("preserves line endpoints in every drag direction", () => {
    for (const end of [
      { x: 80, y: 45 },
      { x: -80, y: 45 },
      { x: 80, y: -45 },
      { x: -80, y: -45 },
      { x: 0, y: -80 },
    ]) {
      const start = { x: 10, y: 20 },
        node = dragShape(rect(), "line", start, end, "#123456", 4, "none");
      const m = worldMatrix(node, [node]),
        a = mapPoint(m, { x: 0, y: 0 }),
        b = mapPoint(m, { x: node.transform.width, y: node.transform.height });
      // Minimum one-unit boxes keep perfectly horizontal/vertical lines valid.
      expect(Math.hypot(a.x - start.x, a.y - start.y)).toBeLessThan(1);
      expect(Math.hypot(b.x - end.x, b.y - end.y)).toBeLessThan(1);
    }
  });
  it("keeps arrows editable with existing ink and recolors without losing pressure metadata", () => {
    const node = dragShape(
      rect(),
      "arrow",
      { x: 50, y: 80 },
      { x: -100, y: 10 },
      "#123456",
      4,
      "none",
    );
    expect(node.content.kind).toBe("ink");
    const styled = styleNode(node, { color: "#d76956", size: "l" });
    expect(styled.content).toMatchObject({
      kind: "ink",
      color: "#d76956",
      brush: { size: 7 },
      points: expect.arrayContaining([
        expect.objectContaining({ pressure: 0.5 }),
      ]),
    });
    expect(node.content).toMatchObject({
      color: "#123456",
      brush: { size: 4 },
    });
  });
  it("selects nested objects in world coordinates and respects inherited locks", () => {
    const frame = starterScene("screenshot").nodes[0];
    frame.transform = { x: 100, y: 100, width: 200, height: 200, rotation: 90 };
    const child = { ...rect(20, 30), parent_id: frame.id };
    const locked = rect(130, 150);
    locked.locked = true;
    expect(
      marqueeSelection([frame, child, locked], {
        x: 100,
        y: 100,
        width: 200,
        height: 200,
      }),
    ).toEqual([child.id]);
    frame.locked = true;
    expect(
      marqueeSelection([frame, child], { x: 0, y: 0, width: 500, height: 500 }),
    ).toEqual([]);
  });
  it("duplicates a subtree once and preserves child coordinates and asset references", () => {
    const frame = starterScene("screenshot").nodes[0];
    const image = createNode(
      { kind: "image", asset_id: "existing", crop: null },
      { x: 10, y: 20, width: 100, height: 50 },
      frame.id,
    );
    const copy = duplicateSelection([frame.id, image.id], [image, frame], {
      x: 20,
      y: 30,
    });
    const parent = copy.find((n) => n.content.kind === "frame")!,
      child = copy.find((n) => n.content.kind === "image")!;
    expect(copy).toHaveLength(2);
    expect(parent.id).not.toBe(frame.id);
    expect(parent.transform).toMatchObject({ x: 20, y: 30 });
    expect(child.parent_id).toBe(parent.id);
    expect(child.transform).toEqual(image.transform);
    expect(child.content).toEqual(image.content);
  });
  it("changes shape color while retaining its fill intent", () => {
    const hollow = rect(),
      soft = styleNode(hollow, { fill: "soft" });
    expect(styleNode(hollow, { color: "#abcdef" }).content).toMatchObject({
      fill: "transparent",
      stroke: "#abcdef",
    });
    expect(styleNode(soft, { color: "#abcdef" }).content).toMatchObject({
      fill: "#abcdef24",
      stroke: "#abcdef",
    });
  });
});

it("keeps erasing hollow shapes and ink precise", () => {
  const node = rect();
  expect(hitCanvasNode(node, [node], { x: 30, y: 20 }, 2)).toBe(false);
  expect(hitCanvasNode(node, [node], { x: 0, y: 20 }, 2)).toBe(true);
  expect(shapeFill("#abc", "soft")).toBe("#aabbcc24");
  expect(canvasInk("#fff", "#e7e6df")).toBe("#393e33");
  expect(canvasInk("transparent", "#e7e6df")).toBe("#e7e6df");
});

it("keeps rotated text anchored when changing its size", () => {
  const node = createNode(
    {
      kind: "text",
      text: "Title",
      color: "#000",
      font_size: 32,
      font_family: "sans-serif",
    },
    { x: 10, y: 20, width: 100, height: 40 },
  );
  node.transform.rotation = 45;
  const before = mapPoint(worldMatrix(node, [node]), { x: 0, y: 0 });
  const next = styleNode(node, { size: "xl" }),
    after = mapPoint(worldMatrix(next, [next]), { x: 0, y: 0 });
  expect(after.x).toBeCloseTo(before.x);
  expect(after.y).toBeCloseTo(before.y);
  expect(next.transform.width).toBe(225);
});
