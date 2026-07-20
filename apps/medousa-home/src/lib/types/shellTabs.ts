import type { Surface } from "$lib/types/ui";

/** Shell center tab — first-class open work (chat, doc, page, or singleton surface). */
export type ShellTab =
  | {
      id: string;
      kind: "chat";
      sessionId: string;
      title: string;
    }
  | {
      id: string;
      kind: "lme";
      lmeTabId: string;
      title: string;
    }
  | {
      id: string;
      kind: "web";
      browserTabId: string;
      title: string;
    }
  | {
      id: string;
      kind: "surface";
      surfaceId: Surface;
      title: string;
    };

/** Future split-view pane. Phase 0–3 use a single group. */
export type EditorGroup = {
  id: string;
  tabIds: string[];
  activeTabId: string | null;
};

export type ShellTabKind = ShellTab["kind"];

/** Singleton surfaces that open as at most one tab each. */
export const SHELL_SURFACE_TAB_IDS = new Set<string>([
  "library",
  "peers",
  "messaging",
  "context",
  "work",
  "calendar",
  "settings",
  "runtime",
  "profiles",
]);

export function isShellSurfaceTabId(surfaceId: string): boolean {
  return SHELL_SURFACE_TAB_IDS.has(surfaceId);
}
