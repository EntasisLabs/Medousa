import { describe, expect, it } from "vitest";
import {
  flattenExpandedTreeRows,
  visibleWindow,
} from "./vaultTreeVirtual";
import type { VaultTreeNode } from "$lib/types/vault";

function folder(name: string, children: VaultTreeNode[]): VaultTreeNode {
  return {
    name,
    path: null,
    isFolder: true,
    dropPrefix: `${name}/`,
    children,
    kind: undefined,
    spaceId: null,
  };
}

function note(path: string): VaultTreeNode {
  return {
    name: path.split("/").pop()!,
    path,
    isFolder: false,
    dropPrefix: null,
    children: [],
    kind: undefined,
    spaceId: null,
  };
}

describe("vaultTreeVirtual", () => {
  it("omits collapsed children from the flat list", () => {
    const tree = [
      folder("a", [note("a/one.md"), note("a/two.md")]),
      note("root.md"),
    ];
    const collapsed = flattenExpandedTreeRows(
      tree,
      () => false,
      (node) => node.dropPrefix ?? node.path ?? node.name,
    );
    expect(collapsed.map((row) => row.node.name)).toEqual(["a", "root.md"]);
    const expanded = flattenExpandedTreeRows(
      tree,
      () => true,
      (node) => node.dropPrefix ?? node.path ?? node.name,
    );
    expect(expanded.map((row) => row.node.name)).toEqual([
      "a",
      "one.md",
      "two.md",
      "root.md",
    ]);
  });

  it("windows rows with overscan", () => {
    const window = visibleWindow(100, 280, 100, 28, 2);
    expect(window.start).toBe(8);
    expect(window.end).toBeGreaterThan(window.start);
    expect(window.totalHeight).toBe(2800);
  });
});
