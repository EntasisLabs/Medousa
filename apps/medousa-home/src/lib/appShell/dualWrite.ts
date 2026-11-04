/**
 * ShellCore commit helpers: await the mutation, then paint the returned
 * snapshot onto the Svelte shell host. localStorage persist runs from paint.
 */

import type { ShellDesktop, ShellTab } from "$lib/types/shellTabs";
import type {
  ShellDesktop as AppShellDesktop,
  ShellSnapshot,
  ShellTab as AppShellTab,
} from "$lib/types/generated/app_api";
import type { SplitEdge } from "$lib/types/generated/app_api";
import {
  closeShellPane,
  closeShellTab,
  createShellDesktop,
  focusShellGroup,
  focusShellTab,
  hydrateShellLayout,
  openShellTab,
  patchShellTabTitle,
  removeShellDesktop,
  renameShellDesktop,
  setShellSplitRatio,
  setShellZoomedGroup,
  splitShellPane,
  switchShellDesktop,
  moveShellTabToDesktop,
  moveShellPaneToDesktop,
} from "./client";

export type ShellSnapshotPainter = (snapshot: ShellSnapshot) => void;

export function toAppShellTab(tab: ShellTab): AppShellTab {
  switch (tab.kind) {
    case "create":
      return {
        kind: "create",
        id: tab.id,
        documentId: tab.documentId,
        workshopId: tab.workshopId,
        title: tab.title,
      };
    case "chat":
      return {
        kind: "chat",
        id: tab.id,
        sessionId: tab.sessionId,
        title: tab.title,
      };
    case "lme":
      return {
        kind: "lme",
        id: tab.id,
        lmeTabId: tab.lmeTabId,
        title: tab.title,
      };
    case "web":
      return {
        kind: "web",
        id: tab.id,
        browserTabId: tab.browserTabId,
        title: tab.title,
      };
    case "surface":
      return {
        kind: "surface",
        id: tab.id,
        surfaceId: tab.surfaceId,
        title: tab.title,
      };
    case "terminal":
      return {
        kind: "terminal",
        id: tab.id,
        sessionId: tab.sessionId,
        title: tab.title,
        workId: tab.workId ?? null,
        executionRuntimeId: tab.executionRuntimeId ?? null,
      };
  }
}

export function toAppShellDesktops(desktops: ShellDesktop[]): AppShellDesktop[] {
  return desktops as unknown as AppShellDesktop[];
}

function commitShell(paint: ShellSnapshotPainter, run: () => Promise<ShellSnapshot>) {
  void run()
    .then((snapshot) => paint(snapshot))
    .catch(() => {
      /* Core may be unavailable in pure-DOM vitest without the stdio backend. */
    });
}

export function commitHydrateShell(
  paint: ShellSnapshotPainter,
  options: {
    workspaceScopeId: string;
    desktops: ShellDesktop[];
    activeDesktopId: string;
  },
) {
  commitShell(paint, () =>
    hydrateShellLayout({
      workspaceScopeId: options.workspaceScopeId,
      desktops: toAppShellDesktops(options.desktops),
      activeDesktopId: options.activeDesktopId,
    }),
  );
}

export function commitOpenShellTab(
  paint: ShellSnapshotPainter,
  tab: ShellTab,
  options?: { groupId?: string; activate?: boolean },
) {
  commitShell(paint, () =>
    openShellTab({
      tab: toAppShellTab(tab),
      groupId: options?.groupId ?? null,
      activate: options?.activate ?? true,
    }),
  );
}

export function commitFocusShellTab(paint: ShellSnapshotPainter, tabId: string) {
  commitShell(paint, () => focusShellTab(tabId));
}

export function commitCloseShellTab(paint: ShellSnapshotPainter, tabId: string) {
  commitShell(paint, () => closeShellTab(tabId));
}

export function commitSwitchShellDesktop(
  paint: ShellSnapshotPainter,
  desktopId: string,
) {
  commitShell(paint, () => switchShellDesktop(desktopId));
}

export function commitSplitShellPane(
  paint: ShellSnapshotPainter,
  options: {
    groupId: string;
    edge: SplitEdge;
    newGroupId: string;
    moveTabId?: string | null;
  },
) {
  commitShell(paint, () =>
    splitShellPane({
      groupId: options.groupId,
      edge: options.edge,
      newGroupId: options.newGroupId,
      moveTabId: options.moveTabId ?? null,
    }),
  );
}

export function commitCloseShellPane(paint: ShellSnapshotPainter, groupId: string) {
  commitShell(paint, () => closeShellPane(groupId));
}

export function commitSetShellSplitRatio(
  paint: ShellSnapshotPainter,
  options: { branchId: string; ratio: number },
) {
  commitShell(paint, () => setShellSplitRatio(options));
}

export function commitFocusShellGroup(paint: ShellSnapshotPainter, groupId: string) {
  commitShell(paint, () => focusShellGroup(groupId));
}

export function commitCreateShellDesktop(
  paint: ShellSnapshotPainter,
  options: {
    desktopId: string;
    name: string;
    /** When set, restore this desktop after create (core CreateDesktop activates). */
    restoreDesktopId?: string | null;
  },
) {
  commitShell(paint, async () => {
    const snapshot = await createShellDesktop({
      desktopId: options.desktopId,
      name: options.name,
    });
    const restore = options.restoreDesktopId?.trim();
    if (!restore || restore === options.desktopId) return snapshot;
    return switchShellDesktop(restore);
  });
}

export function commitRenameShellDesktop(
  paint: ShellSnapshotPainter,
  options: { desktopId: string; name: string },
) {
  commitShell(paint, () => renameShellDesktop(options));
}

export function commitRemoveShellDesktop(
  paint: ShellSnapshotPainter,
  desktopId: string,
) {
  commitShell(paint, () => removeShellDesktop(desktopId));
}

export function commitSetShellZoomedGroup(
  paint: ShellSnapshotPainter,
  groupId?: string | null,
) {
  commitShell(paint, () => setShellZoomedGroup(groupId));
}

export function commitPatchShellTabTitle(
  paint: ShellSnapshotPainter,
  options: { tabId: string; title: string },
) {
  commitShell(paint, () => patchShellTabTitle(options));
}

export function commitMoveShellTabToDesktop(
  paint: ShellSnapshotPainter,
  options: { tabId: string; desktopId: string },
) {
  commitShell(paint, () => moveShellTabToDesktop(options));
}

export function commitMoveShellPaneToDesktop(
  paint: ShellSnapshotPainter,
  options: { groupId: string; desktopId: string },
) {
  commitShell(paint, () => moveShellPaneToDesktop(options));
}
