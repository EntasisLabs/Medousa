import { chat } from "$lib/stores/chat.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { leafOrder } from "$lib/utils/shellSplitTree";
import type { ShellDesktopLayout, ShellTab } from "$lib/types/shellTabs";
import type { WorkshopCommand, CommandPreview } from "$lib/commands/types";

function matchesWorkspaceQuery(command: WorkshopCommand, value: string): boolean {
  const needle = value.trim().toLowerCase();
  if (!needle) return true;
  return `${command.label} ${command.subtitle ?? ""} ${command.keywords ?? ""}`
    .toLowerCase()
    .includes(needle);
}

export function collectDesktopGroups(
  desktopId: string,
  layout: ShellDesktopLayout,
  value: string,
  previewForTab: (tab: ShellTab) => CommandPreview | undefined,
  tabKindLabel: (tab: ShellTab) => string,
): Array<{ id: string; label: string; commands: WorkshopCommand[] }> {
  const order = leafOrder(layout.splitRoot);
  const paneByTabId = new Map<string, number>();
  for (const [paneOffset, groupId] of order.entries()) {
    const group = layout.groups.find((entry) => entry.id === groupId);
    for (const tabId of group?.tabIds ?? []) paneByTabId.set(tabId, paneOffset + 1);
  }

  const tabs = layout.tabs
    .map((tab): WorkshopCommand => ({
      id: `spotlight-tab:${desktopId}:${tab.id}`,
      section: "open",
      label: tab.title,
      subtitle: `${tabKindLabel(tab)} · Pane ${paneByTabId.get(tab.id) ?? 1}`,
      keywords: `${tab.kind} ${tab.title}`,
      preview: previewForTab(tab),
      run: async (runCtx) => {
        await shellTabs.revealSearchHit(desktopId, tab.id);
        runCtx.callbacks.close();
      },
    }))
    .filter((command) => matchesWorkspaceQuery(command, value));

  const panes = order
    .map((groupId, paneOffset): WorkshopCommand | null => {
      const group = layout.groups.find((entry) => entry.id === groupId);
      if (!group) return null;
      const paneTabs = group.tabIds
        .map((id) => layout.tabs.find((tab) => tab.id === id))
        .filter((tab): tab is ShellTab => Boolean(tab));
      const active = paneTabs.find((tab) => tab.id === group.activeTabId) ?? paneTabs[0];
      const count = paneTabs.length;
      return {
        id: `spotlight-pane:${desktopId}:${groupId}`,
        section: "open",
        label: `Pane ${paneOffset + 1}${active ? ` · ${active.title}` : ""}`,
        subtitle: count === 0 ? "Empty tiled window" : `${count} open tab${count === 1 ? "" : "s"}`,
        keywords: `pane tile window ${paneTabs.map((tab) => tab.title).join(" ")}`,
        run: async (runCtx) => {
          if (desktopId !== shellTabs.activeDesktopId) {
            await shellTabs.switchDesktop(desktopId);
          }
          shellTabs.focusGroup(groupId);
          runCtx.callbacks.close();
        },
      };
    })
    .filter((command): command is WorkshopCommand => Boolean(command))
    .filter((command) => matchesWorkspaceQuery(command, value));

  return [
    ...(tabs.length > 0 ? [{ id: "open-tabs", label: "Open tabs", commands: tabs }] : []),
    ...(panes.length > 0
      ? [{ id: "tiled-windows", label: "Tiled windows", commands: panes }]
      : []),
  ];
}


export function spotlightTabKind(tab: ShellTab): string {
  if (tab.kind === "lme") {
    const lme = lmeWorkspace.tabs.find(entry => entry.tabId === tab.lmeTabId);
    return lme?.kind === "note" ? "Note" : lme?.kind === "code" ? "Code" : "Document";
  }
  if (tab.kind === "surface") return tab.surfaceId;
  return tab.kind === "chat" ? "Chat" : tab.kind[0]!.toUpperCase() + tab.kind.slice(1);
}
export function spotlightTabPreview(tab: ShellTab): CommandPreview | undefined {
  if (tab.kind === "chat") return { kind: "chat", sessionId: tab.sessionId, text: chat.sessions.find(session => session.session_id === tab.sessionId)?.preview?.trim() || `Open conversation “${tab.title}”.` };
  if (tab.kind === "lme") {
    const lme = lmeWorkspace.tabs.find(entry => entry.tabId === tab.lmeTabId);
    if (lme?.kind === "note") return { kind: "note", path: lme.path };
  }
  return undefined;
}
export function spotlightWorkspaceLayout(scopeId: string): ShellDesktopLayout | null {
  const desktop = shellTabs.desktops.find(entry => entry.id === scopeId);
  if (!desktop) return null;
  return desktop.id !== shellTabs.activeDesktopId ? desktop.layout : {
    tabs: shellTabs.tabs, groups: shellTabs.groups, splitRoot: shellTabs.splitRoot,
    activeGroupId: shellTabs.activeGroupId, zoomedGroupId: shellTabs.zoomedGroupId,
  };
}
