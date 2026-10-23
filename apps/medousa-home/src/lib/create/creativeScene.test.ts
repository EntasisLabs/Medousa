import { describe, it, expect } from "vitest";
import {
  applySceneOperations,
  createNode,
  inverse,
  mapPoint,
  parentPoint,
  worldMatrix,
  selectionRoots,
  nodeBounds,
  nodeDimensions,
  starterScene,
} from "./creativeScene";
describe("mixed canvas geometry", () => {
  it("maps gestures through rotated parent transforms without moving children twice", () => {
    const frame = createNode(
      { kind: "frame", name: "Frame", background: "#fff" },
      { x: 100, y: 200, width: 400, height: 300 },
    );
    frame.transform.rotation = 90;
    const child = createNode(
      {
        kind: "text",
        text: "Title",
        color: "#000",
        font_family: "sans-serif",
        font_size: 20,
      },
      { x: 20, y: 40, width: 80, height: 30 },
      frame.id,
    );
    const nodes = [frame, child],
      world = mapPoint(worldMatrix(child, nodes), { x: 5, y: 8 }),
      local = mapPoint(inverse(worldMatrix(child, nodes)), world);
    expect(local.x).toBeCloseTo(5);
    expect(local.y).toBeCloseTo(8);
    expect(parentPoint(child, nodes, world).x).toBeCloseTo(25);
    expect(selectionRoots([frame.id, child.id], nodes)).toEqual([frame]);
    expect(nodeBounds(child, nodes).width).toBeCloseTo(nodeDimensions(child).height);
  });
  it("honors locks inherited from a frame", () => {
    const frame = starterScene("screenshot").nodes[0];
    frame.locked = true;
    const child = createNode(
      { kind: "group" },
      { x: 0, y: 0, width: 10, height: 10 },
      frame.id,
    );
    expect(selectionRoots([child.id], [frame, child])).toEqual([]);
  });
  it("previews mixed edits without mutating source and removes descendants regardless of stored order", () => {
    const frame = starterScene("screenshot").nodes[0],
      child = createNode(
        { kind: "group" },
        { x: 0, y: 0, width: 30, height: 30 },
        frame.id,
      ),
      grandchild = createNode(
        { kind: "group" },
        { x: 0, y: 0, width: 10, height: 10 },
        child.id,
      );
    const scene = {
      ...starterScene("blank"),
      nodes: [grandchild, child, frame],
    };
    expect(
      applySceneOperations(scene, [{ op: "remove_node", id: frame.id }]).nodes,
    ).toEqual([]);
    expect(scene.nodes).toHaveLength(3);
    const changed = { ...child, opacity: 0.5 };
    expect(
      applySceneOperations(scene, [{ op: "put_node", node: changed }]).nodes[1]
        .opacity,
    ).toBe(0.5);
    expect(child.opacity).toBe(1);
  });
});
it("resizing rotated ink keeps its anchor and input metadata", async () => {
  const { resizeNode, localMatrix } = await import("./creativeScene");
  const node = createNode(
    {
      kind: "ink",
      color: "#000",
      input: "pen",
      brush: {
        kind: "pen",
        size: 3,
        thinning: 0.5,
        smoothing: 0.5,
        streamline: 0.5,
        opacity: 1,
      },
      points: [{ x: 10, y: 5, pressure: 0.7, tilt_x: 15, elapsed_ms: 20 }],
    },
    { x: 30, y: 40, width: 20, height: 10 },
  );
  node.transform.rotation = 45;
  const before = localMatrix(node),
    after = resizeNode(node, 40, 30);
  expect(localMatrix(after)[4]).toBeCloseTo(before[4]);
  expect(localMatrix(after)[5]).toBeCloseTo(before[5]);
  expect(after.content).toMatchObject({
    points: [{ x: 20, y: 15, pressure: 0.7, tilt_x: 15, elapsed_ms: 20 }],
  });
});
it("fits large frames and retains a zoom anchor below legacy drawing's minimum", async () => {
  const { creativeFit, creativeZoom } = await import("./creativeScene");
  const fitted = creativeFit(
    { x: 100, y: 200, width: 8192, height: 8192 },
    { width: 320, height: 600 },
  );
  expect(fitted.zoom).toBeLessThan(0.25);
  expect(fitted.zoom * 8192).toBeLessThan(320);
  const anchor = { x: 150, y: 250 },
    before = {
      x: (anchor.x - fitted.panX) / fitted.zoom,
      y: (anchor.y - fitted.panY) / fitted.zoom,
    },
    zoomed = creativeZoom(fitted, anchor, 0.15);
  expect((anchor.x - zoomed.panX) / zoomed.zoom).toBeCloseTo(before.x);
  expect((anchor.y - zoomed.panY) / zoomed.zoom).toBeCloseTo(before.y);
});
