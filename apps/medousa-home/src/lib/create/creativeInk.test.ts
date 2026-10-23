import { expect, it } from "vitest";
import { createDrawBrush } from "$lib/draw/drawDocument";
import {
  creativeStroke,
  creativeStrokeNode,
  eraseCreativeInk,
  creativeSceneChanges,
  hitCreativeScene,
} from "./creativeInk";
import { createNode, worldMatrix, mapPoint } from "./creativeScene";
import type { CreativeScene } from "$lib/types/generated/daemon_api";
it("round-trips input metadata through rotated parents and splits with daemon-safe IDs", () => {
  const frame = createNode(
    { kind: "frame", name: "Frame", background: "#fff" },
    { x: 100, y: 200, width: 400, height: 300 },
  );
  frame.transform.rotation = 90;
  const node = createNode(
    {
      kind: "ink",
      color: "#123456",
      input: "pen",
      brush: createDrawBrush("pencil", 5),
      points: [
        { x: 0, y: 0, pressure: 0.8, tilt_x: 20, elapsed_ms: 0 },
        { x: 100, y: 0, pressure: 0.4, tilt_x: 30, elapsed_ms: 100 },
      ],
    },
    { x: 30, y: 40, width: 100, height: 1 },
    frame.id,
  );
  const scene: CreativeScene = {
    schema: "medousa-create",
    version: 3,
    background: "transparent",
    nodes: [frame, node],
  };
  const stroke = creativeStroke(node, scene.nodes)!;
  const rebuilt = creativeStrokeNode(stroke, scene.nodes, frame, node);
  expect(creativeStroke(rebuilt, [frame, rebuilt])).toEqual(stroke);
  const middle = mapPoint(worldMatrix(node, scene.nodes), { x: 50, y: 0 });
  const erased = eraseCreativeInk(scene, [middle], 8, "partial");
  const parts = erased.nodes.filter((n) => n.content.kind === "ink");
  expect(parts).toHaveLength(2);
  expect(parts[0].id).toBe(node.id);
  expect(parts.every((n) => /^[a-zA-Z0-9_-]+$/.test(n.id))).toBe(true);
  for (const part of parts) {
    expect(part.parent_id).toBe(frame.id);
    expect(part.content).toMatchObject({
      kind: "ink",
      input: "pen",
      brush: { kind: "pencil", size: 5 },
      points: expect.arrayContaining([
        expect.objectContaining({
          pressure: expect.any(Number),
          tilt_x: expect.any(Number),
        }),
      ]),
    });
  }
  expect(creativeSceneChanges(scene, erased)).toHaveLength(2);
});
it("respects inherited locks and does not select empty space inside ink bounds", () => {
  const frame = createNode(
    { kind: "frame", name: "Frame", background: "#fff" },
    { x: 0, y: 0, width: 200, height: 200 },
  );
  const node = createNode(
    {
      kind: "ink",
      color: "#000",
      input: "mouse",
      brush: createDrawBrush(),
      points: [
        { x: 0, y: 0 },
        { x: 100, y: 100 },
      ],
    },
    { x: 0, y: 0, width: 100, height: 100 },
    frame.id,
  );
  const scene: CreativeScene = {
    schema: "medousa-create",
    version: 3,
    background: "transparent",
    nodes: [frame, node],
  };
  expect(hitCreativeScene(scene, { x: 90, y: 10 }, 4)).toBeNull();
  frame.locked = true;
  expect(eraseCreativeInk(scene, [{ x: 50, y: 50 }], 20, "partial")).toBe(
    scene,
  );
});

it("picks the frontmost painted leaf after grouping changes global storage order", () => {
  const group = createNode({ kind: "group" }, { x: 0, y: 0, width: 200, height: 200 });
  const shape = { kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 2 } as const;
  const front = createNode(shape, { x: 10, y: 10, width: 50, height: 50 }, group.id);
  const back = createNode(shape, { x: 10, y: 10, width: 50, height: 50 });
  const scene: CreativeScene = { schema: "medousa-create", version: 3, background: "transparent", nodes: [front, back, group] };
  expect(hitCreativeScene(scene, { x: 30, y: 30 }, 1)).toBe(front.id);
  expect(hitCreativeScene({ ...scene, nodes: [front, group, back] }, { x: 30, y: 30 }, 1)).toBe(back.id);
});
