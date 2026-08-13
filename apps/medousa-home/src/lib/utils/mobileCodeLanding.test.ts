import { describe, expect, it } from "vitest";

import {
  projectHasAttention,
  resolveMobileCodeFilesFilter,
  resolveMobileCodeLanding,
} from "./mobileCodeLanding";

describe("projectHasAttention", () => {
  it("treats review and needs_attention as the front door", () => {
    expect(projectHasAttention({ humanPhase: "review" })).toBe(true);
    expect(projectHasAttention({ humanPhase: "needs_attention" })).toBe(true);
    expect(projectHasAttention({ forgeState: "awaiting_review" })).toBe(true);
    expect(projectHasAttention({ forgeState: "applying_decision" })).toBe(true);
  });

  it("treats a dirty working copy or dirty buffers as attention", () => {
    expect(projectHasAttention({ dirtyWorkingCopy: true })).toBe(true);
    expect(projectHasAttention({ dirtyBuffers: true })).toBe(true);
  });

  it("is quiet for a clean in-progress project", () => {
    expect(
      projectHasAttention({
        humanPhase: "work",
        forgeState: "in_progress",
      }),
    ).toBe(false);
  });
});

describe("resolveMobileCodeLanding", () => {
  it("lands on Changes when there is attention", () => {
    expect(
      resolveMobileCodeLanding({ hasAttention: true, hasOpenFile: true }),
    ).toBe("changes");
  });

  it("lands on Editor when a buffer is already open", () => {
    expect(
      resolveMobileCodeLanding({ hasAttention: false, hasOpenFile: true }),
    ).toBe("editor");
  });

  it("lands on Files for a newly provisioned or empty project", () => {
    expect(
      resolveMobileCodeLanding({ hasAttention: false, hasOpenFile: false }),
    ).toBe("files");
  });
});

describe("resolveMobileCodeFilesFilter", () => {
  it("prefers Changed, then Recent, then the tree", () => {
    expect(
      resolveMobileCodeFilesFilter({
        hasChangedFiles: true,
        hasRecentFiles: true,
      }),
    ).toBe("changed");
    expect(
      resolveMobileCodeFilesFilter({
        hasChangedFiles: false,
        hasRecentFiles: true,
      }),
    ).toBe("recent");
    expect(
      resolveMobileCodeFilesFilter({
        hasChangedFiles: false,
        hasRecentFiles: false,
      }),
    ).toBe("tree");
  });
});
