import { describe, expect, it } from "vitest";
import {
  countLeaves,
  leafOrder,
  neighborInDirection,
  mergeTargetForLeaf,
  removeLeaf,
  setBranchRatio,
  splitLeaf,
  splitLeafAtEdge,
} from "./shellSplitTree";
import type { SplitNode } from "$lib/types/shellTabs";

describe("shellSplitTree", () => {
  it("splits a leaf to the right", () => {
    const root: SplitNode = { type: "group", id: "main" };
    const result = splitLeaf(root, "main", "right", "g2");
    expect(result).toBeTruthy();
    expect(countLeaves(result!.root)).toBe(2);
    expect(result!.root.type).toBe("branch");
    if (result!.root.type === "branch") {
      expect(result!.root.direction).toBe("column");
      expect(result!.root.a).toEqual({ type: "group", id: "main" });
      expect(result!.root.b).toEqual({ type: "group", id: "g2" });
    }
  });

  it("places the new leaf on the requested edge", () => {
    const root: SplitNode = { type: "group", id: "main" };

    const left = splitLeafAtEdge(root, "main", "left", "g2");
    expect(left?.root.type).toBe("branch");
    if (left?.root.type === "branch") {
      expect(left.root.direction).toBe("column");
      expect(left.root.a).toEqual({ type: "group", id: "g2" });
      expect(left.root.b).toEqual({ type: "group", id: "main" });
    }

    const top = splitLeafAtEdge(root, "main", "top", "g3");
    expect(top?.root.type).toBe("branch");
    if (top?.root.type === "branch") {
      expect(top.root.direction).toBe("row");
      expect(top.root.a).toEqual({ type: "group", id: "g3" });
      expect(top.root.b).toEqual({ type: "group", id: "main" });
    }

    const bottom = splitLeafAtEdge(root, "main", "bottom", "g4");
    expect(bottom?.root.type).toBe("branch");
    if (bottom?.root.type === "branch") {
      expect(bottom.root.direction).toBe("row");
      expect(bottom.root.a).toEqual({ type: "group", id: "main" });
      expect(bottom.root.b).toEqual({ type: "group", id: "g4" });
    }
  });

  it("removes a leaf and promotes sibling", () => {
    const root: SplitNode = {
      type: "branch",
      id: "b1",
      direction: "column",
      ratio: 0.5,
      a: { type: "group", id: "main" },
      b: { type: "group", id: "g2" },
    };
    const result = removeLeaf(root, "g2");
    expect(result.removed).toBe(true);
    expect(result.root).toEqual({ type: "group", id: "main" });
  });

  it("picks the sash-adjacent leaf as the merge target", () => {
    const root: SplitNode = {
      type: "branch",
      id: "b1",
      direction: "column",
      ratio: 0.5,
      a: { type: "group", id: "main" },
      b: {
        type: "branch",
        id: "b2",
        direction: "row",
        ratio: 0.5,
        a: { type: "group", id: "g2" },
        b: { type: "group", id: "g3" },
      },
    };
    expect(mergeTargetForLeaf(root, "main")).toBe("g2");
    expect(mergeTargetForLeaf(root, "g3")).toBe("g2");
    expect(mergeTargetForLeaf(root, "g2")).toBe("g3");
  });

  it("refuses removing the last leaf", () => {
    const root: SplitNode = { type: "group", id: "main" };
    const result = removeLeaf(root, "main");
    expect(result.removed).toBe(false);
  });

  it("clamps branch ratio", () => {
    const root: SplitNode = {
      type: "branch",
      id: "b1",
      direction: "row",
      ratio: 0.5,
      a: { type: "group", id: "a" },
      b: { type: "group", id: "b" },
    };
    const next = setBranchRatio(root, "b1", 0.05);
    expect(next.type === "branch" && next.ratio).toBe(0.2);
  });

  it("orders leaves and finds neighbors", () => {
    const root: SplitNode = {
      type: "branch",
      id: "b1",
      direction: "column",
      ratio: 0.5,
      a: { type: "group", id: "a" },
      b: { type: "group", id: "b" },
    };
    expect(leafOrder(root)).toEqual(["a", "b"]);
    expect(neighborInDirection(root, "a", "right")).toBe("b");
    expect(neighborInDirection(root, "b", "left")).toBe("a");
  });
});
