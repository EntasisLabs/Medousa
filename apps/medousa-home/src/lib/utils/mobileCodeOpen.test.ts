import { beforeEach, describe, expect, it, vi } from "vitest";

const harness = vi.hoisted(() => {
  const mobileCodeWorkspaceState = {
    selectedWorkId: null as string | null,
    beginProjectOpen: vi.fn(),
    enterProject: vi.fn(),
  };
  mobileCodeWorkspaceState.beginProjectOpen.mockImplementation((id: string) => {
    mobileCodeWorkspaceState.selectedWorkId = id;
  });
  return {
    mobileCodeWorkspaceState,
    undertakings: {
      detail: null,
      select: vi.fn(),
    },
    codeWorkspace: {
      hydrate: vi.fn(),
      tabsFor: vi.fn(() => []),
      isDirty: vi.fn(() => false),
      activeFor: vi.fn(() => null),
    },
    ensureCodeWorkspaceTree: vi.fn(),
  };
});

vi.mock("$lib/stores/chat.svelte", () => ({ chat: {} }));
vi.mock("$lib/daemon", () => ({ setSessionCodeBinding: vi.fn() }));
vi.mock("$lib/mobileNavigation", () => ({ switchMobileTab: vi.fn() }));
vi.mock("$lib/stores/codeWorkspace.svelte", () => ({
  codeWorkspace: harness.codeWorkspace,
}));
vi.mock("$lib/stores/mobileCodeWorkspaceState.svelte", () => ({
  mobileCodeWorkspaceState: harness.mobileCodeWorkspaceState,
}));
vi.mock("$lib/stores/undertakings.svelte", () => ({
  undertakings: harness.undertakings,
}));
vi.mock("$lib/utils/codeWorkspaceController", () => ({
  ensureCodeWorkspaceTree: harness.ensureCodeWorkspaceTree,
}));

import { enterMobileCodeProject } from "$lib/utils/mobileCodeOpen";

describe("enterMobileCodeProject", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    harness.mobileCodeWorkspaceState.selectedWorkId = null;
    harness.undertakings.detail = null;
    harness.undertakings.select.mockResolvedValue(undefined);
    harness.codeWorkspace.hydrate.mockResolvedValue(undefined);
    harness.ensureCodeWorkspaceTree.mockResolvedValue({ files: [] });
  });

  it("leaves the project list before project detail finishes loading", async () => {
    let finishSelection!: () => void;
    harness.undertakings.select.mockImplementationOnce(
      () => new Promise<void>((resolve) => (finishSelection = resolve)),
    );

    const opening = enterMobileCodeProject("work-1");

    expect(harness.mobileCodeWorkspaceState.beginProjectOpen).toHaveBeenCalledWith("work-1");
    expect(harness.mobileCodeWorkspaceState.selectedWorkId).toBe("work-1");
    expect(harness.codeWorkspace.hydrate).not.toHaveBeenCalled();

    finishSelection();
    await opening;

    expect(harness.mobileCodeWorkspaceState.enterProject).toHaveBeenCalledWith(
      "work-1",
      "files",
    );
  });

  it("does not reopen a project after the user backs out while it is loading", async () => {
    harness.undertakings.select.mockImplementationOnce(async () => {
      harness.mobileCodeWorkspaceState.selectedWorkId = null;
    });

    await enterMobileCodeProject("work-1");

    expect(harness.codeWorkspace.hydrate).not.toHaveBeenCalled();
    expect(harness.mobileCodeWorkspaceState.enterProject).not.toHaveBeenCalled();
  });
});
