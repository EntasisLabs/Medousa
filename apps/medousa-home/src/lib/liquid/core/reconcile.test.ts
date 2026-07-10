import { describe, expect, it } from "vitest";
import { reconcile } from "./reconcile";
import { applyOp } from "./ops";
import { createNode, createScene, findNode, type SceneNode } from "./scene";

describe("reconcile", () => {
  it("returns next as-is when there is no prev", () => {
    const next = createNode({ id: "doc", type: "document" });
    expect(reconcile(null, next)).toBe(next);
  });

  it("preserves a ready fillState across a skeleton resubmit", () => {
    const prev = createNode({ id: "sec", type: "section", fillState: "ready" });
    const next = createNode({ id: "sec", type: "section", fillState: "skeleton" });
    expect(reconcile(prev, next).fillState).toBe("ready");
  });

  it("lets an explicit non-skeleton state win", () => {
    const prev = createNode({ id: "sec", type: "section", fillState: "ready" });
    const next = createNode({ id: "sec", type: "section", fillState: "error" });
    expect(reconcile(prev, next).fillState).toBe("error");
  });

  it("replaces wholesale when the type changes", () => {
    const prev = createNode({ id: "x", type: "section", fillState: "ready" });
    const next = createNode({ id: "x", type: "card", fillState: "skeleton" });
    const merged = reconcile(prev, next);
    expect(merged.type).toBe("card");
    expect(merged.fillState).toBe("skeleton");
  });

  it("keeps a prior binding when next omits one", () => {
    const prev = createNode({
      id: "sec",
      type: "section",
      binding: { source: "vault:query", ref: "tag:x", mode: "read" },
    });
    const next = createNode({ id: "sec", type: "section" });
    expect(reconcile(prev, next).binding?.ref).toBe("tag:x");
  });

  it("unions precomputed variants (new wins on conflict)", () => {
    const prev = createNode({
      id: "sec",
      type: "section",
      precomputed: {
        a: createNode({ id: "old-a", type: "document" }),
        b: createNode({ id: "old-b", type: "document" }),
      },
    });
    const next = createNode({
      id: "sec",
      type: "section",
      precomputed: { a: createNode({ id: "new-a", type: "document" }) },
    });
    const merged = reconcile(prev, next);
    expect(merged.precomputed?.a.id).toBe("new-a");
    expect(merged.precomputed?.b.id).toBe("old-b");
  });

  it("recurses into slots preserving nested transient state", () => {
    const prev: SceneNode = createNode({
      id: "doc",
      type: "document",
      slots: { flow: [createNode({ id: "sec", type: "section", fillState: "ready" })] },
    });
    const next: SceneNode = createNode({
      id: "doc",
      type: "document",
      slots: {
        flow: [
          createNode({ id: "sec", type: "section", fillState: "skeleton" }),
          createNode({ id: "sec2", type: "section" }),
        ],
      },
    });
    const merged = reconcile(prev, next);
    expect(findNode(merged, "sec")?.fillState).toBe("ready");
    expect(findNode(merged, "sec2")).not.toBeNull();
  });

  it("plan_layout resubmit does not regress a filled node (end-to-end)", () => {
    let scene = applyOp(createScene("chat"), {
      op: "plan_layout",
      surfaceId: "chat",
      root: createNode({ id: "sec", type: "section" }),
      rev: 1,
    });
    scene = applyOp(scene, { op: "set_fill_state", nodeId: "sec", state: "ready" });
    scene = applyOp(scene, {
      op: "plan_layout",
      surfaceId: "chat",
      root: createNode({ id: "sec", type: "section" }),
      rev: 2,
    });
    expect(findNode(scene.root, "sec")?.fillState).toBe("ready");
    expect(scene.rev).toBe(2);
  });
});
