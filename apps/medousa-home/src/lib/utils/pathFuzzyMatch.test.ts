import { describe, expect, it } from "vitest";

import { fuzzyMatchPaths, fuzzyScorePath } from "./pathFuzzyMatch";

describe("pathFuzzyMatch", () => {
  it("scores contiguous and subsequence matches", () => {
    expect(fuzzyScorePath("src", "src/lib.rs")).toBeGreaterThan(
      fuzzyScorePath("src", "apps/source.ts"),
    );
    expect(fuzzyScorePath("cw", "codeWorkspace.ts")).toBeGreaterThan(0);
    expect(fuzzyScorePath("zzz", "codeWorkspace.ts")).toBe(0);
  });

  it("ranks filename hits above deep path noise", () => {
    const files = [
      { path: "apps/medousa-home/src/lib/stores/codeWorkspace.svelte.ts" },
      { path: "docs/codeWorkspace.md" },
      { path: "crates/medousa-code/src/workspace.rs" },
    ];
    const hits = fuzzyMatchPaths(files, "codeWorkspace", 10);
    expect(hits[0]?.path).toContain("codeWorkspace.svelte.ts");
    expect(hits.map((file) => file.path)).toContain("docs/codeWorkspace.md");
  });

  it("falls back to path subsequences when the exact name misses", () => {
    const files = [
      { path: "apps/medousa-home/src/lib/components/work/CodeSourceEditor.svelte" },
      { path: "README.md" },
    ];
    const hits = fuzzyMatchPaths(files, "cse", 5);
    expect(hits.some((file) => file.path.includes("CodeSourceEditor"))).toBe(true);
  });
});
