import { describe, expect, it } from "vitest";
import {
  ancestorsForPath,
  buildVaultLookupSnapshot,
  resolveWikilinkWithLookup,
} from "./vaultLookup";

describe("vaultLookup", () => {
  it("builds shared maps once and resolves wikilinks in O(L)", () => {
    const snapshot = buildVaultLookupSnapshot(
      [
        { path: "a/one.md", title: "One", modified_at_utc: "" },
        { path: "b/two.md", title: "Two", modified_at_utc: "" },
        { path: "c/same.md", title: "Same", modified_at_utc: "" },
        { path: "d/same.md", title: "Same", modified_at_utc: "" },
      ],
      7,
      "a/one.md",
    );
    expect(snapshot.generation).toBe(7);
    expect(snapshot.knownPaths.size).toBe(4);
    expect(resolveWikilinkWithLookup("one", "a/x.md", snapshot)).toEqual({
      kind: "resolved",
      path: "a/one.md",
    });
    expect(resolveWikilinkWithLookup("same", null, snapshot).kind).toBe("ambiguous");
    expect(snapshot.ancestorIdsForSelection.has("a")).toBe(true);
  });

  it("computes ancestors in O(depth)", () => {
    const parents = new Map<string, string | null>([
      ["a/b/c.md", "a/b"],
      ["a/b", "a"],
      ["a", null],
    ]);
    const ancestors = ancestorsForPath("a/b/c.md", parents);
    expect(ancestors.has("a")).toBe(true);
    expect(ancestors.has("a/b")).toBe(true);
    expect(ancestors.has("a/b/c.md")).toBe(true);
  });
});
