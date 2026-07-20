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

/** Leaf pane — ordered tabs + focused tab. */
export type EditorGroup = {
  id: string;
  tabIds: string[];
  activeTabId: string | null;
};

/** Binary split tree for TMUX-style tiling. */
export type SplitNode =
  | { type: "group"; id: string }
  | {
      type: "branch";
      id: string;
      direction: "row" | "column";
      /** Share for child `a` in 0..1 (clamped on write). */
      ratio: number;
      a: SplitNode;
      b: SplitNode;
    };

export type ShellTabKind = ShellTab["kind"];
export type SplitDirection = "right" | "down";

/** Soft cap on leaf panes (v1). */
export const MAX_SHELL_PANES = 4;

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
