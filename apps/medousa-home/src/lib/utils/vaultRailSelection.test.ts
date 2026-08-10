import { describe, expect, it } from "vitest";
import {
  flattenTreeNotePaths,
  rangePathsBetween,
} from "./vaultRailSelection";
import type { VaultTreeNode } from "$lib/types/vault";

describe("vaultRailSelection", () => {
  it("flattens tree note paths in DFS order", () => {
    const tree: VaultTreeNode[] = [
      {
        name: "journal",
        path: null,
        isFolder: true,
        children: [
          {
            name: "a.md",
            path: "journal/a.md",
            isFolder: false,
            children: [],
          },
          {
            name: "nested",
            path: null,
            isFolder: true,
            children: [
              {
                name: "b.md",
                path: "journal/nested/b.md",
                isFolder: false,
                children: [],
              },
            ],
          },
        ],
      },
    ];
    expect(flattenTreeNotePaths(tree)).toEqual([
      "journal/a.md",
      "journal/nested/b.md",
    ]);
  });

  it("builds inclusive ranges between anchors", () => {
    const ordered = ["a", "b", "c", "d"];
    expect(rangePathsBetween(ordered, "b", "d")).toEqual(["b", "c", "d"]);
    expect(rangePathsBetween(ordered, "d", "b")).toEqual(["b", "c", "d"]);
    expect(rangePathsBetween(ordered, "missing", "c")).toEqual(["c"]);
  });
});
