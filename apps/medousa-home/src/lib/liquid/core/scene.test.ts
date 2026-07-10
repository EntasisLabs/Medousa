import { describe, expect, it } from "vitest";
import {
  cloneNode,
  collectNodeIds,
  createNode,
  createScene,
  findNode,
  mapNode,
  removeNode,
  walk,
  type SceneNode,
} from "./scene";

function doc(): SceneNode {
  return createNode({
    id: "doc",
    type: "document",
    slots: {
      flow: [
        createNode({ id: "sec", type: "section", slots: { body: [createNode({ id: "prose", type: "prose" })] } }),
        createNode({ id: "card", type: "card" }),
      ],
    },
  });
}

describe("scene helpers", () => {
  it("createNode applies defaults", () => {
    const node = createNode({ id: "n", type: "prose" });
    expect(node.fillState).toBe("skeleton");
    expect(node.owner).toBe("agent");
    expect(node.props).toEqual({});
  });

  it("createScene starts empty", () => {
    expect(createScene("chat")).toEqual({ surfaceId: "chat", root: null, rev: 0 });
  });

  it("findNode locates nested nodes and misses cleanly", () => {
    const root = doc();
    expect(findNode(root, "prose")?.type).toBe("prose");
    expect(findNode(root, "card")?.id).toBe("card");
    expect(findNode(root, "nope")).toBeNull();
    expect(findNode(null, "x")).toBeNull();
  });

  it("walk can stop descending", () => {
    const seen: string[] = [];
    walk(doc(), (node) => {
      seen.push(node.id);
      if (node.id === "sec") return false;
    });
    expect(seen).toContain("sec");
    expect(seen).not.toContain("prose");
  });

  it("collectNodeIds returns every id", () => {
    expect(collectNodeIds(doc()).sort()).toEqual(["card", "doc", "prose", "sec"]);
  });

  it("mapNode returns new refs only along the changed path", () => {
    const root = doc();
    const next = mapNode(root, "prose", (n) => ({ ...n, fillState: "ready" }));
    expect(next).not.toBe(root);
    expect(findNode(next, "prose")?.fillState).toBe("ready");
    // untouched sibling subtree keeps referential identity
    const card = root.slots!.flow[1];
    expect(next.slots!.flow[1]).toBe(card);
  });

  it("mapNode returns same ref when id absent", () => {
    const root = doc();
    expect(mapNode(root, "ghost", (n) => n)).toBe(root);
  });

  it("removeNode drops a nested node", () => {
    const root = doc();
    const next = removeNode(root, "card");
    expect(findNode(next, "card")).toBeNull();
    expect(findNode(next, "prose")).not.toBeNull();
  });

  it("removeNode leaves root untouched when id is root", () => {
    const root = doc();
    expect(removeNode(root, "doc")).toBe(root);
  });

  it("cloneNode deep-copies slots and props", () => {
    const root = doc();
    const clone = cloneNode(root);
    expect(clone).not.toBe(root);
    expect(clone.slots!.flow[0]).not.toBe(root.slots!.flow[0]);
    expect(collectNodeIds(clone).sort()).toEqual(collectNodeIds(root).sort());
  });
});
