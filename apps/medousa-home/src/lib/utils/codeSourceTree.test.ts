import { describe, expect, it } from "vitest";
import { buildCodeSourceTree, flattenCodeSourceTree } from "./codeSourceTree";

describe("codeSourceTree", () => {
  it("builds directories before files and flattens expanded branches", () => {
    const tree = buildCodeSourceTree([
      { path: "README.md", byte_size: 12 },
      { path: "src/z.rs", byte_size: 2 },
      { path: "src/a.rs", byte_size: 1 },
    ]);
    expect(tree.map((node) => node.name)).toEqual(["src", "README.md"]);
    expect(tree[0].children.map((node) => node.name)).toEqual(["a.rs", "z.rs"]);
    expect(flattenCodeSourceTree(tree, new Set(["src"])).map((row) => row.path)).toEqual([
      "src",
      "src/a.rs",
      "src/z.rs",
      "README.md",
    ]);
  });
});
