import { beforeEach, describe, expect, it } from "vitest";

import { mobileCodeWorkspaceState } from "./mobileCodeWorkspaceState.svelte";

describe("mobileCodeWorkspaceState", () => {
  beforeEach(() => {
    mobileCodeWorkspaceState.resetForWorkshopSwitch();
  });

  it("enters a project on the requested landing surface", () => {
    mobileCodeWorkspaceState.enterProject("work-1", "files");
    expect(mobileCodeWorkspaceState.selectedWorkId).toBe("work-1");
    expect(mobileCodeWorkspaceState.surface).toBe("files");
    expect(mobileCodeWorkspaceState.chromeMode).toBe("files");
  });

  it("restores the last room on re-enter unless attention lands on Changes", () => {
    mobileCodeWorkspaceState.enterProject("work-1", "files");
    mobileCodeWorkspaceState.switchRoom("editor");
    mobileCodeWorkspaceState.leaveProject();
    mobileCodeWorkspaceState.enterProject("work-1", "files");
    expect(mobileCodeWorkspaceState.surface).toBe("editor");

    mobileCodeWorkspaceState.leaveProject();
    mobileCodeWorkspaceState.enterProject("work-1", "changes");
    expect(mobileCodeWorkspaceState.surface).toBe("changes");
  });

  it("treats a switcher tap as a sibling room, not a jump", () => {
    mobileCodeWorkspaceState.enterProject("work-1", "editor");
    mobileCodeWorkspaceState.switchRoom("terminal");
    expect(mobileCodeWorkspaceState.handleBack()).toBe(true);
    expect(mobileCodeWorkspaceState.surface).toBe("editor");
    expect(mobileCodeWorkspaceState.handleBack()).toBe(true);
    expect(mobileCodeWorkspaceState.selectedWorkId).toBeNull();
  });

  it("pops a Files jump before leaving the project", () => {
    mobileCodeWorkspaceState.enterProject("work-1", "files");
    mobileCodeWorkspaceState.jumpToEditor("files", "src/main.ts");
    expect(mobileCodeWorkspaceState.surface).toBe("editor");
    expect(mobileCodeWorkspaceState.presentation?.lastOpenedPath).toBe("src/main.ts");
    expect(mobileCodeWorkspaceState.handleBack()).toBe(true);
    expect(mobileCodeWorkspaceState.surface).toBe("files");
    expect(mobileCodeWorkspaceState.handleBack()).toBe(true);
    expect(mobileCodeWorkspaceState.selectedWorkId).toBeNull();
  });

  it("returns Editor → Terminal switch to Editor, not Files, after a Files jump", () => {
    mobileCodeWorkspaceState.enterProject("work-1", "files");
    mobileCodeWorkspaceState.jumpToEditor("files", "a.ts");
    mobileCodeWorkspaceState.switchRoom("terminal");
    expect(mobileCodeWorkspaceState.handleBack()).toBe(true);
    expect(mobileCodeWorkspaceState.surface).toBe("editor");
    expect(mobileCodeWorkspaceState.handleBack()).toBe(true);
    expect(mobileCodeWorkspaceState.surface).toBe("files");
  });

  it("closes sheets before popping a jump", () => {
    mobileCodeWorkspaceState.enterProject("work-1", "files");
    mobileCodeWorkspaceState.jumpToEditor("files");
    mobileCodeWorkspaceState.fileSwitcherOpen = true;
    expect(mobileCodeWorkspaceState.handleBack()).toBe(true);
    expect(mobileCodeWorkspaceState.fileSwitcherOpen).toBe(false);
    expect(mobileCodeWorkspaceState.surface).toBe("editor");
  });

  it("resolves the auto Files filter from changed/recent evidence", () => {
    mobileCodeWorkspaceState.enterProject("work-1", "files");
    expect(
      mobileCodeWorkspaceState.resolvedFilesFilter({
        hasChangedFiles: true,
        hasRecentFiles: true,
      }),
    ).toBe("changed");
    mobileCodeWorkspaceState.setFilesFilter("tree");
    expect(
      mobileCodeWorkspaceState.resolvedFilesFilter({
        hasChangedFiles: true,
        hasRecentFiles: true,
      }),
    ).toBe("tree");
  });
});
