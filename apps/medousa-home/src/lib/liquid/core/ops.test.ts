import { describe, expect, it } from "vitest";
import { applyOp, applyOps, type SceneOp } from "./ops";
import { createNode, createScene, findNode, type Scene, type SceneNode } from "./scene";

function skeletonDoc(): SceneNode {
  return createNode({
    id: "doc",
    type: "document",
    slots: { flow: [createNode({ id: "sec", type: "section" })] },
  });
}

function planned(rev = 1): Scene {
  return applyOp(createScene("chat"), {
    op: "plan_layout",
    surfaceId: "chat",
    root: skeletonDoc(),
    rev,
  });
}

describe("applyOp — plan_layout", () => {
  it("sets root and rev", () => {
    const scene = planned(3);
    expect(scene.rev).toBe(3);
    expect(scene.root?.id).toBe("doc");
  });

  it("ignores a layout for another surface", () => {
    const scene = createScene("chat");
    const next = applyOp(scene, {
      op: "plan_layout",
      surfaceId: "vault",
      root: skeletonDoc(),
      rev: 1,
    });
    expect(next).toBe(scene);
  });

  it("drops an older layout rev", () => {
    const scene = planned(5);
    const next = applyOp(scene, {
      op: "plan_layout",
      surfaceId: "chat",
      root: skeletonDoc(),
      rev: 4,
    });
    expect(next).toBe(scene);
  });
});

describe("applyOp — fill_slot", () => {
  it("fills a named slot", () => {
    const scene = planned();
    const next = applyOp(scene, {
      op: "fill_slot",
      nodeId: "sec",
      slot: "body",
      nodes: [createNode({ id: "prose", type: "prose", props: { markdown: "hi" } })],
    });
    expect(findNode(next.root, "prose")?.props.markdown).toBe("hi");
  });

  it("is idempotent (same op twice → equal tree)", () => {
    const op: SceneOp = {
      op: "fill_slot",
      nodeId: "sec",
      slot: "body",
      nodes: [createNode({ id: "prose", type: "prose" })],
    };
    const once = applyOp(planned(), op);
    const twice = applyOp(once, op);
    expect(twice.root).toEqual(once.root);
  });

  it("no-ops for a missing node (same ref)", () => {
    const scene = planned();
    const next = applyOp(scene, { op: "fill_slot", nodeId: "ghost", slot: "x", nodes: [] });
    expect(next).toBe(scene);
  });

  it("drops a stale-rev fill", () => {
    const scene = planned(5);
    const next = applyOp(scene, {
      op: "fill_slot",
      nodeId: "sec",
      slot: "body",
      nodes: [createNode({ id: "prose", type: "prose" })],
      rev: 4,
    });
    expect(next).toBe(scene);
  });
});

describe("applyOp — patch_props / set_binding / set_fill_state", () => {
  it("merges props", () => {
    const scene = applyOp(planned(), {
      op: "patch_props",
      nodeId: "sec",
      props: { a: 1 },
    });
    const next = applyOp(scene, { op: "patch_props", nodeId: "sec", props: { b: 2 } });
    expect(findNode(next.root, "sec")?.props).toEqual({ a: 1, b: 2 });
  });

  it("sets a binding", () => {
    const next = applyOp(planned(), {
      op: "set_binding",
      nodeId: "sec",
      binding: { source: "vault:query", ref: "tag:x", mode: "read" },
    });
    expect(findNode(next.root, "sec")?.binding?.ref).toBe("tag:x");
  });

  it("sets fill state and error", () => {
    const next = applyOp(planned(), {
      op: "set_fill_state",
      nodeId: "sec",
      state: "error",
      error: "boom",
    });
    const sec = findNode(next.root, "sec");
    expect(sec?.fillState).toBe("error");
    expect(sec?.meta?.error).toBe("boom");
  });
});

describe("applyOp — precompute / remove", () => {
  it("stores a precomputed variant", () => {
    const next = applyOp(planned(), {
      op: "precompute",
      nodeId: "sec",
      variant: "detail",
      root: createNode({ id: "detail-doc", type: "document" }),
    });
    expect(findNode(next.root, "sec")?.precomputed?.detail?.id).toBe("detail-doc");
  });

  it("removes a nested node", () => {
    const scene = applyOp(planned(), {
      op: "fill_slot",
      nodeId: "sec",
      slot: "body",
      nodes: [createNode({ id: "prose", type: "prose" })],
    });
    const next = applyOp(scene, { op: "remove", nodeId: "prose" });
    expect(findNode(next.root, "prose")).toBeNull();
  });

  it("removing the root clears the scene", () => {
    const next = applyOp(planned(), { op: "remove", nodeId: "doc" });
    expect(next.root).toBeNull();
  });
});

describe("applyOps", () => {
  it("applies a batch in order (bones then fill)", () => {
    const scene = applyOps(createScene("chat"), [
      { op: "plan_layout", surfaceId: "chat", root: skeletonDoc(), rev: 1 },
      { op: "fill_slot", nodeId: "sec", slot: "body", nodes: [createNode({ id: "prose", type: "prose" })] },
      { op: "set_fill_state", nodeId: "sec", state: "ready" },
    ]);
    expect(scene.rev).toBe(1);
    expect(findNode(scene.root, "prose")).not.toBeNull();
    expect(findNode(scene.root, "sec")?.fillState).toBe("ready");
  });
});
