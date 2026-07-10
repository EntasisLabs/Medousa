import { describe, expect, it } from "vitest";
import { applyOps, createScene, findNode } from "$lib/liquid/core";
import { coerceNode, decodeSceneOp, decodeSceneOps } from "./sceneStream";

describe("coerceNode", () => {
  it("requires id and type", () => {
    expect(coerceNode({ type: "prose" })).toBeNull();
    expect(coerceNode({ id: "a" })).toBeNull();
    expect(coerceNode("nope")).toBeNull();
    expect(coerceNode(null)).toBeNull();
  });

  it("defaults fillState and owner, empty props", () => {
    const node = coerceNode({ id: "a", type: "prose" });
    expect(node).toEqual({ id: "a", type: "prose", props: {}, fillState: "skeleton", owner: "agent" });
  });

  it("keeps valid fillState/owner and rejects unknown", () => {
    const ok = coerceNode({ id: "a", type: "prose", fillState: "ready", owner: "user" });
    expect(ok?.fillState).toBe("ready");
    expect(ok?.owner).toBe("user");
    const bad = coerceNode({ id: "a", type: "prose", fillState: "bogus", owner: "root" });
    expect(bad?.fillState).toBe("skeleton");
    expect(bad?.owner).toBe("agent");
  });

  it("coerces slots recursively and drops malformed children", () => {
    const node = coerceNode({
      id: "doc",
      type: "document",
      slots: {
        flow: [
          { id: "p1", type: "prose", props: { markdown: "hi" } },
          { type: "prose" }, // missing id → dropped
          "garbage", // not a record → dropped
        ],
        notSlot: "ignored",
      },
    });
    expect(node?.slots?.flow).toHaveLength(1);
    expect(node?.slots?.flow[0].id).toBe("p1");
    expect(node?.slots?.notSlot).toBeUndefined();
  });

  it("coerces a valid binding and rejects an unknown source", () => {
    const ok = coerceNode({
      id: "n",
      type: "board",
      binding: { source: "work:board", mode: "readwrite", ref: "b1", live: true },
    });
    expect(ok?.binding).toEqual({ source: "work:board", mode: "readwrite", ref: "b1", live: true });
    const bad = coerceNode({ id: "n", type: "board", binding: { source: "evil:exec", mode: "read" } });
    expect(bad?.binding).toBeUndefined();
  });
});

describe("decodeSceneOp", () => {
  it("drops non-records and unknown discriminators", () => {
    expect(decodeSceneOp("x")).toBeNull();
    expect(decodeSceneOp({ op: "delete_everything", nodeId: "a" })).toBeNull();
  });

  it("plan_layout requires a valid root and a surface", () => {
    expect(decodeSceneOp({ op: "plan_layout", root: { id: "r", type: "stack" } })).toBeNull();
    const op = decodeSceneOp({ op: "plan_layout", surfaceId: "s", root: { id: "r", type: "stack" }, rev: 2 });
    expect(op).toMatchObject({ op: "plan_layout", surfaceId: "s", rev: 2 });
  });

  it("stamps the surface override onto plan_layout regardless of model input", () => {
    const op = decodeSceneOp(
      { op: "plan_layout", surfaceId: "model-chosen", root: { id: "r", type: "stack" } },
      "chat:turn-1",
    );
    expect(op).toMatchObject({ op: "plan_layout", surfaceId: "chat:turn-1", rev: 0 });
  });

  it("fill_slot requires nodeId, slot and an array of nodes", () => {
    expect(decodeSceneOp({ op: "fill_slot", nodeId: "r", slot: "flow" })).toBeNull();
    const op = decodeSceneOp({
      op: "fill_slot",
      nodeId: "r",
      slot: "flow",
      nodes: [{ id: "p", type: "prose" }, { type: "prose" }],
      rev: 1,
    });
    expect(op).toMatchObject({ op: "fill_slot", nodeId: "r", slot: "flow", rev: 1 });
    expect(op && "nodes" in op && op.nodes).toHaveLength(1);
  });

  it("set_fill_state validates the state and keeps an error", () => {
    expect(decodeSceneOp({ op: "set_fill_state", nodeId: "r", state: "melting" })).toBeNull();
    const op = decodeSceneOp({ op: "set_fill_state", nodeId: "r", state: "error", error: "boom" });
    expect(op).toEqual({ op: "set_fill_state", nodeId: "r", state: "error", error: "boom", rev: undefined });
  });

  it("remove requires a nodeId", () => {
    expect(decodeSceneOp({ op: "remove" })).toBeNull();
    expect(decodeSceneOp({ op: "remove", nodeId: "r" })).toMatchObject({ op: "remove", nodeId: "r" });
  });
});

describe("decodeSceneOps → applyOps (bones-first streaming)", () => {
  it("drops invalid ops from a batch", () => {
    const ops = decodeSceneOps([
      { op: "plan_layout", surfaceId: "ignored", root: { id: "r", type: "stack" } },
      { op: "nope" },
      42,
    ], "chat:t1");
    expect(ops).toHaveLength(1);
  });

  it("paints skeleton bones, then fills the slot in place", () => {
    const surface = "chat:t1";
    const wire = [
      {
        op: "plan_layout",
        surfaceId: surface,
        rev: 1,
        root: {
          id: "doc",
          type: "document",
          fillState: "ready",
          slots: { flow: [{ id: "p1", type: "prose", fillState: "skeleton" }] },
        },
      },
    ];
    let scene = applyOps(createScene(surface), decodeSceneOps(wire, surface));
    expect(scene.rev).toBe(1);
    expect(findNode(scene.root, "p1")?.fillState).toBe("skeleton");

    const fill = [
      {
        op: "fill_slot",
        nodeId: "doc",
        slot: "flow",
        rev: 1,
        nodes: [{ id: "p1", type: "prose", props: { markdown: "Filled." }, fillState: "ready" }],
      },
    ];
    scene = applyOps(scene, decodeSceneOps(fill, surface));
    const p1 = findNode(scene.root, "p1");
    expect(p1?.fillState).toBe("ready");
    expect(p1?.props.markdown).toBe("Filled.");
  });

  it("drops a fill that targets a superseded revision", () => {
    const surface = "chat:t1";
    const planned = applyOps(
      createScene(surface),
      decodeSceneOps(
        [{ op: "plan_layout", surfaceId: surface, rev: 3, root: { id: "doc", type: "document", slots: { flow: [{ id: "p1", type: "prose" }] } } }],
        surface,
      ),
    );
    const stale = applyOps(
      planned,
      decodeSceneOps(
        [{ op: "fill_slot", nodeId: "doc", slot: "flow", rev: 2, nodes: [{ id: "p1", type: "prose", props: { markdown: "stale" } }] }],
        surface,
      ),
    );
    expect(stale).toBe(planned);
  });
});
