import { chat } from "$lib/stores/chat.svelte";
import { contextShell } from "$lib/stores/contextShell.svelte";
import { contextThreads } from "$lib/stores/contextThreads.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import {
  lmeWorkspace,
  type LmeExplorerMode,
  type LmeTab,
} from "$lib/stores/lmeWorkspace.svelte";
import { peersShell } from "$lib/stores/peersShell.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { hostnameFromUrl, tabDisplayLabel } from "$lib/utils/browserFavicon";
import { buildContextThreadEntries } from "$lib/utils/contextThreads";
import { formatSessionLabel, formatSessionWhen } from "$lib/utils/formatSession";

export const NAV_RAIL_NEST_LIMIT = 5;
export const LME_NOTE_NEST_PATH_PREFIX = "note-path:";

/** Surfaces that can show Cursor-style nested recent items in nav mode. */
export const NAV_RAIL_NEST_SURFACES = new Set([
  "chat",
  "peers",
  "web",
  "context",
]);

/** Explorer mode → open LME tab kind for rail nests. */
export const LME_MODE_TAB_KIND: Partial<Record<LmeExplorerMode, LmeTab["kind"]>> = {
  notes: "note",
  files: "file",
  presentations: "deck",
  scripts: "script",
  agents: "manuscript",
  flows: "flow",
  schedules: "schedule",
};

export type NavRailNestItem = {
  id: string;
  label: string;
  meta?: string;
  /** Soft accent (e.g. unread peer). */
  accent?: boolean;
};

export function nestKeyForLmeMode(mode: LmeExplorerMode): string {
  return `lme:${mode}`;
}

export function surfaceSupportsRailNest(surfaceId: string): boolean {
  return NAV_RAIL_NEST_SURFACES.has(surfaceId);
}

export function lmeModeSupportsRailNest(mode: LmeExplorerMode): boolean {
  return Boolean(LME_MODE_TAB_KIND[mode]);
}

export function nestItemsForLmeMode(mode: LmeExplorerMode): NavRailNestItem[] {
  const kind = LME_MODE_TAB_KIND[mode];
  if (!kind) return [];
  const tabs = lmeWorkspace.tabs.filter((tab) => tab.kind === kind);
  // Newest tabs are appended — show the most recent first.
  const items: NavRailNestItem[] = tabs
    .slice(-NAV_RAIL_NEST_LIMIT)
    .reverse()
    .map((tab) => ({
      id: tab.tabId,
      label: tab.title?.trim() || prettyKind(tab.kind),
    }));

  // Notes: fill remaining slots from vault recents (Chat-style always-there list).
  if (mode === "notes" && items.length < NAV_RAIL_NEST_LIMIT) {
    const openPaths = new Set(
      tabs.filter((tab) => tab.kind === "note").map((tab) => tab.path),
    );
    for (const note of vault.recentNotesList(NAV_RAIL_NEST_LIMIT)) {
      if (items.length >= NAV_RAIL_NEST_LIMIT) break;
      if (openPaths.has(note.path)) continue;
      items.push({
        id: `${LME_NOTE_NEST_PATH_PREFIX}${note.path}`,
        label: note.title?.trim() || note.path.split("/").pop() || note.path,
      });
    }
  }

  return items;
}

export function nestItemIsActiveForLmeMode(
  mode: LmeExplorerMode,
  itemId: string,
): boolean {
  if (!lmeModeSupportsRailNest(mode)) return false;
  if (itemId.startsWith(LME_NOTE_NEST_PATH_PREFIX)) {
    const path = itemId.slice(LME_NOTE_NEST_PATH_PREFIX.length);
    const active = lmeWorkspace.activeTab;
    return active?.kind === "note" && active.path === path;
  }
  return lmeWorkspace.activeTabId === itemId;
}

export async function activateLmeModeNestItem(
  mode: LmeExplorerMode,
  itemId: string,
): Promise<void> {
  lmeWorkspace.setExplorerMode(mode);
  if (itemId.startsWith(LME_NOTE_NEST_PATH_PREFIX)) {
    await lmeWorkspace.openNote(itemId.slice(LME_NOTE_NEST_PATH_PREFIX.length));
    return;
  }
  await lmeWorkspace.activateTab(itemId);
}

export function nestItemsForSurface(surfaceId: string): NavRailNestItem[] {
  switch (surfaceId) {
    case "chat":
      return chat.sessions.slice(0, NAV_RAIL_NEST_LIMIT).map((session) => ({
        id: session.session_id,
        label: formatSessionLabel(session),
        meta: formatSessionWhen(session.last_timestamp) || undefined,
      }));
    case "peers":
      return peersShell.rows.slice(0, NAV_RAIL_NEST_LIMIT).map((row) => ({
        id: row.workshopId,
        label: row.label,
        meta: row.lastMessage?.body?.trim()
          ? truncate(row.lastMessage.body.trim(), 28)
          : row.nearby
            ? "Nearby"
            : undefined,
        accent: row.unreadCount > 0,
      }));
    case "web":
      return humanBrowser.tabs.slice(0, NAV_RAIL_NEST_LIMIT).map((tab) => {
        const label = tabDisplayLabel(tab.title, tab.url);
        const host = compactHost(tab.url);
        // Avoid "facebook.com" + "facebook.com" when title is just the host.
        const meta =
          host && !label.toLowerCase().includes(host.toLowerCase()) ? host : undefined;
        return { id: tab.id, label, meta };
      });
    case "context":
      return buildContextThreadEntries(contextThreads.nodes)
        .slice(0, NAV_RAIL_NEST_LIMIT)
        .map((entry) => ({
          id: entry.syncKey,
          label: entry.title,
          // Compact relative time (matches chat nest) — not "Tuesday · 7:34 PM".
          meta: formatSessionWhen(entry.timestamp) || undefined,
        }));
    default:
      return [];
  }
}

export function nestItemIsActive(surfaceId: string, itemId: string): boolean {
  switch (surfaceId) {
    case "chat":
      return (
        (shellTabs.activeTab?.kind === "chat" &&
          shellTabs.activeTab.sessionId === itemId) ||
        chat.sessionId === itemId
      );
    case "peers":
      return peersShell.selectedPeerId === itemId;
    case "web":
      return humanBrowser.activeTab?.id === itemId;
    case "context":
      return (
        contextShell.selectedThreadId === itemId ||
        contextThreads.railFocusSyncKey === itemId ||
        contextThreads.detail?.node.sync_key === itemId
      );
    default:
      return false;
  }
}

export async function activateNestItem(
  surfaceId: string,
  itemId: string,
): Promise<void> {
  switch (surfaceId) {
    case "chat":
      // switchSession mirrors the shell tab (same path as SessionSidebar).
      // Avoid openChat+activate first — that raced reopen of the prior session.
      await chat.switchSession(itemId);
      return;
    case "peers":
      shellTabs.openSurface("peers", { activate: true });
      peersShell.selectPeer(itemId);
      return;
    case "web":
      await humanBrowser.activateTab(itemId);
      return;
    case "context":
      shellTabs.openSurface("context", { activate: true });
      contextShell.activeTab = "threads";
      contextShell.selectedThreadId = itemId;
      contextThreads.focusThreadFromRail(itemId);
      return;
    default:
      return;
  }
}

/** Soft prefetch so nest rows aren’t empty on first paint. */
export function prefetchRailNestData(): void {
  if (chat.sessions.length === 0) {
    void chat.refreshSessions();
  }
  if (contextThreads.nodes.length === 0) {
    void contextThreads.refresh({ limit: NAV_RAIL_NEST_LIMIT });
  }
}

function truncate(value: string, max: number): string {
  if (value.length <= max) return value;
  return `${value.slice(0, max - 1)}…`;
}

function compactHost(url: string): string | undefined {
  const host = hostnameFromUrl(url).replace(/^www\./, "");
  if (!host || host === "about:blank") return undefined;
  return host;
}

function prettyKind(kind: LmeTab["kind"]): string {
  if (kind === "note") return "Note";
  if (kind === "script") return "Script";
  if (kind === "file") return "File";
  if (kind === "deck") return "Deck";
  if (kind === "manuscript") return "Manuscript";
  if (kind === "flow") return "Flow";
  if (kind === "schedule") return "Schedule";
  return kind;
}
