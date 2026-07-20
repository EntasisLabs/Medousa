import { describe, expect, it } from "vitest";
import {
  countLeaves,
  leafOrder,
  neighborInDirection,
  removeLeaf,
  setBranchRatio,
  splitLeaf,
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
