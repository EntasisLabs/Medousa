import { expect, it } from "vitest";
import { createDrawBrush } from "$lib/draw/drawDocument";
import { createNode, mapPoint, worldMatrix, nodeDimensions } from "./creativeScene";
import { dragShape, styleNode, nodeColor } from "./creativeInteraction";
import { lineGeometry, linePaint, hitLine, snapLinePoint } from "./creativeLine";
import { moveLineEndpoint, rotateSelection, selectionFrame } from "./creativeSelection";

function seed() { return createNode({ kind: "shape", shape: "line", fill: "transparent", stroke: "#000", stroke_width: 4 }, { x: 0, y: 0, width: 1, height: 1 }); }
function endpoints(node: ReturnType<typeof seed>, nodes = [node]) {
  const line = lineGeometry(node)!, m = worldMatrix(node, nodes);
  return [mapPoint(m, line.start), mapPoint(m, line.end)];
}
it("keeps exact endpoints in every quadrant and makes sharp heads at the requested tip", () => {
  for (const end of [{ x: 200, y: 0 }, { x: 0, y: 200 }, { x: -150, y: 70 }, { x: -50, y: -20 }]) {
    const node = dragShape(seed(), "arrow", { x: 0, y: 0 }, end, "#123456", 6, "none");
    expect(endpoints(node)).toEqual([{ x: 0, y: 0 }, end]);
    const line = lineGeometry(node)!;
    expect(linePaint(line).heads).toHaveLength(1);
    expect(linePaint(line).heads[0][0]).toEqual(line.end);
    expect(linePaint(line).heads[0]).toHaveLength(3);
    expect(hitLine(line, linePaint(line).heads[0][1], 0)).toBe(true);
  }
});
it("keeps the opposite endpoint fixed inside a rotated frame and snaps in world space", () => {
  const frame = createNode({ kind: "frame", name: "Frame", background: "#fff" }, { x: 200, y: 200, width: 600, height: 400 });
  frame.transform.rotation = 32;
  const node = { ...dragShape(seed(), "line", { x: 40, y: 50 }, { x: 250, y: 100 }, "#123456", 4, "none"), parent_id: frame.id };
  node.transform.rotation = -15;
  const nodes = [frame, node], before = endpoints(node, nodes), target = { x: before[0].x + 100, y: before[0].y + 20 };
  const next = moveLineEndpoint(node, nodes, "end", target, true), after = endpoints(next, [frame, next]);
  expect(after[0].x).toBeCloseTo(before[0].x); expect(after[0].y).toBeCloseTo(before[0].y);
  expect(after[1].y).toBeCloseTo(before[0].y);
  expect(after[1].x).toBeCloseTo(before[0].x + Math.hypot(100, 20));
  expect(snapLinePoint({ x: 0, y: 0 }, { x: -99, y: -100 }, true).x).toBeCloseTo(snapLinePoint({ x: 0, y: 0 }, { x: -99, y: -100 }, true).y);
});
it("retains the stroke color through head, dash, and thickness changes", () => {
  const original = dragShape(seed(), "arrow", { x: 30, y: 20 }, { x: 200, y: 60 }, "#123456", 6, "none");
  const styled = styleNode(styleNode(original, { dashed: true }), { start_arrow: true, stroke_width: 8 });
  expect(nodeColor(styled)).toBe("#123456");
  expect(lineGeometry(styled)).toMatchObject({ color: "#123456", stroke_width: 8, dashed: true, start_arrow: true, end_arrow: true });
  expect(endpoints(styled)).toEqual(endpoints(original));
});
it("recognizes old arrow geometry, preserves opacity on edit, and leaves ordinary ink alone", () => {
  const length = 200, head = 28;
  const points = [{ x: 0, y: 0 }, { x: length, y: 0 }, { x: length - Math.cos(-.5) * head, y: -Math.sin(-.5) * head }, { x: length, y: 0 }, { x: length - Math.cos(.5) * head, y: -Math.sin(.5) * head }].map(p => ({ ...p, pressure: .5 }));
  const node = createNode({ kind: "ink", input: "mouse", brush: { ...createDrawBrush("pen", 4), opacity: .4 }, color: "#000", points }, { x: 10, y: 20, width: 200, height: 30 });
  expect(lineGeometry(node)?.end_arrow).toBe(true);
  const next = styleNode(node, { dashed: true, start_arrow: true });
  expect(next.content.kind).toBe("shape"); expect(next.opacity).toBe(.4);
  expect(next.transform).toEqual(node.transform);
  expect(endpoints(next)).toEqual(endpoints(node));
  expect(lineGeometry({ ...node, content: { ...node.content, kind: "ink", input: "pen", brush: createDrawBrush("pen", 4), color: "#000", points } })).toBeNull();
  const zero = { ...lineGeometry(next)!, end: { ...lineGeometry(next)!.start } };
  expect(hitLine(zero, { x: 999, y: 999 }, 1)).toBe(false);
});
it("rotates a group once around its visible center while retaining an oriented selection", () => {
  const parent = createNode({ kind: "frame", name: "Frame", background: "#fff" }, { x: 200, y: 100, width: 800, height: 600 }); parent.transform.rotation = 25;
  const group = createNode({ kind: "group" }, { x: 20, y: 20, width: 500, height: 400 }, parent.id);
  const child = { ...dragShape(seed(), "arrow", { x: 30, y: 40 }, { x: 180, y: 100 }, "#000", 4, "none"), parent_id: group.id };
  const nodes = [parent, group, child], before = endpoints(child, nodes), frame = selectionFrame([group.id], nodes)!;
  const next = rotateSelection([group.id, child.id], nodes, frame.center, 90), after = endpoints(child, next);
  expect(next[0]).toBe(parent); expect(next[2]).toBe(child);
  before.forEach((p, i) => { expect(after[i].x).toBeCloseTo(frame.center.x - (p.y - frame.center.y)); expect(after[i].y).toBeCloseTo(frame.center.y + p.x - frame.center.x); });
  const rotated = selectionFrame([group.id], next)!;
  expect(rotated.width).toBeCloseTo(frame.width); expect(rotated.height).toBeCloseTo(frame.height);
  expect(rotated.center.x).toBeCloseTo(frame.center.x); expect(rotated.center.y).toBeCloseTo(frame.center.y);
  expect(rotated.angle).toBeCloseTo(frame.angle + 90);
});
it("rotates separate objects around a shared center, keeping measured text and locks intact", () => {
  const a = createNode({ kind: "text", text: "Title", color: "#000", font_family: "sans-serif", font_size: 32 }, { x: 20, y: 40, width: 900, height: 20 });
  const b = createNode({ kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 2 }, { x: 300, y: 100, width: 100, height: 100 });
  const nodes = [a, b], center = selectionFrame([a.id, b.id], nodes)!.center;
  const next = rotateSelection([a.id, b.id], nodes, center, 180);
  for (const [i, old] of nodes.entries()) {
    const point = { x: nodeDimensions(old).width / 2, y: nodeDimensions(old).height / 2 }, before = mapPoint(worldMatrix(old, nodes), point), after = mapPoint(worldMatrix(next[i], next), point);
    expect(after.x).toBeCloseTo(2 * center.x - before.x); expect(after.y).toBeCloseTo(2 * center.y - before.y);
  }
  b.locked = true;
  expect(rotateSelection([a.id, b.id], nodes, center, 30)[1]).toBe(b);
});
