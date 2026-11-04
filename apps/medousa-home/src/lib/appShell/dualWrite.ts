/**
 * Dual-write helpers: Home shell store remains the live paint owner while
 * `ShellCore` records the same layout mutations for headless / future apply.
 */

import type { ShellDesktop, ShellTab } from "$lib/types/shellTabs";
import type {
  ShellDesktop as AppShellDesktop,
  ShellTab as AppShellTab,
} from "$lib/types/generated/app_api";
import { dualWriteShell } from "./client";

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

export function dualWriteHydrateShell(options: {
  workspaceScopeId: string;
  desktops: ShellDesktop[];
  activeDesktopId: string;
}) {
  dualWriteShell({
    type: "hydrate",
    workspace_scope_id: options.workspaceScopeId,
    desktops: toAppShellDesktops(options.desktops),
    active_desktop_id: options.activeDesktopId,
  });
}

export function dualWriteOpenShellTab(
  tab: ShellTab,
  options?: { groupId?: string; activate?: boolean },
) {
  dualWriteShell({
    type: "open_tab",
    tab: toAppShellTab(tab),
    group_id: options?.groupId ?? null,
    activate: options?.activate ?? true,
  });
}

export function dualWriteFocusShellTab(tabId: string) {
  dualWriteShell({ type: "focus_tab", tab_id: tabId });
}

export function dualWriteCloseShellTab(tabId: string) {
  dualWriteShell({ type: "close_tab", tab_id: tabId });
}

export function dualWriteSwitchShellDesktop(desktopId: string) {
  dualWriteShell({ type: "switch_desktop", desktop_id: desktopId });
}
