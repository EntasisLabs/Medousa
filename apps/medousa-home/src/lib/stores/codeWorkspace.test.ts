import { beforeEach, describe, expect, it } from "vitest";

import { codeWorkbenchState } from "$lib/code/codeWorkbenchState.svelte";
import { codeWorkspace } from "./codeWorkspace.svelte";

describe("Code workbench navigation history", () => {
  beforeEach(() => {
    codeWorkspace.resetForWorkshopSwitch();
    codeWorkbenchState.reset();
  });

  it("records precise cross-file origin and target locations", () => {
    codeWorkspace.recordNavigationLocation("work-1", "src/origin.ts", 14, "g1");
    codeWorkspace.recordNavigationLocation("work-1", "src/target.ts", 3, "g2");

    expect(codeWorkbenchState.entriesFor("work-1")).toEqual([
      { workId: "work-1", path: "src/origin.ts", line: 14, groupId: "g1" },
      { workId: "work-1", path: "src/target.ts", line: 3, groupId: "g2" },
    ]);
    expect(codeWorkbenchState.indexFor("work-1")).toBe(1);
    expect(codeWorkspace.canNavigate("work-1", -1)).toBe(true);
    expect(codeWorkspace.canNavigate("work-1", 1)).toBe(false);
  });

  it("drops forward history after navigation branches", () => {
    codeWorkspace.recordNavigationLocation("work-1", "a.ts", 1, "g1");
    codeWorkspace.recordNavigationLocation("work-1", "b.ts", 2, "g1");
    codeWorkspace.recordNavigationLocation("work-1", "c.ts", 3, "g1");
    codeWorkbenchState.restoreIndex("work-1", 1);

    codeWorkspace.recordNavigationLocation("work-1", "d.ts", 4, "g2");

    expect(codeWorkbenchState.entriesFor("work-1")).toEqual([
      { workId: "work-1", path: "a.ts", line: 1, groupId: "g1" },
      { workId: "work-1", path: "b.ts", line: 2, groupId: "g1" },
      { workId: "work-1", path: "d.ts", line: 4, groupId: "g2" },
    ]);
  });

  it("coalesces identical consecutive locations including group", () => {
    codeWorkspace.recordNavigationLocation("work-1", "a.ts", 7, "g1");
    codeWorkspace.recordNavigationLocation("work-1", "a.ts", 7, "g1");

    expect(codeWorkbenchState.entriesFor("work-1")).toHaveLength(1);

    codeWorkspace.recordNavigationLocation("work-1", "a.ts", 7, "g2");
    expect(codeWorkbenchState.entriesFor("work-1")).toHaveLength(2);
  });

  it("steps back to the prior entry with group id", () => {
    codeWorkspace.recordNavigationLocation("work-1", "a.ts", 1, "left");
    codeWorkspace.recordNavigationLocation("work-1", "b.ts", 2, "right");
    const prior = codeWorkbenchState.step("work-1", -1);
    expect(prior).toEqual({
      workId: "work-1",
      path: "a.ts",
      line: 1,
      groupId: "left",
    });
  });
});
