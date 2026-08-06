import { beforeEach, describe, expect, it } from "vitest";
import { codeWorkspace } from "./codeWorkspace.svelte";

describe("Code workspace navigation history", () => {
  beforeEach(() => codeWorkspace.resetForWorkshopSwitch());

  it("records precise cross-file origin and target locations", () => {
    codeWorkspace.recordNavigationLocation("work-1", "src/origin.ts", 14);
    codeWorkspace.recordNavigationLocation("work-1", "src/target.ts", 3);

    expect(codeWorkspace.navigationByWorkId["work-1"]).toEqual([
      { path: "src/origin.ts", line: 14 },
      { path: "src/target.ts", line: 3 },
    ]);
    expect(codeWorkspace.navigationIndexByWorkId["work-1"]).toBe(1);
    expect(codeWorkspace.canNavigate("work-1", -1)).toBe(true);
    expect(codeWorkspace.canNavigate("work-1", 1)).toBe(false);
  });

  it("drops forward history after navigation branches", () => {
    codeWorkspace.recordNavigationLocation("work-1", "a.ts", 1);
    codeWorkspace.recordNavigationLocation("work-1", "b.ts", 2);
    codeWorkspace.recordNavigationLocation("work-1", "c.ts", 3);
    codeWorkspace.navigationIndexByWorkId = { "work-1": 1 };

    codeWorkspace.recordNavigationLocation("work-1", "d.ts", 4);

    expect(codeWorkspace.navigationByWorkId["work-1"]).toEqual([
      { path: "a.ts", line: 1 },
      { path: "b.ts", line: 2 },
      { path: "d.ts", line: 4 },
    ]);
  });

  it("coalesces identical consecutive locations", () => {
    codeWorkspace.recordNavigationLocation("work-1", "a.ts", 7);
    codeWorkspace.recordNavigationLocation("work-1", "a.ts", 7);

    expect(codeWorkspace.navigationByWorkId["work-1"]).toHaveLength(1);
  });
});
