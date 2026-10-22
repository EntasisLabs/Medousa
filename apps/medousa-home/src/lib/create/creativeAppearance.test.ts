import { expect, it } from "vitest";
import { createNode } from "./creativeScene";
import { defaultShadow, visualBounds } from "./creativeAppearance";
import { hitCanvasNode, styleNode } from "./creativeInteraction";
import { resizeCursor } from "./creativeCursors";
import { resizeSelectionFromHandle } from "./creativeSelection";
const card = () => createNode({ kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#123456", stroke_width: 3 }, { x: 0, y: 0, width: 100, height: 80 });
it("changes radius, shadow, fill and border independently", () => {
  const node = styleNode(card(), { corner_radius: 20, shadow: defaultShadow });
  expect(node.content).toEqual(card().content);
  const next = styleNode(node, { fill_color: "#ff0000", border_width: 0 });
  expect(next.content).toMatchObject({ fill: "#ff0000", stroke: "#123456", stroke_width: 0 });
  expect(styleNode(next, { corner_radius: 5 }).content).toEqual(next.content);
  expect(styleNode(next, { shadow: null }).appearance?.shadow).toBeNull();
});
it("picks rounded surfaces and leaves the clipped corner empty", () => {
  const node = styleNode(card(), { corner_radius: 30 });
  expect(hitCanvasNode(node, [node], { x: 0, y: 0 }, 0)).toBe(false);
  expect(hitCanvasNode(node, [node], { x: 50, y: 40 }, 0)).toBe(true);
  const hollow = styleNode(node, { fill_color: "transparent" });
  expect(hitCanvasNode(hollow, [hollow], { x: 50, y: 40 }, 0)).toBe(false);
  expect(hitCanvasNode(hollow, [hollow], { x: 30, y: 0 }, 0)).toBe(true);
});
it("includes a rotated shadow in pixel bounds without enlarging object dimensions", () => {
  const node = styleNode(card(), { shadow: defaultShadow });
  node.transform.rotation = 45;
  const bounds = visualBounds(node, [node]);
  expect(bounds.width).toBeGreaterThan(230);
  expect(node.transform.width).toBe(100);
});
it("uses world-angle resize cursors", () => {
  expect(resizeCursor("e", 90)).toBe("ns-resize");
  expect(resizeCursor("nw", 45)).toBe("ns-resize");
  expect(resizeCursor("ne", -45)).toBe("ns-resize");
});
it("side handles wrap text while corner handles scale fonts and fixed boxes", () => {
  const text = styleNode(createNode({ kind: "text", text: "One two three four five six", color: "#000", font_family: "monospace", font_size: 20 }, { x: 0, y: 0, width: 300, height: 30 }), {});
  const wrapped = resizeSelectionFromHandle([text.id], [text], "e", { x: 80, y: 0 })[0];
  expect(wrapped.text_layout?.width).toBe(80);
  expect(wrapped.content).toMatchObject({ font_size: 20 });
  const scaled = resizeSelectionFromHandle([wrapped.id], [wrapped], "se", { x: 160, y: wrapped.transform.height * 2 })[0];
  expect(scaled.text_layout?.width).toBeCloseTo(160);
  expect(scaled.content).toMatchObject({ font_size: 40 });
});
it("does not pick rounded image corners or children clipped outside a rounded frame", () => {
  const frame = createNode({ kind: "frame", name: "Card", background: "#fff" }, { x: 0, y: 0, width: 100, height: 100 });
  frame.appearance = { corner_radius: 30 };
  const image = createNode({ kind: "image", asset_id: "picture" }, { x: 0, y: 0, width: 100, height: 100 }, frame.id);
  expect(hitCanvasNode(image, [frame, image], { x: 1, y: 1 }, 0)).toBe(false);
  expect(hitCanvasNode(image, [frame, image], { x: 50, y: 50 }, 0)).toBe(true);
  image.parent_id = null; image.appearance = { corner_radius: 30 };
  expect(hitCanvasNode(image, [image], { x: 1, y: 1 }, 0)).toBe(false);
});
