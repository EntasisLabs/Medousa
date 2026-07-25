import type { ShellTab, ShellTabKind } from "$lib/types/shellTabs";

/** One open shell tab, located across virtual desktops / panes. */
export type ShellTabSearchHit = {
  tabId: string;
  title: string;
  kind: ShellTabKind;
  desktopId: string;
  desktopName: string;
  groupId: string;
  /** 1-based pane index within that desktop's leaf order. */
  paneIndex: number;
  /** Currently focused shell tab. */
  isActive: boolean;
  /** Lives on the focused virtual desktop. */
  isActiveDesktop: boolean;
};

export function hitMatchesQuery(hit: ShellTabSearchHit, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return (
    hit.title.toLowerCase().includes(q) ||
    hit.kind.toLowerCase().includes(q) ||
    hit.desktopName.toLowerCase().includes(q) ||
    `pane ${hit.paneIndex}`.includes(q) ||
    String(hit.paneIndex).includes(q)
  );
}

export function filterTabSearchHits(
  hits: ShellTabSearchHit[],
  query: string,
): ShellTabSearchHit[] {
  const q = query.trim();
  if (!q) {
    // Active tab first, then current desktop, then catalog order.
    return [...hits].sort((a, b) => {
      if (a.isActive !== b.isActive) return a.isActive ? -1 : 1;
      if (a.isActiveDesktop !== b.isActiveDesktop) return a.isActiveDesktop ? -1 : 1;
      return 0;
    });
  }
  return hits.filter((hit) => hitMatchesQuery(hit, q));
}

export function tabKindLabel(kind: ShellTabKind): string {
  if (kind === "chat") return "Chat";
  if (kind === "web") return "Web";
  if (kind === "lme") return "Doc";
  return "Surface";
}

export function titleOfTab(tab: ShellTab): string {
  return tab.title.trim() || tabKindLabel(tab.kind);
}
