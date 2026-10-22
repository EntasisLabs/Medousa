import { expect, it } from "vitest";
import { createNode, applySceneOperations, nodeBounds, worldMatrix, mapPoint } from "./creativeScene";
import { alignObjects, containingFrame, distributeObjects, defaultLayout, layoutSnapTargets, snapLayoutMove } from "./creativeLayout";
import { creativeLayoutDrag } from "./creativeLayoutDrag";
import { groupSelection } from "./creativeSelection";
import type { CreativeScene } from "$lib/types/generated/daemon_api";
const rect = (x: number, y: number, width = 20, height = 20, parent?: string) => createNode({ kind: "shape", shape: "rectangle", stroke: "#000", fill: "#fff", stroke_width: 1 }, { x, y, width, height }, parent);
const scene = (nodes: CreativeScene["nodes"]): CreativeScene => ({ schema: "medousa-create", version: 3, background: "transparent", nodes });

it("centers a single object in a rotated frame's axes, including nested rotated groups", () => {
  const frame = createNode({ kind: "frame", name: "Frame", background: "#fff" }, { x: 100, y: 200, width: 400, height: 300 });
  frame.transform.rotation = 30;
  const a = rect(10, 20, 40, 60, frame.id), b = rect(80, 90, 60, 20, frame.id);
  const grouped = groupSelection([a.id, b.id], [frame, a, b]);
  const base = applySceneOperations(scene([frame, a, b]), grouped.operations);
  const group = base.nodes.find(n => n.id === grouped.selected[0])!;
  group.transform.rotation = 35;
  const before = structuredClone(base);
  expect(containingFrame([a.id], base.nodes)?.id).toBe(frame.id);
  const aligned = applySceneOperations(base, alignObjects([a.id], base.nodes, "center", "frame"));
  const next = aligned.nodes.find(n => n.id === a.id)!;
  const center = mapPoint(worldMatrix(next, aligned.nodes), { x: 20, y: 30 });
  const expected = mapPoint(worldMatrix(frame, base.nodes), { x: 200, y: 0 });
  // Project the center back onto the frame's local horizontal axis.
  const origin = mapPoint(worldMatrix(frame, base.nodes), { x: 0, y: 0 });
  expect((center.x - expected.x) * Math.cos(Math.PI / 6) + (center.y - expected.y) * Math.sin(Math.PI / 6)).toBeCloseTo(0);
  expect(Math.hypot(center.x - origin.x, center.y - origin.y)).toBeGreaterThan(0);
  expect(next.transform.rotation).toBe(a.transform.rotation);
  expect(base).toEqual(before);
});
it("spaces different-sized objects by equal gaps, preserves endpoints and skips locked objects", () => {
  const a = rect(0, 0, 20), b = rect(40, 30, 40), c = rect(200, 50, 60), locked = rect(30, 0);
  locked.locked = true;
  const base = scene([a, b, c, locked]);
  const next = applySceneOperations(base, distributeObjects([a.id, b.id, c.id, locked.id], base.nodes, "x"));
  expect(next.nodes[0]).toBe(a); expect(next.nodes[2]).toBe(c); expect(next.nodes[3]).toBe(locked);
  expect(next.nodes[1].transform).toMatchObject({ x: 90, y: 30, width: 40 });
  expect(distributeObjects([a.id, b.id], base.nodes, "x")).toEqual([]);
});
it("keeps nested descendants together and distributes visible rotated bounds", () => {
  const frame = createNode({ kind: "frame", name: "F", background: "#fff" }, { x: 10, y: 20, width: 500, height: 500 });
  frame.transform.rotation = 20;
  const a = rect(20, 10, 20, 40, frame.id), b = rect(110, 100, 30, 80, frame.id), c = rect(200, 300, 40, 30, frame.id);
  b.transform.rotation = 45;
  const base = scene([frame, a, b, c]), next = applySceneOperations(base, distributeObjects([a.id, b.id, c.id], base.nodes, "y"));
  const boxes = next.nodes.slice(1).map(n => nodeBounds(n, next.nodes)).sort((a, b) => a.y - b.y);
  expect(boxes[1].y - boxes[0].y - boxes[0].height).toBeCloseTo(boxes[2].y - boxes[1].y - boxes[1].height);
});
it("snaps edges/centers with a screen-pixel tolerance and can bypass all snapping", () => {
  const bounds = { x: 0, y: 0, width: 20, height: 20 }, target = { x: 100, y: 50, width: 40, height: 40 };
  const snapped = snapLayoutMove(bounds, [target], { x: 77, y: 29 }, defaultLayout, 1);
  expect(snapped.delta).toEqual({ x: 80, y: 30 }); expect(snapped.guides).toHaveLength(2);
  expect(snapLayoutMove(bounds, [target], { x: 77, y: 29 }, defaultLayout, 4).delta.x).toBe(77);
  const grid = { ...defaultLayout, smartGuides: false, snapGrid: true };
  expect(snapLayoutMove(bounds, [], { x: -9, y: 21 }, grid, 1).delta).toEqual({ x: -16, y: 16 });
  expect(snapLayoutMove(bounds, [target], { x: 77, y: 29 }, grid, 1, true).delta).toEqual({ x: 77, y: 29 });
});
it("excludes moving descendants, hidden artwork and unrelated frames from guide targets", () => {
  const frame = createNode({ kind: "frame", name: "F", background: "#fff" }, { x: 0, y: 0, width: 500, height: 500 });
  const a = rect(10, 20, 20, 20, frame.id), b = rect(100, 120, 20, 20, frame.id), hidden = rect(200, 220, 20, 20, frame.id), unrelated = rect(600, 600);
  hidden.visible = false; b.locked = true;
  expect(layoutSnapTargets([a.id], [frame, a, b, hidden, unrelated])).toEqual([nodeBounds(frame, [frame]), nodeBounds(b, [frame, b])]);
  expect(alignObjects([unrelated.id], [unrelated], "center", "frame")).toEqual([]);
});
it("movement reuses original geometry, never accumulates snap errors, and resets guides", () => {
  const a = rect(0, 0), b = rect(100, 50), base = scene([a, b]); let guides: unknown[] = [];
  const drag = creativeLayoutDrag(() => ({ settings: defaultLayout, zoom: 1, bypass: false }), value => guides = value);
  expect(drag.move(base, [a.id], { x: 77, y: 29 }).nodes[0].transform.x).toBe(80);
  expect(drag.move(base, [a.id], { x: 30, y: 10 }).nodes[0].transform.x).toBe(30);
  expect(a.transform.x).toBe(0); drag.reset(); expect(guides).toEqual([]);
});
