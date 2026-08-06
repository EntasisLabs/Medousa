import { beforeEach, describe, expect, it } from "vitest";

import { codeWorkbenchState } from "./codeWorkbenchState.svelte";

describe("codeWorkbenchState", () => {
  beforeEach(() => codeWorkbenchState.reset());

  it("keeps independent stacks per work id", () => {
    codeWorkbenchState.record("a", "a.ts", 1, "g1");
    codeWorkbenchState.record("b", "b.ts", 2, "g2");
    expect(codeWorkbenchState.entriesFor("a")).toHaveLength(1);
    expect(codeWorkbenchState.entriesFor("b")).toHaveLength(1);
    expect(codeWorkbenchState.canNavigate("a", -1)).toBe(false);
  });
});
