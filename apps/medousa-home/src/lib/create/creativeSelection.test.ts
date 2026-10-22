import { expect, it } from "vitest";
import { createNode, applySceneOperations, worldMatrix, mapPoint } from "./creativeScene";
import { groupSelection, ungroupSelection, scaleSelection, selectionBounds, selectionTarget, layerRows, arrangeSelection, alignSelection } from "./creativeSelection";
import { createDrawBrush } from "$lib/draw/drawDocument";
import type { CreativeNode, CreativeScene } from "$lib/types/generated/daemon_api";
function fixture() {
  const frame = createNode({ kind: "frame", name: "Artboard", background: "#fff" }, { x: 100, y: 200, width: 500, height: 400 });
  frame.transform.rotation = 25;
  const ink = createNode({ kind: "ink", color: "#123456", input: "pen", brush: createDrawBrush("pencil", 6), points: [{ x: 0, y: 0, pressure: .4, tilt_x: 20 }, { x: 80, y: 40, pressure: .9, tilt_x: 30 }] }, { x: 20, y: 40, width: 80, height: 40 }, frame.id);
  ink.transform.rotation = 15;
  const text = createNode({ kind: "text", text: "Smiley", color: "#123456", font_family: "sans-serif", font_size: 32 }, { x: 150, y: 70, width: 120, height: 45 }, frame.id);
  text.transform.rotation = -10;
  const other = createNode({ kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 2 }, { x: 300, y: 80, width: 50, height: 70 }, frame.id);
  const scene: CreativeScene = { schema: "medousa-create", version: 3, background: "transparent", nodes: [ink, other, text, frame] };
  return { scene, frame, ink, text, other };
}
function corners(node: CreativeNode, nodes: CreativeNode[]) {
  return [{ x: 0, y: 0 }, { x: node.transform.width, y: 0 }, { x: node.transform.width, y: node.transform.height }].map(p => mapPoint(worldMatrix(node, nodes), p));
}
function close(actual: { x: number; y: number }[], expected: { x: number; y: number }[]) {
  actual.forEach((p, i) => { expect(p.x).toBeCloseTo(expected[i].x); expect(p.y).toBeCloseTo(expected[i].y); });
}
it("groups siblings in a rotated frame without moving their artwork or changing ink", () => {
  const { scene, ink, text, frame, other } = fixture(), before = structuredClone(scene);
  const edit = groupSelection([ink.id, text.id], scene.nodes), grouped = applySceneOperations(scene, edit.operations);
  const group = grouped.nodes.find(n => n.id === edit.selected[0])!;
  expect(group).toMatchObject({ parent_id: frame.id, name: "Group 1", content: { kind: "group" } });
  for (const node of [ink, text]) {
    const next = grouped.nodes.find(n => n.id === node.id)!;
    expect(next.parent_id).toBe(group.id);
    close(corners(next, grouped.nodes), corners(node, scene.nodes));
    expect(next.content).toEqual(node.content);
  }
  expect(grouped.nodes.find(n => n.id === other.id)).toBe(other);
  expect(scene).toEqual(before);
  expect(grouped.nodes.filter(n => n.parent_id === frame.id).map(n => n.id)).toEqual([other.id, group.id]);
});
it("ungroups after moving and rotating a group without jumping or losing descendants", () => {
  const { scene, ink, text, frame } = fixture(), edit = groupSelection([ink.id, text.id], scene.nodes);
  const grouped = applySceneOperations(scene, edit.operations), group = grouped.nodes.find(n => n.id === edit.selected[0])!;
  group.transform.x += 70; group.transform.rotation = 45;
  const before = [ink, text].map(n => corners(grouped.nodes.find(next => next.id === n.id)!, grouped.nodes));
  const ungroup = ungroupSelection([group.id], grouped.nodes), result = applySceneOperations(grouped, ungroup.operations);
  expect(result.nodes.some(n => n.id === group.id)).toBe(false);
  for (const [i, node] of [ink, text].entries()) {
    const next = result.nodes.find(n => n.id === node.id)!;
    expect(next.parent_id).toBe(frame.id); expect(next.opacity).toBe(node.opacity);
    close(corners(next, result.nodes), before[i]);
  }
  expect(new Set(ungroup.selected)).toEqual(new Set([ink.id, text.id]));
});
it("scales grouped ink, text, and geometry together under a rotated parent", () => {
  const { scene, ink, text, frame } = fixture(), edit = groupSelection([ink.id, text.id], scene.nodes);
  const grouped = applySceneOperations(scene, edit.operations), before = structuredClone(grouped), anchor = { x: 50, y: 60 }, factor = 2;
  const resized = scaleSelection(edit.selected, grouped.nodes, anchor, factor);
  for (const old of grouped.nodes.filter(n => n.id !== frame.id && n.id !== scene.nodes[1].id)) {
    const next = resized.find(n => n.id === old.id)!;
    close(corners(next, resized), corners(old, grouped.nodes).map(p => ({ x: anchor.x + (p.x - anchor.x) * factor, y: anchor.y + (p.y - anchor.y) * factor })));
  }
  expect(resized.find(n => n.id === frame.id)).toBe(frame);
  expect(resized.find(n => n.id === ink.id)!.content).toMatchObject({ brush: { size: 12 }, points: [expect.objectContaining({ pressure: .4, tilt_x: 20 }), expect.objectContaining({ pressure: .9, tilt_x: 30 })] });
  expect(resized.find(n => n.id === text.id)!.content).toMatchObject({ font_size: 64 });
  expect(grouped).toEqual(before);
});
it("rejects grouping across containers and respects inherited locks", () => {
  const { scene, ink, text, frame } = fixture();
  expect(() => groupSelection([frame.id, ink.id], scene.nodes)).toThrow("at least two");
  expect(() => groupSelection([ink.id, { ...text, parent_id: null }.id], scene.nodes.map(n => n.id === text.id ? { ...n, parent_id: null } : n))).toThrow("same frame");
  frame.locked = true;
  expect(() => groupSelection([ink.id, text.id], scene.nodes)).toThrow("at least two");
});
it("layers follow containment rather than global storage order, with collapsible groups", () => {
  const { scene, ink, text, frame, other } = fixture(), edit = groupSelection([ink.id, text.id], scene.nodes), grouped = applySceneOperations(scene, edit.operations);
  const closed = layerRows(grouped.nodes, n => n.content.kind === "group");
  expect(closed.map(r => [r.node.id, r.depth])).toEqual([[frame.id, 0], [edit.selected[0], 1], [other.id, 1]]);
  expect(layerRows(grouped.nodes, () => true).map(r => r.node.id)).toEqual([frame.id]);
  expect(layerRows(grouped.nodes, () => false).map(r => r.node.id)).toEqual([frame.id, edit.selected[0], text.id, ink.id, other.id]);
  expect(selectionTarget(ink.id, grouped.nodes, null)).toBe(edit.selected[0]);
  expect(selectionTarget(ink.id, grouped.nodes, edit.selected[0])).toBe(ink.id);
  expect(selectionTarget(ink.id, grouped.nodes, null, [ink.id])).toBe(ink.id);
});
it("aligns in world space and arranges only siblings", () => {
  const { scene, ink, text, frame } = fixture();
  const aligned = applySceneOperations(scene, alignSelection([ink.id, text.id], scene.nodes, "left"));
  expect(selectionBounds([ink.id], aligned.nodes)!.x).toBeCloseTo(selectionBounds([text.id], aligned.nodes)!.x);
  const ordered = applySceneOperations(scene, arrangeSelection([ink.id], scene.nodes, 1));
  expect(ordered.nodes.filter(n => n.parent_id === frame.id).map(n => n.id)).toEqual([scene.nodes[1].id, ink.id, text.id]);
});

it("keeps translucent groups intact rather than changing overlap compositing", () => {
  const { scene, ink, text } = fixture(), edit = groupSelection([ink.id, text.id], scene.nodes);
  const grouped = applySceneOperations(scene, edit.operations);
  grouped.nodes.find(n => n.id === edit.selected[0])!.opacity = .5;
  const before = structuredClone(grouped);
  expect(() => ungroupSelection(edit.selected, grouped.nodes)).toThrow("opacity to 100%");
  expect(grouped).toEqual(before);
});
